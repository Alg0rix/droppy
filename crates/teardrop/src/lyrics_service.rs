//! Synced-lyrics service: LRCLIB fetch + disk cache + position lookup.
//!
//! Cache: `%LOCALAPPDATA%\Teardrop\lyrics\<artist> — <title>.lrc` (sanitized).
//! Fetch: `https://lrclib.net/api/get?artist_name=&track_name=&album_name=`
//! via WinINet (no extra HTTP deps, keeps the binary small). All network
//! failures are silent — the island just shows no lyrics.

use teardrop_core::lyrics::{parse_lrc, LyricLine};

/// Percent-encode a query value (UTF-8, RFC 3986 unreserved set kept).
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Extract `syncedLyrics` from an LRCLIB `/get` JSON body.
/// Returns None when instrumental / missing / null.
pub fn extract_synced(json_body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json_body).ok()?;
    match &v["syncedLyrics"] {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    }
}

pub fn cache_path(cache_dir: &std::path::Path, artist: &str, title: &str) -> std::path::PathBuf {
    let bad = |c: char| matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*');
    let clean = |s: &str| {
        s.chars()
            .map(|c| if bad(c) || c.is_control() { '_' } else { c })
            .collect::<String>()
            .trim()
            .chars()
            .take(80)
            .collect::<String>()
    };
    let a = clean(artist);
    let t = clean(title);
    let name = if a.is_empty() { t } else { format!("{a} — {t}") };
    cache_dir.join("lyrics").join(format!("{name}.lrc"))
}

pub fn load_cached(cache_dir: &std::path::Path, artist: &str, title: &str) -> Option<Vec<LyricLine>> {
    let p = cache_path(cache_dir, artist, title);
    let text = std::fs::read_to_string(p).ok()?;
    let lines = parse_lrc(&text);
    if lines.is_empty() {
        None
    } else {
        Some(lines)
    }
}

pub fn save_cache(cache_dir: &std::path::Path, artist: &str, title: &str, lrc: &str) {
    let p = cache_path(cache_dir, artist, title);
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(p, lrc);
}

#[cfg(windows)]
pub mod net {
    use super::url_encode;
    use windows::core::PCWSTR;

    /// Blocking HTTPS GET via WinINet. Returns body text on HTTP 200.
    /// Short timeouts so the 2s slow-tick never stalls the island.
    pub fn https_get(url: &str) -> Option<String> {
        unsafe {
            use windows::Win32::Networking::WinInet::*;
            let agent: Vec<u16> = "Teardrop/0.1\0".encode_utf16().collect();
            let hinet = InternetOpenW(
                PCWSTR::from_raw(agent.as_ptr()),
                0, // PRECONFIG
                None,
                None,
                0,
            );
            if hinet.is_null() {
                return None;
            }
            // 8s connect+receive cap.
            let timeout: u32 = 8000;
            let _ = InternetSetOptionW(
                Some(hinet as *const _),
                2, // INTERNET_OPTION_CONNECT_TIMEOUT
                Some(&timeout as *const u32 as *const _),
                4,
            );
            let _ = InternetSetOptionW(
                Some(hinet as *const _),
                6, // INTERNET_OPTION_RECEIVE_TIMEOUT
                Some(&timeout as *const u32 as *const _),
                4,
            );
            let wurl: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
            let hurl = InternetOpenUrlW(
                hinet as *const _,
                PCWSTR::from_raw(wurl.as_ptr()),
                None,
                0x80000000 | 0x00800000, // RELOAD | SECURE
                0,
            );
            if hurl.is_null() {
                let _ = InternetCloseHandle(hinet as *const _);
                return None;
            }
            let mut out: Vec<u8> = Vec::with_capacity(16_384);
            let mut buf = [0u8; 4096];
            loop {
                let mut read: u32 = 0;
                if windows::Win32::Networking::WinInet::InternetReadFile(
                    hurl as *const _,
                    buf.as_mut_ptr() as *mut _,
                    buf.len() as u32,
                    &mut read,
                )
                .is_err()
                {
                    break;
                }
                if read == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..read as usize]);
                if out.len() > 256_000 {
                    break;
                }
            }
            let _ = InternetCloseHandle(hurl as *const _);
            let _ = InternetCloseHandle(hinet as *const _);
            String::from_utf8(out).ok()
        }
    }

    pub fn fetch_lrclib(artist: &str, title: &str, album: &str) -> Option<String> {
        let url = format!(
            "https://lrclib.net/api/get?artist_name={}&track_name={}&album_name={}",
            url_encode(artist),
            url_encode(title),
            url_encode(album)
        );
        let body = https_get(&url)?;
        super::extract_synced(&body)
    }
}

/// Resolve lyrics: cache hit wins, otherwise one network attempt (Windows),
/// then persist whatever LRCLIB returned. Never panics, never blocks long.
pub fn get_lyrics(
    cache_dir: &std::path::Path,
    artist: &str,
    title: &str,
    album: &str,
) -> Vec<LyricLine> {
    if artist.trim().is_empty() && title.trim().is_empty() {
        return Vec::new();
    }
    if let Some(hit) = load_cached(cache_dir, artist, title) {
        return hit;
    }
    #[cfg(windows)]
    {
        if let Some(lrc) = net::fetch_lrclib(artist, title, album) {
            save_cache(cache_dir, artist, title, &lrc);
            return parse_lrc(&lrc);
        }
    }
    let _ = album;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query() {
        assert_eq!(url_encode("a b/c?"), "a%20b%2Fc%3F");
        assert_eq!(url_encode("Midnight Droplet"), "Midnight%20Droplet");
    }

    #[test]
    fn extracts_synced() {
        let body = r#"{"id":1,"syncedLyrics":"[00:01.00]hi\n[00:05.00]yo\n","instrumental":false}"#;
        assert!(extract_synced(body).unwrap().contains("[00:01.00]hi"));
        assert!(extract_synced(r#"{"syncedLyrics":null}"#).is_none());
        assert!(extract_synced("not json").is_none());
    }

    #[test]
    fn cache_roundtrip() {
        let dir = std::env::temp_dir().join(format!("teardrop-lyr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        save_cache(&dir, "A/B", "T:C", "[00:01.00]hello\n");
        let hit = load_cached(&dir, "A/B", "T:C").expect("cache hit");
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].text, "hello");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
