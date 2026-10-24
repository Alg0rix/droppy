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

/// LRCLIB holds some one-line placeholder entries; anything that short is
/// not worth showing, so lookups move on to the next guess instead.
pub fn usable(lrc: &str) -> bool {
    parse_lrc(lrc).len() >= 3
}

/// Words that mark a bracketed chunk as video noise, not part of the song name.
const NOISE: [&str; 19] = [
    "official", "video", "audio", "lyric", "lyrics", "visualizer", "visualiser", "mv", "hd", "4k", "remaster", "remastered", "hq", "explicit", "full", "version", "feat", "ft", "featuring",
];

fn is_noise(chunk: &str) -> bool {
    let l = chunk.to_lowercase();
    l.split(|c: char| !c.is_alphanumeric()).any(|w| NOISE.contains(&w))
}

/// Drop `(Official Video)`, `[Lyrics]`, `| promo` style noise and `feat.` credits.
pub fn clean_title(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    let mut chunk = String::new();
    let mut open = '(';
    for ch in title.chars() {
        match ch {
            '(' | '[' | '{' if depth == 0 => {
                depth = 1;
                open = ch;
                chunk.clear();
            }
            ')' | ']' | '}' if depth == 1 => {
                depth = 0;
                if !is_noise(&chunk) {
                    out.push(open);
                    out.push_str(&chunk);
                    out.push(ch);
                }
            }
            _ if depth > 0 => chunk.push(ch),
            _ => out.push(ch),
        }
    }
    if depth > 0 {
        out.push(open);
        out.push_str(&chunk);
    }
    let mut s = out.as_str();
    for sep in [" | ", " // "] {
        if let Some(i) = s.find(sep) {
            s = &s[..i];
        }
    }
    let lower = s.to_lowercase();
    for key in [" feat.", " feat ", " ft.", " ft ", " featuring "] {
        if let Some(i) = lower.find(key) {
            s = &s[..i];
            break;
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// YouTube channels: "ArtistVEVO", "Artist - Topic", "Artist Official".
pub fn clean_artist(artist: &str) -> String {
    let mut a = artist.trim().to_string();
    for suf in [" - Topic", "VEVO", "Vevo", " Official", " OFFICIAL"] {
        if let Some(x) = a.strip_suffix(suf) {
            a = x.trim().to_string();
        }
    }
    a
}

/// Ordered (artist, title) guesses for a player's metadata. Browser tabs put
/// "Artist - Song" in the title and the channel name in the artist field.
pub fn candidates(artist: &str, title: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |a: String, t: String| {
        if !t.is_empty() && !out.iter().any(|(x, y)| x.eq_ignore_ascii_case(&a) && y.eq_ignore_ascii_case(&t)) {
            out.push((a, t));
        }
    };
    let (ca, ct) = (clean_artist(artist), clean_title(title));
    for sep in [" - ", " \u{2013} ", " \u{2014} ", " \u{2212} "] {
        if let Some((l, r)) = ct.split_once(sep) {
            push(clean_artist(l), clean_title(r));
            // "Song - Artist" order is rarer but happens.
            push(clean_artist(r), clean_title(l));
            break;
        }
    }
    push(ca, ct.clone());
    push(artist.trim().to_string(), title.trim().to_string());
    push(String::new(), ct);
    out
}

/// Best synced lyrics from an LRCLIB `/search` array: closest duration wins
/// (within 6 s when our duration is known).
pub fn pick_synced(json_body: &str, duration_ms: u64) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json_body).ok()?;
    let want = duration_ms as f64 / 1000.0;
    v.as_array()?
        .iter()
        .filter_map(|e| {
            let lrc = e["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty())?;
            let diff = if want > 0.0 { (e["duration"].as_f64().unwrap_or(0.0) - want).abs() } else { 0.0 };
            (want <= 0.0 || diff <= 6.0).then_some((diff, lrc))
        })
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, lrc)| lrc.to_string())
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

    /// Try every cleaned-up (artist, title) guess against LRCLIB: an exact
    /// `/get` first, then `/search`, finally a free-text search.
    pub fn fetch_lrclib(artist: &str, title: &str, album: &str, duration_ms: u64) -> Option<String> {
        let secs = duration_ms / 1000;
        for (a, t) in super::candidates(artist, title) {
            let mut url = format!(
                "https://lrclib.net/api/get?artist_name={}&track_name={}",
                url_encode(&a),
                url_encode(&t)
            );
            if !album.trim().is_empty() {
                url.push_str(&format!("&album_name={}", url_encode(album)));
            }
            if secs > 0 {
                url.push_str(&format!("&duration={secs}"));
            }
            if let Some(lrc) = https_get(&url).and_then(|b| super::extract_synced(&b)).filter(|l| super::usable(l)) {
                return Some(lrc);
            }
            let url = format!("https://lrclib.net/api/search?track_name={}&artist_name={}", url_encode(&t), url_encode(&a));
            if let Some(lrc) = https_get(&url).and_then(|b| super::pick_synced(&b, duration_ms)).filter(|l| super::usable(l)) {
                return Some(lrc);
            }
        }
        let (a, t) = super::candidates(artist, title).into_iter().next()?;
        let url = format!("https://lrclib.net/api/search?q={}", url_encode(format!("{a} {t}").trim()));
        https_get(&url).and_then(|b| super::pick_synced(&b, duration_ms)).filter(|l| super::usable(l))
    }
}

