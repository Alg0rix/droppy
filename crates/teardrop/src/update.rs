//! Update checks against GitHub Releases.
//!
//! `check()` asks the GitHub API for the latest published release and compares
//! its tag with this build. `install()` downloads the one-click installer from
//! that release, verifies it against the release's `SHA256SUMS.txt` and runs
//! it (the installer closes Teardrop, replaces it and relaunches).
//!
//! Nothing is downloaded or run without the user clicking "Update", and a
//! missing or mismatching checksum aborts the install.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

pub const REPO: &str = "Alg0rix/teardrop";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

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
    let (setup_name, setup_url) = find(&|n| n.starts_with("teardrop-setup-") && n.ends_with(".exe"))?;
    let (_, sums_url) = find(&|n| n.eq_ignore_ascii_case("SHA256SUMS.txt"))?;
    // Only ever fetch from the project's own GitHub release downloads.
    let ok = |u: &str| u.starts_with(&format!("https://github.com/{REPO}/releases/download/"));
    (ok(&setup_url) && ok(&sums_url)).then(|| Release { version: tag.trim_start_matches(['v', 'V']).to_string(), setup_name, setup_url, sums_url })
}

/// Expected hash of `name` from a `sha256sum`-style listing.
pub fn find_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut p = l.split_whitespace();
        let (hash, file) = (p.next()?, p.next()?.trim_start_matches('*'));
        (file == name && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
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
            let sums = net::get_text(&release.sums_url)?;
            let want = find_sum(&sums, &release.setup_name)?;
            let bytes = net::get_bytes(&release.setup_url, 64 * 1024 * 1024)?;
            if sha256_hex(&bytes) != want {
                return None;
            }
            let dir = std::env::temp_dir().join("teardrop-update");
            std::fs::create_dir_all(&dir).ok()?;
            let path = dir.join(&release.setup_name);
            std::fs::write(&path, &bytes).ok()?;
            std::process::Command::new(&path).spawn().ok()?;
            Some(())
        })();
        if ok.is_none() {
            set(|s| s.view.phase = UpdatePhase::Failed);
        }
        // On success the installer closes this process and relaunches the new one.
    });
}

#[cfg(windows)]
mod net {
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinInet::*;

    /// Blocking HTTPS GET (follows redirects) of up to `limit` bytes.
    pub fn get_bytes(url: &str, limit: usize) -> Option<Vec<u8>> {
        unsafe {
            let agent: Vec<u16> = "Teardrop-Updater\0".encode_utf16().collect();
            let hinet = InternetOpenW(PCWSTR::from_raw(agent.as_ptr()), 0, None, None, 0);
            if hinet.is_null() {
                return None;
            }
            let timeout: u32 = 15_000;
            for opt in [2u32, 6] {
                let _ = InternetSetOptionW(Some(hinet as *const _), opt, Some(&timeout as *const u32 as *const _), 4);
            }
            let wurl: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
            // GitHub's API wants a UA (set above) and asks for JSON via Accept.
            let headers: Vec<u16> = "Accept: application/vnd.github+json\r\n".encode_utf16().collect();
            let hurl = InternetOpenUrlW(hinet as *const _, PCWSTR::from_raw(wurl.as_ptr()), Some(&headers), 0x8000_0000 | 0x0080_0000, 0);
            if hurl.is_null() {
                let _ = InternetCloseHandle(hinet as *const _);
                return None;
            }
            let mut status = [0u16; 8];
            let mut len = (status.len() * 2) as u32;
            let ok_status = HttpQueryInfoW(hurl as *const _, HTTP_QUERY_STATUS_CODE, Some(status.as_mut_ptr() as *mut _), &mut len, None).is_ok()
                && String::from_utf16_lossy(&status[..(len as usize / 2)]).trim() == "200";
            let mut out = Vec::new();
            if ok_status {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    let mut read = 0u32;
                    if InternetReadFile(hurl as *const _, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read).is_err() || read == 0 {
                        break;
                    }
                    out.extend_from_slice(&buf[..read as usize]);
                    if out.len() > limit {
                        out.clear();
                        break;
                    }
                }
            }
            let _ = InternetCloseHandle(hurl as *const _);
            let _ = InternetCloseHandle(hinet as *const _);
            (!out.is_empty()).then_some(out)
        }
    }

    pub fn get_text(url: &str) -> Option<String> {
        String::from_utf8(get_bytes(url, 2 * 1024 * 1024)?).ok()
    }
}

#[cfg(not(windows))]
mod net {
    pub fn get_bytes(_: &str, _: usize) -> Option<Vec<u8>> {
        None
    }
    pub fn get_text(_: &str) -> Option<String> {
        None
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
    fn sha256_known_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = vec![b'a'; 1000];
        assert_eq!(sha256_hex(&long).len(), 64);
    }

    #[test]
    fn release_json_is_parsed_and_urls_are_pinned() {
        let base = format!("https://github.com/{REPO}/releases/download/v0.2.0");
        let body = format!(
            r#"{{"tag_name":"v0.2.0","assets":[
              {{"name":"teardrop-v0.2.0.exe","browser_download_url":"{base}/teardrop-v0.2.0.exe"}},
              {{"name":"teardrop-setup-0.2.0.exe","browser_download_url":"{base}/teardrop-setup-0.2.0.exe"}},
              {{"name":"SHA256SUMS.txt","browser_download_url":"{base}/SHA256SUMS.txt"}}]}}"#
        );
        let r = parse_release(&body).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.setup_name, "teardrop-setup-0.2.0.exe");
        // An asset hosted anywhere else is rejected.
        let evil = body.replace("github.com/Alg0rix", "evil.example/Alg0rix");
        assert!(parse_release(&evil).is_none());
        assert!(parse_release(r#"{"message":"Not Found"}"#).is_none());
    }

    #[test]
    fn checksum_lookup() {
        let h = "a".repeat(64);
        let sums = format!("{h}  teardrop-setup-0.2.0.exe\n{}  other.zip\n", "b".repeat(64));
        assert_eq!(find_sum(&sums, "teardrop-setup-0.2.0.exe"), Some(h));
        assert_eq!(find_sum(&sums, "missing.exe"), None);
    }
}
