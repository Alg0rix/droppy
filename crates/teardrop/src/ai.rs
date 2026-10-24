//! AI Chat: multi-provider assistant with day context, reversible Teardrop
//! actions, durable user memory, and encrypted credentials.
//!
//! Supported providers:
//! 1. **Sign in with ChatGPT** (`AiProvider::ChatGpt`) — OpenAI Codex subscription
//!    OAuth 2.0 Device Code flow + Responses API (`https://chatgpt.com/backend-api/codex`).
//! 2. **OpenCode Go** (`AiProvider::OpenCodeGo`) — `https://opencode.ai/zen/go/v1`
//!    with automatic protocol routing (`responses` / `messages` / `chat/completions`),
//!    custom `User-Agent` and `x-opencode-session` headers, and live `/models` discovery.
//! 3. **OpenAI-compatible** (`AiProvider::OpenAiCompat`) — any `/v1/chat/completions`
//!    endpoint (OpenAI, OpenRouter, Groq, Together, LM Studio, vLLM, llama.cpp).
//! 4. **Google Gemini** (`AiProvider::Gemini`) — `generativelanguage.googleapis.com`.
//! 5. **Claude (Anthropic)** (`AiProvider::Claude`) — `api.anthropic.com/v1/messages`.
//! 6. **Ollama (Local)** (`AiProvider::Ollama`) — local `http://localhost:11434`.
//!
//! Security rules:
//! * API keys and OAuth tokens are encrypted at rest with Windows DPAPI
//!   (`CryptProtectData` / `CryptUnprotectData`) in `%APPDATA%\Teardrop\ai-secrets.json`.
//! * Secrets are never copied into `ViewModel`, `AiView`, logs, or error strings.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use teardrop_core::{AiProvider, Settings};

pub const MAX_CHATS: usize = 40;
pub const MAX_MEMORIES: usize = 32;

pub const CHATGPT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const CHATGPT_ISSUER: &str = "https://auth.openai.com";
pub const CHATGPT_DEVICE_VERIFY_URL: &str = "https://auth.openai.com/codex/device";
pub const CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const OPENCODE_GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";

pub const STARTER_PROMPTS: [(&str, &str); 4] = [
    ("Plan my day", "What's on my schedule today and what should I focus on first?"),
    ("Task + Focus", "Add a task 'Deep work session' and start my Pomodoro focus timer."),
    ("Water & PC check", "Log 250 ml of water and tell me how my PC resources look."),
    ("Prioritize tasks", "Summarize my open tasks and suggest the top 3 priorities."),
];

pub const CODEX_DEFAULT_MODELS: &[&str] = &[
    "gpt-5.3-codex",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.5",
    "gpt-5.6-sol",
    "gpt-5.6-sol-pro",
    "gpt-5.6-terra",
    "gpt-5.6-terra-pro",
    "gpt-5.6-luna",
    "gpt-5.6-luna-pro",
    "gpt-5.3-codex-spark",
];

pub const OPENCODE_DEFAULT_MODELS: &[&str] = &[
    "gpt-5.4",
    "claude-sonnet-4-6",
    "deepseek-r1",
    "gemini-2.5-pro",
    "qwen3-coder",
    "glm-5",
];

/// Starter suggestions shown when a chat is empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// Reversible mutation performed by the assistant on Teardrop state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoOp {
    /// Remove the task with this id (undo of `ADD_TASK`).
    RemoveAddedTask(String),
    /// Toggle the task with this id back to open (undo of `DONE_TASK`).
    ReopenTask(String),
    /// Restore a deleted task `(title, was_done)` (undo of `DEL_TASK`).
    RestoreDeletedTask(String, bool),
    /// Subtract logged water in ml (undo of `WATER`).
    SubtractWater(u32),
    /// Remove a learned memory string (undo of `REMEMBER`).
    ForgetMemory(String),
    /// Non-reversible action (e.g. Pomodoro control).
    None,
}

/// An action chip attached to an assistant message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiActionBadge {
    pub label: String,
    pub undo: UndoOp,
    #[serde(default)]
    pub undone: bool,
}

impl AiActionBadge {
    pub fn can_undo(&self) -> bool {
        !self.undone && !matches!(self.undo, UndoOp::None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub text: String,
    #[serde(default)]
    pub actions: Vec<AiActionBadge>,
    #[serde(default)]
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: u64,
    pub title: String,
    pub updated_at: i64,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatStore {
    pub conversations: Vec<Conversation>,
    #[serde(default)]
    pub active_id: u64,
    /// Learned durable user facts/preferences (`Alg0rix/tomo` learning loop).
    #[serde(default)]
    pub memories: Vec<String>,
}

/// Summary of a conversation for the History drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSummary {
    pub id: u64,
    pub title: String,
    pub msg_count: usize,
}

/// Which text field in the AI tab currently receives keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiFocusField {
    #[default]
    Prompt,
    Model,
    BaseUrl,
}

/// Public, secret-free snapshot rendered by `ui/pages.rs`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AiView {
    pub active_id: u64,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub history: Vec<ChatSummary>,
    pub memories_count: usize,
    pub input: String,
    pub focus_field: AiFocusField,
    pub show_history: bool,
    pub show_setup: bool,
    pub busy: bool,
    pub error: String,
    pub status_note: String,
    /// Whether the active provider has a stored API key / OAuth token.
    pub configured: bool,
    pub chatgpt_signed_in: bool,
    pub chatgpt_login_busy: bool,
    pub chatgpt_user_code: String,
    pub available_models: Vec<String>,
    pub models_loading: bool,
    pub scroll: i32,
}

/// Concrete directive parsed from an assistant reply (`[ACTION:...]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedDirective {
    AddTask(String),
    DoneTask(String),
    DelTask(String),
    LogWater(u32),
    Focus(String),
    Remember(String),
}

// --------------------------------------------------------- DPAPI & Secrets

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SecretStore {
    #[serde(default)]
    opencode_key: String,
    #[serde(default)]
    openai_key: String,
    #[serde(default)]
    gemini_key: String,
    #[serde(default)]
    claude_key: String,
    #[serde(default)]
    chatgpt_access_token: String,
    #[serde(default)]
    chatgpt_refresh_token: String,
    #[serde(default)]
    chatgpt_expires_at: i64,
    #[serde(default)]
    chatgpt_account_id: String,
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
        use windows::Win32::Foundation::{HLOCAL, LocalFree};
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
            use windows::Win32::Foundation::{HLOCAL, LocalFree};
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

fn appdata_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("Teardrop");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn chats_path() -> PathBuf {
    appdata_dir().join("chats.json")
}

fn secrets_path() -> PathBuf {
    appdata_dir().join("ai-secrets.json")
}

fn load_secrets() -> SecretStore {
    std::fs::read_to_string(secrets_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_secrets(sec: &SecretStore) {
    if let Ok(json) = serde_json::to_string_pretty(sec) {
        let tmp = secrets_path().with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, secrets_path());
        }
    }
}

fn load_chat_store() -> ChatStore {
    let mut store: ChatStore = std::fs::read_to_string(chats_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if store.conversations.is_empty() {
        let id = now_millis();
        store.conversations.push(Conversation {
            id,
            title: "New chat".into(),
            updated_at: (id / 1000) as i64,
            messages: Vec::new(),
        });
        store.active_id = id;
    } else if !store.conversations.iter().any(|c| c.id == store.active_id) {
        store.active_id = store.conversations[0].id;
    }
    store
}

fn save_chat_store(store: &ChatStore) {
    if let Ok(json) = serde_json::to_string_pretty(store) {
        let tmp = chats_path().with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, chats_path());
        }
    }
}

fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis().max(1) as u64
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

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
    decode_jwt_payload(token)
        .and_then(|v| v.get("exp").and_then(|e| e.as_i64().or_else(|| e.as_f64().map(|f| f as i64))))
        .unwrap_or(0)
}

/// Extract `chatgpt_account_id` from `claims["https://api.openai.com/auth"]["chatgpt_account_id"]`.
pub fn extract_chatgpt_account_id(token: &str) -> String {
    decode_jwt_payload(token)
        .and_then(|v| {
            v.get("https://api.openai.com/auth")
                .and_then(|a| a.get("chatgpt_account_id"))
                .and_then(|id| id.as_str())
                .map(str::to_string)
        })
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
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

// --------------------------------------------------- Protocol & SSE Parsers

/// OpenCode Go wire protocol per model family (mirrors `Alg0rix/tomo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenCodeProtocol {
    /// OpenAI Responses API (`/v1/responses`) for `gpt-*`, `grok-*`, `muse-spark*`.
    Responses,
    /// Anthropic Messages API (`/v1/messages`) for `claude-*`, `union-alpha*`, `minimax-*`, `qwen*`.
    Messages,
    /// OpenAI Chat Completions (`/v1/chat/completions`) for `deepseek-*`, `glm-*`, `kimi-*`, etc.
    Chat,
}

pub fn opencode_model_protocol(model: &str) -> OpenCodeProtocol {
    let m = model.trim().to_ascii_lowercase();
    let m = m.strip_prefix("opencode-go/").unwrap_or(&m);
    if m.starts_with("gpt-") || m.starts_with("grok-") || m.starts_with("muse-spark") {
        OpenCodeProtocol::Responses
    } else if m.starts_with("claude-")
        || m.starts_with("union-alpha")
        || m.starts_with("minimax-")
        || m.starts_with("qwen")
    {
        OpenCodeProtocol::Messages
    } else {
        OpenCodeProtocol::Chat
    }
}

/// Normalize an OpenAI-compatible base URL into its `/v1` root (without trailing slash).
pub fn normalize_openai_base_url(raw: &str) -> String {
    let mut s = raw.trim().trim_end_matches('/').to_string();
    if s.is_empty() {
        return "https://api.openai.com/v1".into();
    }
    if let Some(stripped) = s.strip_suffix("/chat/completions") {
        s = stripped.trim_end_matches('/').to_string();
    }
    if !s.starts_with("https://") && !s.starts_with("http://") {
        s = format!("https://{s}");
    }
    // If the URL has no path after the host (e.g. `https://api.openai.com`), append `/v1`.
    if let Some(rest) = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")) {
        if !rest.contains('/') {
            s.push_str("/v1");
        }
    }
    s
}

/// Parse a Codex / OpenAI `/responses` payload (supports both SSE streams and JSON objects).
pub fn parse_responses_sse_or_json(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            if let Some(err) = v.get("error").and_then(|e| e.get("message").or(Some(e))).and_then(|m| m.as_str()) {
                return Err(err.to_string());
            }
            if let Some(txt) = v.get("output_text").and_then(|t| t.as_str()) {
                if !txt.is_empty() {
                    return Ok(txt.to_string());
                }
            }
            let mut chunks = Vec::new();
            if let Some(items) = v.get("output").and_then(|o| o.as_array()) {
                for item in items {
                    if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
                        for part in content {
                            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                                chunks.push(t.to_string());
                            }
                        }
                    }
                }
            }
            if !chunks.is_empty() {
                return Ok(chunks.join(""));
            }
        }
    }

    let mut deltas = Vec::new();
    let mut fallback = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(ev) = serde_json::from_str::<Value>(data) else { continue };
        if let Some(err) = ev.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
            return Err(err.to_string());
        }
        let kind = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match kind {
            "response.output_text.delta" => {
                if let Some(d) = ev.get("delta").and_then(|d| d.as_str()) {
                    deltas.push(d.to_string());
                }
            }
            "response.output_item.done" if deltas.is_empty() => {
                if let Some(content) = ev.pointer("/item/content").and_then(|c| c.as_array()) {
                    for c in content {
                        if let Some(t) = c.get("text").and_then(|t| t.as_str()) {
                            fallback.push(t.to_string());
                        }
                    }
                }
            }
            "response.completed" | "response.done" if deltas.is_empty() && fallback.is_empty() => {
                if let Some(output) = ev.pointer("/response/output").and_then(|o| o.as_array()) {
                    for item in output {
                        if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
                            for c in content {
                                if let Some(t) = c.get("text").and_then(|t| t.as_str()) {
                                    fallback.push(t.to_string());
                                }
                            }
                        }
                    }
                }
            }
            "error" | "response.failed" => {
                let msg = ev
                    .pointer("/error/message")
                    .or_else(|| ev.pointer("/response/error/message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Responses API error");
                return Err(msg.to_string());
            }
            _ => {}
        }
    }

    if !deltas.is_empty() {
        Ok(deltas.join(""))
    } else if !fallback.is_empty() {
        Ok(fallback.join(""))
    } else {
        Err("Empty response from model".into())
    }
}

/// Parse an OpenAI `/chat/completions` response (JSON or SSE stream).
pub fn parse_openai_chat_response(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.starts_with("data:") || trimmed.contains("\ndata:") {
        let mut out = String::new();
        for line in trimmed.lines() {
            let Some(data) = line.trim().strip_prefix("data:") else { continue };
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(delta) = v.pointer("/choices/0/delta/content").and_then(|c| c.as_str()) {
                    out.push_str(delta);
                }
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }

    let v: Value = serde_json::from_str(trimmed).map_err(|_| "Invalid JSON from server".to_string())?;
    if let Some(err) = v.pointer("/error/message").and_then(|m| m.as_str()) {
        return Err(err.to_string());
    }
    if let Some(content) = v.pointer("/choices/0/message/content") {
        if let Some(s) = content.as_str() {
            return Ok(s.to_string());
        }
        if let Some(arr) = content.as_array() {
            let joined: String = arr
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("");
            if !joined.is_empty() {
                return Ok(joined);
            }
        }
    }
    if let Some(s) = v.pointer("/message/content").and_then(|c| c.as_str()) {
        return Ok(s.to_string());
    }
    Err("No message content in response".into())
}

/// Parse an Anthropic `/v1/messages` response.
pub fn parse_anthropic_response(raw: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(raw.trim()).map_err(|_| "Invalid JSON from Claude".to_string())?;
    if let Some(err) = v.pointer("/error/message").and_then(|m| m.as_str()) {
        return Err(err.to_string());
    }
    if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
        let mut texts = Vec::new();
        for block in arr {
            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                texts.push(t);
            }
        }
        if !texts.is_empty() {
            return Ok(texts.join(""));
        }
    }
    Err("Empty response from Claude".into())
}

