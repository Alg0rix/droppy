//! AI Chat: multi-provider assistant with day context, reversible Droppy
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
//!   (`CryptProtectData` / `CryptUnprotectData`) in `%APPDATA%\Droppy\ai-secrets.json`.
//! * Secrets are never copied into `ViewModel`, `AiView`, logs, or error strings.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use teardrop_core::{AiProvider, Settings};

mod auth;
mod models_cache;
mod prompt;
mod protocol;
mod secrets;
mod undo;
mod worker;

pub use auth::*;
use models_cache::*;
pub use prompt::*;
pub use protocol::*;
pub use secrets::*;
pub use undo::*;
pub use worker::*;

pub const MAX_CHATS: usize = 40;
pub const MAX_MEMORIES: usize = 32;

/// Model catalogs are verbose (per-model capability blobs): the live Codex
/// list is already >256 KB, so allow plenty of headroom before truncating.
const MODEL_CATALOG_LIMIT: usize = 8 * 1024 * 1024;

pub const CHATGPT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const CHATGPT_ISSUER: &str = "https://auth.openai.com";
pub const CHATGPT_DEVICE_VERIFY_URL: &str = "https://auth.openai.com/codex/device";
pub const CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const OPENCODE_GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";

pub const STARTER_PROMPTS: [(&str, &str); 4] = [
    ("Plan my day", "What's on my schedule today and what should I focus on first?"),
    ("Task + Focus", "Add a task 'Deep work session' and start my Pomodoro focus timer."),
    ("Water & PC check", "Log a glass of water and tell me how my PC resources look."),
    ("Prioritize tasks", "Summarize my open tasks and suggest the top 3 priorities."),
];

/// Curated fallback, kept in sync with the live `/codex/models` catalog
/// (visible entries, priority order). The live probe is authoritative; this
/// only covers the fresh install / offline case.
pub const CODEX_DEFAULT_MODELS: &[&str] = &["gpt-6.1-sol", "gpt-6-astra", "gpt-6-sol", "gpt-6-luna", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"];

pub const OPENCODE_DEFAULT_MODELS: &[&str] = &["gpt-5.4", "claude-sonnet-4-6", "deepseek-r1", "gemini-2.5-pro", "qwen3-coder", "glm-5"];

/// Starter suggestions shown when a chat is empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// Reversible mutation performed by the assistant on Droppy state.
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
    /// Remove the learned memory string (undo of `REMEMBER`).
    ForgetMemory(String),
    /// Remove the note with this id (undo of `NOTE`).
    RemoveAddedNote(String),
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

/// Millilitres for a `[ACTION:WATER:..]` argument. Replies speak glasses
/// ("1", "0.5", "2"); an explicit "ml" suffix, or a bare number too big to be
/// a glass count (older prompts said ml), is taken as millilitres.
fn water_arg_ml(arg: &str) -> u32 {
    let a = arg.trim().to_ascii_lowercase();
    let ml = if let Some(n) = a.strip_suffix("ml") {
        n.trim().parse::<f32>().unwrap_or(teardrop_core::GLASS_ML as f32)
    } else {
        let n = a.trim_end_matches("glasses").trim_end_matches("glass").trim().parse::<f32>().unwrap_or(1.0);
        if n > 24.0 {
            n
        } else {
            n * teardrop_core::GLASS_ML as f32
        }
    };
    (ml.round() as u32).clamp(50, 2000)
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
    /// Save a scratch note (single line; the notes page holds the bodies).
    Note(String),
}

fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis().max(1) as u64
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

// --------------------------------------------------- Runtime State & Worker

struct Runtime {
    store: ChatStore,
    secrets: SecretStore,
    view: AiView,
    /// Provider `view.available_models` currently belongs to, so switching
    /// providers does not leak another provider's catalog into the picker.
    models_provider: Option<AiProvider>,
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
        let mut rt = Runtime { store, secrets, view: AiView::default(), models_provider: None, pending_directives: Vec::new() };
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
                self.store.conversations.push(Conversation { id, title: "New chat".into(), updated_at: (id / 1000) as i64, messages: Vec::new() });
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
        self.view.history = self.store.conversations.iter().map(|c| ChatSummary { id: c.id, title: c.title.clone(), msg_count: c.messages.len() }).collect();
        self.view.chatgpt_signed_in = !decrypt_secret(&self.secrets.chatgpt_access_token).is_empty();
        self.view.configured = self.has_stored_key(provider);
        if self.models_provider != Some(provider) {
            self.models_provider = Some(provider);
            let cached = load_model_cache(provider);
            self.view.available_models = if cached.is_empty() { default_models_for(provider) } else { cached };
        }
    }
}