/// Resolve lyrics: cache hit wins, otherwise one network attempt (Windows),
/// then persist whatever LRCLIB returned. Never panics, never blocks long.
pub fn get_lyrics(
    cache_dir: &std::path::Path,
    artist: &str,
    title: &str,
    album: &str,
    duration_ms: u64,
) -> Vec<LyricLine> {
    if artist.trim().is_empty() && title.trim().is_empty() {
        return Vec::new();
    }
    if let Some(hit) = load_cached(cache_dir, artist, title) {
        return hit;
    }
    #[cfg(windows)]
    {
        if let Some(lrc) = net::fetch_lrclib(artist, title, album, duration_ms) {
            save_cache(cache_dir, artist, title, &lrc);
            return parse_lrc(&lrc);
        }
    }
    let _ = (album, duration_ms);
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
    fn youtube_titles_get_cleaned() {
        assert_eq!(clean_title("Never Gonna Give You Up (Official Video) [4K Remaster]"), "Never Gonna Give You Up");
        assert_eq!(clean_title("Song (feat. Someone) (Live)"), "Song (Live)");
        assert_eq!(clean_title("Blinding Lights | The Weeknd"), "Blinding Lights");
        assert_eq!(clean_artist("TheWeekndVEVO"), "TheWeeknd");
        assert_eq!(clean_artist("Daft Punk - Topic"), "Daft Punk");
    }

    #[test]
    fn candidates_split_artist_dash_title() {
        let c = candidates("RickAstleyVEVO", "Rick Astley - Never Gonna Give You Up (Official Music Video)");
        assert_eq!(c[0], ("Rick Astley".to_string(), "Never Gonna Give You Up".to_string()));
        let plain = candidates("Band", "Song");
        assert_eq!(plain[0], ("Band".to_string(), "Song".to_string()));
    }

    #[test]
    fn search_pick_prefers_matching_duration() {
        let body = r#"[{"duration":300,"syncedLyrics":"[00:01.00]long"},{"duration":212,"syncedLyrics":"[00:01.00]right"},{"duration":211,"syncedLyrics":null}]"#;
        assert!(pick_synced(body, 213_000).unwrap().contains("right"));
        assert!(pick_synced(body, 100_000).is_none(), "nothing within 6s");
        assert!(pick_synced(body, 0).is_some());
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

#[cfg(all(test, windows))]
mod live {
    /// Needs internet: `cargo test -p teardrop live_youtube_lyrics -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_youtube_lyrics() {
        let dir = std::env::temp_dir().join("teardrop-live-lyrics");
        let _ = std::fs::remove_dir_all(&dir);
        let lines = super::get_lyrics(&dir, "TheWeekndVEVO", "The Weeknd - Blinding Lights (Official Video)", "", 200_000);
        println!("{} lines", lines.len());
        assert!(lines.len() > 10);
    }
}