/// Parse a Google Gemini `generateContent` response.
pub fn parse_gemini_response(raw: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(raw.trim()).map_err(|_| "Invalid JSON from Gemini".to_string())?;
    if let Some(err) = v.pointer("/error/message").and_then(|m| m.as_str()) {
        return Err(err.to_string());
    }
    if let Some(parts) = v.pointer("/candidates/0/content/parts").and_then(|p| p.as_array()) {
        let mut out = String::new();
        for p in parts {
            if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    Err("Empty response from Gemini".into())
}

/// Parse `/codex/models` JSON response into a sorted list of visible model slugs.
pub fn parse_codex_models(raw: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(raw) else { return Vec::new() };
    let Some(models) = v.get("models").and_then(|m| m.as_array()) else { return Vec::new() };
    let mut ranked: Vec<(i64, String)> = Vec::new();
    for entry in models {
        let Some(slug) = entry.get("slug").and_then(|s| s.as_str()).map(str::trim) else { continue };
        if slug.is_empty() {
            continue;
        }
        let vis = entry.get("visibility").and_then(|v| v.as_str()).unwrap_or("");
        if vis.eq_ignore_ascii_case("hide") || vis.eq_ignore_ascii_case("hidden") {
            continue;
        }
        let prio = entry.get("priority").and_then(|p| p.as_i64()).unwrap_or(999);
        ranked.push((prio, slug.to_string()));
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut seen = Vec::new();
    for (_, slug) in ranked {
        if !seen.contains(&slug) {
            seen.push(slug);
        }
    }
    seen
}

/// Parse OpenAI / OpenCode Go `/v1/models` JSON response (`{"data": [{"id": ...}]}`).
pub fn parse_openai_models_list(raw: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(raw) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(arr) = v.get("data").or_else(|| v.get("models")).and_then(|d| d.as_array()) {
        for item in arr {
            let id = item
                .get("id")
                .or_else(|| item.get("name"))
                .and_then(|s| s.as_str())
                .map(str::trim)
                .unwrap_or("");
            if !id.is_empty() && !out.iter().any(|x| x == id) {
                out.push(id.to_string());
            }
        }
    }
    out
}

// ------------------------------------------------ Action Extraction & Prompt

/// Strip `[ACTION:...]` tags from the assistant's reply and return `(clean_text, directives)`.
pub fn extract_directives(raw: &str) -> (String, Vec<ParsedDirective>) {
    let mut clean = String::with_capacity(raw.len());
    let mut directives = Vec::new();
    let mut rest = raw;

    while let Some(start) = rest.find("[ACTION:") {
        clean.push_str(&rest[..start]);
        let after_start = &rest[start + 8..];
        if let Some(end) = after_start.find(']') {
            let inner = after_start[..end].trim();
            if let Some((kind, arg)) = inner.split_once(':') {
                let kind = kind.trim().to_ascii_uppercase();
                let arg = arg.trim();
                match kind.as_str() {
                    "ADD_TASK" if !arg.is_empty() => directives.push(ParsedDirective::AddTask(arg.to_string())),
                    "DONE_TASK" if !arg.is_empty() => directives.push(ParsedDirective::DoneTask(arg.to_string())),
                    "DEL_TASK" if !arg.is_empty() => directives.push(ParsedDirective::DelTask(arg.to_string())),
                    "WATER" => {
                        let ml = arg.trim_end_matches("ml").trim().parse::<u32>().unwrap_or(250).clamp(50, 2000);
                        directives.push(ParsedDirective::LogWater(ml));
                    }
                    "FOCUS" if !arg.is_empty() => directives.push(ParsedDirective::Focus(arg.to_ascii_lowercase())),
                    "REMEMBER" if !arg.is_empty() => directives.push(ParsedDirective::Remember(arg.to_string())),
                    _ => {}
                }
            }
            rest = &after_start[end + 1..];
        } else {
            clean.push_str(&rest[start..]);
            rest = "";
            break;
        }
    }
    clean.push_str(rest);
    let cleaned = clean
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    (cleaned, directives)
}

pub fn build_system_prompt(day_context: Option<&str>, memories: &[String]) -> String {
    let mut prompt = String::from(
        "You are Teardrop AI, a concise, friendly desktop companion living inside the Teardrop island at the top of the user's Windows screen.\n\
         Keep replies clear, warm, and compact (2-5 sentences or short bullet points) so they fit comfortably in a compact notch card.\n\n\
         When the user asks you to perform an action in Teardrop, include one or more action tags at the very end of your response:\n\
         - Add a task: [ACTION:ADD_TASK:Task title]\n\
         - Complete a task: [ACTION:DONE_TASK:Task title]\n\
         - Delete a task: [ACTION:DEL_TASK:Task title]\n\
         - Log water intake (in ml): [ACTION:WATER:250]\n\
         - Control Pomodoro focus timer: [ACTION:FOCUS:start] or [ACTION:FOCUS:pause] or [ACTION:FOCUS:reset]\n\
         - Save a durable user preference, habit, or personal fact for future chats: [ACTION:REMEMBER:concise fact]\n\
         Only emit [ACTION:...] tags when the user's request calls for an action or shares a lasting preference.",
    );

    if !memories.is_empty() {
        prompt.push_str("\n\nLearned user memories & preferences:\n");
        for m in memories {
            prompt.push_str("- ");
            prompt.push_str(m);
            prompt.push('\n');
        }
    }

    if let Some(ctx) = day_context.filter(|s| !s.trim().is_empty()) {
        prompt.push_str("\n\nCurrent user day context:\n");
        prompt.push_str(ctx);
    }

    prompt
}

// --------------------------------------------------- Runtime State & Worker

struct Runtime {
    store: ChatStore,
    secrets: SecretStore,
    view: AiView,
    pending_directives: Vec<(u64, usize, Vec<ParsedDirective>)>,
}

static STATE: Mutex<Option<Runtime>> = Mutex::new(None);
static REVISION: AtomicU32 = AtomicU32::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);
static LOGIN_BUSY: AtomicBool = AtomicBool::new(false);
static LOGIN_CANCEL: AtomicBool = AtomicBool::new(false);

fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    let mut guard = STATE.lock().unwrap();
    let rt = guard.get_or_insert_with(|| {
        let store = load_chat_store();
        let mut secrets = load_secrets();
        // If ChatGPT tokens aren't stored yet, check if Codex CLI (`~/.codex/auth.json`) has them.
        try_import_codex_cli_tokens(&mut secrets);
        let mut rt = Runtime {
            store,
            secrets,
            view: AiView::default(),
            pending_directives: Vec::new(),
        };
        rt.rebuild_view(AiProvider::ChatGpt);
        rt
    });
    let out = f(rt);
    REVISION.fetch_add(1, Ordering::Release);
    out
}

impl Runtime {
    fn active_conv_mut(&mut self) -> &mut Conversation {
        if !self.store.conversations.iter().any(|c| c.id == self.store.active_id) {
            if self.store.conversations.is_empty() {
                let id = now_millis();
                self.store.conversations.push(Conversation {
                    id,
                    title: "New chat".into(),
                    updated_at: (id / 1000) as i64,
                    messages: Vec::new(),
                });
            }
            self.store.active_id = self.store.conversations[0].id;
        }
        let id = self.store.active_id;
        self.store.conversations.iter_mut().find(|c| c.id == id).unwrap()
    }

    fn has_stored_key(&self, provider: AiProvider) -> bool {
        match provider {
            AiProvider::ChatGpt => !decrypt_secret(&self.secrets.chatgpt_access_token).is_empty(),
            AiProvider::OpenCodeGo => !decrypt_secret(&self.secrets.opencode_key).is_empty(),
            AiProvider::OpenAiCompat => !decrypt_secret(&self.secrets.openai_key).is_empty(),
            AiProvider::Gemini => !decrypt_secret(&self.secrets.gemini_key).is_empty(),
            AiProvider::Claude => !decrypt_secret(&self.secrets.claude_key).is_empty(),
            AiProvider::Ollama => true,
        }
    }

    fn rebuild_view(&mut self, provider: AiProvider) {
        let conv = self.active_conv_mut().clone();
        self.view.active_id = conv.id;
        self.view.title = conv.title;
        self.view.messages = conv.messages;
        self.view.memories_count = self.store.memories.len();
        self.view.history = self
            .store
            .conversations
            .iter()
            .map(|c| ChatSummary {
                id: c.id,
                title: c.title.clone(),
                msg_count: c.messages.len(),
            })
            .collect();
        self.view.chatgpt_signed_in = !decrypt_secret(&self.secrets.chatgpt_access_token).is_empty();
        self.view.configured = self.has_stored_key(provider);
        if self.view.available_models.is_empty() {
            self.view.available_models = default_models_for(provider);
        }
    }
}

pub fn default_models_for(provider: AiProvider) -> Vec<String> {
    match provider {
        AiProvider::ChatGpt => CODEX_DEFAULT_MODELS.iter().map(|s| (*s).to_string()).collect(),
        AiProvider::OpenCodeGo => OPENCODE_DEFAULT_MODELS.iter().map(|s| (*s).to_string()).collect(),
        AiProvider::OpenAiCompat => vec![
            "gpt-4o-mini".into(),
            "gpt-4.1-mini".into(),
            "gpt-4o".into(),
            "deepseek-chat".into(),
        ],
        AiProvider::Gemini => vec![
            "gemini-3.5-flash-lite".into(),
            "gemini-2.5-flash".into(),
            "gemini-2.5-pro".into(),
        ],
        AiProvider::Claude => vec![
            "claude-opus-5-5".into(),
            "claude-sonnet-4-6".into(),
            "claude-haiku-4-5".into(),
        ],
        AiProvider::Ollama => vec![
            "llama3.2".into(),
            "qwen2.5-coder".into(),
            "deepseek-r1".into(),
            "mistral".into(),
        ],
    }
}

/// Optional seamless bootstrap: if the user already authenticated Codex CLI
/// (`~/.codex/auth.json`), import and DPAPI-encrypt the tokens whenTeardrop has none.
fn try_import_codex_cli_tokens(sec: &mut SecretStore) {
    if !sec.chatgpt_access_token.is_empty() {
        return;
    }
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".codex")));
    let Some(auth_path) = home.map(|d| d.join("auth.json")).filter(|p| p.exists()) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(auth_path) else { return };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return };
    let tokens = v.get("tokens").unwrap_or(&v);
    let access = tokens.get("access_token").and_then(|s| s.as_str()).unwrap_or("");
    let refresh = tokens.get("refresh_token").and_then(|s| s.as_str()).unwrap_or("");
    if !access.is_empty() {
        sec.chatgpt_access_token = encrypt_secret(access);
        if !refresh.is_empty() {
            sec.chatgpt_refresh_token = encrypt_secret(refresh);
        }
        sec.chatgpt_expires_at = extract_jwt_expiry(access);
        let acct = tokens
            .get("account_id")
            .and_then(|s| s.as_str())
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_chatgpt_account_id(access));
        sec.chatgpt_account_id = acct;
        save_secrets(sec);
    }
}

