//! View-model: the data the island shows, no Windows calls.
//!
//! Services fill these structs; `ui::layout` turns them into a scene. Fully
//! unit-tested on any OS.

use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use teardrop_core::pomodoro::Phase;
use teardrop_core::{ClipboardRing, Hud, HudKind, IdleLook, NoteList, Page, Playback, Pomodoro, Shelf, SystemSnapshot, TaskList, Track, WaterLog};

#[derive(Debug, Clone)]
pub struct ViewModel {
    pub page: Page,
    pub track: Track,
    pub lyric_idx: Option<usize>,
    pub lyric_lines: Vec<teardrop_core::lyrics::LyricLine>,
    pub pomo: Pomodoro,
    pub water: WaterLog,
    pub tasks: TaskList,
    pub notes: NoteList,
    pub shelf: Shelf,
    pub clipboard: ClipboardRing,
    pub system: SystemSnapshot,
    pub hud: Option<Hud>,
    pub idle_look: IdleLook,
    pub env: Ambient,
    pub speed: SpeedView,
    pub update: crate::update::UpdateView,
    pub usage: crate::usage::UsageView,
    pub voice: crate::voice::VoiceView,
    pub calendar: crate::calendar::CalendarView,
    /// Connected calendar feeds (the links themselves stay in Settings).
    pub calendar_feeds: usize,
    pub ai: crate::ai::AiView,
    pub pinned: bool,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeedPhase {
    #[default]
    Idle,
    Ping,
    Down,
    Up,
    Done,
    Failed,
}

impl SpeedPhase {
    pub fn running(self) -> bool {
        matches!(self, SpeedPhase::Ping | SpeedPhase::Down | SpeedPhase::Up)
    }
}

/// Internet speed test progress/result (Mbit/s, ms).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpeedView {
    pub phase: SpeedPhase,
    pub ping_ms: Option<f32>,
    pub down: f32,
    pub up: f32,
}

impl SpeedView {
    pub const IDLE: SpeedView = SpeedView { phase: SpeedPhase::Idle, ping_ms: None, down: 0.0, up: 0.0 };
}

/// Coarse local weather (opt-in fetch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weather {
    #[default]
    Unknown,
    Clear,
    Cloudy,
    Rain,
    Snow,
    Storm,
    Fog,
}

