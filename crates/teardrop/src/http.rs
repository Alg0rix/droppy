//! Minimal blocking HTTP/HTTPS GET & POST over WinINet, shared by the updater,
//! the usage readers, and AI Chat. Callers pin the hosts they talk to (see `host_is`).

/// True when `url` is HTTPS on exactly `host` (so a token only ever goes to
/// the provider it belongs to).
pub fn host_is(url: &str, host: &str) -> bool {
    url.strip_prefix("https://").map(|rest| rest.split('/').next() == Some(host)).unwrap_or(false)
}

/// Split an HTTP(S) URL into `(is_https, host, port, path_and_query)`.
/// Rejects userinfo (`@`) and empty hosts. Plain `http://` is permitted for
/// local model servers (e.g. Ollama on `localhost:11434`).
pub fn split_url(url: &str) -> Option<(bool, &str, u16, &str)> {
    let (https, rest, default_port) = if let Some(r) = url.strip_prefix("https://") {
        (true, r, 443u16)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r, 80u16)
    } else {
        return None;
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if let Some((h, p)) = authority.rsplit_once(':') {
        if h.contains(':') && !h.starts_with('[') {
            return None;
        }
        (h, p.parse::<u16>().ok()?)
    } else {
        (authority, default_port)
    };
    if host.is_empty() {
        return None;
    }
    Some((https, host, port, path))
}

/// A WinINet handle that closes itself. WinINet wants parents closed after
/// children; locals drop in reverse declaration order (hreq, hconn, hinet),
/// which keeps that true.
#[cfg(windows)]
struct Handle(*const core::ffi::c_void);

#[cfg(windows)]
impl Handle {
    fn is_null(&self) -> bool {
        self.0.is_null()
    }
}

#[cfg(windows)]
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = windows::Win32::Networking::WinInet::InternetCloseHandle(self.0);
            }
        }
    }
}

/// One request: `verb` on `url` with `body` (empty slice = no body), reading up
/// to `limit` response bytes when `read_when(status)` is true. Per-stage
/// errors are the short labels callers turn into their own error shapes.
#[cfg(windows)]
fn request(verb: &str, url: &str, headers: &[(&str, &str)], body: &[u8], recv_ms: u32, limit: usize, read_when: impl Fn(u32) -> bool) -> Result<(u32, Vec<u8>), &'static str> {
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinInet::*;
    let (https, host, port, path) = split_url(url).ok_or("Invalid URL")?;
    unsafe {
        let agent: Vec<u16> = concat!("droppy/", env!("CARGO_PKG_VERSION"), "\0").encode_utf16().collect();
        let hinet = Handle(InternetOpenW(PCWSTR::from_raw(agent.as_ptr()), 0, None, None, 0));
        if hinet.is_null() {
            return Err("WinINet init failed");
        }
        let connect_timeout: u32 = 15_000;
        let _ = InternetSetOptionW(Some(hinet.0), INTERNET_OPTION_CONNECT_TIMEOUT, Some(&connect_timeout as *const u32 as *const _), 4);
        let _ = InternetSetOptionW(Some(hinet.0), INTERNET_OPTION_SEND_TIMEOUT, Some(&recv_ms as *const u32 as *const _), 4);
        let _ = InternetSetOptionW(Some(hinet.0), INTERNET_OPTION_RECEIVE_TIMEOUT, Some(&recv_ms as *const u32 as *const _), 4);

        let whost: Vec<u16> = host.encode_utf16().chain(std::iter::once(0)).collect();
        let hconn = Handle(InternetConnectW(hinet.0, PCWSTR::from_raw(whost.as_ptr()), port, PCWSTR::null(), PCWSTR::null(), INTERNET_SERVICE_HTTP, 0, 0));
        if hconn.is_null() {
            return Err("Could not connect to server");
        }

        let wverb: Vec<u16> = verb.encode_utf16().chain(std::iter::once(0)).collect();
        let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut flags: u32 = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_COOKIES | INTERNET_FLAG_NO_UI;
        if https {
            flags |= INTERNET_FLAG_SECURE;
        }
        let hreq = Handle(HttpOpenRequestW(hconn.0, PCWSTR::from_raw(wverb.as_ptr()), PCWSTR::from_raw(wpath.as_ptr()), PCWSTR::null(), PCWSTR::null(), None, flags, 0));
        if hreq.is_null() {
            return Err("Could not open request");
        }

        let hdr_str: String = headers.iter().map(|(k, v)| format!("{k}: {v}\r\n")).collect();
        let hdr: Vec<u16> = hdr_str.encode_utf16().collect();
        // Never pass an empty slice: WinINet falls back to reading it as a
        // NUL-terminated string and walks off the end of the buffer.
        let sent =
            HttpSendRequestW(hreq.0, if hdr.is_empty() { None } else { Some(&hdr) }, if body.is_empty() { None } else { Some(body.as_ptr() as *const _) }, body.len() as u32);
        if sent.is_err() {
            return Err("Network request failed");
        }

        let mut st = [0u16; 8];
        let mut len = (st.len() * 2) as u32;
        let status = if HttpQueryInfoW(hreq.0, HTTP_QUERY_STATUS_CODE, Some(st.as_mut_ptr() as *mut _), &mut len, None).is_ok() {
            String::from_utf16_lossy(&st[..(len as usize / 2)]).trim().parse::<u32>().unwrap_or(0)
        } else {
            0
        };

        let mut out = Vec::new();
        if read_when(status) {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let mut read = 0u32;
                if InternetReadFile(hreq.0, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read).is_err() || read == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..read as usize]);
                if out.len() > limit {
                    out.truncate(limit);
                    break;
                }
            }
        }
        Ok((status, out))
    }
}

