//! Display brightness get/set for the Quick page (Windows).
//!
//! Two backends, tried in order:
//! - Internal panels (laptops): `DeviceIoControl` on the `\\.\LCD` handle with
//!   `IOCTL_VIDEO_QUERY/SET_DISPLAY_BRIGHTNESS` — the same path the Settings
//!   slider and `GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS` broadcast use.
//! - External monitors: `Get/SetMonitorBrightness` on the first physical
//!   monitor of the primary display (DDC/CI).
//!
//! Every failure yields `None`/`false` — callers hide or grey out the slider.

/// Current brightness 0..=100, or None when nothing controllable is present.
pub fn get() -> Option<u8> {
    #[cfg(windows)]
    return imp::get();
    #[cfg(not(windows))]
    None
}

/// Set brightness 0..=100. Returns false when no backend accepted it.
pub fn set(pct: u8) -> bool {
    #[cfg(windows)]
    return imp::set(pct.min(100));
    #[cfg(not(windows))]
    {
        let _ = pct;
        false
    }
}

#[cfg(windows)]
mod imp {
    use windows::core::w;
    use windows::Win32::Devices::Display::{
        DestroyPhysicalMonitor, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, DISPLAY_BRIGHTNESS,
        IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS, IOCTL_VIDEO_SET_DISPLAY_BRIGHTNESS, PHYSICAL_MONITOR,
    };
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Graphics::Gdi::{MonitorFromWindow, HMONITOR, MONITOR_DEFAULTTOPRIMARY};
    use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

    /// `\\.\LCD` is the display driver's brightness alias on internal panels.
    fn lcd() -> Option<HANDLE> {
        unsafe {
            CreateFileW(
                w!("\\\\.\\LCD"),
                0xC0000000, // GENERIC_READ | GENERIC_WRITE
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
            .ok()
        }
    }

    fn lcd_get() -> Option<u8> {
        let h = lcd()?;
        unsafe {
            let mut out = DISPLAY_BRIGHTNESS::default();
            let mut n = 0u32;
            let ok = DeviceIoControl(
                h,
                IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS,
                None,
                0,
                Some(&mut out as *mut _ as *mut _),
                std::mem::size_of::<DISPLAY_BRIGHTNESS>() as u32,
                Some(&mut n),
                None,
            )
            .is_ok();
            let _ = CloseHandle(h);
            if !ok {
                return None;
            }
            // The panel keeps separate AC/DC levels; report the live one.
            let mut ps = SYSTEM_POWER_STATUS::default();
            let on_ac = GetSystemPowerStatus(&mut ps).is_ok() && ps.ACLineStatus != 0;
            Some((if on_ac { out.ucACBrightness } else { out.ucDCBrightness }).min(100))
        }
    }

    fn lcd_set(pct: u8) -> bool {
        let Some(h) = lcd() else { return false };
        unsafe {
            // Policy 0 = explicit level; set both AC and DC so it applies on
            // whichever power source is live right now.
            let mut io = DISPLAY_BRIGHTNESS { ucDisplayPolicy: 0, ucACBrightness: pct, ucDCBrightness: pct };
            let ok =
                DeviceIoControl(h, IOCTL_VIDEO_SET_DISPLAY_BRIGHTNESS, Some(&mut io as *mut _ as *const _), std::mem::size_of::<DISPLAY_BRIGHTNESS>() as u32, None, 0, None, None)
                    .is_ok();
            let _ = CloseHandle(h);
            ok
        }
    }

    /// First physical monitor on the primary display, if any.
    fn physical_monitor() -> Option<PHYSICAL_MONITOR> {
        unsafe {
            let mon: HMONITOR = MonitorFromWindow(GetDesktopWindow(), MONITOR_DEFAULTTOPRIMARY);
            let mut n = 0u32;
            if GetNumberOfPhysicalMonitorsFromHMONITOR(mon, &mut n).is_err() || n == 0 {
                return None;
            }
            let mut arr: [PHYSICAL_MONITOR; 1] = std::mem::zeroed();
            if GetPhysicalMonitorsFromHMONITOR(mon, &mut arr).is_err() {
                return None;
            }
            Some(arr[0])
        }
    }

    fn monitor_get() -> Option<u8> {
        unsafe {
            let pm = physical_monitor()?;
            let (mut lo, mut cur, mut hi) = (0u32, 0u32, 0u32);
            let ok = GetMonitorBrightness(pm.hPhysicalMonitor, &mut lo, &mut cur, &mut hi) != 0;
            let _ = DestroyPhysicalMonitor(pm.hPhysicalMonitor);
            if !ok || hi <= lo {
                return None;
            }
            Some(((cur - lo) * 100 / (hi - lo)).min(100) as u8)
        }
    }

    fn monitor_set(pct: u8) -> bool {
        unsafe {
            let Some(pm) = physical_monitor() else { return false };
            let (mut lo, mut cur, mut hi) = (0u32, 0u32, 0u32);
            let ok = GetMonitorBrightness(pm.hPhysicalMonitor, &mut lo, &mut cur, &mut hi) != 0
                && hi > lo
                && SetMonitorBrightness(pm.hPhysicalMonitor, lo + (hi - lo) * pct as u32 / 100) != 0;
            let _ = DestroyPhysicalMonitor(pm.hPhysicalMonitor);
            ok
        }
    }

    pub fn get() -> Option<u8> {
        lcd_get().or_else(monitor_get)
    }

    pub fn set(pct: u8) -> bool {
        lcd_set(pct) || monitor_set(pct)
    }
}
