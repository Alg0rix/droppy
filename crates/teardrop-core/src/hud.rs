//! Transient collapsed-island notices (volume, charging, headset, water).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hud {
    pub kind: HudKind,
    pub title: String,
    pub detail: String,
    pub progress: Option<f32>,
    pub expires_ms: u64,
    pub action_label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudKind { Volume, Charging, Bluetooth, Water, Timer, Muted }

impl Hud {
    pub fn transient(kind: HudKind, title: &str, detail: &str, now_ms: u64, ttl_ms: u64) -> Self {
        Self { kind, title: title.to_string(), detail: detail.to_string(), progress: None, expires_ms: now_ms + ttl_ms, action_label: None }
    }

    pub fn alive(&self, now_ms: u64) -> bool {
        now_ms < self.expires_ms
    }
}
