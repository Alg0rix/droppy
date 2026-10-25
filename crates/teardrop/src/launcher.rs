//! Installed-app launcher index for the Windows page search field.
//!
//! Windows Search via ISearchQueryHelper only emits SQL for OLE DB, so the
//! v1 index is the Start Menu itself: every .lnk under both Start Menu
//! roots, scanned once lazily. Searching filters by substring.

use std::sync::OnceLock;

/// (display name, .lnk path) — Start Menu order, stable across searches.
static APPS: OnceLock<Vec<(String, String)>> = OnceLock::new();

fn scan_dir(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan_dir(&p, out);
        } else if p.extension().map(|x| x == "lnk").unwrap_or(false) {
            let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            if !name.is_empty() {
                out.push((name, p.to_string_lossy().into_owned()));
            }
        }
    }
}

pub fn installed_apps() -> &'static [(String, String)] {
    APPS.get_or_init(|| {
        let mut out = Vec::new();
        for dir in [
            std::env::var("APPDATA").map(|a| format!("{a}\\Microsoft\\Windows\\Start Menu\\Programs")),
            std::env::var("PROGRAMDATA").map(|a| format!("{a}\\Microsoft\\Windows\\Start Menu\\Programs")),
        ]
        .into_iter()
        .flatten()
        {
            scan_dir(std::path::Path::new(&dir), &mut out);
        }
        out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        out
    })
}

/// App hits for `query` — name contains it, case-insensitive.
pub fn hits(query: &str) -> Vec<(String, String)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    installed_apps().iter().filter(|(n, _)| n.to_lowercase().contains(&q)).take(6).cloned().collect()
}

/// Open a .lnk shortcut via the shell (no window focus needed on us).
pub fn launch(path: &str) {
    unsafe {
        let _ = windows::Win32::UI::Shell::ShellExecuteW(windows::Win32::Foundation::HWND(std::ptr::null_mut()), windows::core::w!("open"), &windows::core::HSTRING::from(path), windows::core::PCWSTR::null(), windows::core::PCWSTR::null(), windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
    }
}
