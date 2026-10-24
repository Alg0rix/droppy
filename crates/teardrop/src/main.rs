//! Teardrop — a living notch for Windows 10/11.
//!
//! Native Rust, no web view. One black droplet hangs from the top of the
//! screen: collapsed it shows live activities (music, timer, HUDs); click
//! or hover it and it stretches open like a teardrop into quick pages
//! (music + synced lyrics, focus, water, shelf, tasks, clipboard, today).

#[cfg(windows)]
mod demo;
mod lyrics_service;
mod media_smtc;
mod paths;
mod render_d2d;
mod selftest;
mod services_system;
mod startup;
mod state;
mod ui;
mod store;
mod viewmodel;
mod volume;
mod window;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--selftest" {
        std::process::exit(selftest::run(&args[2]));
    }
    #[cfg(windows)]
    if args.len() >= 3 && args[1] == "--demo" {
        std::process::exit(demo::run(&args[2]));
    }
    if args.len() >= 2 && (args[1] == "--help" || args[1] == "-h") {
        println!("teardrop [--selftest <dir>] [--demo <dir>]");
        println!("  no args      run the island (Windows 10/11)");
        println!("  --selftest   screenshots of every state + report.json into <dir>");
        println!("  --demo       render demo.gif / demo.mp4 into <dir> (needs ffmpeg)");
        return;
    }
    let (_data, _cache) = paths::ensure_dirs();
    std::process::exit(window::run());
}
