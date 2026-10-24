//! Interaction + animation state that lives between frames.
//!
//! The view-model says *what* to show; this says *how far along* every
//! transition is (hover glow, sliding tab pill, page slide, water level,
//! compact width...). Everything eases with frame-rate independent
//! exponential smoothing so a dropped frame never looks like a jump.

use super::scene::Rect;
use crate::viewmodel::ViewModel;
use std::collections::HashMap;
use teardrop_core::{IdleLook, Page, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingKey {
    OpenOnHover,
    Lyrics,
    WaterReminders,
    ClipboardHistory,
    Sounds,
    HideFullscreen,
    StartWithWindows,
}

impl SettingKey {
    pub const ALL: [SettingKey; 7] = [
        SettingKey::OpenOnHover,
        SettingKey::Lyrics,
        SettingKey::WaterReminders,
        SettingKey::ClipboardHistory,
        SettingKey::Sounds,
        SettingKey::HideFullscreen,
        SettingKey::StartWithWindows,
    ];

    pub fn get(self, s: &Settings) -> bool {
        match self {
            SettingKey::OpenOnHover => s.open_on_hover,
            SettingKey::Lyrics => s.lyrics_enabled,
            SettingKey::WaterReminders => s.water_reminders,
            SettingKey::ClipboardHistory => s.clipboard_history,
            SettingKey::Sounds => s.sounds,
            SettingKey::HideFullscreen => s.hide_in_fullscreen,
            SettingKey::StartWithWindows => s.start_with_windows,
        }
    }

    pub fn flip(self, s: &mut Settings) {
        let v = !self.get(s);
        match self {
            SettingKey::OpenOnHover => s.open_on_hover = v,
            SettingKey::Lyrics => s.lyrics_enabled = v,
            SettingKey::WaterReminders => s.water_reminders = v,
            SettingKey::ClipboardHistory => s.clipboard_history = v,
            SettingKey::Sounds => s.sounds = v,
            SettingKey::HideFullscreen => s.hide_in_fullscreen = v,
            SettingKey::StartWithWindows => s.start_with_windows = v,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SettingKey::OpenOnHover => "Open on hover",
            SettingKey::Lyrics => "Synced lyrics",
            SettingKey::WaterReminders => "Water reminders",
            SettingKey::ClipboardHistory => "Clipboard history",
            SettingKey::Sounds => "Sounds",
            SettingKey::HideFullscreen => "Hide in fullscreen",
            SettingKey::StartWithWindows => "Start with Windows",
        }
    }
}

/// Everything clickable. Hover/press state is keyed by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Island,
    Tab(Page),
    Gear,
    MediaPrev,
    MediaPlayPause,
    MediaNext,
    FocusToggle,
    FocusReset,
    FocusMinus,
    FocusPlus,
    WaterAdd(u32),
    WaterUndo,
    HudWater,
    ShelfOpen(u16),
    ShelfReveal(u16),
    ShelfRemove(u16),
    TaskToggle(u16),
    TaskRemove(u16),
    TaskInput,
    TaskClearDone,
    ClipCopy(u16),
    ClipClear,
    Toggle(SettingKey),
    Idle(IdleLook),
    GoalStep(i8),
    Quit,
}

#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub rect: Rect,
    pub action: Action,
    /// Show an I-beam instead of a hand.
    pub text: bool,
}

pub fn page_index(p: Page) -> usize {
    Page::all().iter().position(|&x| x == p).unwrap_or(Page::all().len())
}

fn approach(cur: &mut f32, target: f32, rate: f32, dt: f32) -> bool {
    let k = 1.0 - (-rate * dt).exp();
    *cur += (target - *cur) * k;
    if (*cur - target).abs() < 0.002 {
        *cur = target;
        false
    } else {
        true
    }
}

pub struct UiState {
    pub hover: Option<Action>,
    pub pressed: Option<Action>,
    /// Pointer in window-local logical px.
    pub mouse: Option<(f32, f32)>,
    amt: HashMap<Action, f32>,
    tog: HashMap<SettingKey, f32>,
    pub tab_amt: [f32; 8],
    pub page: Page,
    pub prev_page: Page,
    pub page_t: f32,
    pub page_dir: f32,
    pub compact_w: f32,
    pub compact_h: f32,
    pub peek_until_ms: u64,
    pub peek_amt: f32,
    track_key: String,
    pub scroll: [usize; 8],
    /// Task text being typed (None = field not focused).
    pub input: Option<String>,
    pub toast: Option<(String, u64)>,
    pub clock_s: f32,
    pub water_level: f32,
}

