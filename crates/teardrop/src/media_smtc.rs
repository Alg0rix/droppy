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

#[cfg(windows)]
mod worker {
    use super::SmtcSnapshot;
    use std::sync::mpsc::{RecvTimeoutError, Sender};
    use std::sync::{Mutex, Once, OnceLock};
    use std::time::{Duration, Instant};

    const POLL_EVERY: Duration = Duration::from_secs(5);

    static SLOT: Mutex<Option<(SmtcSnapshot, Instant)>> = Mutex::new(None);
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
            .and_then(|(s, at)| {
                if at.elapsed() <= max_age {
                    Some(s)
                } else {
                    None
                }
            })
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

    fn nudge_published_position(target_ms: u64) {
        if let Ok(mut slot) = SLOT.lock() {
            if let Some((s, _)) = slot.as_mut() {
                s.position_ms = target_ms.min(s.duration_ms);
                *slot = Some((s.clone(), Instant::now()));
            }
        }
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
        let (position_ms, duration_ms) = session
            .GetTimelineProperties()
            .ok()
            .map(|t| {
                let pos = t.Position().ok().map(|s| (s.Duration / 10_000).max(0) as u64).unwrap_or(0);
                let end = t.EndTime().ok().map(|s| (s.Duration / 10_000).max(0) as u64).unwrap_or(0);
                (pos, end)
            })
            .unwrap_or((0, 0));
        let playing = session
            .GetPlaybackInfo()
            .ok()
            .and_then(|i| i.PlaybackStatus().ok())
            .map(|st| st.0 == 4)
            .unwrap_or(false);
        let app = session
            .SourceAppUserModelId()
            .ok()
            .map(|h| super::friendly_app(&hstr(&h)).to_string())
            .unwrap_or_else(|| "Music".to_string());
        Some(SmtcSnapshot { title, artist, album, app, position_ms, duration_ms, playing })
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
        };
        let t = snapshot_to_track(&s);
        assert_eq!(t.position_ms, 60_000);
        assert_eq!(t.playback, Playback::Playing);
        assert!((t.progress() - 1.0).abs() < 0.001);
    }

    #[test]
    fn friendly_names() {
        assert_eq!(friendly_app("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), "Spotify");
        assert_eq!(friendly_app("Chrome"), "Chrome");
        assert_eq!(friendly_app("Microsoft.MicrosoftEdge_8wekyb3d8bbwe!MicrosoftEdge"), "Edge");
        assert_eq!(friendly_app(""), "Music");
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
