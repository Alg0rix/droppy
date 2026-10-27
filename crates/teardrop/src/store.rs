//! JSON persistence in %APPDATA%\Teardrop (kept on uninstall).
//! Cache + log live in %LOCALAPPDATA%\Teardrop and are safe to delete.
use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

fn file(data_dir: &std::path::Path, name: &str) -> PathBuf {
    data_dir.join(format!("{name}.json"))
}

pub fn load<T: DeserializeOwned + Default>(data_dir: &std::path::Path, name: &str) -> T {
    std::fs::read(file(data_dir, name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save<T: Serialize>(data_dir: &std::path::Path, name: &str, value: &T) {
    if let Ok(b) = serde_json::to_string_pretty(value) {
        let _ = std::fs::write(file(data_dir, name), b);
    }
}

/// Append one line to the log file in the cache dir.
pub fn log_line(cache_dir: &std::path::Path, msg: &str) {
    use std::io::Write;
    let p = cache_dir.join("droppy.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{}", msg);
    }
}