impl UiState {
    pub fn new(page: Page) -> Self {
        let mut tab_amt = [0.0; 8];
        tab_amt[page_index(page).min(7)] = if page == Page::Settings { 0.0 } else { 1.0 };
        Self {
            hover: None,
            pressed: None,
            mouse: None,
            amt: HashMap::new(),
            tog: HashMap::new(),
            tab_amt,
            page,
            prev_page: page,
            page_t: 1.0,
            page_dir: 1.0,
            compact_w: 120.0,
            compact_h: 36.0,
            peek_until_ms: 0,
            peek_amt: 0.0,
            track_key: String::new(),
            scroll: [0; 8],
            input: None,
            toast: None,
            clock_s: 0.0,
            water_level: 0.0,
        }
    }

    /// Hover glow for `a` (0..1), with press counting as full.
    pub fn hover_amt(&self, a: Action) -> f32 {
        if self.pressed == Some(a) {
            return 1.0;
        }
        self.amt.get(&a).copied().unwrap_or(0.0)
    }

    pub fn toggle_amt(&self, k: SettingKey) -> f32 {
        self.tog.get(&k).copied().unwrap_or(0.0)
    }

    pub fn page_ease(&self) -> f32 {
        let t = self.page_t.clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }

    pub fn scroll_of(&self, p: Page) -> usize {
        self.scroll[page_index(p).min(7)]
    }

    pub fn scroll_by(&mut self, p: Page, delta: i32, max: usize) {
        let s = &mut self.scroll[page_index(p).min(7)];
        *s = (*s as i32 + delta).clamp(0, max as i32) as usize;
    }

    pub fn set_hover(&mut self, h: Option<Action>) -> bool {
        let changed = self.hover != h;
        self.hover = h;
        if changed {
            self.amt.entry_or(h);
        }
        changed
    }

    pub fn toast(&mut self, msg: &str, now_ms: u64) {
        self.toast = Some((msg.to_string(), now_ms + 1600));
    }

    /// Compact (collapsed) size the island wants right now.
    pub fn compact_target(&self, vm: &ViewModel, s: &Settings, now_ms: u64) -> (f32, f32) {
        if let Some(h) = vm.live_hud(now_ms) {
            use teardrop_core::HudKind::*;
            let w = match h.kind {
                Volume | Muted => 270.0,
                Water => 310.0,
                _ => 290.0,
            };
            return (w, 38.0);
        }
        if vm.focus_visible() {
            return (176.0, 36.0);
        }
        if vm.music_active() {
            return (if now_ms < self.peek_until_ms { 340.0 } else { 232.0 }, 36.0);
        }
        match s.idle_look {
            IdleLook::Clock => (116.0, 34.0),
            IdleLook::Eyes => (104.0, 34.0),
            IdleLook::Minimal => (70.0, 28.0),
            IdleLook::Hidden => (132.0, 6.0),
        }
    }

    /// True while a continuous (non-spring) animation wants frames.
    pub fn ambient(&self, vm: &ViewModel, s: &Settings, open: f32, now_ms: u64) -> bool {
        if open > 0.6 {
            return match vm.page {
                Page::Water => true,
                Page::Music => vm.track.playback == teardrop_core::Playback::Playing,
                Page::Tasks => self.input.is_some(),
                _ => false,
            } || self.toast.is_some();
        }
        if vm.live_hud(now_ms).is_some() {
            return true;
        }
        (vm.music_active() && vm.hud.is_none()) || (s.idle_look == IdleLook::Eyes && !vm.focus_visible())
    }