pub fn revision() -> u32 {
    REVISION.load(Ordering::Acquire)
}

pub fn snapshot(provider: AiProvider) -> AiView {
    with_runtime(|rt| {
        rt.rebuild_view(provider);
        rt.view.clone()
    })
}

pub fn set_provider(provider: AiProvider) {
    with_runtime(|rt| {
        rt.view.available_models = default_models_for(provider);
        rt.view.error.clear();
        rt.view.status_note.clear();
        rt.rebuild_view(provider);
    });
}

pub fn toggle_history() {
    with_runtime(|rt| {
        rt.view.show_history = !rt.view.show_history;
        if rt.view.show_history {
            rt.view.show_setup = false;
        }
        rt.view.scroll = 0;
    });
}

pub fn toggle_setup(provider: AiProvider) {
    with_runtime(|rt| {
        rt.view.show_setup = !rt.view.show_setup;
        if rt.view.show_setup {
            rt.view.show_history = false;
        }
        rt.rebuild_view(provider);
    });
}

pub fn set_focus_field(field: AiFocusField) {
    with_runtime(|rt| {
        rt.view.focus_field = field;
    });
}

pub fn scroll_by(delta: i32) {
    with_runtime(|rt| {
        rt.view.scroll = (rt.view.scroll + delta).max(0);
    });
}

pub fn input_set(text: String) {
    with_runtime(|rt| {
        rt.view.input = text.chars().take(600).collect();
    });
}

