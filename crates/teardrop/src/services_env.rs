//! Ambient samplers the idle eyes react to: input idle time, mic/camera in
//! use, connectivity, do-not-disturb, crash dialogs, finished downloads and
//! (opt-in) the local weather.
//!
//! Everything is best-effort: a failed probe reads as "nothing going on".
//! The parsing/classifying helpers are pure and unit-tested on any OS.

use crate::viewmodel::Weather;

/// wttr.in / WWO condition code -> coarse weather.
pub fn weather_from_code(code: u32) -> Weather {
    match code {
        113 => Weather::Clear,
        116 | 119 | 122 => Weather::Cloudy,
        143 | 248 | 260 => Weather::Fog,
        200 | 386 | 389 | 392 | 395 => Weather::Storm,
        179 | 182 | 185 | 227 | 230 | 317 | 320 | 323 | 326 | 329 | 332 | 335 | 338 | 350 | 362 | 365 | 368 | 371 | 374 | 377 => Weather::Snow,
        176 | 263 | 266 | 281 | 284 | 293 | 296 | 299 | 302 | 305 | 308 | 311 | 314 | 353 | 356 | 359 => Weather::Rain,
        _ => Weather::Unknown,
    }
}

/// `current_condition[0].weatherCode` from a wttr.in `?format=j1` body.
pub fn parse_weather(body: &str) -> Option<Weather> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let code = &v["current_condition"][0]["weatherCode"];
    let code = code.as_str().and_then(|s| s.parse().ok()).or_else(|| code.as_u64().map(|c| c as u32))?;
    Some(weather_from_code(code))
}

/// A file name that is a finished download (not a browser's partial file).
pub fn is_finished_download(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let partial = [".crdownload", ".part", ".partial", ".tmp", ".download", ".opdownload"];
    !n.is_empty() && !n.starts_with('~') && !n.starts_with('.') && n != "desktop.ini" && !partial.iter().any(|p| n.ends_with(p))
}

/// Windows Error Reporting's crash dialog.
pub fn is_crash_reporter(exe_path: &str) -> bool {
    let name = exe_path.rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase();
    name == "werfault.exe"
}

#[cfg(windows)]
pub mod sys {
    use super::{is_crash_reporter, is_finished_download, parse_weather};
    use crate::viewmodel::Weather;
    use std::collections::HashSet;
    use std::sync::Mutex;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Registry::*;

    static WEATHER: Mutex<Option<Weather>> = Mutex::new(None);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Milliseconds since the last keyboard/mouse input anywhere.
    pub fn idle_ms() -> u64 {
        use windows::Win32::System::SystemInformation::GetTickCount;
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
        unsafe {
            let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
            if !GetLastInputInfo(&mut li).as_bool() {
                return 0;
            }
            GetTickCount().wrapping_sub(li.dwTime) as u64
        }
    }

