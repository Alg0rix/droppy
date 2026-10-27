use super::*;
// ------------------------------------------------------ JWT & OAuth helpers

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut buf = Vec::with_capacity(input.len() * 3 / 4 + 4);
    let mut accum: u32 = 0;
    let mut bits: u8 = 0;
    for b in input.bytes() {
        if b == b'=' {
            break;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        accum = (accum << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            buf.push(((accum >> bits) & 0xFF) as u8);
        }
    }
    Some(buf)
}

fn decode_jwt_payload(jwt: &str) -> Option<Value> {
    let mut parts = jwt.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let _sig = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let bytes = base64url_decode(payload)?;
    serde_json::from_slice(&bytes).ok()
}

/// Extract Unix expiry seconds (`exp` claim) from a JWT access token.
pub fn extract_jwt_expiry(token: &str) -> i64 {
    decode_jwt_payload(token).and_then(|v| v.get("exp").and_then(|e| e.as_i64().or_else(|| e.as_f64().map(|f| f as i64)))).unwrap_or(0)
}

/// Extract `chatgpt_account_id` from `claims["https://api.openai.com/auth"]["chatgpt_account_id"]`.
pub fn extract_chatgpt_account_id(token: &str) -> String {
    decode_jwt_payload(token)
        .and_then(|v| v.get("https://api.openai.com/auth").and_then(|a| a.get("chatgpt_account_id")).and_then(|id| id.as_str()).map(str::to_string))
        .unwrap_or_default()
}

fn form_urlencode(pairs: &[(&str, &str)]) -> String {
    let encode = |s: &str| -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
                _ => {
                    use std::fmt::Write;
                    let _ = write!(&mut out, "%{b:02X}");
                }
            }
        }
        out
    };
    pairs.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&")
}

/// Keep the ChatGPT tokens in step with the Codex CLI (`~/.codex/auth.json`).
///
/// Adopts the CLI's pair when Teardrop has none, when the CLI's access token
/// lasts longer (it refreshed after us), or when ours is expiring while the
/// CLI's is still good. Without this the app can stay pinned to a refresh
/// token the CLI already rotated, and every catalog sync fails silently.
pub(super) fn try_import_codex_cli_tokens(sec: &mut SecretStore) {
    let home =
        std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".codex")));
    let Some(auth_path) = home.map(|d| d.join("auth.json")).filter(|p| p.exists()) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(auth_path) else { return };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return };
    let tokens = v.get("tokens").unwrap_or(&v);
    let access = tokens.get("access_token").and_then(|s| s.as_str()).unwrap_or("");
    if access.is_empty() {
        return;
    }
    let refresh = tokens.get("refresh_token").and_then(|s| s.as_str()).unwrap_or("");
    let imported_exp = extract_jwt_expiry(access);
    let stored_exp = sec.chatgpt_expires_at;
    let now = now_secs();
    let take = decrypt_secret(&sec.chatgpt_access_token).is_empty() || imported_exp > stored_exp + 30 || (stored_exp > 0 && stored_exp - now <= 120 && imported_exp > now);
    if !take {
        return;
    }
    sec.chatgpt_access_token = encrypt_secret(access);
    if !refresh.is_empty() {
        sec.chatgpt_refresh_token = encrypt_secret(refresh);
    }
    sec.chatgpt_expires_at = imported_exp;
    let acct = tokens.get("account_id").and_then(|s| s.as_str()).map(str::to_string).filter(|s| !s.is_empty()).unwrap_or_else(|| extract_chatgpt_account_id(access));
    sec.chatgpt_account_id = acct;
    save_secrets(sec);
}

pub fn sign_out_chatgpt(provider: AiProvider) {
    LOGIN_CANCEL.store(true, Ordering::SeqCst);
    with_runtime(|rt| {
        rt.secrets.chatgpt_access_token.clear();
        rt.secrets.chatgpt_refresh_token.clear();
        rt.secrets.chatgpt_expires_at = 0;
        rt.secrets.chatgpt_account_id.clear();
        save_secrets(&rt.secrets);
        rt.view.chatgpt_signed_in = false;
        rt.view.chatgpt_login_busy = false;
        rt.view.chatgpt_user_code.clear();
        rt.view.status_note = "Signed out of ChatGPT".into();
        rt.rebuild_view(provider);
    });
}

pub fn cancel_chatgpt_login() {
    LOGIN_CANCEL.store(true, Ordering::SeqCst);
    LOGIN_BUSY.store(false, Ordering::SeqCst);
    with_runtime(|rt| {
        rt.view.chatgpt_login_busy = false;
        rt.view.chatgpt_user_code.clear();
        rt.view.status_note = "ChatGPT sign-in cancelled".into();
    });
}