pub fn new_chat(provider: AiProvider) {
    with_runtime(|rt| {
        // If current active chat is already empty, just switch to it.
        if let Some(existing) = rt.store.conversations.iter().find(|c| c.messages.is_empty()) {
            rt.store.active_id = existing.id;
        } else {
            let id = now_millis();
            rt.store.conversations.insert(
                0,
                Conversation {
                    id,
                    title: "New chat".into(),
                    updated_at: now_secs(),
                    messages: Vec::new(),
                },
            );
            if rt.store.conversations.len() > MAX_CHATS {
                rt.store.conversations.truncate(MAX_CHATS);
            }
            rt.store.active_id = id;
            save_chat_store(&rt.store);
        }
        rt.view.show_history = false;
        rt.view.show_setup = false;
        rt.view.error.clear();
        rt.view.scroll = 0;
        rt.rebuild_view(provider);
    });
}

pub fn select_chat(id: u64, provider: AiProvider) {
    with_runtime(|rt| {
        if rt.store.conversations.iter().any(|c| c.id == id) {
            rt.store.active_id = id;
            save_chat_store(&rt.store);
        }
        rt.view.show_history = false;
        rt.view.error.clear();
        rt.view.scroll = 0;
        rt.rebuild_view(provider);
    });
}

pub fn delete_chat(id: u64, provider: AiProvider) {
    with_runtime(|rt| {
        rt.store.conversations.retain(|c| c.id != id);
        if rt.store.conversations.is_empty() {
            let new_id = now_millis();
            rt.store.conversations.push(Conversation {
                id: new_id,
                title: "New chat".into(),
                updated_at: now_secs(),
                messages: Vec::new(),
            });
            rt.store.active_id = new_id;
        } else if !rt.store.conversations.iter().any(|c| c.id == rt.store.active_id) {
            rt.store.active_id = rt.store.conversations[0].id;
        }
        save_chat_store(&rt.store);
        rt.rebuild_view(provider);
    });
}

