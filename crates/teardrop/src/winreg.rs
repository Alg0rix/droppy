//! Desktop window registry for the switcher page.
//!
//! A snapshot of every visible top-level window, identified by HWND + owning
//! executable so rows can group windows per app. Refreshed on the slow tick;
//! cheap enough (~1ms for ~100 windows) to keep live.

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, MAX_PATH};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible};

/// One visible top-level window.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub hwnd: isize,
    /// Executable file name, lower-cased (e.g. `code.exe`).
    pub exe: String,
    /// Full image path, for extracting the app's icon.
    pub path: String,
    /// Display name of the owning app (exe minus extension, capitalized).
    pub app: String,
    pub title: String,
}

/// All visible windows except Teardrop's own, newest first per the Z order.
pub fn enumerate(own_hwnd: isize) -> Vec<WindowInfo> {
    let mut out: Vec<WindowInfo> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut out as *mut _ as isize));
    }
    out.retain(|w| w.hwnd != own_hwnd);
    out
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lp: LPARAM) -> BOOL {
    unsafe {
        let list = &mut *(lp.0 as *mut Vec<WindowInfo>);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        // Cloaked = invisible UWP/ghost windows DWM keeps off-screen.
        let mut cloaked: u32 = 0;
        if DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, std::mem::size_of::<u32>() as u32).is_ok() && cloaked != 0 {
            return BOOL(1);
        }
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return BOOL(1);
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..n as usize]).trim().to_string();
        if title.is_empty() {
            return BOOL(1);
        }
        let mut pid = 0u32;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let (exe, path) = exe_of(pid).unwrap_or_default();
        if exe.is_empty() {
            return BOOL(1);
        }
        let app = app_name(&exe);
        list.push(WindowInfo { hwnd: hwnd.0 as isize, exe, path, app, title });
        BOOL(1)
    }
}

/// (lower-cased file name, full path) of the process image.
fn exe_of(pid: u32) -> Option<(String, String)> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize];
        let mut n = buf.len() as u32;
        let name = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n)
            .ok()
            .map(|_| String::from_utf16_lossy(&buf[..n as usize]));
        let _ = windows::Win32::Foundation::CloseHandle(h);
        name.and_then(|p| p.rsplit(['\\', '/']).next().map(|s| (s.to_lowercase(), p.clone())))
    }
}

fn app_name(exe: &str) -> String {
    let stem = exe.rsplit_once('.').map(|(s, _)| s).unwrap_or(exe);
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => stem.to_string(),
    }
}