    /// Left mouse button held right now (anywhere on screen).
    pub fn caps() -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CAPITAL};
        unsafe { GetKeyState(VK_CAPITAL.0 as i32) & 1 != 0 }
    }

    pub fn lbutton() -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
        unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
    }

    /// (microphone, camera) in use, from the privacy consent store: an app
    /// with a start time but no stop time is using the device right now.
    pub fn mic_cam() -> (bool, bool) {
        (device_in_use("microphone"), device_in_use("webcam"))
    }

    fn device_in_use(cap: &str) -> bool {
        let path = format!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\{cap}");
        unsafe {
            let Some(root) = open(HKEY_CURRENT_USER, &path) else { return false };
            let busy = subkeys(root).iter().any(|name| {
                let Some(k) = open(root, name) else { return false };
                let hit = if name == "NonPackaged" {
                    subkeys(k).iter().any(|n| open(k, n).map(|kk| in_use(kk, true)).unwrap_or(false))
                } else {
                    in_use(k, false)
                };
                let _ = RegCloseKey(k);
                hit
            });
            let _ = RegCloseKey(root);
            busy
        }
    }

    unsafe fn open(parent: HKEY, sub: &str) -> Option<HKEY> {
        let w = wide(sub);
        let mut k = HKEY::default();
        (RegOpenKeyExW(parent, PCWSTR::from_raw(w.as_ptr()), 0, KEY_READ, &mut k).is_ok()).then_some(k)
    }

    unsafe fn subkeys(k: HKEY) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = [0u16; 512];
        for i in 0..512u32 {
            let mut len = buf.len() as u32;
            if RegEnumKeyExW(k, i, PWSTR::from_raw(buf.as_mut_ptr()), &mut len, None, PWSTR::null(), None, None).is_err() {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        out
    }

    /// Reads LastUsedTimeStart/Stop and closes `k` when `close`.
    unsafe fn in_use(k: HKEY, close: bool) -> bool {
        let q = |name: &str| -> u64 {
            let w = wide(name);
            let mut v: u64 = 0;
            let mut len = 8u32;
            let ok = RegQueryValueExW(k, PCWSTR::from_raw(w.as_ptr()), None, None, Some(&mut v as *mut u64 as *mut u8), Some(&mut len)).is_ok();
            if ok {
                v
            } else {
                0
            }
        };
        let busy = q("LastUsedTimeStart") != 0 && q("LastUsedTimeStop") == 0;
        if close {
            let _ = RegCloseKey(k);
        }
        busy
    }

    pub fn online() -> bool {
        use windows::Win32::Networking::WinInet::{InternetGetConnectedState, INTERNET_CONNECTION};
        unsafe {
            let mut flags = INTERNET_CONNECTION(0);
            InternetGetConnectedState(&mut flags, 0).is_ok()
        }
    }

    /// Do-not-disturb / quiet hours / presentation mode.
    pub fn quiet() -> bool {
        use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME};
        unsafe { matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_QUIET_TIME || s == QUNS_PRESENTATION_MODE) }
    }

    /// The window belongs to the Windows crash reporter.
    pub fn is_crash_window(hwnd: HWND) -> bool {
        use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return false;
            }
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR::from_raw(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(h);
            ok && is_crash_reporter(&String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    fn downloads_dir() -> Option<std::path::PathBuf> {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
        unsafe {
            if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None) {
                let s = p.to_string().ok();
                CoTaskMemFree(Some(p.0 as *const _));
                if let Some(s) = s {
                    return Some(s.into());
                }
            }
        }
        std::env::var_os("USERPROFILE").map(|u| std::path::PathBuf::from(u).join("Downloads"))
    }

    fn finished_names(dir: &std::path::Path) -> HashSet<String> {
        std::fs::read_dir(dir)
            .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).filter(|n| is_finished_download(n)).collect())
            .unwrap_or_default()
    }

    /// Watches the Downloads folder; posts `msg` to the island when a new
    /// finished file appears (browsers rename `.crdownload` -> final name).
    pub fn spawn_downloads_watcher(hwnd: HWND, msg: u32) {
        let raw = hwnd.0 as isize;
        std::thread::spawn(move || {
            use windows::Win32::Storage::FileSystem::{FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification, FILE_NOTIFY_CHANGE_FILE_NAME};
            use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
            use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
            let Some(dir) = downloads_dir() else { return };
            let w = wide(&dir.to_string_lossy());
            unsafe {
                let Ok(h) = FindFirstChangeNotificationW(PCWSTR::from_raw(w.as_ptr()), false, FILE_NOTIFY_CHANGE_FILE_NAME) else { return };
                let mut seen = finished_names(&dir);
                loop {
                    if WaitForSingleObject(h, INFINITE).0 != 0 {
                        break;
                    }
                    let now = finished_names(&dir);
                    if now.iter().any(|n| !seen.contains(n)) {
                        let _ = PostMessageW(HWND(raw as *mut _), msg, WPARAM(0), LPARAM(0));
                    }
                    seen = now;
                    if FindNextChangeNotification(h).is_err() {
                        break;
                    }
                }
                let _ = FindCloseChangeNotification(h);
            }
        });
    }

    /// Fetch the weather once in the background (wttr.in geolocates by IP).
    pub fn refresh_weather() {
        std::thread::spawn(|| {
            let w = crate::lyrics_service::net::https_get("https://wttr.in/?format=j1").and_then(|b| parse_weather(&b));
            if let (Some(w), Ok(mut slot)) = (w, WEATHER.lock()) {
                *slot = Some(w);
            }
        });
    }

    pub fn weather() -> Weather {
        WEATHER.lock().ok().and_then(|w| *w).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_codes_and_body() {
        assert_eq!(weather_from_code(113), Weather::Clear);
        assert_eq!(weather_from_code(302), Weather::Rain);
        assert_eq!(weather_from_code(338), Weather::Snow);
        assert_eq!(weather_from_code(389), Weather::Storm);
        assert_eq!(weather_from_code(248), Weather::Fog);
        assert_eq!(parse_weather(r#"{"current_condition":[{"weatherCode":"296"}]}"#), Some(Weather::Rain));
        assert_eq!(parse_weather("not json"), None);
    }

    #[test]
    fn partial_downloads_do_not_count() {
        assert!(is_finished_download("report.pdf"));
        assert!(!is_finished_download("report.pdf.crdownload"));
        assert!(!is_finished_download("movie.mkv.part"));
        assert!(!is_finished_download("desktop.ini"));
        assert!(!is_finished_download("~$draft.docx"));
    }

    #[test]
    fn crash_reporter_by_exe_name() {
        assert!(is_crash_reporter("C:\\Windows\\System32\\WerFault.exe"));
        assert!(!is_crash_reporter("C:\\Program Files\\App\\app.exe"));
    }
}
