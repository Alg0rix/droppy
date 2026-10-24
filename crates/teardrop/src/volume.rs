//! Real output-volume readout via Core Audio (no dependencies).
//!
//! [`try_read`] talks to the default render endpoint (MMDeviceEnumerator →
//! IAudioEndpointVolume) and returns the master scalar + mute state. Every
//! failure yields None — callers fall back to static HUD copy. Mapping
//! helpers are pure and unit-tested on any OS.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeState {
    pub pct: u8,
    pub muted: bool,
}

/// Master scalar 0.0–1.0 → 0–100, clamped.
pub fn scalar_to_pct(scalar: f32) -> u8 {
    (scalar.clamp(0.0, 1.0) * 100.0).round() as u8
}

/// HUD copy for a volume state: ("Volume", "42%") or ("Muted", "output muted").
pub fn label(v: VolumeState) -> (&'static str, String) {
    if v.muted {
        ("Muted", "output muted".to_string())
    } else {
        ("Volume", format!("{}%", v.pct))
    }
}

#[cfg(windows)]
pub fn try_read() -> Option<VolumeState> {
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
        let scalar = volume.GetMasterVolumeLevelScalar().ok()?;
        let muted = volume.GetMute().ok()?.as_bool();
        Some(VolumeState { pct: scalar_to_pct(scalar), muted })
    }
}

#[cfg(not(windows))]
pub fn try_read() -> Option<VolumeState> {
    None
}

/// Output peak level 0..1 of the default render endpoint (what is playing
/// right now, after mixing). The meter is cached per thread and re-acquired
/// every few seconds so a device switch is picked up.
#[cfg(windows)]
pub fn peak() -> Option<f32> {
    use std::cell::RefCell;
    use std::time::{Duration, Instant};
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

    thread_local! {
        static METER: RefCell<Option<(IAudioMeterInformation, Instant)>> = const { RefCell::new(None) };
    }
    METER.with(|m| {
        let mut m = m.borrow_mut();
        if m.as_ref().map(|(_, at)| at.elapsed() > Duration::from_secs(8)).unwrap_or(true) {
            *m = unsafe {
                (|| {
                    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
                    let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
                    let meter: IAudioMeterInformation = dev.Activate(CLSCTX_ALL, None).ok()?;
                    Some((meter, Instant::now()))
                })()
            };
        }
        let v = m.as_ref().and_then(|(meter, _)| unsafe { meter.GetPeakValue().ok() });
        if v.is_none() {
            *m = None;
        }
        v.map(|v| v.clamp(0.0, 1.0))
    })
}

#[cfg(not(windows))]
pub fn peak() -> Option<f32> {
    None
}

/// Post `msg` to `hwnd` (wParam = percent, lParam = muted) the moment the
/// default output's volume or mute changes, so the HUD and the eyes follow
/// the keys live instead of on the 1 s poll. The registration lives as long
/// as the calling thread; a device switch is still caught by the poll.
#[cfg(windows)]
pub fn watch(hwnd: isize, msg: u32) -> bool {
    use std::cell::RefCell;
    use windows::core::implement;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl};
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA};
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

    #[implement(IAudioEndpointVolumeCallback)]
    struct Notify {
        hwnd: isize,
        msg: u32,
    }

    impl IAudioEndpointVolumeCallback_Impl for Notify_Impl {
        fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
            // Called on a Core Audio worker thread: only hand the values over.
            if let Some(d) = unsafe { data.as_ref() } {
                let pct = scalar_to_pct(d.fMasterVolume) as usize;
                let muted = d.bMuted.as_bool() as isize;
                unsafe {
                    let _ = PostMessageW(HWND(self.hwnd as *mut _), self.msg, WPARAM(pct), LPARAM(muted));
                }
            }
            Ok(())
        }
    }

    thread_local! {
        static HELD: RefCell<Option<(IAudioEndpointVolume, IAudioEndpointVolumeCallback)>> = const { RefCell::new(None) };
    }
    let held = unsafe {
        (|| {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).ok()?;
            let cb: IAudioEndpointVolumeCallback = Notify { hwnd, msg }.into();
            vol.RegisterControlChangeNotify(&cb).ok()?;
            Some((vol, cb))
        })()
    };
    let ok = held.is_some();
    HELD.with(|h| *h.borrow_mut() = held);
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_maps() {
        assert_eq!(scalar_to_pct(0.0), 0);
        assert_eq!(scalar_to_pct(0.424), 42);
        assert_eq!(scalar_to_pct(1.0), 100);
        assert_eq!(scalar_to_pct(1.7), 100);
        assert_eq!(scalar_to_pct(-0.2), 0);
    }

    #[test]
    fn labels() {
        let (t, d) = label(VolumeState { pct: 42, muted: false });
        assert_eq!((t, d.as_str()), ("Volume", "42%"));
        let (t, d) = label(VolumeState { pct: 0, muted: true });
        assert_eq!((t, d.as_str()), ("Muted", "output muted"));
    }
}