/// GET `url` (redirects followed) and return up to `limit` bytes of a 200
/// response. `Err(status)` carries the HTTP status, or 0 for a transport error.
#[cfg(windows)]
pub fn get(url: &str, headers: &[(&str, &str)], limit: usize) -> Result<Vec<u8>, u32> {
    match request("GET", url, headers, &[], 30_000, limit, |s| s == 200) {
        Ok((200, out)) => Ok(out),
        Ok((status, _)) => Err(status),
        Err(_) => Err(0),
    }
}

#[cfg(not(windows))]
pub fn get(_url: &str, _headers: &[(&str, &str)], _limit: usize) -> Result<Vec<u8>, u32> {
    Err(0)
}

/// POST `body` to `url` and return up to `limit` bytes on a 2xx status.
/// On non-2xx or transport failure, returns `Err((status, body_snippet))`.
#[cfg(windows)]
pub fn post(url: &str, headers: &[(&str, &str)], body: &[u8], limit: usize) -> Result<Vec<u8>, (u32, String)> {
    match request("POST", url, headers, body, 90_000, limit, |_| true) {
        Ok((status, out)) if (200..=299).contains(&status) => Ok(out),
        Ok((status, out)) => {
            let snippet = String::from_utf8_lossy(&out).chars().take(240).collect();
            Err((status, snippet))
        }
        Err(msg) => Err((0, msg.into())),
    }
}

#[cfg(not(windows))]
pub fn post(_url: &str, _headers: &[(&str, &str)], _body: &[u8], _limit: usize) -> Result<Vec<u8>, (u32, String)> {
    Err((0, "Unsupported platform".into()))
}

#[cfg(test)]
mod tests {
    use super::{host_is, split_url};

    #[test]
    fn hosts_are_pinned_exactly() {
        assert!(host_is("https://api.anthropic.com/api/oauth/usage", "api.anthropic.com"));
        assert!(!host_is("https://api.anthropic.com.evil.example/x", "api.anthropic.com"));
        assert!(!host_is("http://api.anthropic.com/x", "api.anthropic.com"));
        assert!(!host_is("https://evil.example/api.anthropic.com/", "api.anthropic.com"));
        assert!(!host_is("https://api.anthropic.com@evil.example/", "api.anthropic.com"));
    }

    #[test]
    fn split_url_handles_https_and_local_ports() {
        assert_eq!(split_url("https://opencode.ai/zen/go/v1/chat/completions"), Some((true, "opencode.ai", 443, "/zen/go/v1/chat/completions")));
        assert_eq!(split_url("http://localhost:11434/api/chat"), Some((false, "localhost", 11434, "/api/chat")));
        assert_eq!(
            split_url("https://chatgpt.com/backend-api/codex/models?client_version=1.0.0"),
            Some((true, "chatgpt.com", 443, "/backend-api/codex/models?client_version=1.0.0"))
        );
        assert!(split_url("https://user@evil.example/v1").is_none());
        assert!(split_url("ftp://example.com/v1").is_none());
    }
}