/// Save an API key for `provider` (DPAPI encrypted on disk).
pub fn save_api_key(provider: AiProvider, raw_key: &str) {
    let trimmed = raw_key.trim();
    with_runtime(|rt| {
        let enc = encrypt_secret(trimmed);
        match provider {
            AiProvider::OpenCodeGo => rt.secrets.opencode_key = enc,
            AiProvider::OpenAiCompat => rt.secrets.openai_key = enc,
            AiProvider::Gemini => rt.secrets.gemini_key = enc,
            AiProvider::Claude => rt.secrets.claude_key = enc,
            _ => {}
        }
        save_secrets(&rt.secrets);
        rt.view.error.clear();
        rt.view.status_note = if trimmed.is_empty() {
            "API key cleared".into()
        } else {
            format!("{} key saved (DPAPI encrypted)", provider.label())
        };
        rt.rebuild_view(provider);
    });
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
        let resp = crate::http::post(
            &usercode_url,
            &[("Content-Type", "application/json"), ("Accept", "application/json")],
            body.as_bytes(),
            64 * 1024,
        );
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
        let user_code = v
            .get("user_code")
            .or_else(|| v.get("usercode"))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let device_auth_id = v
            .get("device_auth_id")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let interval: u64 = v
            .get("interval")
            .and_then(|i| i.as_u64().or_else(|| i.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(5)
            .max(3);

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
            match crate::http::post(
                &poll_url,
                &[("Content-Type", "application/json"), ("Accept", "application/json")],
                poll_body.as_bytes(),
                64 * 1024,
            ) {
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
                    match crate::http::post(
                        &token_url,
                        &[("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json")],
                        form.as_bytes(),
                        64 * 1024,
                    ) {
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
        let _ = ShellExecuteW(
            None,
            PCWSTR::from_raw(op.as_ptr()),
            PCWSTR::from_raw(wurl.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(not(windows))]
pub fn open_browser(_url: &str) {}

/// Ensure the ChatGPT OAuth access token is fresh (proactively refreshes within 120s of expiry).
fn ensure_fresh_chatgpt_token(sec: &mut SecretStore) -> Result<(String, String), String> {
    let mut access = decrypt_secret(&sec.chatgpt_access_token);
    if access.is_empty() {
        try_import_codex_cli_tokens(sec);
        access = decrypt_secret(&sec.chatgpt_access_token);
    }
    if access.is_empty() {
        return Err("Not signed in \u{2014} click Setup \u{2192} Sign in with ChatGPT".into());
    }
    let now = now_secs();
    if sec.chatgpt_expires_at > 0 && now >= sec.chatgpt_expires_at - 120 {
        let refresh = decrypt_secret(&sec.chatgpt_refresh_token);
        if !refresh.is_empty() {
            let token_url = format!("{CHATGPT_ISSUER}/oauth/token");
            let form = form_urlencode(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &refresh),
                ("client_id", CHATGPT_CLIENT_ID),
            ]);
            if let Ok(bytes) = crate::http::post(
                &token_url,
                &[("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json")],
                form.as_bytes(),
                64 * 1024,
            ) {
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
    let acct = if sec.chatgpt_account_id.is_empty() {
        extract_chatgpt_account_id(&access)
    } else {
        sec.chatgpt_account_id.clone()
    };
    Ok((access, acct))
}

/// Fetch live models for the active provider on a background thread.
pub fn refresh_models(provider: AiProvider, base_url: String) {
    with_runtime(|rt| {
        rt.view.models_loading = true;
        rt.view.status_note = format!("Fetching {} models\u{2026}", provider.label());
    });
    std::thread::spawn(move || {
        let mut sec = load_secrets();
        let fetched: Vec<String> = match provider {
            AiProvider::ChatGpt => {
                if let Ok((token, acct)) = ensure_fresh_chatgpt_token(&mut sec) {
                    let url = format!("{CODEX_BASE_URL}/models?client_version=1.0.0");
                    let auth = format!("Bearer {token}");
                    let mut hdrs = vec![("Authorization", auth.as_str()), ("Accept", "application/json")];
                    if !acct.is_empty() {
                        hdrs.push(("ChatGPT-Account-Id", acct.as_str()));
                    }
                    crate::http::get(&url, &hdrs, 256 * 1024)
                        .ok()
                        .map(|b| parse_codex_models(&String::from_utf8_lossy(&b)))
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            AiProvider::OpenCodeGo => {
                let key = decrypt_secret(&sec.opencode_key);
                let url = format!("{OPENCODE_GO_BASE_URL}/models");
                let auth = format!("Bearer {key}");
                let mut hdrs = vec![
                    ("Accept", "application/json"),
                    ("x-opencode-session", "teardrop-models"),
                ];
                if !key.is_empty() {
                    hdrs.push(("Authorization", auth.as_str()));
                }
                crate::http::get(&url, &hdrs, 256 * 1024)
                    .ok()
                    .map(|b| parse_openai_models_list(&String::from_utf8_lossy(&b)))
                    .unwrap_or_default()
            }
            AiProvider::OpenAiCompat => {
                let key = decrypt_secret(&sec.openai_key);
                let base = normalize_openai_base_url(&base_url);
                let url = format!("{base}/models");
                let auth = format!("Bearer {key}");
                let mut hdrs = vec![("Accept", "application/json")];
                if !key.is_empty() {
                    hdrs.push(("Authorization", auth.as_str()));
                }
                crate::http::get(&url, &hdrs, 256 * 1024)
                    .ok()
                    .map(|b| parse_openai_models_list(&String::from_utf8_lossy(&b)))
                    .unwrap_or_default()
            }
            _ => default_models_for(provider),
        };

        with_runtime(|rt| {
            rt.view.models_loading = false;
            if !fetched.is_empty() {
                let count = fetched.len();
                rt.view.available_models = fetched;
                rt.view.status_note = format!("Loaded {count} models");
            } else {
                rt.view.available_models = default_models_for(provider);
                rt.view.status_note = "Using default model catalog".into();
            }
        });
    });
}

/// Send a user prompt (either typed or clicked from starter suggestions).
pub fn send_prompt(prompt: String, settings: Settings, day_context: Option<String>) {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() || BUSY.swap(true, Ordering::SeqCst) {
        return;
    }

    let provider = settings.ai_provider;
    let model = settings.active_ai_model().to_string();
    let base_url = settings.active_ai_url().to_string();

    let (conv_id, history, memories) = with_runtime(|rt| {
        rt.view.input.clear();
        rt.view.error.clear();
        rt.view.status_note.clear();
        rt.view.show_history = false;
        rt.view.show_setup = false;
        rt.view.busy = true;
        rt.view.scroll = 0;

        let conv = rt.active_conv_mut();
        if conv.messages.is_empty() {
            conv.title = prompt.chars().take(36).collect();
        }
        conv.updated_at = now_secs();
        conv.messages.push(ChatMessage {
            role: Role::User,
            text: prompt.clone(),
            actions: Vec::new(),
            timestamp: now_secs(),
        });
        let conv_id = conv.id;
        let history = conv.messages.clone();
        let memories = rt.store.memories.clone();
        save_chat_store(&rt.store);
        rt.rebuild_view(provider);
        (conv_id, history, memories)
    });

    std::thread::spawn(move || {
        let mut sec = load_secrets();
        let sys_prompt = build_system_prompt(day_context.as_deref(), &memories);
        let result = call_provider(provider, &model, &base_url, &mut sec, conv_id, &sys_prompt, &history);

        match result {
            Ok(raw_reply) => {
                let (clean_text, directives) = extract_directives(&raw_reply);
                let display_text = if clean_text.is_empty() {
                    "Done!".to_string()
                } else {
                    clean_text
                };
                with_runtime(|rt| {
                    rt.view.busy = false;
                    if let Some(conv) = rt.store.conversations.iter_mut().find(|c| c.id == conv_id) {
                        let msg_idx = conv.messages.len();
                        conv.messages.push(ChatMessage {
                            role: Role::Assistant,
                            text: display_text,
                            actions: Vec::new(),
                            timestamp: now_secs(),
                        });
                        conv.updated_at = now_secs();
                        if !directives.is_empty() {
                            rt.pending_directives.push((conv_id, msg_idx, directives));
                        }
                    }
                    save_chat_store(&rt.store);
                    rt.rebuild_view(provider);
                });
            }
            Err(err) => {
                with_runtime(|rt| {
                    rt.view.busy = false;
                    rt.view.error = err;
                    rt.rebuild_view(provider);
                });
            }
        }
        BUSY.store(false, Ordering::SeqCst);
    });
}

fn call_provider(
    provider: AiProvider,
    model: &str,
    base_url: &str,
    sec: &mut SecretStore,
    conv_id: u64,
    sys_prompt: &str,
    history: &[ChatMessage],
) -> Result<String, String> {
    // Keep up to the last 16 messages for context window efficiency.
    let start = history.len().saturating_sub(16);
    let recent = &history[start..];

    match provider {
        AiProvider::ChatGpt => {
            let (token, acct) = ensure_fresh_chatgpt_token(sec)?;
            call_responses_endpoint(
                &format!("{CODEX_BASE_URL}/responses"),
                &token,
                Some(&acct),
                model,
                sys_prompt,
                recent,
                &format!("teardrop-{conv_id}"),
                false,
            )
        }
        AiProvider::OpenCodeGo => {
            let key = decrypt_secret(&sec.opencode_key);
            if key.is_empty() {
                return Err("OpenCode Go API key not set \u{2014} click Setup \u{2192} Paste Key".into());
            }
            let session = format!("teardrop-{conv_id}");
            match opencode_model_protocol(model) {
                OpenCodeProtocol::Responses => call_responses_endpoint(
                    &format!("{OPENCODE_GO_BASE_URL}/responses"),
                    &key,
                    None,
                    model,
                    sys_prompt,
                    recent,
                    &session,
                    true,
                ),
                OpenCodeProtocol::Messages => call_anthropic_endpoint(
                    &format!("{OPENCODE_GO_BASE_URL}/messages"),
                    &key,
                    model,
                    sys_prompt,
                    recent,
                    Some(&session),
                ),
                OpenCodeProtocol::Chat => call_openai_chat_endpoint(
                    &format!("{OPENCODE_GO_BASE_URL}/chat/completions"),
                    &key,
                    model,
                    sys_prompt,
                    recent,
                    Some(&session),
                ),
            }
        }
        AiProvider::OpenAiCompat => {
            let key = decrypt_secret(&sec.openai_key);
            let base = normalize_openai_base_url(base_url);
            if base.contains("api.openai.com") && key.is_empty() {
                return Err("OpenAI API key not set \u{2014} click Setup \u{2192} Paste Key".into());
            }
            call_openai_chat_endpoint(
                &format!("{base}/chat/completions"),
                &key,
                model,
                sys_prompt,
                recent,
                None,
            )
        }
        AiProvider::Gemini => {
            let key = decrypt_secret(&sec.gemini_key);
            if key.is_empty() {
                return Err("Gemini API key not set \u{2014} click Setup \u{2192} Paste Key".into());
            }
            call_gemini_endpoint(&key, model, sys_prompt, recent)
        }
        AiProvider::Claude => {
            let key = decrypt_secret(&sec.claude_key);
            if key.is_empty() {
                return Err("Claude API key not set \u{2014} click Setup \u{2192} Paste Key".into());
            }
            call_anthropic_endpoint(
                "https://api.anthropic.com/v1/messages",
                &key,
                model,
                sys_prompt,
                recent,
                None,
            )
        }
        AiProvider::Ollama => {
            let base = base_url.trim().trim_end_matches('/');
            let base = if base.is_empty() { "http://localhost:11434" } else { base };
            call_ollama_endpoint(base, model, sys_prompt, recent)
        }
    }
}

fn call_responses_endpoint(
    url: &str,
    bearer: &str,
    account_id: Option<&str>,
    model: &str,
    sys_prompt: &str,
    messages: &[ChatMessage],
    session_id: &str,
    is_opencode: bool,
) -> Result<String, String> {
    let input: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                "content": m.text,
            })
        })
        .collect();
    let payload = json!({
        "model": model,
        "instructions": sys_prompt,
        "input": input,
        "store": false,
        "stream": true,
    })
    .to_string();

    let auth = format!("Bearer {bearer}");
    let mut headers = vec![
        ("Authorization", auth.as_str()),
        ("Content-Type", "application/json"),
        ("Accept", "text/event-stream"),
    ];
    if let Some(acct) = account_id.filter(|a| !a.is_empty()) {
        headers.push(("ChatGPT-Account-Id", acct));
    }
    if is_opencode {
        headers.push(("x-opencode-session", session_id));
    } else {
        headers.push(("session-id", session_id));
    }

    match crate::http::post(url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_responses_sse_or_json(&String::from_utf8_lossy(&bytes)),
        Err((status, snippet)) => Err(friendly_http_error("Responses API", status, &snippet)),
    }
}

fn call_openai_chat_endpoint(
    url: &str,
    api_key: &str,
    model: &str,
    sys_prompt: &str,
    messages: &[ChatMessage],
    opencode_session: Option<&str>,
) -> Result<String, String> {
    let mut wire = Vec::with_capacity(messages.len() + 1);
    wire.push(json!({ "role": "system", "content": sys_prompt }));
    for m in messages {
        wire.push(json!({
            "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
            "content": m.text,
        }));
    }
    let payload = json!({
        "model": model,
        "messages": wire,
        "temperature": 0.4,
    })
    .to_string();

    let auth = format!("Bearer {api_key}");
    let mut headers = vec![
        ("Content-Type", "application/json"),
        ("Accept", "application/json"),
    ];
    if !api_key.is_empty() {
        headers.push(("Authorization", auth.as_str()));
    }
    if let Some(sess) = opencode_session {
        headers.push(("x-opencode-session", sess));
    }

    match crate::http::post(url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_openai_chat_response(&String::from_utf8_lossy(&bytes)),
        Err((status, snippet)) => Err(friendly_http_error("Chat API", status, &snippet)),
    }
}

fn call_anthropic_endpoint(
    url: &str,
    api_key: &str,
    model: &str,
    sys_prompt: &str,
    messages: &[ChatMessage],
    opencode_session: Option<&str>,
) -> Result<String, String> {
    let wire: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                "content": m.text,
            })
        })
        .collect();
    let payload = json!({
        "model": model,
        "max_tokens": 1024,
        "system": sys_prompt,
        "messages": wire,
    })
    .to_string();

    let auth = format!("Bearer {api_key}");
    let mut headers = vec![
        ("Content-Type", "application/json"),
        ("Accept", "application/json"),
        ("anthropic-version", "2023-06-01"),
        ("x-api-key", api_key),
    ];
    if let Some(sess) = opencode_session {
        headers.push(("Authorization", auth.as_str()));
        headers.push(("x-opencode-session", sess));
    }

    match crate::http::post(url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_anthropic_response(&String::from_utf8_lossy(&bytes)),
        Err((status, snippet)) => Err(friendly_http_error("Claude", status, &snippet)),
    }
}

fn call_gemini_endpoint(
    api_key: &str,
    model: &str,
    sys_prompt: &str,
    messages: &[ChatMessage],
) -> Result<String, String> {
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let contents: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "model" },
                "parts": [{ "text": m.text }],
            })
        })
        .collect();
    let payload = json!({
        "systemInstruction": { "parts": [{ "text": sys_prompt }] },
        "contents": contents,
    })
    .to_string();

    let headers = [
        ("Content-Type", "application/json"),
        ("Accept", "application/json"),
        ("x-goog-api-key", api_key),
    ];
    match crate::http::post(&url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_gemini_response(&String::from_utf8_lossy(&bytes)),
        Err((status, snippet)) => Err(friendly_http_error("Gemini", status, &snippet)),
    }
}

