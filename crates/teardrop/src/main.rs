//! Droppy — a living notch for Windows 10/11.
//!
//! Native Rust, no web view. One black droplet hangs from the top of the
//! screen: collapsed it shows live activities (music, timer, HUDs); click
//! or hover it and it stretches open like a teardrop into quick pages
//! (music + synced lyrics, focus, water, shelf, tasks, clipboard, today, AI).
#![windows_subsystem = "windows"]

mod ai;
#[cfg(windows)]
mod appicons;
mod brightness;
#[cfg(windows)]
mod btdev;
mod calendar;
mod chat;
#[cfg(windows)]
mod demo;
#[cfg(windows)]
mod drop_target;
mod http;
#[cfg(windows)]
mod launcher;
mod lyrics_service;
mod media_smtc;
mod mesh_ble;
#[cfg(windows)]
mod mixer;
mod paths;
mod radios;
mod recur;
mod render_d2d;
mod selftest;
mod services_env;
mod services_system;
#[cfg(windows)]
mod share;
#[cfg(windows)]
mod shelf_drag;
#[cfg(windows)]
mod shellcatalog;
mod speedtest;
mod sqlite;
mod startup;
mod state;
mod store;
mod sysmon;
mod tz;
mod ui;
mod update;
mod usage;
mod viewmodel;
mod voice;
mod volume;
mod window;
#[cfg(windows)]
mod winreg;
#[cfg(windows)]
mod winsearch;

fn main() {
    // Release builds abort on panic and the app has no console: keep a copy of
    // every panic in droppy.log, then defer to the default handler.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dir = paths::cache_dir();
        let _ = std::fs::create_dir_all(&dir);
        let name = std::thread::current().name().unwrap_or("<unnamed>").to_string();
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        store::log_line(&dir, &format!("{ts} panic in thread '{name}': {info}"));
        default_hook(info);
    }));
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 {
        #[cfg(windows)]
        unsafe {
            use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
    if args.len() >= 3 && args[1] == "--selftest" {
        std::process::exit(selftest::run(&args[2]));
    }
    #[cfg(windows)]
    if args.len() >= 3 && args[1] == "--demo" {
        std::process::exit(demo::run(&args[2]));
    }
    if args.len() >= 2 && (args[1] == "--help" || args[1] == "-h") {
        println!("droppy [--selftest <dir>] [--demo <dir>]");
        println!("  no args      run the island (Windows 10/11)");
        println!("  --selftest   screenshots of every state + report.json into <dir>");
        println!("  --demo       render demo.gif / demo.mp4 into <dir> (needs ffmpeg)");
        return;
    }
    #[cfg(windows)]
    if !single_instance_guard() {
        return;
    }
    let (_data, _cache) = paths::ensure_dirs();
    // Pre-rename installs started with Windows under the name "Teardrop".
    startup::migrate_legacy();
    std::process::exit(window::run());
}

/// One island per session: a named mutex tells a second launch that
/// Droppy is already running. The handle is leaked on purpose — the
/// mutex must live as long as the process does.
#[cfg(windows)]
fn single_instance_guard() -> bool {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow};
    unsafe {
        let _keep = CreateMutexW(None, true, w!("Local\\TeardropIslandMutex"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(hwnd) = FindWindowW(w!("TeardropIsland"), PCWSTR::null()) {
                let _ = SetForegroundWindow(hwnd);
            }
            return false;
        }
        let _ = _keep;
        true
    }
}
