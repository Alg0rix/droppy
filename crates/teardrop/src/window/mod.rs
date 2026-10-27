//! Win32 shell: a layered, per-pixel-alpha popup pinned to the top of the
//! primary monitor. All drawing goes through `ui::build` + the Direct2D
//! painter; this file only owns the window, timers, input and services.
//!
//! Timer model: one animation timer (16 ms while springs move, 40 ms for
//! ambient motion such as the equalizer, off when nothing changes) plus a
//! 1 s service tick. Collapsed + idle means zero wakeups besides the service tick.

#[cfg(windows)]
mod inner;

#[cfg(windows)]
pub use inner::run;

#[cfg(not(windows))]
pub fn run() -> i32 {
    eprintln!("droppy: the island window runs on Windows 10/11. Try `--selftest <dir>` here.");
    3
}
