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
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self { idle_look: IdleLook::Clock, lyrics_enabled: true, water_reminders: true, clipboard_history: true, sounds: false, hide_in_fullscreen: true, start_with_windows: false, goal_ml: 2000, open_on_hover: true }
    }
}
