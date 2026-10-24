//! Daily water log with weekly history and gentle reminders.
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaterLog {
    pub goal_ml: u32,
    pub days: BTreeMap<NaiveDate, u32>,
    pub reminder_every_min: u32,
    pub last_reminder: Option<DateTime<Utc>>,
    pub reminders_enabled: bool,
}

impl Default for WaterLog {
    fn default() -> Self {
        Self { goal_ml: 2000, days: BTreeMap::new(), reminder_every_min: 60, last_reminder: None, reminders_enabled: true }
    }
}

impl WaterLog {
    pub fn today(&self, now: DateTime<Utc>) -> u32 {
        self.days.get(&now.date_naive()).copied().unwrap_or(0)
    }

    pub fn add(&mut self, now: DateTime<Utc>, ml: u32) -> u32 {
        let e = self.days.entry(now.date_naive()).or_insert(0);
        *e = e.saturating_add(ml);
        *e
    }

    /// Undo helper: removes up to `ml` from today's total. Returns the new total.
    pub fn sub(&mut self, now: DateTime<Utc>, ml: u32) -> u32 {
        let e = self.days.entry(now.date_naive()).or_insert(0);
        *e = e.saturating_sub(ml);
        *e
    }

    pub fn progress(&self, now: DateTime<Utc>) -> f32 {
        (self.today(now) as f32 / self.goal_ml.max(1) as f32).clamp(0.0, 1.0)
    }

    /// Last 7 days ending today, oldest first.
    pub fn week(&self, now: DateTime<Utc>) -> Vec<(NaiveDate, u32)> {
        let today = now.date_naive();
        (0..7)
            .map(|back| {
                let day = today - chrono::Duration::days((6 - back) as i64);
                (day, self.days.get(&day).copied().unwrap_or(0))
            })
            .collect()
    }

    /// `now` is the real instant (for the interval); `today_ml` is today's
    /// total as the caller buckets it (the user's local day, not UTC's).
    pub fn reminder_due(&self, now: DateTime<Utc>, today_ml: u32) -> bool {
        if !self.reminders_enabled { return false; }
        if today_ml >= self.goal_ml { return false; }
        match self.last_reminder {
            None => true,
            Some(t) => (now - t).num_minutes() >= self.reminder_every_min as i64,
        }
    }

    pub fn snooze(&mut self, now: DateTime<Utc>) {
        self.last_reminder = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_waits_one_interval_and_stops_at_goal() {
        let now = Utc::now();
        let mut w = WaterLog::default();
        assert!(w.reminder_due(now, 0), "never reminded -> due");
        w.snooze(now);
        assert!(!w.reminder_due(now + chrono::Duration::minutes(59), 0));
        assert!(w.reminder_due(now + chrono::Duration::minutes(61), 0));
        assert!(!w.reminder_due(now + chrono::Duration::minutes(61), w.goal_ml), "goal met -> quiet");
        w.reminders_enabled = false;
        assert!(!w.reminder_due(now + chrono::Duration::minutes(61), 0));
    }

    #[test]
    fn sub_never_underflows() {
        let now = Utc::now();
        let mut w = WaterLog::default();
        w.add(now, 100);
        assert_eq!(w.sub(now, 250), 0);
    }
}