/// Start the OpenAI Codex OAuth 2.0 Device Code flow (`Sign in with ChatGPT`).
pub fn start_chatgpt_login() {
    if LOGIN_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    LOGIN_CANCEL.store(false, Ordering::SeqCst);
    with_runtime(|rt| {
        rt.view.chatgpt_login_busy = true;
        rt.view.chatgpt_user_code.clear();
        rt.view.error.clear();
        rt.view.status_note = "Requesting ChatGPT device code\u{2026}".into();
    });

    std::thread::spawn(|| {
        let usercode_url = format!("{CHATGPT_ISSUER}/api/accounts/deviceauth/usercode");
        let body = json!({ "client_id": CHATGPT_CLIENT_ID }).to_string();
        let resp = crate::http::post(&usercode_url, &[("Content-Type", "application/json"), ("Accept", "application/json")], body.as_bytes(), 64 * 1024);
        let Ok(bytes) = resp else {
            LOGIN_BUSY.store(false, Ordering::SeqCst);
            with_runtime(|rt| {
                rt.view.chatgpt_login_busy = false;
                rt.view.error = "Could not reach auth.openai.com".into();
            });
            return;
        };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
            LOGIN_BUSY.store(false, Ordering::SeqCst);
            with_runtime(|rt| {
                rt.view.chatgpt_login_busy = false;
                rt.view.error = "Invalid device code response".into();
            });
            return;
        };
        let user_code = v.get("user_code").or_else(|| v.get("usercode")).and_then(|s| s.as_str()).unwrap_or("").to_string();
        let device_auth_id = v.get("device_auth_id").and_then(|s| s.as_str()).unwrap_or("").to_string();
        let interval: u64 = v.get("interval").and_then(|i| i.as_u64().or_else(|| i.as_str().and_then(|s| s.parse().ok()))).unwrap_or(5).max(3);

        if user_code.is_empty() || device_auth_id.is_empty() {
            LOGIN_BUSY.store(false, Ordering::SeqCst);
            with_runtime(|rt| {
                rt.view.chatgpt_login_busy = false;
                rt.view.error = "Device auth payload incomplete".into();
            });
            return;
        }

        with_runtime(|rt| {
            rt.view.chatgpt_user_code = user_code.clone();
            rt.view.status_note = format!("Enter code {user_code} in your browser");
        });

        // Open the verification page in the default browser.
        open_browser(CHATGPT_DEVICE_VERIFY_URL);

        let poll_url = format!("{CHATGPT_ISSUER}/api/accounts/deviceauth/token");
        let token_url = format!("{CHATGPT_ISSUER}/oauth/token");
        let redirect_uri = format!("{CHATGPT_ISSUER}/deviceauth/callback");
        let started = std::time::Instant::now();

        while started.elapsed() < std::time::Duration::from_secs(900) {
            std::thread::sleep(std::time::Duration::from_secs(interval));
            if LOGIN_CANCEL.load(Ordering::SeqCst) {
                LOGIN_BUSY.store(false, Ordering::SeqCst);
                return;
            }
            let poll_body = json!({
                "device_auth_id": device_auth_id,
                "user_code": user_code,
            })
            .to_string();
            match crate::http::post(&poll_url, &[("Content-Type", "application/json"), ("Accept", "application/json")], poll_body.as_bytes(), 64 * 1024) {
                Ok(poll_bytes) => {
                    let Ok(pv) = serde_json::from_slice::<Value>(&poll_bytes) else { continue };
                    let auth_code = pv.get("authorization_code").and_then(|s| s.as_str()).unwrap_or("");
                    let verifier = pv.get("code_verifier").and_then(|s| s.as_str()).unwrap_or("");
                    if auth_code.is_empty() || verifier.is_empty() {
                        continue;
                    }
                    let form = form_urlencode(&[
                        ("grant_type", "authorization_code"),
                        ("code", auth_code),
                        ("redirect_uri", &redirect_uri),
                        ("client_id", CHATGPT_CLIENT_ID),
                        ("code_verifier", verifier),
                    ]);
                    match crate::http::post(&token_url, &[("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json")], form.as_bytes(), 64 * 1024) {
                        Ok(tok_bytes) => {
                            if let Ok(tv) = serde_json::from_slice::<Value>(&tok_bytes) {
                                let access = tv.get("access_token").and_then(|s| s.as_str()).unwrap_or("");
                                let refresh = tv.get("refresh_token").and_then(|s| s.as_str()).unwrap_or("");
                                if !access.is_empty() {
                                    let exp = extract_jwt_expiry(access);
                                    let acct = extract_chatgpt_account_id(access);
                                    with_runtime(|rt| {
                                        rt.secrets.chatgpt_access_token = encrypt_secret(access);
                                        if !refresh.is_empty() {
                                            rt.secrets.chatgpt_refresh_token = encrypt_secret(refresh);
                                        }
                                        rt.secrets.chatgpt_expires_at = exp;
                                        rt.secrets.chatgpt_account_id = acct;
                                        save_secrets(&rt.secrets);
                                        rt.view.chatgpt_signed_in = true;
                                        rt.view.chatgpt_login_busy = false;
                                        rt.view.chatgpt_user_code.clear();
                                        rt.view.status_note = "Signed in with ChatGPT!".into();
                                        rt.rebuild_view(AiProvider::ChatGpt);
                                    });
                                    LOGIN_BUSY.store(false, Ordering::SeqCst);
                                    refresh_models(AiProvider::ChatGpt, CODEX_BASE_URL.into());
                                    return;
                                }
                            }
                        }
                        Err((st, _)) => {
                            with_runtime(|rt| {
                                rt.view.chatgpt_login_busy = false;
                                rt.view.error = format!("Token exchange failed (HTTP {st})");
                            });
                            LOGIN_BUSY.store(false, Ordering::SeqCst);
                            return;
                        }
                    }
                }
                Err((403 | 404, _)) => {
                    // User has not authorized the code yet; keep polling.
                    continue;
                }
                Err((st, _)) => {
                    with_runtime(|rt| {
                        rt.view.chatgpt_login_busy = false;
                        rt.view.error = format!("Device sign-in stopped (HTTP {st})");
                    });
                    LOGIN_BUSY.store(false, Ordering::SeqCst);
                    return;
                }
            }
        }

        LOGIN_BUSY.store(false, Ordering::SeqCst);
        with_runtime(|rt| {
            rt.view.chatgpt_login_busy = false;
            rt.view.chatgpt_user_code.clear();
            rt.view.error = "Device code expired \u{2014} try signing in again".into();
        });
    });
}

