//! Page model: order, scroll-switching, badges.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Page {
    Music,
    Focus,
    Water,
    Shelf,
    Tasks,
    Notes,
    Clipboard,
    Today,
    /// Live CPU / GPU / memory / drives / network / battery monitor.
    System,
    /// Quick settings: radios, mutes, brightness / volume sliders.
    Quick,
    Speed,
    Usage,
    Voice,
    Ai,
    /// BLE mesh chat (bitchat whitepaper).
    Chat,
    /// The desktop window switcher.
    Windows,
    /// Reached from the gear in the header; not part of the tab cycle.
    Settings,
}

impl Default for Page {
    fn default() -> Self {
        Page::Music
    }
}

impl Page {
    /// The tab pages in header order (Settings is reached via the gear).
    pub fn all() -> &'static [Page] {
        &[
            Page::Music,
            Page::Focus,
            Page::Water,
            Page::Shelf,
            Page::Windows,
            Page::Tasks,
            Page::Notes,
            Page::Clipboard,
            Page::Today,
            Page::System,
            Page::Quick,
            Page::Speed,
            Page::Usage,
            Page::Voice,
            Page::Ai,
            Page::Chat,
        ]
    }

    pub fn title(self) -> &'static str {
        match self {
            Page::Music => "Music",
            Page::Focus => "Focus",
            Page::Water => "Water",
            Page::Shelf => "Shelf",
            Page::Tasks => "Tasks",
            Page::Notes => "Notes",
            Page::Clipboard => "Clipboard",
            Page::Today => "Today",
            Page::System => "System",
            Page::Quick => "Quick",
            Page::Speed => "Speed",
            Page::Usage => "Usage",
            Page::Voice => "Voice",
            Page::Ai => "AI",
            Page::Chat => "Chat",
            Page::Windows => "Windows",
            Page::Settings => "Settings",
        }
    }

    /// Scroll over the header: +1 = next, -1 = previous, wraps.
    pub fn step(self, delta: i32) -> Page {
        let all = Self::all();
        let Some(i) = all.iter().position(|&p| p == self) else {
            return if delta >= 0 { all[0] } else { all[all.len() - 1] };
        };
        let i = i as i32;
        let n = all.len() as i32;
        all[((i + delta).rem_euclid(n)) as usize]
    }
}
