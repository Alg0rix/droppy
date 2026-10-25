//! Live media metadata via the OS media controls (SMTC).
//!
//! A background MTA worker polls the current SMTC session every 5s and
//! publishes the latest snapshot to a process-wide slot. The UI thread only
//! ever calls [`latest`] (non-blocking), so a slow/stuck media pipeline can
//! never hitch the island animation.
//!
//! Mapping helpers are pure and unit-tested on any OS.

use teardrop_core::{Playback, Track};

/// Point-in-time view of whatever the OS is playing.
#[derive(Debug, Clone, PartialEq)]
pub struct SmtcSnapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub app: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
    /// Playback speed (1.0 = normal) used to extrapolate the position.
    pub rate: f32,
}

/// Position `age_ms` after a snapshot was taken: players only report the
/// timeline when something changes, so a playing track keeps advancing.
pub fn extrapolate_ms(position_ms: u64, duration_ms: u64, playing: bool, rate: f32, age_ms: u64) -> u64 {
    if !playing {
        return position_ms;
    }
    let advanced = position_ms + (age_ms as f64 * rate.clamp(0.0, 4.0) as f64) as u64;
    if duration_ms > 0 { advanced.min(duration_ms) } else { advanced }
}

/// Current extrapolated position of the published snapshot, without cloning it.
#[cfg(windows)]
pub fn position_now() -> Option<u64> {
    worker::position_now()
}

#[cfg(not(windows))]
pub fn position_now() -> Option<u64> {
    None
}

/// WinRT playback-status code (GlobalSystemMediaTransportControlsSessionPlaybackStatus.0).
pub fn status_to_playback(code: i32) -> Playback {
    match code {
        4 => Playback::Playing, // Playing
        5 => Playback::Paused,  // Paused
        _ => Playback::Stopped,
    }
}

/// Friendly app name from a SourceAppUserModelId (best-effort prefix match).
pub fn friendly_app(aumid: &str) -> &str {
    let lower = aumid.to_lowercase();
    if lower.contains("spotify") {
        "Spotify"
    } else if lower.contains("chrome") {
        "Chrome"
    } else if lower.contains("msedge") || lower.contains("edge") {
        "Edge"
    } else if lower.contains("firefox") {
        "Firefox"
    } else if lower.contains("vlc") {
        "VLC"
    } else if lower.contains("foobar") {
        "foobar2000"
    } else if lower.contains("itunes") || lower.contains("applemusic") {
        "Apple Music"
    } else if lower.contains("youtube") {
        "YouTube"
    } else {
        "Music"
    }
}

pub fn snapshot_to_track(s: &SmtcSnapshot) -> Track {
    Track {
        title: s.title.clone(),
        artist: s.artist.clone(),
        album: s.album.clone(),
        duration_ms: s.duration_ms,
        position_ms: s.position_ms.min(s.duration_ms),
        playback: if s.playing { Playback::Playing } else { Playback::Paused },
        app: s.app.clone(),
    }
}

/// Where a per-mille seek click lands inside `duration_ms`.
pub fn seek_target_ms(duration_ms: u64, permille: u16) -> u64 {
    let p = permille.min(1000) as u128;
    ((duration_ms as u128 * p) / 1000) as u64
}

/// Stable identity for the artwork of a title/artist/album triple: covers
/// are fetched and cached per key so a previous song's image can never show
/// for the next one.
pub fn art_key(title: &str, artist: &str, album: &str) -> String {
    [title, artist, album].join("\u{1}")
}

/// Cached artwork for `t`, or None while the gradient placeholder draws.
/// Non-blocking; only returns a cover fetched for this exact track.
#[cfg(windows)]
pub fn art_for(t: &Track) -> Option<std::sync::Arc<crate::ui::scene::Image>> {
    worker::art_for(t)
}

#[cfg(not(windows))]
pub fn art_for(_t: &Track) -> Option<std::sync::Arc<crate::ui::scene::Image>> {
    None
}

