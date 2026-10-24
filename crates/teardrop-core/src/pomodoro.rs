//! Pomodoro / focus timer state machine.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase { Idle, Focus, ShortBreak, LongBreak }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pomodoro {
    pub phase: Phase,
    pub focus_min: u32,
    pub short_min: u32,
    pub long_min: u32,
    pub sessions_before_long: u32,
    pub completed_focus: u32,
    /// When the current phase ends (None = paused/idle).
    pub ends_at: Option<DateTime<Utc>>,
    pub remaining_ms_when_paused: Option<u64>,
}

impl Default for Pomodoro {
    fn default() -> Self {
        Self { phase: Phase::Idle, focus_min: 25, short_min: 5, long_min: 15, sessions_before_long: 4, completed_focus: 0, ends_at: None, remaining_ms_when_paused: None }
    }
}

impl Pomodoro {
    pub fn phase_len_ms(&self, phase: Phase) -> u64 {
        let m = match phase { Phase::Focus => self.focus_min, Phase::ShortBreak => self.short_min, Phase::LongBreak => self.long_min, Phase::Idle => 0 };
        m as u64 * 60_000
    }

    pub fn start_focus(&mut self, now: DateTime<Utc>) {
        self.phase = Phase::Focus;
        self.ends_at = Some(now + chrono::Duration::milliseconds(self.phase_len_ms(Phase::Focus) as i64));
        self.remaining_ms_when_paused = None;
    }

    pub fn pause(&mut self, now: DateTime<Utc>) {
        if let Some(end) = self.ends_at {
            let rem = (end - now).num_milliseconds().max(0) as u64;
            self.remaining_ms_when_paused = Some(rem);
            self.ends_at = None;
        }
    }

    pub fn resume(&mut self, now: DateTime<Utc>) {
        if let Some(rem) = self.remaining_ms_when_paused.take() {
            self.ends_at = Some(now + chrono::Duration::milliseconds(rem as i64));
        }
    }

    pub fn remaining_ms(&self, now: DateTime<Utc>) -> u64 {
        if let Some(end) = self.ends_at {
            (end - now).num_milliseconds().max(0) as u64
        } else {
            self.remaining_ms_when_paused.unwrap_or_else(|| self.phase_len_ms(self.phase))
        }
    }

    /// Advance on expiry; returns the new phase if it changed.
    pub fn tick(&mut self, now: DateTime<Utc>) -> Option<Phase> {
        let end = self.ends_at?;
        if now < end { return None; }
        let next = match self.phase {
            Phase::Focus => {
                self.completed_focus += 1;
                if self.completed_focus % self.sessions_before_long == 0 { Phase::LongBreak } else { Phase::ShortBreak }
            }
            Phase::ShortBreak | Phase::LongBreak => Phase::Focus,
            Phase::Idle => return None,
        };
        // Auto-chain only breaks -> focus; focus completion parks at break start paused? Keep running for simplicity.
        self.phase = next;
        self.ends_at = Some(now + chrono::Duration::milliseconds(self.phase_len_ms(next) as i64));
        Some(next)
    }

    pub fn stop(&mut self) {
        self.phase = Phase::Idle;
        self.ends_at = None;
        self.remaining_ms_when_paused = None;
    }
}
