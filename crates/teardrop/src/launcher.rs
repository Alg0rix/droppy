//! Installed-app launcher index for the Windows page search field.
//!
//! App hits come from the Start Menu scan (every .lnk under both roots).
//! On top of those, `winsearch` layers real Windows Search index hits —
//! files, folders, shell items — the same catalog Start Menu queries.

use std::sync::{Mutex, OnceLock};

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

/// One launcher hit: display name, a path to open (lnk, file, or shell
/// parsing path), and the right-side tag ("app" or the file's type text).
#[derive(Clone)]
pub struct Hit {
    pub name: String,
    pub path: String,
    pub tag: String,
}

/// (query, hits) — `hits()` runs every layout frame, so the result is
/// memoized per query and only recomputed when the text changes.
static LAST: OnceLock<Mutex<(String, Vec<Hit>)>> = OnceLock::new();

/// Hits for `query`: Start Menu apps whose name matches, then real
/// Windows Search rows (deduped). Index order is rank order.
pub fn hits(query: &str) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let last = LAST.get_or_init(|| Mutex::new((String::new(), Vec::new())));
    let mut g = last.lock().unwrap();
    if g.0 == q {
        return g.1.clone();
    }
    let out = compute(&q);
    *g = (q, out.clone());
    out
}

fn compute(q: &str) -> Vec<Hit> {
    let mut out: Vec<Hit> = Vec::new();
    for (n, p) in installed_apps().iter().filter(|(n, _)| n.to_lowercase().contains(q)) {
        out.push(Hit { name: n.clone(), path: p.clone(), tag: "app".to_string() });
        if out.len() == 6 {
            break;
        }
    }
    for r in crate::winsearch::search(q, 25) {
        if r.name.is_empty() || r.path.is_empty() {
            continue;
        }
        if out.iter().any(|h| h.path.eq_ignore_ascii_case(&r.path) || h.name.eq_ignore_ascii_case(&r.name)) {
            continue;
        }
        let tag = if r.path.to_lowercase().ends_with(".lnk") {
            "app".to_string()
        } else if r.kind.is_empty() {
            "file".to_string()
        } else {
            r.kind.to_lowercase()
        };
        out.push(Hit { name: r.name, path: r.path, tag });
        if out.len() == 12 {
            break;
        }
    }
    out
}

/// Open a .lnk shortcut via the shell (no window focus needed on us).
pub fn launch(path: &str) {
    unsafe {
        let _ = windows::Win32::UI::Shell::ShellExecuteW(windows::Win32::Foundation::HWND(std::ptr::null_mut()), windows::core::w!("open"), &windows::core::HSTRING::from(path), windows::core::PCWSTR::null(), windows::core::PCWSTR::null(), windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
    }
}
