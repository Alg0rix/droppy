//! User settings (persisted as JSON in %APPDATA%).
use crate::apps::EyeTheme;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleLook {
    Clock,
    Eyes,
    Minimal,
    Hidden,
}

impl Default for IdleLook {
    fn default() -> Self {
        IdleLook::Clock
    }
}

/// How the idle eyes are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EyeStyle {
    #[default]
    Round,
    Cat,
    Pixel,
    Anime,
    Robot,
}

impl EyeStyle {
    pub const ALL: [EyeStyle; 5] = [EyeStyle::Round, EyeStyle::Cat, EyeStyle::Pixel, EyeStyle::Anime, EyeStyle::Robot];

    pub fn label(self) -> &'static str {
        match self {
            EyeStyle::Round => "Round",
            EyeStyle::Cat => "Cat",
            EyeStyle::Pixel => "Pixel",
            EyeStyle::Anime => "Anime",
            EyeStyle::Robot => "Robot",
        }
    }
}

/// Island body theme. `Onyx` is the classic black droplet; the rest repaint
/// the whole UI palette (see `ui::theme` in the app crate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IslandTheme {
    /// The classic black droplet.
    #[default]
    Onyx,
    /// macOS-style frosted glass: translucent, specular, dark ink.
    LiquidGlass,
    /// Solid light mode.
    Porcelain,
    /// Deep ocean blue.
    Ocean,
    /// Deep forest green.
    Forest,
    /// Warm ember brown.
    Ember,
    /// Deep violet.
    Amethyst,
    /// Dark rose.
    Rose,
    /// Pure OLED black.
    Stealth,
}

impl IslandTheme {
    pub const ALL: [IslandTheme; 9] = [
        IslandTheme::Onyx,
        IslandTheme::LiquidGlass,
        IslandTheme::Porcelain,
        IslandTheme::Ocean,
        IslandTheme::Forest,
        IslandTheme::Ember,
        IslandTheme::Amethyst,
        IslandTheme::Rose,
        IslandTheme::Stealth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            IslandTheme::Onyx => "Onyx",
            IslandTheme::LiquidGlass => "Liquid Glass",
            IslandTheme::Porcelain => "Porcelain",
            IslandTheme::Ocean => "Ocean",
            IslandTheme::Forest => "Forest",
            IslandTheme::Ember => "Ember",
            IslandTheme::Amethyst => "Amethyst",
            IslandTheme::Rose => "Rose",
            IslandTheme::Stealth => "Stealth",
        }
    }
}

/// Where the island sits: glued to the screen top, or floating below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IslandStyle {
    /// Flat top edge pinned to the screen top.
    #[default]
    Anchored,
    /// Free-floating capsule with a configurable gap to the screen top.
    Pill,
}

/// Which AI Chat provider is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    /// Sign in with ChatGPT (Codex subscription OAuth device flow).
    #[default]
    ChatGpt,
    /// OpenCode Go (`https://opencode.ai/zen/go/v1`).
    OpenCodeGo,
    /// Any OpenAI-compatible `/v1/chat/completions` endpoint (OpenAI, Groq, OpenRouter, LM Studio, vLLM, ...).
    OpenAiCompat,
    /// Google Gemini (`generativelanguage.googleapis.com`).
    Gemini,
    /// Anthropic Claude (`api.anthropic.com`).
    Claude,
    /// Local Ollama (`http://localhost:11434`).
    Ollama,
}

impl AiProvider {
    pub const ALL: [AiProvider; 6] = [AiProvider::ChatGpt, AiProvider::OpenCodeGo, AiProvider::OpenAiCompat, AiProvider::Gemini, AiProvider::Claude, AiProvider::Ollama];

