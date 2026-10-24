//! Minimal blocking HTTPS GET over WinINet, shared by the updater and the
//! usage readers. Callers pin the hosts they talk to (see `host_is`).

/// True when `url` is HTTPS on exactly `host` (so a token only ever goes to
/// the provider it belongs to).
pub fn host_is(url: &str, host: &str) -> bool {
    url.strip_prefix("https://").map(|rest| rest.split('/').next() == Some(host)).unwrap_or(false)
}

/// GET `url` (redirects followed) and return up to `limit` bytes of a 200
/// response. `Err(status)` carries the HTTP status, or 0 for a transport error.
#[cfg(windows)]
pub fn get(url: &str, headers: &[(&str, &str)], limit: usize) -> Result<Vec<u8>, u32> {
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinInet::*;
    unsafe {
        let agent: Vec<u16> = "Teardrop/0.2\0".encode_utf16().collect();
        let hinet = InternetOpenW(PCWSTR::from_raw(agent.as_ptr()), 0, None, None, 0);
        if hinet.is_null() {
            return Err(0);
        }
        let timeout: u32 = 15_000;
        for opt in [2u32, 6] {
            let _ = InternetSetOptionW(Some(hinet as *const _), opt, Some(&timeout as *const u32 as *const _), 4);
        }
        let wurl: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        let hdr: Vec<u16> = headers.iter().map(|(k, v)| format!("{k}: {v}\r\n")).collect::<String>().encode_utf16().collect();
        let hurl = InternetOpenUrlW(hinet as *const _, PCWSTR::from_raw(wurl.as_ptr()), Some(&hdr), 0x8000_0000 | 0x0080_0000, 0);
        if hurl.is_null() {
            let _ = InternetCloseHandle(hinet as *const _);
            return Err(0);
        }
        let mut st = [0u16; 8];
        let mut len = (st.len() * 2) as u32;
        let status = if HttpQueryInfoW(hurl as *const _, HTTP_QUERY_STATUS_CODE, Some(st.as_mut_ptr() as *mut _), &mut len, None).is_ok() {
            String::from_utf16_lossy(&st[..(len as usize / 2)]).trim().parse::<u32>().unwrap_or(0)
        } else {
            0
        };
        let mut out = Vec::new();
        if status == 200 {
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
        if status == 200 && !out.is_empty() {
            Ok(out)
        } else {
            Err(status)
        }
    }
}

#[cfg(not(windows))]
pub fn get(_url: &str, _headers: &[(&str, &str)], _limit: usize) -> Result<Vec<u8>, u32> {
    Err(0)
}

#[cfg(test)]
mod tests {
    use super::host_is;

    #[test]
    fn hosts_are_pinned_exactly() {
        assert!(host_is("https://api.anthropic.com/api/oauth/usage", "api.anthropic.com"));
        assert!(!host_is("https://api.anthropic.com.evil.example/x", "api.anthropic.com"));
        assert!(!host_is("http://api.anthropic.com/x", "api.anthropic.com"));
        assert!(!host_is("https://evil.example/api.anthropic.com/", "api.anthropic.com"));
        assert!(!host_is("https://api.anthropic.com@evil.example/", "api.anthropic.com"));
    }
}