#[cfg(windows)]
mod worker {
    use super::SmtcSnapshot;
    use crate::ui::scene::Image;
    use std::sync::mpsc::{RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex, Once, OnceLock};
    use std::time::{Duration, Instant};
    use teardrop_core::Track;
    use windows::Win32::Graphics::Imaging::{IWICBitmapSource, IWICImagingFactory};

    const POLL_EVERY: Duration = Duration::from_secs(2);
    /// Cover art is decoded square at this size; every surface downscales.
    const ART_PX: u32 = 256;
    /// Some players publish the thumbnail a poll or two after the title, so
    /// a missing cover is retried this many times per track, not forever.
    const ART_ATTEMPTS: u8 = 5;

    static SLOT: Mutex<Option<(SmtcSnapshot, Instant)>> = Mutex::new(None);
    /// Latest fetched cover: (art key, image, consecutive failures).
    type ArtSlot = Option<(String, Option<Arc<Image>>, u8)>;
    static ART: Mutex<ArtSlot> = Mutex::new(None);
    static ONCE: Once = Once::new();
    static SEEK_TX: OnceLock<Sender<u64>> = OnceLock::new();

    pub fn publish(s: SmtcSnapshot) {
        if let Ok(mut slot) = SLOT.lock() {
            *slot = Some((s, Instant::now()));
        }
    }

    /// Non-blocking read; returns None when no poll yet or data is stale.
    pub fn latest(max_age: Duration) -> Option<SmtcSnapshot> {
        SLOT.lock()
            .ok()
            .and_then(|slot| slot.clone())
            .and_then(|(mut s, at)| {
                let age = at.elapsed();
                if age > max_age {
                    return None;
                }
                s.position_ms = super::extrapolate_ms(s.position_ms, s.duration_ms, s.playing, s.rate, age.as_millis() as u64);
                Some(s)
            })
    }

    pub fn position_now() -> Option<u64> {
        let slot = SLOT.lock().ok()?;
        let (s, at) = slot.as_ref()?;
        if at.elapsed() > Duration::from_secs(15) || s.duration_ms == 0 {
            return None;
        }
        Some(super::extrapolate_ms(s.position_ms, s.duration_ms, s.playing, s.rate, at.elapsed().as_millis() as u64))
    }

    /// Queue a seek at `target_ms`. Returns immediately; the media worker
    /// performs the WinRT call so the UI thread never waits on the player.
    pub fn seek(target_ms: u64) {
        if let Some(tx) = SEEK_TX.get() {
            let _ = tx.send(target_ms);
        }
    }

