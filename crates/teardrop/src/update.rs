//! Update checks against GitHub Releases.
//!
//! `check()` asks the GitHub API for the latest published release and compares
//! its tag with this build. `install()` downloads the one-click installer from
//! that release, verifies it against the release's `SHA256SUMS.txt`, runs it,
//! then closes Droppy itself so the installer never has to kill a running
//! copy before replacing the exe.
//!
//! The checksum file itself is verified first: `SHA256SUMS.txt.sig` must hold
//! a valid ed25519 signature (from `UPDATE_PUBKEY`) over the exact bytes of
//! `SHA256SUMS.txt`, so a tampered asset or checksum can't install.
//!
//! Nothing is downloaded or run without the user clicking "Update", and a
//! missing signature or mismatching checksum aborts the install.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use teardrop_core::release::{sha256_hex, verify_signature};

pub const REPO: &str = "Alg0rix/droppy";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Hex ed25519 public key the release checksums must be signed with.
/// Empty means updates refuse to install (generate a pair with
/// `cargo run -p teardrop-core --example release_sign -- keygen <file>`).
pub const UPDATE_PUBKEY: &str = "";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Failed,
}

impl UpdatePhase {
    pub fn busy(self) -> bool {
        matches!(self, UpdatePhase::Checking | UpdatePhase::Downloading)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UpdateView {
    pub phase: UpdatePhase,
    /// Latest published version, e.g. "0.2.0" (set once known).
    pub version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub setup_name: String,
    pub setup_url: String,
    pub sums_url: String,
    pub sig_url: String,
}

struct Shared {
    view: UpdateView,
    release: Option<Release>,
}

static STATE: Mutex<Shared> = Mutex::new(Shared { view: UpdateView { phase: UpdatePhase::Idle, version: String::new() }, release: None });
static REVISION: AtomicU32 = AtomicU32::new(0);

fn set(f: impl FnOnce(&mut Shared)) {
    if let Ok(mut s) = STATE.lock() {
        f(&mut s);
        REVISION.fetch_add(1, Ordering::Release);
    }
}

/// Changes whenever the shared state does; lets the UI skip copying it.
pub fn revision() -> u32 {
    REVISION.load(Ordering::Acquire)
}

pub fn snapshot() -> UpdateView {
    STATE.lock().map(|s| s.view.clone()).unwrap_or_default()
}

// ------------------------------------------------------------------ pure bits

/// "v1.2.3" / "1.2" / "1.2.3-beta" -> (1, 2, 3). None when unparsable.
pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let core = v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().map(|s| s.parse().ok()).unwrap_or(Some(0))?;
    let patch = it.next().map(|s| s.parse().ok()).unwrap_or(Some(0))?;
    Some((major, minor, patch))
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    match (parse_version(remote), parse_version(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
}

/// Pull the tag and installer/checksum assets out of a `/releases/latest` body.
pub fn parse_release(body: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let tag = v["tag_name"].as_str()?;
    parse_version(tag)?;
    let assets = v["assets"].as_array()?;
    let find = |pred: &dyn Fn(&str) -> bool| {
        assets.iter().find_map(|a| {
            let name = a["name"].as_str()?;
            let url = a["browser_download_url"].as_str()?;
            pred(name).then(|| (name.to_string(), url.to_string()))
        })
    };
    let (setup_name, setup_url) = find(&|n| n.starts_with("droppy-setup-") && n.ends_with(".exe"))?;
    let (_, sums_url) = find(&|n| n.eq_ignore_ascii_case("SHA256SUMS.txt"))?;
    let (_, sig_url) = find(&|n| n.eq_ignore_ascii_case("SHA256SUMS.txt.sig"))?;
    // Only ever fetch from the project's own GitHub release downloads.
    let ok = |u: &str| u.starts_with(&format!("https://github.com/{REPO}/releases/download/"));
    (ok(&setup_url) && ok(&sums_url) && ok(&sig_url)).then(|| Release { version: tag.trim_start_matches(['v', 'V']).to_string(), setup_name, setup_url, sums_url, sig_url })
}

/// Expected hash of `name` from a `sha256sum`-style listing.
pub fn find_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut p = l.split_whitespace();
        let (hash, file) = (p.next()?, p.next()?.trim_start_matches('*'));
        (file == name && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

// -------------------------------------------------------------------- actions

/// Look for a newer release in the background.
pub fn check() {
    let go = STATE.lock().map(|s| !s.view.phase.busy()).unwrap_or(false);
    if !go {
        return;
    }
    set(|s| s.view.phase = UpdatePhase::Checking);
    std::thread::spawn(|| {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let release = net::get_text(&url).and_then(|b| parse_release(&b));
        set(|s| match release {
            Some(r) if is_newer(&r.version, CURRENT) => {
                s.view = UpdateView { phase: UpdatePhase::Available, version: r.version.clone() };
                s.release = Some(r);
            }
            Some(r) => {
                s.view = UpdateView { phase: UpdatePhase::UpToDate, version: r.version };
                s.release = None;
            }
            // No release yet (404), offline or rate-limited: say nothing alarming.
            None => s.view.phase = if s.view.version.is_empty() { UpdatePhase::UpToDate } else { s.view.phase },
        });
    });
}

/// Download, verify and run the installer for the release found by `check()`.
pub fn install() {
    let release = STATE.lock().ok().and_then(|s| matches!(s.view.phase, UpdatePhase::Available | UpdatePhase::Failed).then(|| s.release.clone()).flatten());
    let Some(release) = release else { return };
    set(|s| s.view.phase = UpdatePhase::Downloading);
    std::thread::spawn(move || {
        let ok = (|| {
            // Verify the checksum file's signature before trusting its contents.
            let sums_bytes = net::get_bytes(&release.sums_url, 2 * 1024 * 1024)?;
            let sig = net::get_text(&release.sig_url)?;
            if !verify_signature(UPDATE_PUBKEY, &sums_bytes, &sig) {
                return None;
            }
            let sums = String::from_utf8(sums_bytes).ok()?;
            let want = find_sum(&sums, &release.setup_name)?;
            let bytes = net::get_bytes(&release.setup_url, 64 * 1024 * 1024)?;
            if sha256_hex(&bytes) != want {
                return None;
            }
            let dir = std::env::temp_dir().join("droppy-update");
            std::fs::create_dir_all(&dir).ok()?;
            let path = dir.join(&release.setup_name);
            std::fs::write(&path, &bytes).ok()?;
            std::process::Command::new(&path).spawn().ok()?;
            quit_app();
            Some(())
        })();
        if ok.is_none() {
            set(|s| s.view.phase = UpdatePhase::Failed);
        }
    });
}

/// Close the island like the user quit it: WM_CLOSE runs the normal
/// WM_DESTROY cleanup (saving state, stopping recordings). A running exe
/// can't be replaced, and the installer's own taskkill is only a fallback.
#[cfg(windows)]
fn quit_app() {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
    unsafe {
        if let Ok(hwnd) = FindWindowW(w!("TeardropIsland"), PCWSTR::null()) {
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(not(windows))]
fn quit_app() {}

mod net {
    /// Blocking HTTPS GET (follows redirects) of up to `limit` bytes.
    pub fn get_bytes(url: &str, limit: usize) -> Option<Vec<u8>> {
        crate::http::get(url, &[("Accept", "application/vnd.github+json")], limit).ok()
    }

    pub fn get_text(url: &str) -> Option<String> {
        String::from_utf8(get_bytes(url, 2 * 1024 * 1024)?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
    }

    #[test]
    fn release_json_is_parsed_and_urls_are_pinned() {
        let base = format!("https://github.com/{REPO}/releases/download/v0.2.0");
        let body = format!(
            r#"{{"tag_name":"v0.2.0","assets":[
              {{"name":"droppy-v0.2.0.exe","browser_download_url":"{base}/droppy-v0.2.0.exe"}},
              {{"name":"droppy-setup-0.2.0.exe","browser_download_url":"{base}/droppy-setup-0.2.0.exe"}},
              {{"name":"SHA256SUMS.txt","browser_download_url":"{base}/SHA256SUMS.txt"}},
              {{"name":"SHA256SUMS.txt.sig","browser_download_url":"{base}/SHA256SUMS.txt.sig"}}]}}"#
        );
        let r = parse_release(&body).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.setup_name, "droppy-setup-0.2.0.exe");
        // An asset hosted anywhere else is rejected.
        let evil = body.replace("github.com/Alg0rix", "evil.example/Alg0rix");
        assert!(parse_release(&evil).is_none());
        // A release without the signature asset is rejected.
        let no_sig = body.replace(&format!(",\n              {{\"name\":\"SHA256SUMS.txt.sig\",\"browser_download_url\":\"{base}/SHA256SUMS.txt.sig\"}}"), "");
        assert!(parse_release(&no_sig).is_none());
        // A signature URL hosted anywhere else is rejected.
        let evil_sig = body.replace(&format!("{base}/SHA256SUMS.txt.sig"), "https://evil.example/SHA256SUMS.txt.sig");
        assert!(parse_release(&evil_sig).is_none());
        assert!(parse_release(r#"{"message":"Not Found"}"#).is_none());
    }

    #[test]
    fn checksum_lookup() {
        let h = "a".repeat(64);
        let sums = format!("{h}  droppy-setup-0.2.0.exe\n{}  other.zip\n", "b".repeat(64));
        assert_eq!(find_sum(&sums, "droppy-setup-0.2.0.exe"), Some(h));
        assert_eq!(find_sum(&sums, "missing.exe"), None);
    }
}
