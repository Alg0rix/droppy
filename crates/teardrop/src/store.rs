//! JSON persistence in %APPDATA%\Droppy (kept on uninstall).
//! Cache + log live in %LOCALAPPDATA%\Droppy and are safe to delete.
use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

fn file(data_dir: &std::path::Path, name: &str) -> PathBuf {
    data_dir.join(format!("{name}.json"))
}

/// Write `bytes` to `path` atomically: a sibling temp file is flushed to disk
/// first, then renamed over the target so a crash mid-save can't truncate it.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.tmp"));
    let r = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

pub fn load<T: DeserializeOwned + Default>(data_dir: &std::path::Path, name: &str) -> T {
    let p = file(data_dir, name);
    match std::fs::read(&p) {
        Ok(b) => match serde_json::from_slice(&b) {
            Ok(v) => v,
            // Keep the unreadable file: rename it aside rather than letting the
            // next save destroy the user's only copy.
            Err(_) => {
                let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let corrupt = p.with_file_name(format!("{}.corrupt-{secs}", p.file_name().and_then(|n| n.to_str()).unwrap_or_default()));
                let _ = std::fs::rename(&p, &corrupt);
                log_line(&crate::paths::cache_dir(), &format!("{} failed to parse; quarantined as {}", p.display(), corrupt.display()));
                T::default()
            }
        },
        Err(_) => T::default(),
    }
}

pub fn save<T: Serialize>(data_dir: &std::path::Path, name: &str, value: &T) {
    if let Ok(b) = serde_json::to_string_pretty(value) {
        let _ = write_atomic(&file(data_dir, name), b.as_bytes());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("droppy-store-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn save_then_load_round_trips() {
        let d = scratch("rt");
        save(&d, "tasks", &serde_json::json!({"a": 1, "b": [2, 3]}));
        let v: serde_json::Value = load(&d, "tasks");
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"][1], 3);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_leaves_no_tmp_file() {
        let d = scratch("tmp");
        save(&d, "tasks", &serde_json::json!({"a": 1}));
        let leftovers: Vec<_> = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "temp file left behind: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_file_loads_as_default() {
        let d = scratch("missing");
        let v: serde_json::Value = load(&d, "nope");
        assert_eq!(v, serde_json::Value::Null);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupt_file_is_quarantined_and_loads_default() {
        let d = scratch("corrupt");
        let p = file(&d, "notes");
        std::fs::write(&p, b"{not json!!!").unwrap();
        let v: serde_json::Value = load(&d, "notes");
        assert_eq!(v, serde_json::Value::Null);
        assert!(!p.exists(), "the corrupt original should be moved aside");
        let kept: Vec<_> = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("notes.json.corrupt-")).collect();
        assert_eq!(kept.len(), 1, "expected one quarantined copy");
        assert_eq!(std::fs::read(kept[0].path()).unwrap(), b"{not json!!!");
        // A fresh save still works and doesn't touch the quarantined copy.
        save(&d, "notes", &serde_json::json!({"ok": true}));
        assert!(kept[0].path().exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn write_atomic_replaces_existing_file() {
        let d = scratch("atomic");
        let p: PathBuf = d.join("x.json");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"two");
        let _ = std::fs::remove_dir_all(&d);
    }
}