    /// Advance every animated value. Returns true while anything still moves.
    pub fn tick(&mut self, dt: f32, vm: &ViewModel, s: &Settings, now_ms: u64) -> bool {
        let dt = dt.clamp(0.0, 0.1);
        let mut moving = false;
        self.clock_s += dt;

        // Hover glow.
        let hover = self.hover;
        self.amt.entry_or(hover);
        let mut dead = Vec::new();
        for (k, v) in self.amt.iter_mut() {
            let target = if Some(*k) == hover { 1.0 } else { 0.0 };
            let m = approach(v, target, 22.0, dt);
            moving |= m;
            if !m && target == 0.0 {
                dead.push(*k);
            }
        }
        for k in dead {
            self.amt.remove(&k);
        }

        // Toggle knobs.
        for k in SettingKey::ALL {
            let e = self.tog.entry(k).or_insert(if k.get(s) { 1.0 } else { 0.0 });
            moving |= approach(e, if k.get(s) { 1.0 } else { 0.0 }, 18.0, dt);
        }

        // Tab pill.
        let want = page_index(vm.page);
        for i in 0..8 {
            let target = if i == want && vm.page != Page::Settings { 1.0 } else { 0.0 };
            moving |= approach(&mut self.tab_amt[i], target, 16.0, dt);
        }

        // Page slide.
        if vm.page != self.page {
            self.prev_page = self.page;
            self.page = vm.page;
            self.page_t = 0.0;
            self.page_dir = if page_index(vm.page) >= page_index(self.prev_page) { 1.0 } else { -1.0 };
            self.input = None;
        }
        if self.page_t < 1.0 {
            self.page_t = (self.page_t + dt * 5.5).min(1.0);
            moving = true;
        }

        // Compact size.
        let (tw, th) = self.compact_target(vm, s, now_ms);
        moving |= approach(&mut self.compact_w, tw, 14.0, dt);
        moving |= approach(&mut self.compact_h, th, 14.0, dt);

        // Track-change peek.
        let key = format!("{}\0{}", vm.track.title, vm.track.artist);
        if key != self.track_key {
            if !self.track_key.is_empty() && !vm.track.title.is_empty() {
                self.peek_until_ms = now_ms + 3400;
            }
            self.track_key = key;
        }
        let peek_target = if now_ms < self.peek_until_ms { 1.0 } else { 0.0 };
        moving |= approach(&mut self.peek_amt, peek_target, 10.0, dt);
        if now_ms < self.peek_until_ms {
            moving = true;
        }

        // Water level eases to the logged amount.
        let level = vm.water.progress(vm.day());
        moving |= approach(&mut self.water_level, level, 5.0, dt);

        if let Some((_, until)) = &self.toast {
            if now_ms >= *until {
                self.toast = None;
            }
        }
        moving
    }
}

/// Small helper so `amt.entry_or(Some(a))` reads naturally.
trait EntryOr {
    fn entry_or(&mut self, a: Option<Action>);
}

impl EntryOr for HashMap<Action, f32> {
    fn entry_or(&mut self, a: Option<Action>) {
        if let Some(a) = a {
            self.entry(a).or_insert(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn hover_glow_rises_and_falls() {
        let vm = ViewModel::demo(Utc::now());
        let s = Settings::default();
        let mut ui = UiState::new(Page::Music);
        ui.set_hover(Some(Action::MediaNext));
        for _ in 0..30 {
            ui.tick(1.0 / 60.0, &vm, &s, 0);
        }
        assert!(ui.hover_amt(Action::MediaNext) > 0.95);
        ui.set_hover(None);
        for _ in 0..90 {
            ui.tick(1.0 / 60.0, &vm, &s, 0);
        }
        assert_eq!(ui.hover_amt(Action::MediaNext), 0.0);
    }

    #[test]
    fn page_change_starts_a_slide_with_direction() {
        let mut vm = ViewModel::demo(Utc::now());
        let s = Settings::default();
        let mut ui = UiState::new(Page::Music);
        vm.page = Page::Tasks;
        ui.tick(1.0 / 60.0, &vm, &s, 0);
        assert!(ui.page_t < 1.0);
        assert_eq!(ui.prev_page, Page::Music);
        assert!(ui.page_dir > 0.0);
        for _ in 0..60 {
            ui.tick(1.0 / 60.0, &vm, &s, 0);
        }
        assert_eq!(ui.page_t, 1.0);
        vm.page = Page::Focus;
        ui.tick(1.0 / 60.0, &vm, &s, 0);
        assert!(ui.page_dir < 0.0, "going back slides the other way");
    }

    #[test]
    fn compact_size_follows_activity() {
        let now = Utc::now();
        let mut vm = ViewModel::demo(now);
        let s = Settings::default();
        let ui = UiState::new(Page::Music);
        let now_ms = now.timestamp_millis() as u64;
        let music = ui.compact_target(&vm, &s, now_ms).0;
        vm.track.playback = teardrop_core::Playback::Stopped;
        let idle = ui.compact_target(&vm, &s, now_ms).0;
        assert!(music > idle, "music {music} should be wider than idle {idle}");
    }

    #[test]
    fn scroll_is_clamped() {
        let mut ui = UiState::new(Page::Tasks);
        ui.scroll_by(Page::Tasks, 99, 3);
        assert_eq!(ui.scroll_of(Page::Tasks), 3);
        ui.scroll_by(Page::Tasks, -99, 3);
        assert_eq!(ui.scroll_of(Page::Tasks), 0);
    }
}
