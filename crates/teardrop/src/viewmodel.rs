//! View-model: the data the island shows, no Windows calls.
//!
//! Services fill these structs; `ui::layout` turns them into a scene. Fully
//! unit-tested on any OS.

use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use teardrop_core::pomodoro::Phase;
use teardrop_core::{ClipboardRing, Hud, HudKind, IdleLook, Page, Playback, Pomodoro, Shelf, SystemSnapshot, TaskList, Track, WaterLog};

#[derive(Debug, Clone)]
pub struct ViewModel {
    pub page: Page,
    pub track: Track,
    pub lyric_idx: Option<usize>,
    pub lyric_lines: Vec<teardrop_core::lyrics::LyricLine>,
    pub pomo: Pomodoro,
    pub water: WaterLog,
    pub tasks: TaskList,
    pub shelf: Shelf,
    pub clipboard: ClipboardRing,
    pub system: SystemSnapshot,
    pub hud: Option<Hud>,
    pub idle_look: IdleLook,
    pub now: DateTime<Utc>,
}

/// Wall-clock time in the user's zone.
pub fn local_naive(now: DateTime<Utc>) -> NaiveDateTime {
    now.with_timezone(&Local).naive_local()
}

impl ViewModel {
    pub fn demo(now: DateTime<Utc>) -> Self {
        let mut water = WaterLog::default();
        let day = Self::day_of(now);
        water.add(day, 1250);
        let mut tasks = TaskList::default();
        tasks.add("Ship the island redesign", now);
        tasks.add("Review droplet morph frames", now);
        tasks.add("Reply to design feedback", now);
        tasks.add("Book dentist", now);
        tasks.toggle(&tasks.tasks[3].id.clone());
        let mut shelf = Shelf::new();
        shelf.add("C:\\Work\\spec-v2.pdf", now);
        shelf.add("C:\\Work\\mockup-final.png", now);
        shelf.add("C:\\Work\\budget.xlsx", now);
        shelf.add("C:\\Work\\notes.md", now);
        let mut clipboard = ClipboardRing::new();
        clipboard.push("cargo test -p teardrop-core");
        clipboard.push("https://example.com/design/tokens?theme=dark");
        clipboard.push("Meeting moved to 3pm - bring the prototype");
        Self {
            page: Page::Music,
            track: Track {
                title: "Midnight Droplet".into(),
                artist: "Teardrop Ensemble".into(),
                album: "Surface Tension".into(),
                duration_ms: 214_000,
                position_ms: 63_000,
                playback: Playback::Playing,
                app: "Spotify".into(),
            },
            lyric_idx: None,
            lyric_lines: vec![],
            pomo: Pomodoro::default(),
            water,
            tasks,
            shelf,
            clipboard,
            system: SystemSnapshot {
                cpu_pct: 18.0,
                mem_used_gb: 9.8,
                mem_total_gb: 16.0,
                disk_used_gb: 310.0,
                disk_total_gb: 512.0,
                battery_pct: Some(87),
                charging: false,
            },
            hud: None,
            idle_look: IdleLook::Clock,
            now,
        }
    }