#[cfg(windows)]
pub fn open_browser(url: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    if !url.starts_with("https://") {
        return;
    }
    unsafe {
        let op: Vec<u16> = "open\0".encode_utf16().collect();
        let wurl: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = ShellExecuteW(None, PCWSTR::from_raw(op.as_ptr()), PCWSTR::from_raw(wurl.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

#[cfg(not(windows))]
pub fn open_browser(_url: &str) {}

/// Ensure the ChatGPT OAuth access token is fresh (proactively refreshes within 120s of expiry).
pub(super) fn ensure_fresh_chatgpt_token(sec: &mut SecretStore) -> Result<(String, String), String> {
    // Pick up a newer pair from the Codex CLI before falling back to ours.
    try_import_codex_cli_tokens(sec);
    let mut access = decrypt_secret(&sec.chatgpt_access_token);
    if access.is_empty() {
        return Err("Not signed in \u{2014} click Setup \u{2192} Sign in with ChatGPT".into());
    }
    let now = now_secs();
    if sec.chatgpt_expires_at > 0 && now >= sec.chatgpt_expires_at - 120 {
        let refresh = decrypt_secret(&sec.chatgpt_refresh_token);
        if !refresh.is_empty() {
            let token_url = format!("{CHATGPT_ISSUER}/oauth/token");
            let form = form_urlencode(&[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", CHATGPT_CLIENT_ID)]);
            if let Ok(bytes) = crate::http::post(&token_url, &[("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json")], form.as_bytes(), 64 * 1024) {
                if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                    if let Some(new_acc) = v.get("access_token").and_then(|s| s.as_str()).filter(|s| !s.is_empty()) {
                        access = new_acc.to_string();
                        sec.chatgpt_access_token = encrypt_secret(&access);
                        if let Some(new_ref) = v.get("refresh_token").and_then(|s| s.as_str()).filter(|s| !s.is_empty()) {
                            sec.chatgpt_refresh_token = encrypt_secret(new_ref);
                        }
                        sec.chatgpt_expires_at = extract_jwt_expiry(&access);
                        let acct = extract_chatgpt_account_id(&access);
                        if !acct.is_empty() {
                            sec.chatgpt_account_id = acct;
                        }
                        save_secrets(sec);
                    }
                }
            }
        }
    }
    let acct = if sec.chatgpt_account_id.is_empty() { extract_chatgpt_account_id(&access) } else { sec.chatgpt_account_id.clone() };
    Ok((access, acct))
}