fn call_ollama_endpoint(
    base_url: &str,
    model: &str,
    sys_prompt: &str,
    messages: &[ChatMessage],
) -> Result<String, String> {
    let url = format!("{base_url}/api/chat");
    let mut wire = Vec::with_capacity(messages.len() + 1);
    wire.push(json!({ "role": "system", "content": sys_prompt }));
    for m in messages {
        wire.push(json!({
            "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
            "content": m.text,
        }));
    }
    let payload = json!({
        "model": model,
        "messages": wire,
        "stream": false,
    })
    .to_string();

    let headers = [("Content-Type", "application/json"), ("Accept", "application/json")];
    match crate::http::post(&url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_openai_chat_response(&String::from_utf8_lossy(&bytes)),
        Err((0, _)) => Err("Could not reach Ollama \u{2014} is `ollama serve` running?".into()),
        Err((status, snippet)) => Err(friendly_http_error("Ollama", status, &snippet)),
    }
}

fn friendly_http_error(label: &str, status: u32, snippet: &str) -> String {
    if status == 0 {
        return format!("{label}: network error \u{2014} check connection or URL");
    }
    if let Ok(v) = serde_json::from_str::<Value>(snippet) {
        if let Some(msg) = v
            .pointer("/error/message")
            .or_else(|| v.pointer("/detail"))
            .or_else(|| v.get("message"))
            .and_then(|m| m.as_str())
        {
            let clean: String = msg.chars().take(110).collect();
            return format!("{label} ({status}): {clean}");
        }
    }
    match status {
        401 | 403 => format!("{label} ({status}): unauthorized \u{2014} check API key or sign-in"),
        404 => format!("{label} (404): model or endpoint not found"),
        429 => format!("{label} (429): rate limit reached \u{2014} wait a moment"),
        _ => format!("{label}: HTTP {status}"),
    }
}

// ----------------------------------------- Draining & Undoing App Mutations

/// Drain any newly parsed `[ACTION:...]` directives so `window.rs` can apply them
/// to `AppState` and record their `AiActionBadge`s.
pub fn take_pending_directives() -> Vec<(u64, usize, Vec<ParsedDirective>)> {
    with_runtime(|rt| std::mem::take(&mut rt.pending_directives))
}

/// Attach executed `AiActionBadge`s to a message and persist `chats.json`.
pub fn record_action_badges(
    conv_id: u64,
    msg_idx: usize,
    badges: Vec<AiActionBadge>,
    new_memories: Vec<String>,
    provider: AiProvider,
) {
    with_runtime(|rt| {
        for mem in new_memories {
            if !rt.store.memories.iter().any(|m| m.eq_ignore_ascii_case(&mem)) {
                rt.store.memories.push(mem);
                if rt.store.memories.len() > MAX_MEMORIES {
                    rt.store.memories.remove(0);
                }
            }
        }
        if let Some(conv) = rt.store.conversations.iter_mut().find(|c| c.id == conv_id) {
            if let Some(msg) = conv.messages.get_mut(msg_idx) {
                msg.actions.extend(badges);
            }
        }
        save_chat_store(&rt.store);
        rt.rebuild_view(provider);
    });
}

