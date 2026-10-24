//! Data + cache directory helpers.
//!
//! Data (persisted, kept on uninstall):  %APPDATA%\Teardrop\
//! Cache + log (safe to delete):         %LOCALAPPDATA%\Teardrop\
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        PathBuf::from(appdata).join("Teardrop")
    } else {
        PathBuf::from(".").join("teardrop-data")
    }
}

pub fn cache_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local).join("Teardrop")
    } else {
        PathBuf::from(".").join("teardrop-cache")
    }
}

pub fn ensure_dirs() -> (PathBuf, PathBuf) {
    let d = data_dir();
    let c = cache_dir();
    let _ = std::fs::create_dir_all(&d);
    let _ = std::fs::create_dir_all(&c);
    (d, c)
}