    pub fn spawn_poller() {
        ONCE.call_once(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = SEEK_TX.set(tx);
            std::thread::spawn(move || {
                unsafe {
                    let _ = windows::Win32::System::Com::CoInitializeEx(
                        None,
                        windows::Win32::System::Com::COINIT_MULTITHREADED,
                    );
                }
                // Publish a first snapshot right away; otherwise the Music
                // page stays empty for one whole poll interval.
                publish_poll();
                loop {
                    match rx.recv_timeout(POLL_EVERY) {
                        Ok(target) => {
                            // Several fast clicks: only honour the last target.
                            let mut target = target;
                            while let Ok(next) = rx.try_recv() {
                                target = next;
                            }
                            let changed = seek_once(target);
                            publish_poll();
                            if changed {
                                // Show the clicked spot until the player's
                                // timeline catches up with the seek.
                                nudge_published_position(target);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => publish_poll(),
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        });
    }

    fn publish_poll() {
        if let Some(s) = poll_once() {
            // Skip empty adverts with no title.
            if !s.title.trim().is_empty() {
                publish(s);
            }
        }
    }

    fn seek_once(target_ms: u64) -> bool {
        use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
        let changed = (|| {
            let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok()?.get().ok()?;
            let session = manager.GetCurrentSession().ok()?;
            let ticks = target_ms.saturating_mul(10_000).min(i64::MAX as u64) as i64;
            session.TryChangePlaybackPositionAsync(ticks).ok()?.get().ok()
        })();
        changed.unwrap_or(false)
    }

    /// Artwork for `t` — only when the stored cover was fetched for this
    /// exact title/artist/album key, never a previous song's.
    pub fn art_for(t: &Track) -> Option<Arc<Image>> {
        let key = super::art_key(&t.title, &t.artist, &t.album);
        let g = ART.lock().ok()?;
        let (k, img, _) = g.as_ref()?;
        if *k == key {
            img.clone()
        } else {
            None
        }
    }

    /// A fetch is due when the track changed or the cover is still missing
    /// and retries remain.
    fn art_needed(key: &str) -> bool {
        let Ok(g) = ART.lock() else { return false };
        match g.as_ref() {
            Some((k, img, attempts)) => k != key || (img.is_none() && *attempts < ART_ATTEMPTS),
            None => true,
        }
    }

    fn refresh_art(props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties, key: &str) {
        let img = fetch_art(props);
        if let Ok(mut g) = ART.lock() {
            let attempts = g.as_ref().map(|(k, _, a)| if k == key { *a } else { 0 }).unwrap_or(0);
            *g = match img {
                Some(i) => Some((key.to_string(), Some(Arc::new(i)), 0)),
                None => Some((key.to_string(), None, attempts + 1)),
            };
        }
    }

    /// SMTC thumbnail stream -> WIC decode -> square premultiplied cover.
    /// Any failure returns None; the text snapshot is never at risk.
    fn fetch_art(props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<Image> {
        use windows::core::Interface;
        use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, WICDecodeMetadataCacheOnDemand};
        use windows::Win32::System::Com::{CoCreateInstance, IStream, CLSCTX_INPROC_SERVER};
        use windows::Win32::System::WinRT::CreateStreamOverRandomAccessStream;
        unsafe {
            let ras = props.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
            let stream: IStream = CreateStreamOverRandomAccessStream(&ras).ok()?;
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            let frame = wic
                .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
                .ok()?
                .GetFrame(0)
                .ok()?;
            square_image(&wic, &frame.cast::<IWICBitmapSource>().ok()?, ART_PX)
        }
    }

    /// Center-crop `src` to a square (players publish 16:9 or letterboxed
    /// thumbnails), scale to `px` (never upscale), premultiplied BGRA out.
    pub(super) fn square_image(wic: &IWICImagingFactory, src: &IWICBitmapSource, px: u32) -> Option<Image> {
        use windows::Win32::Graphics::Imaging::{
            GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom, WICRect,
            IWICPalette,
        };
        unsafe {
            let (mut w, mut h) = (0, 0);
            src.GetSize(&mut w, &mut h).ok()?;
            if w == 0 || h == 0 {
                return None;
            }
            let side = w.min(h);
            let crop = WICRect { X: ((w - side) / 2) as i32, Y: ((h - side) / 2) as i32, Width: side as i32, Height: side as i32 };
            let clipper = wic.CreateBitmapClipper().ok()?;
            clipper.Initialize(src, &crop).ok()?;
            let out = px.min(side);
            let scaler = wic.CreateBitmapScaler().ok()?;
            scaler.Initialize(&clipper, out, out, WICBitmapInterpolationModeFant).ok()?;
            let conv = wic.CreateFormatConverter().ok()?;
            conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None::<&IWICPalette>, 0.0, WICBitmapPaletteTypeCustom)
                .ok()?;
            let stride = out * 4;
            let mut bgra = vec![0u8; (stride * out) as usize];
            conv.CopyPixels(&WICRect { X: 0, Y: 0, Width: out as i32, Height: out as i32 }, stride, &mut bgra).ok()?;
            Some(Image::new(out, out, bgra))
        }
    }

    fn nudge_published_position(target_ms: u64) {
        if let Ok(mut slot) = SLOT.lock() {
            if let Some((s, _)) = slot.as_mut() {
                s.position_ms = target_ms.min(s.duration_ms);
                *slot = Some((s.clone(), Instant::now()));
            }
        }
    }

    /// Milliseconds between a WinRT DateTime (100 ns ticks since 1601) and now,
    /// 0 when it is unset or in the future, capped at 12 h.
    fn ticks_since_ms(ticks: i64) -> u64 {
        const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
        if ticks <= 0 {
            return 0;
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| (d.as_nanos() / 100) as i64).unwrap_or(0) + UNIX_EPOCH_TICKS;
        ((now - ticks).max(0) / 10_000).min(12 * 3600 * 1000) as u64
    }

    fn hstr(h: &windows::core::HSTRING) -> String {
        h.to_string_lossy().trim().to_string()
    }

    fn poll_once() -> Option<SmtcSnapshot> {
        use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .ok()?
            .get()
            .ok()?;
        let session = manager.GetCurrentSession().ok()?;
        let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
        let title = props.Title().ok().map(|h| hstr(&h)).unwrap_or_default();
        let artist = props.Artist().ok().map(|h| hstr(&h)).unwrap_or_default();
        let album = props.AlbumTitle().ok().map(|h| hstr(&h)).unwrap_or_default();
        // Thumbnails can trail the text properties by a poll or two; retry
        // while the cover stays missing. Failure never breaks the snapshot.
        let key = super::art_key(&title, &artist, &album);
        if !title.is_empty() && art_needed(&key) {
            refresh_art(&props, &key);
        }
        let info = session.GetPlaybackInfo().ok();
        let playing = info.as_ref().and_then(|i| i.PlaybackStatus().ok()).map(|st| st.0 == 4).unwrap_or(false);
        let rate = info
            .as_ref()
            .and_then(|i| i.PlaybackRate().ok())
            .and_then(|r| r.Value().ok())
            .map(|r| r as f32)
            .filter(|r| *r > 0.0)
            .unwrap_or(1.0);
        let (position_ms, duration_ms) = session
            .GetTimelineProperties()
            .ok()
            .map(|t| {
                let pos = t.Position().ok().map(|s| (s.Duration / 10_000).max(0) as u64).unwrap_or(0);
                let end = t.EndTime().ok().map(|s| (s.Duration / 10_000).max(0) as u64).unwrap_or(0);
                // `Position` is the value at `LastUpdatedTime`, which for
                // browsers (YouTube) can be long ago.
                let since = t.LastUpdatedTime().ok().map(|d| ticks_since_ms(d.UniversalTime)).unwrap_or(0);
                (super::extrapolate_ms(pos, end, playing, rate, since), end)
            })
            .unwrap_or((0, 0));
        let app = session
            .SourceAppUserModelId()
            .ok()
            .map(|h| super::friendly_app(&hstr(&h)).to_string())
            .unwrap_or_else(|| "Music".to_string());
        Some(SmtcSnapshot { title, artist, album, app, position_ms, duration_ms, playing, rate })
    }
}

#[cfg(windows)]
pub use worker::{latest, seek, spawn_poller};

#[cfg(not(windows))]
pub fn latest(_max_age: std::time::Duration) -> Option<SmtcSnapshot> {
    None
}

#[cfg(not(windows))]
pub fn seek(_target_ms: u64) {}

#[cfg(not(windows))]
pub fn spawn_poller() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_status_codes() {
        assert_eq!(status_to_playback(4), Playback::Playing);
        assert_eq!(status_to_playback(5), Playback::Paused);
        for code in [0, 1, 2, 3, 6, 99] {
            assert_eq!(status_to_playback(code), Playback::Stopped, "code {code}");
        }
    }

    #[test]
    fn snapshot_converts_and_clamps() {
        let s = SmtcSnapshot {
            title: "T".into(),
            artist: "A".into(),
            album: "B".into(),
            app: "Spotify".into(),
            position_ms: 90_000,
            duration_ms: 60_000,
            playing: true,
            rate: 1.0,
        };
        let t = snapshot_to_track(&s);
        assert_eq!(t.position_ms, 60_000);
        assert_eq!(t.playback, Playback::Playing);
        assert!((t.progress() - 1.0).abs() < 0.001);
    }

    #[test]
    fn position_keeps_advancing_while_playing() {
        assert_eq!(extrapolate_ms(10_000, 200_000, true, 1.0, 3_000), 13_000);
        assert_eq!(extrapolate_ms(10_000, 200_000, true, 2.0, 3_000), 16_000);
        assert_eq!(extrapolate_ms(10_000, 200_000, false, 1.0, 3_000), 10_000);
        assert_eq!(extrapolate_ms(199_000, 200_000, true, 1.0, 9_000), 200_000, "clamps to the track end");
    }

    #[test]
    fn friendly_names() {
        assert_eq!(friendly_app("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), "Spotify");
        assert_eq!(friendly_app("Chrome"), "Chrome");
        assert_eq!(friendly_app("Microsoft.MicrosoftEdge_8wekyb3d8bbwe!MicrosoftEdge"), "Edge");
        assert_eq!(friendly_app(""), "Music");
    }

    #[test]
    fn art_key_distinguishes_tracks() {
        assert_eq!(art_key("T", "A", "B"), art_key("T", "A", "B"));
        assert_ne!(art_key("T", "A", "B"), art_key("T2", "A", "B"));
        assert_ne!(art_key("T", "A", "B"), art_key("T", "A2", "B"));
        assert_ne!(art_key("T", "A", "B"), art_key("T", "A", "B2"));
        // The separator keeps field boundaries: no accidental joins.
        assert_ne!(art_key("ab", "c", ""), art_key("a", "bc", ""));
    }

    /// WIC path the fetch uses: a 32x18 source must center-crop to a square
    /// and never upscale past the source side.
    #[cfg(windows)]
    #[test]
    fn square_image_crops_to_square() {
        use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory};
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).expect("wic");
            // Opaque colour, wide aspect like a YouTube thumbnail.
            let (w, h) = (32u32, 18u32);
            let mut px = vec![0u8; (w * h * 4) as usize];
            for p in px.chunks_exact_mut(4) {
                p.copy_from_slice(&[10, 20, 30, 255]);
            }
            let bmp = wic.CreateBitmapFromMemory(w, h, &GUID_WICPixelFormat32bppPBGRA, w * 4, &px).expect("bmp");
            let src: IWICBitmapSource = windows::core::Interface::cast(&bmp).expect("cast");
            let img = worker::square_image(&wic, &src, 256).expect("square");
            assert_eq!(img.w, img.h, "not square");
            assert_eq!(img.w, 18, "must not upscale past the cropped side");
            assert_eq!(img.bgra.len() as u32, img.w * img.h * 4);
        }
    }

    /// A rounded Image cmd must clip the corners: opaque centre, transparent
    /// just inside the top-left of the rect.
    #[cfg(windows)]
    #[test]
    fn rounded_image_clips_corners() {
        use crate::ui::scene::{Image, Rect, Scene};
        // WIC/D2D factories need COM on this thread (as in appicons tests).
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        }
        let img = std::sync::Arc::new(Image::new(32, 32, vec![255u8; 32 * 32 * 4]));
        let mut s = Scene::new();
        s.image_rounded(Rect::new(0.0, 0.0, 32.0, 32.0), 10.0, &img);
        let (pw, ph, px) = crate::render_d2d::d2d::render_offscreen(&s, (32.0, 32.0), 1.0).expect("offscreen render");
        let a = |x: usize, y: usize| px[(y * pw as usize + x) * 4 + 3];
        assert_eq!((pw, ph), (32, 32));
        assert!(a(16, 16) > 200, "centre should be opaque, got {}", a(16, 16));
        assert!(a(1, 1) <= 8, "corner should be clipped, got {}", a(1, 1));
    }

    #[test]
    fn seek_target_maps_permille_to_track_time() {
        assert_eq!(seek_target_ms(180_000, 0), 0);
        assert_eq!(seek_target_ms(180_000, 500), 90_000);
        assert_eq!(seek_target_ms(180_000, 1000), 180_000);
        assert_eq!(seek_target_ms(180_000, 2000), 180_000, "clamps past-the-end clicks");
        assert_eq!(seek_target_ms(0, 750), 0);
        assert_eq!(seek_target_ms(185_000, 333), 61_605);
    }
}
