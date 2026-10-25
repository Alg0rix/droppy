//! Data + cache directory helpers.
//!
//! Data (persisted, kept on uninstall):  %APPDATA%\Droppy\
//! Cache + log (safe to delete):         %LOCALAPPDATA%\Droppy\
//!
//! The app used to be called Teardrop; the first run after the rename moves
//! the old folders across so notes, tasks, chats and settings carry over.
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Move `old` to `new` when only `old` exists (rename, else copy). Returns
/// whether anything moved. Never overwrites an existing `new`.
pub fn migrate_dir(old: &Path, new: &Path) -> bool {
    if !old.is_dir() || new.exists() {
        return false;
    }
    if let Some(parent) = new.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::rename(old, new).is_ok() {
        return true;
    }
    // Different volume or a locked file: copy, and leave the old folder be.
    copy_tree(old, new).is_ok()
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), &target)?;
        }
    }
    Ok(())
}

/// One-time carry-over of the pre-rename folders (before anything reads them).
fn migrate_legacy() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let base = PathBuf::from(appdata);
            migrate_dir(&base.join("Teardrop"), &base.join("Droppy"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let base = PathBuf::from(local);
            migrate_dir(&base.join("Teardrop"), &base.join("Droppy"));
        }
    });
}

pub fn data_dir() -> PathBuf {
    migrate_legacy();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        PathBuf::from(appdata).join("Droppy")
    } else {
        PathBuf::from(".").join("droppy-data")
    }
}

pub fn cache_dir() -> PathBuf {
    migrate_legacy();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local).join("Droppy")
    } else {
        PathBuf::from(".").join("droppy-cache")
    }
}

pub fn ensure_dirs() -> (PathBuf, PathBuf) {
    let d = data_dir();
    let c = cache_dir();
    let _ = std::fs::create_dir_all(&d);
    let _ = std::fs::create_dir_all(&c);
    (d, c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("droppy-paths-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn old_folder_moves_over_with_its_contents() {
        let root = scratch("move");
        let (old, new) = (root.join("Teardrop"), root.join("Droppy"));
        std::fs::create_dir_all(old.join("sub")).unwrap();
        std::fs::write(old.join("notes.json"), "{\"n\":1}").unwrap();
        std::fs::write(old.join("sub").join("chats.json"), "[]").unwrap();
        assert!(migrate_dir(&old, &new));
        assert_eq!(std::fs::read_to_string(new.join("notes.json")).unwrap(), "{\"n\":1}");
        assert!(new.join("sub").join("chats.json").exists());
        assert!(!old.exists(), "a successful rename leaves no stale copy");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn never_overwrites_an_existing_new_folder() {
        let root = scratch("keep");
        let (old, new) = (root.join("Teardrop"), root.join("Droppy"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("a.txt"), "old").unwrap();
        std::fs::write(new.join("a.txt"), "new").unwrap();
        assert!(!migrate_dir(&old, &new));
        assert_eq!(std::fs::read_to_string(new.join("a.txt")).unwrap(), "new");
        assert!(old.join("a.txt").exists());
        assert!(!migrate_dir(&root.join("missing"), &root.join("also-new")), "nothing to migrate");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_fallback_preserves_the_tree() {
        let root = scratch("copy");
        let (from, to) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(from.join("x")).unwrap();
        std::fs::write(from.join("x").join("f.txt"), "hi").unwrap();
        copy_tree(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(to.join("x").join("f.txt")).unwrap(), "hi");
        let _ = std::fs::remove_dir_all(&root);
    }
}