    pub fn label(self) -> &'static str {
        match self {
            AiProvider::ChatGpt => "ChatGPT",
            AiProvider::OpenCodeGo => "OpenCode Go",
            AiProvider::OpenAiCompat => "OpenAI-compat",
            AiProvider::Gemini => "Gemini",
            AiProvider::Claude => "Claude",
            AiProvider::Ollama => "Ollama",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            AiProvider::ChatGpt => "ChatGPT",
            AiProvider::OpenCodeGo => "OpenCode",
            AiProvider::OpenAiCompat => "OpenAI",
            AiProvider::Gemini => "Gemini",
            AiProvider::Claude => "Claude",
            AiProvider::Ollama => "Ollama",
        }
    }

    pub fn full_name(self) -> &'static str {
        match self {
            AiProvider::ChatGpt => "Sign in with ChatGPT",
            AiProvider::OpenCodeGo => "OpenCode Go",
            AiProvider::OpenAiCompat => "OpenAI-compatible",
            AiProvider::Gemini => "Google Gemini",
            AiProvider::Claude => "Claude (Anthropic)",
            AiProvider::Ollama => "Ollama (Local)",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            AiProvider::ChatGpt => "gpt-6.1-sol",
            AiProvider::OpenCodeGo => "gpt-5.4",
            AiProvider::OpenAiCompat => "gpt-4o-mini",
            AiProvider::Gemini => "gemini-3.5-flash-lite",
            AiProvider::Claude => "claude-opus-5-5",
            AiProvider::Ollama => "llama3.2",
        }
    }

    pub fn default_url(self) -> &'static str {
        match self {
            AiProvider::OpenCodeGo => "https://opencode.ai/zen/go/v1",
            AiProvider::OpenAiCompat => "https://api.openai.com/v1",
            AiProvider::Ollama => "http://localhost:11434",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub idle_look: IdleLook,
    pub lyrics_enabled: bool,
    pub water_reminders: bool,
    pub clipboard_history: bool,
    pub sounds: bool,
    pub hide_in_fullscreen: bool,
    pub start_with_windows: bool,
    pub goal_ml: u32,
    /// Global hotkeys (each can be switched off if it collides with another app).
    #[serde(default = "yes")]
    pub hotkey_island: bool,
    #[serde(default = "yes")]
    pub hotkey_search: bool,
    #[serde(default = "yes")]
    pub hotkey_note: bool,
    #[serde(default = "yes")]
    pub hotkey_ai: bool,
    /// Open the island when the pointer rests on it (collapses again on leave).
    /// Off by default: the island opens on click or hotkey.
    #[serde(default)]
    pub open_on_hover: bool,
    #[serde(default)]
    pub eye_style: EyeStyle,
    /// Island body theme (palette + glass sheen).
    #[serde(default)]
    pub theme: IslandTheme,
    /// Glued to the screen top, or floating below it as a pill.
    #[serde(default)]
    pub island_style: IslandStyle,
    /// Pill mode: gap between the island and the screen top, logical px.
    #[serde(default = "default_pill_gap")]
    pub pill_gap: u32,
    /// Eye colour drifts with CPU load, time of day and water intake.
    #[serde(default = "yes")]
    pub mood_ring: bool,
    /// Fetch local weather (wttr.in, by IP) so the eyes react to it.
    #[serde(default = "yes")]
    pub weather_mood: bool,
    /// (month, day) for the birthday party.
    #[serde(default)]
    pub birthday: Option<(u8, u8)>,
    /// The eyes act out the kind of app in front (coding, writing,
    /// design...).
    #[serde(default = "yes")]
    pub focus_app_eyes: bool,
    /// App -> theme overrides. Key: exe name (`code`, `figma.exe`) or
    /// `title:<words>` for window titles; value: a category id (`coding`),
    /// a name from `eye_themes`, or `none`.
    #[serde(default)]
    pub app_themes: BTreeMap<String, String>,
    /// User-named themes: a built-in animation with its own iris colour.
    #[serde(default)]
    pub eye_themes: Vec<EyeTheme>,
    /// iCal feed links (secret addresses): never shown, logged or put in a view model.
    #[serde(default)]
    pub calendar_urls: Vec<String>,
    /// Page titles kept out of the island nav dock and page cycle.
    #[serde(default)]
    pub hidden_pages: Vec<String>,
    /// BLE mesh chat radio (bitchat). Off stops advertising + scanning.
    #[serde(default = "yes")]
    pub mesh_chat: bool,
    /// HUD bubble on incoming mesh message. Off = eyes react only, no text.
    #[serde(default = "yes")]
    pub mesh_hud: bool,
    /// Active AI Chat provider.
    #[serde(default)]
    pub ai_provider: AiProvider,
    /// Share day context (calendar, tasks, water, Pomodoro, system stats) with the AI.
    #[serde(default = "yes")]
    pub ai_share_day: bool,
    #[serde(default = "default_chatgpt_model")]
    pub ai_chatgpt_model: String,
    #[serde(default = "default_opencode_model")]
    pub ai_opencode_model: String,
    #[serde(default = "default_openai_url")]
    pub ai_openai_url: String,
    #[serde(default = "default_openai_model")]
    pub ai_openai_model: String,
    #[serde(default = "default_gemini_model")]
    pub ai_gemini_model: String,
    #[serde(default = "default_claude_model")]
    pub ai_claude_model: String,
    #[serde(default = "default_ollama_url")]
    pub ai_ollama_url: String,
    #[serde(default = "default_ollama_model")]
    pub ai_ollama_model: String,
}