    /// Empty state for the real app (no demo data).
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            page: Page::Music,
            track: Track::default(),
            lyric_idx: None,
            lyric_lines: vec![],
            pomo: Pomodoro::default(),
            water: WaterLog::default(),
            tasks: TaskList::default(),
            shelf: Shelf::new(),
            clipboard: ClipboardRing::new(),
            system: SystemSnapshot::default(),
            hud: None,
            idle_look: IdleLook::Clock,
            now,
        }
    }

    /// The local calendar day, disguised as a UTC instant so `WaterLog`
    /// buckets by the user's midnight instead of Greenwich's.
    pub fn day_of(now: DateTime<Utc>) -> DateTime<Utc> {
        Utc.from_utc_datetime(&local_naive(now))
    }

    pub fn day(&self) -> DateTime<Utc> {
        Self::day_of(self.now)
    }

    pub fn live_hud(&self, now_ms: u64) -> Option<&Hud> {
        self.hud.as_ref().filter(|h| h.alive(now_ms))
    }

    /// Focus timer should own the collapsed island (running or paused mid-phase).
    pub fn focus_visible(&self) -> bool {
        self.pomo.phase != Phase::Idle
    }

    pub fn music_active(&self) -> bool {
        self.track.playback == Playback::Playing && !self.track.title.is_empty()
    }

    pub fn focus_total_ms(&self) -> u64 {
        match self.pomo.phase {
            Phase::Idle => self.pomo.phase_len_ms(Phase::Focus),
            p => self.pomo.phase_len_ms(p),
        }
    }

    pub fn focus_remaining_ms(&self) -> u64 {
        if self.pomo.phase == Phase::Idle {
            self.pomo.phase_len_ms(Phase::Focus)
        } else {
            self.pomo.remaining_ms(self.now)
        }
    }

    /// 0..1 of the current focus phase already elapsed.
    pub fn focus_progress(&self) -> f32 {
        let total = self.focus_total_ms().max(1) as f32;
        (1.0 - self.focus_remaining_ms() as f32 / total).clamp(0.0, 1.0)
    }

    pub fn focus_running(&self) -> bool {
        self.pomo.ends_at.is_some()
    }

    pub fn clock_text(&self) -> String {
        local_naive(self.now).format("%H:%M").to_string()
    }
}

/// Volume / mute notice with the level as bar progress.
pub fn hud_volume(pct: u8, muted: bool, now_ms: u64) -> Hud {
    let kind = if muted { HudKind::Muted } else { HudKind::Volume };
    let mut h = Hud::transient(kind, if muted { "Muted" } else { "Volume" }, &format!("{pct}%"), now_ms, 2200);
    h.progress = Some(pct as f32 / 100.0);
    h
}

/// Reminder with a one-click "+250 ml" action.
pub fn hud_water_due(now_ms: u64) -> Hud {
    let mut h = Hud::transient(HudKind::Water, "Water", "Time to hydrate", now_ms, 9000);
    h.action_label = Some("+250 ml".to_string());
    h
}

pub fn hud_water_logged(ml: u32, total_ml: u32, now_ms: u64) -> Hud {
    Hud::transient(HudKind::Water, "Water", &format!("+{ml} ml \u{00b7} {total_ml} ml today"), now_ms, 3200)
}

pub fn mmss(ms: u64) -> String {
    let secs = (ms + 999) / 1000;
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Collapse whitespace/newlines so multi-line clips show as one tidy line.
pub fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmss_rounds_up_so_the_last_second_reads_00_01() {
        assert_eq!(mmss(0), "00:00");
        assert_eq!(mmss(1), "00:01");
        assert_eq!(mmss(25 * 60_000), "25:00");
        assert_eq!(mmss(61_000), "01:01");
    }

    #[test]
    fn hud_builders() {
        assert!(hud_water_due(0).action_label.is_some());
        assert!(hud_water_logged(250, 500, 0).action_label.is_none());
        assert_eq!(hud_volume(40, false, 0).progress, Some(0.4));
        assert_eq!(hud_volume(0, true, 0).kind, HudKind::Muted);
    }

    #[test]
    fn one_line_flattens() {
        assert_eq!(one_line("a\n  b\t c"), "a b c");
    }

    #[test]
    fn focus_progress_is_zero_when_idle_and_grows() {
        let now = Utc::now();
        let mut vm = ViewModel::demo(now);
        assert_eq!(vm.focus_progress(), 0.0);
        assert!(!vm.focus_visible());
        vm.pomo.start_focus(now);
        vm.now = now + chrono::Duration::minutes(5);
        assert!(vm.focus_visible());
        assert!((vm.focus_progress() - 0.2).abs() < 0.01);
    }

    #[test]
    fn water_day_uses_local_wall_clock() {
        let now = Utc::now();
        let d = ViewModel::day_of(now);
        assert_eq!(d.naive_utc(), local_naive(now));
    }

    #[test]
    fn demo_has_data_for_every_page() {
        let vm = ViewModel::demo(Utc::now());
        assert!(vm.music_active());
        assert!(vm.water.today(vm.day()) > 0);
        assert!(vm.tasks.tasks.len() >= 4);
        assert!(vm.shelf.items.len() >= 4);
        assert!(vm.clipboard.items().len() >= 3);
    }
}