/// Mark an action badge on the active conversation as undone and return its `UndoOp`.
pub fn mark_action_undone(msg_idx: usize, action_idx: usize, provider: AiProvider) -> Option<UndoOp> {
    with_runtime(|rt| {
        let active_id = rt.store.active_id;
        let mut op = None;
        if let Some(conv) = rt.store.conversations.iter_mut().find(|c| c.id == active_id) {
            if let Some(msg) = conv.messages.get_mut(msg_idx) {
                if let Some(act) = msg.actions.get_mut(action_idx) {
                    if act.can_undo() {
                        act.undone = true;
                        op = Some(act.undo.clone());
                    }
                }
            }
        }
        if let Some(UndoOp::ForgetMemory(ref fact)) = op {
            rt.store.memories.retain(|m| m != fact);
        }
        if op.is_some() {
            save_chat_store(&rt.store);
            rt.rebuild_view(provider);
        }
        op
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_roundtrips_secrets() {
        let secret = "sk-test-secret-key-987654321";
        let enc = encrypt_secret(secret);
        assert!(!enc.contains(secret));
        assert!(enc.starts_with("dpapi:") || enc.starts_with("hex:"));
        assert_eq!(decrypt_secret(&enc), secret);
        assert_eq!(encrypt_secret(""), "");
        assert_eq!(decrypt_secret(""), "");
    }

    #[test]
    fn jwt_claims_extraction_matches_codex_spec() {
        // Construct a minimal JWT with `exp` and `https://api.openai.com/auth.chatgpt_account_id`.
        // Header: `{"alg":"none"}` -> `eyJhbGciOiJub25lIn0`
        // Payload: `{"exp":1893456000,"https://api.openai.com/auth":{"chatgpt_account_id":"acct_codex_42"}}`
        // Base64url: `eyJleHAiOjE4OTM0NTYwMDAsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2N0X2NvZGV4XzQyIn19`
        let jwt = "eyJhbGciOiJub25lIn0.eyJleHAiOjE4OTM0NTYwMDAsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2N0X2NvZGV4XzQyIn19.sig";
        assert_eq!(extract_jwt_expiry(jwt), 1_893_456_000);
        assert_eq!(extract_chatgpt_account_id(jwt), "acct_codex_42");
        assert_eq!(extract_jwt_expiry("not-a-jwt"), 0);
        assert_eq!(extract_chatgpt_account_id("not-a-jwt"), "");
    }

    #[test]
    fn opencode_go_protocol_routing_matches_tomo() {
        assert_eq!(opencode_model_protocol("gpt-5.4"), OpenCodeProtocol::Responses);
        assert_eq!(opencode_model_protocol("grok-3"), OpenCodeProtocol::Responses);
        assert_eq!(opencode_model_protocol("muse-spark"), OpenCodeProtocol::Responses);
        assert_eq!(opencode_model_protocol("claude-sonnet-4-6"), OpenCodeProtocol::Messages);
        assert_eq!(opencode_model_protocol("qwen3-coder"), OpenCodeProtocol::Messages);
        assert_eq!(opencode_model_protocol("minimax-m2.5"), OpenCodeProtocol::Messages);
        assert_eq!(opencode_model_protocol("deepseek-r1"), OpenCodeProtocol::Chat);
        assert_eq!(opencode_model_protocol("glm-5"), OpenCodeProtocol::Chat);
    }

    #[test]
    fn responses_sse_and_json_parsing() {
        let sse = "event: response.output_text.delta\n\
                   data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello \"}\n\n\
                   event: response.output_text.delta\n\
                   data: {\"type\":\"response.output_text.delta\",\"delta\":\"world!\"}\n\n\
                   data: [DONE]\n";
        assert_eq!(parse_responses_sse_or_json(sse).unwrap(), "Hello world!");

        let done_only = "data: {\"type\":\"response.output_item.done\",\"item\":{\"content\":[{\"text\":\"Fallback text\"}]}}\n";
        assert_eq!(parse_responses_sse_or_json(done_only).unwrap(), "Fallback text");
    }

    #[test]
    fn openai_and_anthropic_and_gemini_parsers() {
        let oai = r#"{"choices":[{"message":{"content":"Hi from OpenAI-compat"}}]}"#;
        assert_eq!(parse_openai_chat_response(oai).unwrap(), "Hi from OpenAI-compat");

        let claude = r#"{"content":[{"type":"text","text":"Hi from Claude"}]}"#;
        assert_eq!(parse_anthropic_response(claude).unwrap(), "Hi from Claude");

        let gemini = r#"{"candidates":[{"content":{"parts":[{"text":"Hi from Gemini"}]}}]}"#;
        assert_eq!(parse_gemini_response(gemini).unwrap(), "Hi from Gemini");
    }

    #[test]
    fn codex_model_catalog_filters_hidden_and_sorts_by_priority() {
        let body = r#"{"models":[
            {"slug":"gpt-5.4","priority":2,"visibility":"list"},
            {"slug":"secret-internal","priority":1,"visibility":"hidden"},
            {"slug":"gpt-5.3-codex","priority":1,"visibility":"list"}
        ]}"#;
        assert_eq!(parse_codex_models(body), vec!["gpt-5.3-codex", "gpt-5.4"]);
    }

    #[test]
    fn directive_extraction_strips_tags_and_parses_actions() {
        let reply = "I've added that task and logged a glass of water for you!\n\
                     [ACTION:ADD_TASK:Write unit tests]\n\
                     [ACTION:WATER:250]\n\
                     [ACTION:REMEMBER:Prefers Rust over Python]";
        let (clean, dirs) = extract_directives(reply);
        assert_eq!(clean, "I've added that task and logged a glass of water for you!");
        assert_eq!(
            dirs,
            vec![
                ParsedDirective::AddTask("Write unit tests".into()),
                ParsedDirective::LogWater(250),
                ParsedDirective::Remember("Prefers Rust over Python".into()),
            ]
        );
    }

    #[test]
    fn base_url_normalization_handles_shortcuts() {
        assert_eq!(normalize_openai_base_url(""), "https://api.openai.com/v1");
        assert_eq!(normalize_openai_base_url("https://api.openai.com/v1/"), "https://api.openai.com/v1");
        assert_eq!(
            normalize_openai_base_url("https://api.groq.com/openai/v1/chat/completions"),
            "https://api.groq.com/openai/v1"
        );
        assert_eq!(normalize_openai_base_url("http://localhost:1234/v1"), "http://localhost:1234/v1");
    }
}
