//! User settings (persisted as JSON in %APPDATA%).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleLook { Clock, Eyes, Minimal, Hidden }

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
    /// Open the island when the pointer rests on it (collapses again on leave).
    #[serde(default = "yes")]
    pub open_on_hover: bool,
    #[serde(default)]
    pub eye_style: EyeStyle,
    /// Eye colour drifts with CPU load, time of day and water intake.
    #[serde(default)]
    pub mood_ring: bool,
    /// Opt-in: fetch local weather (wttr.in, by IP) so the eyes react to it.
    #[serde(default)]
    pub weather_mood: bool,
    /// (month, day) for the birthday party.
    #[serde(default)]
    pub birthday: Option<(u8, u8)>,
    /// iCal feed links (secret addresses): never shown, logged or put in a view model.
    #[serde(default)]
    pub calendar_urls: Vec<String>,
}

fn yes() -> bool {
    true
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
            open_on_hover: true,
            eye_style: EyeStyle::Round,
            mood_ring: false,
            weather_mood: false,
            birthday: None,
            calendar_urls: Vec::new(),
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
        assert!(!s.weather_mood && !s.mood_ring && s.birthday.is_none());
    }
}
