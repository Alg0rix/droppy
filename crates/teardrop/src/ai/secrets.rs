use super::*;
// --------------------------------------------------------- DPAPI & Secrets

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct SecretStore {
    #[serde(default)]
    pub(super) opencode_key: String,
    #[serde(default)]
    pub(super) openai_key: String,
    #[serde(default)]
    pub(super) gemini_key: String,
    #[serde(default)]
    pub(super) claude_key: String,
    #[serde(default)]
    pub(super) chatgpt_access_token: String,
    #[serde(default)]
    pub(super) chatgpt_refresh_token: String,
    #[serde(default)]
    pub(super) chatgpt_expires_at: i64,
    #[serde(default)]
    pub(super) chatgpt_account_id: String,
}

#[cfg(windows)]
#[repr(C)]
struct DataBlob {
    cb_data: u32,
    pb_data: *mut u8,
}

#[cfg(windows)]
#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(
        p_data_in: *const DataBlob,
        sz_data_descr: *const u16,
        p_optional_entropy: *const DataBlob,
        pv_reserved: *mut core::ffi::c_void,
        p_prompt_struct: *mut core::ffi::c_void,
        dw_flags: u32,
        p_data_out: *mut DataBlob,
    ) -> i32;

    fn CryptUnprotectData(
        p_data_in: *const DataBlob,
        ppsz_data_descr: *mut *mut u16,
        p_optional_entropy: *const DataBlob,
        pv_reserved: *mut core::ffi::c_void,
        p_prompt_struct: *mut core::ffi::c_void,
        dw_flags: u32,
        p_data_out: *mut DataBlob,
    ) -> i32;
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        out.push(u8::from_str_radix(&hex[i..i + 2], 16).ok()?);
    }
    Some(out)
}

/// Encrypt a secret with Windows DPAPI (`CryptProtectData`), returning `"dpapi:<hex>"`.
pub fn encrypt_secret(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::{LocalFree, HLOCAL};
        let bytes = plain.as_bytes();
        let mut in_blob = DataBlob { cb_data: bytes.len() as u32, pb_data: bytes.as_ptr() as *mut u8 };
        let mut out_blob = DataBlob { cb_data: 0, pb_data: std::ptr::null_mut() };
        // CRYPTPROTECT_UI_FORBIDDEN = 0x1
        let ok = CryptProtectData(&in_blob, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0x1, &mut out_blob);
        let _ = &mut in_blob;
        if ok != 0 && !out_blob.pb_data.is_null() {
            let slice = std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize);
            let encoded = format!("dpapi:{}", hex_encode(slice));
            let _ = LocalFree(HLOCAL(out_blob.pb_data as *mut _));
            return encoded;
        }
    }
    format!("hex:{}", hex_encode(plain.as_bytes()))
}

/// Decrypt a `"dpapi:<hex>"` secret with Windows DPAPI (`CryptUnprotectData`).
pub fn decrypt_secret(stored: &str) -> String {
    if stored.is_empty() {
        return String::new();
    }
    if let Some(hex) = stored.strip_prefix("dpapi:") {
        let Some(mut cipher) = hex_decode(hex) else { return String::new() };
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Foundation::{LocalFree, HLOCAL};
            let in_blob = DataBlob { cb_data: cipher.len() as u32, pb_data: cipher.as_mut_ptr() };
            let mut out_blob = DataBlob { cb_data: 0, pb_data: std::ptr::null_mut() };
            let ok = CryptUnprotectData(&in_blob, std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0x1, &mut out_blob);
            if ok != 0 && !out_blob.pb_data.is_null() {
                let slice = std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize);
                let plain = String::from_utf8_lossy(slice).into_owned();
                let _ = LocalFree(HLOCAL(out_blob.pb_data as *mut _));
                return plain;
            }
        }
        return String::new();
    }
    if let Some(hex) = stored.strip_prefix("hex:") {
        return hex_decode(hex).and_then(|b| String::from_utf8(b).ok()).unwrap_or_default();
    }
    String::new()
}

pub(super) fn appdata_dir() -> PathBuf {
    let dir = crate::paths::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn chats_path() -> PathBuf {
    appdata_dir().join("chats.json")
}

fn secrets_path() -> PathBuf {
    appdata_dir().join("ai-secrets.json")
}

pub(super) fn load_secrets() -> SecretStore {
    std::fs::read_to_string(secrets_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub(super) fn save_secrets(sec: &SecretStore) {
    if let Ok(json) = serde_json::to_string_pretty(sec) {
        let tmp = secrets_path().with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, secrets_path());
        }
    }
}

pub(super) fn load_chat_store() -> ChatStore {
    let mut store: ChatStore = std::fs::read_to_string(chats_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if store.conversations.is_empty() {
        let id = now_millis();
        store.conversations.push(Conversation { id, title: "New chat".into(), updated_at: (id / 1000) as i64, messages: Vec::new() });
        store.active_id = id;
    } else if !store.conversations.iter().any(|c| c.id == store.active_id) {
        store.active_id = store.conversations[0].id;
    }
    store
}

pub(super) fn save_chat_store(store: &ChatStore) {
    if let Ok(json) = serde_json::to_string_pretty(store) {
        let tmp = chats_path().with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, chats_path());
        }
    }
}