/// Ambient signals the idle eyes react to. Windows samplers fill it; the
/// defaults mean "nothing going on".
#[derive(Debug, Clone, Default)]
pub struct Ambient {
    /// Since the last keyboard/mouse input anywhere.
    pub idle_ms: u64,
    /// Output peak meter 0..1 (None = no meter).
    pub audio_peak: Option<f32>,
    /// Left button held anywhere (something is being dragged).
    pub lbutton: bool,
    pub mic: bool,
    pub camera: bool,
    pub offline: bool,
    /// Do not disturb / presentation mode.
    pub quiet: bool,
    pub locked: bool,
    pub weather: Weather,
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
        let mut notes = NoteList::default();
        notes.add("Ideas\n- Wrapped text in the editor\n- Keyboard-only flow: N, type, Esc", now - chrono::Duration::days(2));
        notes.add("Coffee with Ana\nThursday 10:00, bring the prototype deck.", now - chrono::Duration::hours(3));
        notes.add("Ship notes before the weekend\nThe first line is the row title, the rest is the preview underneath.", now);
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
            notes,
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
            env: Ambient::default(),
            speed: SpeedView::IDLE,
            update: Default::default(),
            usage: Default::default(),
            voice: Default::default(),
            calendar: Default::default(),
            calendar_feeds: 0,
            ai: crate::ai::AiView {
                active_id: 1,
                title: "Morning planning".into(),
                configured: true,
                chatgpt_signed_in: true,
                available_models: crate::ai::default_models_for(teardrop_core::AiProvider::ChatGpt),
                ..Default::default()
            },
            pinned: false,
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
            notes: NoteList::default(),
            shelf: Shelf::new(),
            clipboard: ClipboardRing::new(),
            system: SystemSnapshot::default(),
            hud: None,
            idle_look: IdleLook::Clock,
            env: Ambient::default(),
            speed: SpeedView::IDLE,
            update: Default::default(),
            usage: Default::default(),
            voice: Default::default(),
            calendar: Default::default(),
            calendar_feeds: 0,
            ai: Default::default(),
            pinned: false,
            now,
        }
    }

    /// Build the day summary shared with AI Chat when `ai_share_day` is enabled.
    pub fn day_context_summary(&self) -> String {
        let local = local_naive(self.now);
        let mut lines = Vec::new();
        lines.push(format!("Now: {}", local.format("%A, %Y-%m-%d %H:%M")));

        // Calendar events (up to 6 upcoming events).
        let now_ts = self.now.timestamp();
        let agenda = self.calendar.agenda(now_ts, 6);
        if agenda.is_empty() {
            lines.push("Calendar: no upcoming events".into());
        } else {
            let ev_list: Vec<String> = agenda
                .iter()
                .map(|ev| {
                    let day = crate::calendar::day_label(ev, now_ts);
                    let when = if ev.all_day {
                        format!("{day} All day")
                    } else {
                        format!("{day} {}", crate::calendar::clock_label(ev))
                    };
                    format!("{} ({})", ev.title, when)
                })
                .collect();
            lines.push(format!("Calendar: {}", ev_list.join("; ")));
        }

        // Tasks
        let open_tasks: Vec<&str> = self.tasks.tasks.iter().filter(|t| !t.done).map(|t| t.title.as_str()).collect();
        let done_count = self.tasks.tasks.iter().filter(|t| t.done).count();
        if open_tasks.is_empty() {
            lines.push(format!("Tasks: 0 open ({done_count} completed)"));
        } else {
            lines.push(format!("Open tasks ({}): {} ({done_count} completed)", open_tasks.len(), open_tasks.join(", ")));
        }

        // Notes (titles only — bodies stay local unless the user asks)
        if !self.notes.notes.is_empty() {
            let titles: Vec<String> = self
                .notes
                .notes
                .iter()
                .take(3)
                .map(|n| {
                    let t = teardrop_core::title_of(&n.text);
                    if t.is_empty() { "(untitled)".to_string() } else { t.chars().take(60).collect() }
                })
                .collect();
            lines.push(format!("Notes: {} saved ({})", self.notes.notes.len(), titles.join(", ")));
        }

        // Water
        let day = self.day();
        let today_ml = self.water.today(day);
        lines.push(format!("Water today: {} / {} ml", today_ml, self.water.goal_ml));

        // Pomodoro
        let pomo_state = match self.pomo.phase {
            Phase::Idle => format!("Idle ({} focus sessions completed)", self.pomo.completed_focus),
            p => format!(
                "{:?} ({}, {} remaining, {} sessions completed)",
                p,
                if self.focus_running() { "running" } else { "paused" },
                mmss(self.focus_remaining_ms()),
                self.pomo.completed_focus
            ),
        };
        lines.push(format!("Pomodoro: {pomo_state}"));

        // Music
        if self.music_active() {
            lines.push(format!("Now playing: {} — {}", self.track.title, self.track.display_artist()));
        }

        // System stats
        let sys = &self.system;
        let free_disk = (sys.disk_total_gb - sys.disk_used_gb).max(0.0);
        let mut sys_line = format!(
            "PC: CPU {:.0}%, RAM {:.1}/{:.1} GB ({:.0}%), Free Disk {:.0} GB",
            sys.cpu_pct,
            sys.mem_used_gb,
            sys.mem_total_gb,
            sys.mem_pct(),
            free_disk
        );
        if let Some(bat) = sys.battery_pct {
            sys_line.push_str(&format!(", Battery {bat}%{}", if sys.charging { " (charging)" } else { "" }));
        }
        lines.push(sys_line);

        lines.join("\n")
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

    /// A meeting starting within 10 minutes (or begun within the last 5) that
    /// owns the collapsed island.
    pub fn meeting_soon(&self) -> Option<&crate::calendar::Event> {
        let now = self.now.timestamp();
        self.calendar.next(now).filter(|e| e.start - now <= 600 && now - e.start <= 300)
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

/// Move a (month, day) birthday by whole days, wrapping through the year
/// (a leap year, so Feb 29 is reachable).
pub fn shift_birthday((m, d): (u8, u8), days: i64) -> (u8, u8) {
    use chrono::{Datelike, NaiveDate};
    let base = NaiveDate::from_ymd_opt(2000, m as u32, d as u32).unwrap_or(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    let year_len = 366;
    let n = (base.ordinal0() as i64 + days).rem_euclid(year_len);
    let day = NaiveDate::from_yo_opt(2000, n as u32 + 1).unwrap_or(base);
    (day.month() as u8, day.day() as u8)
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
    fn birthday_shifts_wrap_the_year() {
        assert_eq!(shift_birthday((10, 24), 1), (10, 25));
        assert_eq!(shift_birthday((12, 31), 1), (1, 1));
        assert_eq!(shift_birthday((1, 1), -1), (12, 31));
        assert_eq!(shift_birthday((2, 28), 1), (2, 29));
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
        let summary = vm.day_context_summary();
        assert!(summary.contains("Ship the island redesign"));
        assert!(summary.contains("1250 / 2000 ml"));
    }
}