pub fn default_models_for(provider: AiProvider) -> Vec<String> {
    match provider {
        AiProvider::ChatGpt => CODEX_DEFAULT_MODELS.iter().map(|s| (*s).to_string()).collect(),
        AiProvider::OpenCodeGo => OPENCODE_DEFAULT_MODELS.iter().map(|s| (*s).to_string()).collect(),
        AiProvider::OpenAiCompat => vec!["gpt-4o-mini".into(), "gpt-4.1-mini".into(), "gpt-4o".into(), "deepseek-chat".into()],
        AiProvider::Gemini => vec!["gemini-3.5-flash-lite".into(), "gemini-2.5-flash".into(), "gemini-2.5-pro".into()],
        AiProvider::Claude => vec!["claude-opus-5-5".into(), "claude-sonnet-4-6".into(), "claude-haiku-4-5".into()],
        AiProvider::Ollama => vec!["llama3.2".into(), "qwen2.5-coder".into(), "deepseek-r1".into(), "mistral".into()],
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
            rt.store.conversations.insert(0, Conversation { id, title: "New chat".into(), updated_at: now_secs(), messages: Vec::new() });
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
            rt.store.conversations.push(Conversation { id: new_id, title: "New chat".into(), updated_at: now_secs(), messages: Vec::new() });
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
        rt.view.status_note = if trimmed.is_empty() { "API key cleared".into() } else { format!("{} key saved (DPAPI encrypted)", provider.label()) };
        rt.rebuild_view(provider);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual check: the curated fallback must still match what the Codex
    /// backend actually serves, and the live probe must beat it. Run with
    /// `cargo test -p teardrop live_codex_catalog -- --ignored --nocapture`
    /// (needs a Codex CLI login in `~/.codex/auth.json`).
    #[test]
    #[ignore = "live network probe; needs a Codex CLI login"]
    fn live_codex_catalog_keeps_curated_fallback_fresh() {
        let home = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(".codex"));
        let Ok(text) = std::fs::read_to_string(home.join("auth.json")) else {
            eprintln!("skipped: no Codex CLI auth.json");
            return;
        };
        let v: Value = serde_json::from_str(&text).unwrap();
        let tokens = v.get("tokens").unwrap_or(&v);
        let access = tokens.get("access_token").and_then(|s| s.as_str()).unwrap();
        let acct = tokens.get("account_id").and_then(|s| s.as_str()).map(str::to_string).unwrap_or_else(|| extract_chatgpt_account_id(access));
        let url = format!("{CODEX_BASE_URL}/models?client_version=1.0.0");
        let auth = format!("Bearer {access}");
        let mut hdrs = vec![("Authorization", auth.as_str()), ("Accept", "application/json")];
        if !acct.is_empty() {
            hdrs.push(("ChatGPT-Account-Id", acct.as_str()));
        }
        let body = crate::http::get(&url, &hdrs, MODEL_CATALOG_LIMIT).expect("http::get failed");
        let models = parse_codex_models(&String::from_utf8_lossy(&body));
        assert!(!models.is_empty(), "live catalog came back empty");
        assert_eq!(models.first().map(String::as_str), CODEX_DEFAULT_MODELS.first().copied());
        for fallback in CODEX_DEFAULT_MODELS {
            assert!(models.iter().any(|m| m == fallback), "curated fallback model {fallback} is no longer served: {models:?}");
        }
    }

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
        // Shape copied from a live `GET /backend-api/codex/models` response.
        let body = r#"{"models":[
            {"slug":"gpt-6-sol","priority":4,"visibility":"list"},
            {"slug":"gpt-reserve","priority":4,"visibility":"hide"},
            {"slug":"codex-auto-review","priority":43,"visibility":"hide"},
            {"slug":"gpt-6.1-sol","priority":1,"visibility":"list"},
            {"slug":"gpt-5.6-luna","priority":9,"visibility":"list"},
            {"slug":"gpt-6-astra","priority":2,"visibility":"list"}
        ]}"#;
        assert_eq!(parse_codex_models(body), vec!["gpt-6.1-sol", "gpt-6-astra", "gpt-6-sol", "gpt-5.6-luna"]);
        // The curated fallback must lead with the same model as the live catalog.
        assert_eq!(default_models_for(AiProvider::ChatGpt)[0], "gpt-6.1-sol");
        assert_eq!(AiProvider::ChatGpt.default_model(), "gpt-6.1-sol");
    }

    #[test]
    fn model_cache_keys_are_stable_and_match_settings_json() {
        let dir = std::env::temp_dir().join(format!("teardrop-model-cache-{}", now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        // Exercise the same serialization the cache file uses, without touching %APPDATA%.
        assert_eq!(provider_key(AiProvider::ChatGpt), "chat_gpt");
        assert_eq!(provider_key(AiProvider::OpenCodeGo), "open_code_go");
        assert_eq!(provider_key(AiProvider::OpenAiCompat), "open_ai_compat");
        let mut cache: BTreeMap<String, Vec<String>> = BTreeMap::new();
        cache.insert(provider_key(AiProvider::ChatGpt), CODEX_DEFAULT_MODELS.iter().map(|s| s.to_string()).collect());
        let path = dir.join("ai-models.json");
        std::fs::write(&path, serde_json::to_string(&cache).unwrap()).unwrap();
        let back: BTreeMap<String, Vec<String>> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["chat_gpt"][0], "gpt-6.1-sol");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn directive_extraction_strips_tags_and_parses_actions() {
        let reply = "I've added that task and logged a glass of water for you!\n\
                     [ACTION:ADD_TASK:Write unit tests]\n\
                     [ACTION:WATER:250]\n\
                     [ACTION:REMEMBER:Prefers Rust over Python]\n\
                     [ACTION:NOTE:Call Ana about the prototype]";
        let (clean, dirs) = extract_directives(reply);
        assert_eq!(clean, "I've added that task and logged a glass of water for you!");
        assert_eq!(
            dirs,
            vec![
                ParsedDirective::AddTask("Write unit tests".into()),
                ParsedDirective::LogWater(250),
                ParsedDirective::Remember("Prefers Rust over Python".into()),
                ParsedDirective::Note("Call Ana about the prototype".into()),
            ]
        );
    }

    #[test]
    fn water_directive_speaks_glasses_but_still_accepts_ml() {
        assert_eq!(water_arg_ml("1"), 250);
        assert_eq!(water_arg_ml("2"), 500);
        assert_eq!(water_arg_ml("0.5"), 125);
        assert_eq!(water_arg_ml("1 glass"), 250);
        assert_eq!(water_arg_ml("3 glasses"), 750);
        assert_eq!(water_arg_ml("250ml"), 250);
        assert_eq!(water_arg_ml("500 ml"), 500);
        assert_eq!(water_arg_ml("250"), 250, "a bare ml-sized number is millilitres");
        assert_eq!(water_arg_ml("nonsense"), 250, "falls back to one glass");
        assert_eq!(water_arg_ml("99999"), 2000, "clamped");
    }

    #[test]
    fn base_url_normalization_handles_shortcuts() {
        assert_eq!(normalize_openai_base_url(""), "https://api.openai.com/v1");
        assert_eq!(normalize_openai_base_url("https://api.openai.com/v1/"), "https://api.openai.com/v1");
        assert_eq!(normalize_openai_base_url("https://api.groq.com/openai/v1/chat/completions"), "https://api.groq.com/openai/v1");
        assert_eq!(normalize_openai_base_url("http://localhost:1234/v1"), "http://localhost:1234/v1");
    }
}
