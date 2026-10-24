//! Media model fed by the OS media controls (SMTC on Windows).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Playback { Playing, Paused, Stopped }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub position_ms: u64,
    pub playback: Playback,
    pub app: String,
}

impl Default for Track {
    fn default() -> Self {
        Self { title: String::new(), artist: String::new(), album: String::new(), duration_ms: 0, position_ms: 0, playback: Playback::Stopped, app: String::new() }
    }
}

impl Track {
    pub fn display_artist(&self) -> &str {
        if self.artist.is_empty() { &self.app } else { &self.artist }
    }

    pub fn progress(&self) -> f32 {
        if self.duration_ms == 0 { 0.0 } else { (self.position_ms as f32 / self.duration_ms as f32).clamp(0.0, 1.0) }
    }

    /// Fake equalizer heights (5 bars) driven by wall-clock so the island
    /// pulses while music plays without touching audio APIs.
    pub fn eq_bars(now_ms: u64) -> [f32; 5] {
        let t = now_ms as f32 / 180.0;
        let mut out = [0.0; 5];
        for (i, b) in out.iter_mut().enumerate() {
            let p = t + i as f32 * 1.7;
            *b = 0.35 + 0.65 * (0.5 + 0.5 * (p.sin() * 0.6 + (p * 2.3).sin() * 0.4));
        }
        out
    }
}