/// Pill mode: upper bound for `pill_gap` (logical px).
pub const PILL_GAP_MAX: u32 = 40;
/// Pill mode: `pill_gap` stepper increment (logical px).
pub const PILL_GAP_STEP: u32 = 2;

fn yes() -> bool {
    true
}
fn default_pill_gap() -> u32 {
    10
}
fn default_chatgpt_model() -> String {
    AiProvider::ChatGpt.default_model().into()
}
fn default_opencode_model() -> String {
    AiProvider::OpenCodeGo.default_model().into()
}
fn default_openai_url() -> String {
    AiProvider::OpenAiCompat.default_url().into()
}
fn default_openai_model() -> String {
    AiProvider::OpenAiCompat.default_model().into()
}
fn default_gemini_model() -> String {
    AiProvider::Gemini.default_model().into()
}
fn default_claude_model() -> String {
    AiProvider::Claude.default_model().into()
}
fn default_ollama_url() -> String {
    AiProvider::Ollama.default_url().into()
}
fn default_ollama_model() -> String {
    AiProvider::Ollama.default_model().into()
}

impl Settings {
    /// Gap between the island and the screen top (0 when anchored).
    pub fn top_gap(&self) -> f32 {
        match self.island_style {
            IslandStyle::Anchored => 0.0,
            IslandStyle::Pill => self.pill_gap.min(PILL_GAP_MAX) as f32,
        }
    }

    pub fn active_ai_model(&self) -> &str {
        let s = match self.ai_provider {
            AiProvider::ChatGpt => &self.ai_chatgpt_model,
            AiProvider::OpenCodeGo => &self.ai_opencode_model,
            AiProvider::OpenAiCompat => &self.ai_openai_model,
            AiProvider::Gemini => &self.ai_gemini_model,
            AiProvider::Claude => &self.ai_claude_model,
            AiProvider::Ollama => &self.ai_ollama_model,
        };
        if s.trim().is_empty() {
            self.ai_provider.default_model()
        } else {
            s.trim()
        }
    }

    pub fn set_active_ai_model(&mut self, model: String) {
        let m = if model.trim().is_empty() { self.ai_provider.default_model().to_string() } else { model.trim().to_string() };
        match self.ai_provider {
            AiProvider::ChatGpt => self.ai_chatgpt_model = m,
            AiProvider::OpenCodeGo => self.ai_opencode_model = m,
            AiProvider::OpenAiCompat => self.ai_openai_model = m,
            AiProvider::Gemini => self.ai_gemini_model = m,
            AiProvider::Claude => self.ai_claude_model = m,
            AiProvider::Ollama => self.ai_ollama_model = m,
        }
    }

