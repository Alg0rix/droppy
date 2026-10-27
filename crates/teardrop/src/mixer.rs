//! Per-application audio sessions on the default render endpoint.
//!
//! IAudioSessionManager2 -> enumerator -> per-session control gives us each
//! app's volume + mute, keyed by process id so the switcher page can group
//! them under the same app headers as its windows.

use windows::core::Interface;
use windows::Win32::Media::Audio::{eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

/// One app's audio session on the default output device.
#[derive(Debug, Clone)]
pub struct AudioSession {
    pub pid: u32,
    /// App exe name, resolved through the window registry (`""` when the app
    /// has no visible window).
    pub exe: String,
    /// 0..1 master volume.
    pub volume: f32,
    pub muted: bool,
}

/// Every active audio session with a resolvable process.
pub fn sessions() -> Vec<AudioSession> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok().unwrap();
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok().unwrap();
        let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None).ok().unwrap();
        let enumr = mgr.GetSessionEnumerator().ok().unwrap();
        let n = enumr.GetCount().unwrap_or(0);
        let mut out = Vec::new();
        let regs = crate::winreg::enumerate(0);
        for i in 0..n {
            let Ok(ctl) = enumr.GetSession(i) else { continue };
            let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
            let pid = ctl2.GetProcessId().unwrap_or(0);
            if pid == 0 {
                continue;
            }
            let Ok(vol) = ctl.cast::<ISimpleAudioVolume>() else { continue };
            let exe = regs
                .iter()
                .find(|w| {
                    let mut p = 0u32;
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(windows::Win32::Foundation::HWND(w.hwnd as *mut _), Some(&mut p));
                    p == pid
                })
                .map(|w| w.exe.clone())
                .unwrap_or_default();
            out.push(AudioSession { pid, exe, volume: vol.GetMasterVolume().unwrap_or(0.0), muted: vol.GetMute().unwrap_or(false.into()).as_bool() });
        }
        out
    }
}

/// Set one session's volume (0..1) by process id.
pub fn set_volume(pid: u32, v: f32) {
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return };
        let Ok(dev) = en.GetDefaultAudioEndpoint(eRender, eConsole) else { return };
        let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { return };
        let Ok(enumr) = mgr.GetSessionEnumerator() else { return };
        let n = enumr.GetCount().unwrap_or(0);
        for i in 0..n {
            let Ok(ctl) = enumr.GetSession(i) else { continue };
            let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
            if ctl2.GetProcessId().unwrap_or(0) != pid {
                continue;
            }
            if let Ok(vol) = ctl.cast::<ISimpleAudioVolume>() {
                let _ = vol.SetMasterVolume(v.clamp(0.0, 1.0), std::ptr::null());
            }
            return;
        }
    }
}