    pub fn active_ai_url(&self) -> &str {
        match self.ai_provider {
            AiProvider::OpenCodeGo => AiProvider::OpenCodeGo.default_url(),
            AiProvider::OpenAiCompat => {
                if self.ai_openai_url.trim().is_empty() {
                    AiProvider::OpenAiCompat.default_url()
                } else {
                    self.ai_openai_url.trim()
                }
            }
            AiProvider::Ollama => {
                if self.ai_ollama_url.trim().is_empty() {
                    AiProvider::Ollama.default_url()
                } else {
                    self.ai_ollama_url.trim()
                }
            }
            _ => "",
        }
    }

    pub fn set_active_ai_url(&mut self, url: String) {
        let u = url.trim().to_string();
        match self.ai_provider {
            AiProvider::OpenAiCompat => self.ai_openai_url = if u.is_empty() { default_openai_url() } else { u },
            AiProvider::Ollama => self.ai_ollama_url = if u.is_empty() { default_ollama_url() } else { u },
            _ => {}
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle_look: IdleLook::Clock,
            lyrics_enabled: true,
            water_reminders: true,
            clipboard_history: true,
            sounds: false,
            hide_in_fullscreen: true,
            start_with_windows: false,
            goal_ml: 2000,
            hotkey_island: true,
            hotkey_search: true,
            hotkey_note: true,
            hotkey_ai: true,
            open_on_hover: false,
            eye_style: EyeStyle::Round,
            theme: IslandTheme::Onyx,
            island_style: IslandStyle::Anchored,
            pill_gap: default_pill_gap(),
            mood_ring: true,
            weather_mood: true,
            birthday: None,
            focus_app_eyes: true,
            app_themes: BTreeMap::new(),
            eye_themes: Vec::new(),
            calendar_urls: Vec::new(),
            hidden_pages: Vec::new(),
            mesh_chat: true,
            mesh_hud: true,
            ai_provider: AiProvider::ChatGpt,
            ai_share_day: true,
            ai_chatgpt_model: default_chatgpt_model(),
            ai_opencode_model: default_opencode_model(),
            ai_openai_url: default_openai_url(),
            ai_openai_model: default_openai_model(),
            ai_gemini_model: default_gemini_model(),
            ai_claude_model: default_claude_model(),
            ai_ollama_url: default_ollama_url(),
            ai_ollama_model: default_ollama_model(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_files_still_load() {
        let old = r#"{"idle_look":"eyes","lyrics_enabled":true,"water_reminders":true,"clipboard_history":true,"sounds":false,"hide_in_fullscreen":true,"start_with_windows":false,"goal_ml":2000}"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert_eq!(s.eye_style, EyeStyle::Round);
        assert!(s.island_style == IslandStyle::Anchored && s.pill_gap == 10);
        assert!(s.hotkey_island && s.hotkey_search && s.hotkey_note && s.hotkey_ai, "hotkeys default on");
        assert!(s.weather_mood && s.mood_ring && !s.open_on_hover && s.focus_app_eyes && s.birthday.is_none());
        assert_eq!(s.ai_provider, AiProvider::ChatGpt);
        assert!(s.ai_share_day);
        assert_eq!(s.active_ai_model(), "gpt-6.1-sol");
        assert!(s.focus_app_eyes && s.app_themes.is_empty() && s.eye_themes.is_empty());
    }

    #[test]
    fn app_themes_load_from_json() {
        let j = r##"{"idle_look":"eyes","lyrics_enabled":true,"water_reminders":true,"clipboard_history":true,"sounds":false,"hide_in_fullscreen":true,"start_with_windows":false,"goal_ml":2000,
            "app_themes":{"code":"writing","title:jira":"Sprint"},
            "eye_themes":[{"name":"Sprint","base":"planning","tint":"#22c55e"}]}"##;
        let s: Settings = serde_json::from_str(j).unwrap();
        assert_eq!(s.app_themes.get("code").map(String::as_str), Some("writing"));
        assert_eq!(s.eye_themes[0].base, crate::AppCategory::Planning);
    }
}
