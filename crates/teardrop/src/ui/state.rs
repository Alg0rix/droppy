//! Interaction + animation state that lives between frames.
//!
//! The view-model says *what* to show; this says *how far along* every
//! transition is (hover glow, sliding tab pill, page slide, water level,
//! compact width...). Everything eases with frame-rate independent
//! exponential smoothing so a dropped frame never looks like a jump.

use super::eyes::{self, Eyes, Senses, Spot};
use super::scene::Rect;
use super::theme::PAD;
use crate::viewmodel::{local_naive, ViewModel};
use std::collections::HashMap;
use teardrop_core::{AiProvider, EyeStyle, IdleLook, IslandTheme, NoteDateFilter, Page, Settings, Spring};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingKey {
    OpenOnHover,
    Lyrics,
    WaterReminders,
    ClipboardHistory,
    Sounds,
    HideFullscreen,
    StartWithWindows,
    MoodRing,
    WeatherMood,
    FocusApps,
    AiShareDay,
    HotkeyIsland,
    HotkeySearch,
    HotkeyNote,
    HotkeyAi,
    MeshChat,
    MeshHud,
}

impl SettingKey {
    pub const ALL: [SettingKey; 17] = [
        SettingKey::OpenOnHover,
        SettingKey::Lyrics,
        SettingKey::WaterReminders,
        SettingKey::ClipboardHistory,
        SettingKey::Sounds,
        SettingKey::HideFullscreen,
        SettingKey::StartWithWindows,
        SettingKey::MoodRing,
        SettingKey::WeatherMood,
        SettingKey::FocusApps,
        SettingKey::AiShareDay,
        SettingKey::HotkeyIsland,
        SettingKey::HotkeySearch,
        SettingKey::HotkeyNote,
        SettingKey::HotkeyAi,
        SettingKey::MeshChat,
        SettingKey::MeshHud,
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
            SettingKey::MoodRing => s.mood_ring,
            SettingKey::WeatherMood => s.weather_mood,
            SettingKey::FocusApps => s.focus_app_eyes,
            SettingKey::AiShareDay => s.ai_share_day,
            SettingKey::HotkeyIsland => s.hotkey_island,
            SettingKey::HotkeySearch => s.hotkey_search,
            SettingKey::HotkeyNote => s.hotkey_note,
            SettingKey::HotkeyAi => s.hotkey_ai,
            SettingKey::MeshChat => s.mesh_chat,
            SettingKey::MeshHud => s.mesh_hud,
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
            SettingKey::MoodRing => s.mood_ring = v,
            SettingKey::WeatherMood => s.weather_mood = v,
            SettingKey::FocusApps => s.focus_app_eyes = v,
            SettingKey::AiShareDay => s.ai_share_day = v,
            SettingKey::HotkeyIsland => s.hotkey_island = v,
            SettingKey::HotkeySearch => s.hotkey_search = v,
            SettingKey::HotkeyNote => s.hotkey_note = v,
            SettingKey::HotkeyAi => s.hotkey_ai = v,
            SettingKey::MeshChat => s.mesh_chat = v,
            SettingKey::MeshHud => s.mesh_hud = v,
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
            SettingKey::MoodRing => "Mood ring",
            SettingKey::WeatherMood => "Weather",
            SettingKey::FocusApps => "App eyes",
            SettingKey::AiShareDay => "Share my day",
            SettingKey::HotkeyIsland => "Win+Shift+T",
            SettingKey::HotkeySearch => "Alt+Space",
            SettingKey::HotkeyNote => "Alt+N",
            SettingKey::HotkeyAi => "Alt+I",
            SettingKey::MeshChat => "Mesh chat (BLE)",
            SettingKey::MeshHud => "Message previews",
        }
    }
}

/// Everything clickable. Hover/press state is keyed by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Island,
    Tab(Page),
    /// System page sub-tab (CPU / GPU / Memory / ...).
    SysTab(teardrop_core::SysTab),
    /// Settings chip: keep this page out of the nav and the page cycle.
    TogglePage(Page),
    /// Bring the n-th registered desktop window to the foreground.
    WinActivate(u16),
    /// Drag/click the n-th audio session's volume track (position -> level).
    MixerVol(u16),
    /// Focus the Windows page search field.
    WinSearch,
    /// Launch the n-th launcher hit on the Windows page.
    WinLaunch(u16),
    Gear,
    Pin,
    MediaPrev,
    MediaPlayPause,
    MediaNext,
    /// Seek bar hover/click region; the click turns into `Seek`.
    SeekBar,
    /// Seek to a position in per-mille (0..=1000) of the track.
    Seek(u16),
    FocusToggle,
    FocusReset,
    FocusMinus,
    FocusPlus,
    /// Step the eye theme for the app in front (wheel steps back).
    FocusAppTheme,
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
    /// Start a new note and focus the editor.
    NoteNew,
    /// Open the n-th note in the editor.
    NoteOpen(u16),
    /// Delete the n-th note (or close the editor if it is the open one).
    NoteRemove(u16),
    /// Save the open note and go back to the list.
    NoteClose,
    /// Select all text inside the note editor.
    NoteSelectAll,
    /// Focus the note search input.
    NoteSearchInput,
    /// Clear the note search input.
    NoteSearchClear,
    /// Set the note date filter.
    NoteDateFilter(NoteDateFilter),
    ClipCopy(u16),
    ClipClear,
    Toggle(SettingKey),
    Idle(IdleLook),
    EyeStyle(EyeStyle),
    /// Pick an island body theme (Settings swatch row).
    IslandTheme(IslandTheme),
    /// Set the birthday to today, or clear it (wheel nudges the day).
    Birthday,
    GoalStep(i8),
    /// Step the Today calendar by whole months.
    CalStep(i8),
    /// Jump the Today calendar back to the current month.
    CalToday,
    /// Select a day (`1..=31`) in the currently displayed calendar month.
    CalDay(u8),
    /// Open the detail view for the n-th event in the active calendar list.
    CalEvent(u8),
    /// Close the calendar event detail view back to the list.
    CalCloseEvent,
    /// Copy the currently opened calendar event's details to the clipboard.
    CalCopyEvent,
    /// Run the internet speed test.
    SpeedTest,
    /// Look for a newer release on GitHub.
    UpdateCheck,
    /// Download, verify and run the new installer.
    UpdateInstall,
    /// Re-read Claude / Codex / OpenCode usage.
    UsageRefresh,
    /// Start or stop the voice recorder.
    VoiceToggle,
    /// Open the recordings folder.
    VoiceFolder,
    /// Paste an iCal link from the clipboard as a calendar feed.
    CalendarPaste,
    /// Disconnect all calendars.
    CalendarClear,
    /// Open the join link of the n-th agenda entry.
    Join(u8),
    /// Open the join link of the next meeting (from its alert).
    MeetingJoin,
    /// Focus the AI chat prompt field.
    AiInput,
    /// Send the current AI chat prompt.
    AiSend,
    /// Send one of the 4 starter prompts (`0..4`).
    AiStarter(u8),
    /// Start a new AI conversation.
    AiNewChat,
    /// Toggle the AI conversation history drawer.
    AiToggleHistory,
    /// Open a conversation from the history drawer.
    AiSelectChat(u64),
    /// Delete a conversation from the history drawer.
    AiDeleteChat(u64),
    /// Toggle the AI provider & model setup panel.
    AiToggleSetup,
    /// Switch active AI provider.
    AiSetProvider(AiProvider),
    /// Cycle through available models (`-1` or `1`).
    AiCycleModel(i8),
    /// Select a model chip by index.
    AiSelectModel(u16),
    /// Fetch live models from the provider's `/models` endpoint.
    AiRefreshModels,
    /// Focus the custom model ID input in Setup.
    AiFocusModel,
    /// Focus the custom base URL input in Setup.
    AiFocusBaseUrl,
    /// Paste API key from clipboard (encrypted with DPAPI).
    AiPasteKey,
    /// Paste custom Base URL from clipboard.
    AiPasteUrl,
    /// Start or cancel `Sign in with ChatGPT` OAuth device flow.
    AiChatGptSignIn,
    /// Copy ChatGPT device code and open verification URL.
    AiChatGptCopyCode,
    /// Sign out of ChatGPT OAuth session.
    AiChatGptSignOut,
    /// Copy assistant message text at `msg_idx`.
    AiCopyReply(u16),
    /// Undo an AI-performed action `(msg_idx, action_idx)`.
    AiUndoAction(u16, u8),
    /// Quick page: toggle the Wi-Fi radio.
    QsWifi,
    /// Quick page: toggle the Bluetooth radio.
    QsBt,
    /// Quick page: toggle master output mute.
    QsMute,
    /// Quick page: toggle microphone mute.
    QsMicMute,
    /// Quick page: volume slider track (click/drag position -> level).
    QsVol,
    /// Quick page: brightness slider track (click/drag position -> level).
    QsBright,
    /// Quick page: open a Windows settings deep link.
    QsLink(QsLink),
    /// Quick page: open/close the radio's device list under the tiles.
    QsExpand(QsSection),
    /// Quick page: click a Wi-Fi network row (join / disconnect / ask key).
    QsWifiNet(u16),
    /// Quick page: click a Bluetooth device row (pair).
    QsBtDev(u16),
    /// Quick page: the Wi-Fi password field (text entry).
    QsPassField,
    /// Quick page: the Connect pill next to the password field.
    QsWifiConnect,
    /// Quick page: rescan networks / devices.
    QsRescan,
    /// Chat sidebar row: 0 = mesh room, n = peer index + 1.
    ChatSelect(u16),
    /// Focus the chat message input.
    ChatInput,
    /// Send the chat input.
    ChatSend,
    /// Edit our mesh nickname.
    ChatNick,
    Quit,
}

/// Which Quick-tile list is expanded under the tiles row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QsSection {
    #[default]
    None,
    Wifi,
    Bt,
}

/// A `ms-settings:` deep link opened from a Quick tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QsLink {
    Wifi,
    Bluetooth,
    NightLight,
    Airplane,
    BatterySaver,
    Display,
    Sound,
    Settings,
}

impl QsLink {
    pub fn uri(self) -> &'static str {
        match self {
            QsLink::Wifi => "ms-settings:network-wifi",
            QsLink::Bluetooth => "ms-settings:bluetooth",
            QsLink::NightLight => "ms-settings:nightlight",
            QsLink::Airplane => "ms-settings:network-airplanemode",
            QsLink::BatterySaver => "ms-settings:batterysaver",
            QsLink::Display => "ms-settings:display",
            QsLink::Sound => "ms-settings:sound",
            QsLink::Settings => "ms-settings:",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub rect: Rect,
    pub action: Action,
    /// Show an I-beam instead of a hand.
    pub text: bool,
}

/// Per-page animation slots (every tab plus Settings, with headroom).
pub const PAGE_SLOTS: usize = 17;

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
    pub tab_amt: [f32; PAGE_SLOTS],
    pub page: Page,
    pub prev_page: Page,
    pub page_t: f32,
    pub page_dir: f32,
    pub compact_w: f32,
    pub compact_h: f32,
    /// Open-card size, eased toward the current page's target so the island
    /// resizes itself per activity instead of snapping to one fixed card.
    pub card_w: f32,
    pub card_h: f32,
    /// Glance-pill size, eased per page like `card_w`/`card_h`.
    pub glance_w: f32,
    pub glance_h: f32,
    pub glance_ws: Spring,
    pub glance_hs: Spring,
    pub peek_until_ms: u64,
    pub peek_amt: f32,
    track_key: String,
    pub scroll: [usize; PAGE_SLOTS],
    /// Months the Today calendar is shifted from the current one.
    pub cal_offset: i32,
    /// Selected calendar date `(year, month, day)` on `Page::Today` (`None` = upcoming agenda).
    pub cal_selected_day: Option<(i32, u32, u32)>,
    /// Selected event index in the active calendar list for the detail view (`None` = list view).
    pub cal_selected_event: Option<usize>,
    /// Task / note / AI text being typed (None = field not focused).
    /// Task text being typed (None = field not focused).
    pub input: Option<super::textedit::TextField>,
    /// Selected sub-tab on the System page.
    pub sys_tab: teardrop_core::SysTab,
    /// Highlighted row in the Windows-page launcher results (arrow keys move it).
    pub win_sel: usize,
    /// Id of the note the input buffer belongs to (`None` while typing a brand
    /// new note that does not exist yet, or when `input` is not a note).
    pub note_edit: Option<String>,
    /// Active query in the note search box (`None` = search not focused).
    pub note_search: Option<super::textedit::TextField>,
    /// Active date filter for notes.
    pub note_date_filter: NoteDateFilter,
    pub toast: Option<(String, u64)>,
    pub clock_s: f32,
    pub water_level: f32,
    /// Island morph amount (0 collapsed .. 1 open), set by the shell each frame.
    pub island_open: f32,
    pub eyes: Eyes,
    pub viz: super::viz::Visualizer,
    /// Eased speed-test gauge value (Mbit/s).
    pub speed_val: f32,
    /// Files being carried over the island right now (0 = no drag).
    pub drag_files: u32,
    /// Eased 0..1 "something is hovering to be dropped".
    pub drop_amt: f32,
    /// 1 -> 0 burst right after a drop lands.
    pub absorb_t: f32,
    /// Which half of the card the drag is over: 0 = Shelf, 1 = Phone Link.
    pub drag_zone: u8,
    /// Eased `drag_zone` (0..1) so the highlight glides between halves.
    pub zone_amt: f32,
    /// The half the last drop landed on (for the burst colour).
    pub absorb_zone: u8,
    /// Recent microphone levels for the recorder's waveform (oldest first).
    pub voice_bars: [f32; 40],
    voice_acc: f32,
    /// Quick page: expanded device list under the tiles.
    pub qs_section: QsSection,
    /// SSID the Wi-Fi password field (`input`) is collecting a key for.
    pub qs_pass_ssid: Option<String>,
}

impl UiState {
    pub fn new(page: Page) -> Self {
        let mut tab_amt = [0.0; PAGE_SLOTS];
        tab_amt[page_index(page).min(PAGE_SLOTS - 1)] = if page == Page::Settings { 0.0 } else { 1.0 };
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
            compact_h: 42.0,
            card_w: 680.0,
            card_h: 318.0,
            glance_w: Self::glance_target(page).0,
            glance_h: Self::glance_target(page).1,
            glance_ws: Spring::new(Self::glance_target(page).0, 170.0, 20.0),
            glance_hs: Spring::new(Self::glance_target(page).1, 170.0, 20.0),
            peek_until_ms: 0,
            peek_amt: 0.0,
            track_key: String::new(),
            scroll: [0; PAGE_SLOTS],
            cal_offset: 0,
            cal_selected_day: None,
            cal_selected_event: None,
            input: None,
            sys_tab: teardrop_core::SysTab::Cpu,
            win_sel: 0,
            note_edit: None,
            note_search: None,
            note_date_filter: NoteDateFilter::All,
            toast: None,
            clock_s: 0.0,
            water_level: 0.0,
            island_open: 0.0,
            eyes: Eyes::new(0x7EA2_D20F),
            viz: Default::default(),
            speed_val: 0.0,
            drag_files: 0,
            drop_amt: 0.0,
            absorb_t: 0.0,
            drag_zone: 0,
            zone_amt: 0.0,
            absorb_zone: 0,
            voice_bars: [0.0; 40],
            voice_acc: 0.0,
            qs_section: QsSection::None,
            qs_pass_ssid: None,
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

    /// Back-out ease for the slide offset: the new page overshoots ~8%
    /// then settles, like water finding its level.
    pub fn page_ease_back(&self) -> f32 {
        let t = self.page_t.clamp(0.0, 1.0);
        let u = t - 1.0;
        1.0 + 2.7 * u * u * u + 1.7 * u * u
    }

    pub fn scroll_of(&self, p: Page) -> usize {
        self.scroll[page_index(p).min(PAGE_SLOTS - 1)]
    }

    pub fn scroll_by(&mut self, p: Page, delta: i32, max: usize) {
        let s = &mut self.scroll[page_index(p).min(PAGE_SLOTS - 1)];
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
                Volume | Muted | Brightness if s.idle_look == IdleLook::Eyes => 270.0 + super::layout::HUD_FACE_W,
                Volume | Muted | Brightness => 270.0,
                Water => 310.0,
                Meeting => 340.0,
                Bluetooth if s.idle_look == IdleLook::Eyes => 350.0 + super::layout::HUD_FACE_W,
                Bluetooth => 350.0,
                _ if s.idle_look == IdleLook::Eyes => 290.0 + super::layout::HUD_FACE_W,
                _ => 290.0,
            };
            // The Bluetooth card stacks a name over its status, so it is taller.
            return (w, if h.kind == Bluetooth { 62.0 } else { 44.0 });
        }
        if vm.voice.phase == crate::voice::VoicePhase::Recording {
            return (if s.idle_look == IdleLook::Eyes { 214.0 } else { 170.0 }, 42.0);
        }
        if vm.meeting_soon().is_some() {
            return (236.0, 42.0);
        }
        if vm.focus_visible() {
            // The ring and countdown share the pill with the music capsule:
            // ring + art + title/lyric + visualizer + face + mm:ss.
            if vm.music_active() {
                if let Some((_, w, h)) = crate::ui::layout::compact_lyric(vm, s) {
                    return ((w + 100.0).min(720.0), h.max(42.0));
                }
                let line = format!("{} \u{2014} {}", vm.track.title, vm.track.display_artist());
                let tail = if s.idle_look == IdleLook::Eyes { 44.0 + 42.0 } else { 44.0 };
                let w = 46.0 + crate::ui::layout::est_w(&line, 12.0) * 1.15 + 12.0 + tail + 100.0;
                return (w.clamp(300.0, 640.0), 42.0);
            }
            return (176.0, 42.0);
        }
        if vm.music_active() {
            // A synced lyric line takes over the capsule and may grow taller.
            if let Some((_, w, h)) = crate::ui::layout::compact_lyric(vm, s) {
                return (w, h.max(42.0));
            }
            // art + title + visualizer; wide enough for the whole title up to a cap.
            let line = format!("{} \u{2014} {}", vm.track.title, vm.track.display_artist());
            let tail = if s.idle_look == IdleLook::Eyes { 44.0 + 42.0 } else { 44.0 };
            let w = 46.0 + crate::ui::layout::est_w(&line, 12.0) * 1.15 + 12.0 + tail;
            return (w.clamp(232.0, 390.0), 42.0);
        }
        match s.idle_look {
            IdleLook::Clock => (116.0, 38.0),
            IdleLook::Eyes => (128.0, 46.0),
            IdleLook::Minimal => (70.0, 30.0),
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
                Page::Notes => self.input.is_some(),
                Page::Speed => vm.speed.phase.running(),
                Page::Voice => vm.voice.phase == crate::voice::VoicePhase::Recording,
                Page::Usage => vm.usage.refreshing,
                Page::Ai => vm.ai.busy || vm.ai.chatgpt_login_busy || vm.ai.models_loading || self.input.is_some(),
                _ => false,
            } || self.toast.is_some();
        }
        if vm.live_hud(now_ms).is_some() {
            return true;
        }
        vm.voice.phase == crate::voice::VoicePhase::Recording || (vm.music_active() && vm.hud.is_none()) || s.idle_look == IdleLook::Eyes
    }

    /// Where the eyes sit on the collapsed island right now, if anywhere.
    pub fn face_spot(&self, vm: &ViewModel, s: &Settings, now_ms: u64) -> Option<Spot> {
        if s.idle_look != IdleLook::Eyes {
            return None;
        }
        if let Some(h) = vm.live_hud(now_ms) {
            use teardrop_core::HudKind::*;
            return matches!(h.kind, Volume | Muted | Brightness | Charging | Bluetooth | Device | Timer).then_some(Spot::Hud);
        }
        if vm.meeting_soon().is_some() {
            None
        } else if vm.voice.phase == crate::voice::VoicePhase::Recording {
            Some(Spot::Rec)
        } else if vm.focus_visible() {
            if vm.music_active() { Some(Spot::FocusMusic) } else { Some(Spot::Focus) }
        } else if vm.music_active() {
            Some(Spot::Music)
        } else {
            Some(Spot::Idle)
        }
    }

    /// Glance-pill size for `page` (w, h): the mid-morph peek is sized for
    /// what each page wants to show quickly.
    pub fn glance_target(page: Page) -> (f32, f32) {
        let w = match page {
            Page::Music => 560.0,
            Page::Focus => 440.0,
            Page::Water => 420.0,
            Page::Shelf => 500.0,
            Page::Tasks => 460.0,
            Page::Notes => 500.0,
            Page::Clipboard => 480.0,
            Page::Today => 520.0,
            Page::System => 480.0,
            Page::Quick => 460.0,
            Page::Speed => 440.0,
            Page::Usage => 480.0,
            Page::Voice => 420.0,
            Page::Ai => 480.0,
            Page::Chat => 460.0,
            Page::Windows => 500.0,
            Page::Settings => 440.0,
        };
        (w, 80.0)
    }

    /// Open-card size the island wants for `page` (w, h). Pages drawn as
    /// blobs get their own size; the rest share the classic card.
    pub fn card_target(page: Page) -> (f32, f32) {
        match page {
            Page::Music => (520.0, 280.0),
            Page::Focus => (400.0, 300.0),
            Page::Water => (420.0, 240.0),
            Page::Shelf => (560.0, 290.0),
            Page::Tasks => (440.0, 280.0),
            Page::Notes => (620.0, 310.0),
            Page::Clipboard => (480.0, 270.0),
            Page::Today => (620.0, 310.0),
            Page::System => (720.0, 400.0),
            Page::Quick => (500.0, 340.0),
            Page::Speed => (420.0, 270.0),
            Page::Usage => (480.0, 270.0),
            Page::Voice => (420.0, 260.0),
            Page::Ai => (640.0, 400.0),
            Page::Chat => (620.0, 330.0),
            Page::Windows => (500.0, 300.0),
            Page::Settings => (680.0, 390.0),
        }
    }

    /// The collapsed body rect in window-local px (matches `layout::build`).
    pub fn compact_body(&self) -> Rect {
        Rect::new(PAD, 0.0, self.compact_w, self.compact_h)
    }

    fn tick_eyes(&mut self, dt: f32, vm: &ViewModel, s: &Settings, now_ms: u64) {
        let spot = self.face_spot(vm, s, now_ms);
        let body = self.compact_body();
        let collapsed = self.island_open < 0.12;
        let (fx, fy, k) = eyes::place(spot.unwrap_or(Spot::Idle), body);
        let senses = Senses {
            local: local_naive(vm.now),
            visible: spot.is_some() && collapsed,
            mouse: self.mouse.map(|(x, y)| ((x - fx) / k, (y - fy) / k)),
            hovering: collapsed && self.hover == Some(Action::Island),
            track: &vm.track,
            music: vm.music_active(),
            focus: (spot == Some(Spot::Focus)).then_some(vm.pomo.phase),
            sys: &vm.system,
            water: vm.water.progress(vm.day()),
            env: &vm.env,
            set: s,
            area: eyes::area(spot.unwrap_or(Spot::Idle), body),
            thinking: vm.ai.busy,
            focus_left: (matches!(spot, Some(Spot::Focus | Spot::FocusMusic)) && vm.focus_running()).then(|| vm.focus_remaining_ms() as f32 / 1000.0),
            app: vm.focus_app(s).and_then(|a| a.theme),
            recording: (vm.voice.phase == crate::voice::VoicePhase::Recording).then_some(vm.voice.level.clamp(0.0, 1.0)),
        };
        self.eyes.tick(dt, &senses);
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
        for i in 0..PAGE_SLOTS {
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
            self.note_edit = None;
            self.qs_section = QsSection::None;
            self.qs_pass_ssid = None;
        }
        if self.page_t < 1.0 {
            self.page_t = (self.page_t + dt * 5.5).min(1.0);
            moving = true;
        }

        // Compact size.
        let (tw, th) = self.compact_target(vm, s, now_ms);
        moving |= approach(&mut self.compact_w, tw, 14.0, dt);
        moving |= approach(&mut self.compact_h, th, 14.0, dt);

        // Open-card size follows the page (all pages share 680x318 today;
        // redesigned pages can claim their own blob size).
        let (cw, ch) = Self::card_target(vm.page);
        moving |= approach(&mut self.card_w, cw, 14.0, dt);
        moving |= approach(&mut self.card_h, ch, 14.0, dt);

        // Glance size eases per page too, so switching pages while peeking
        // reflows the pill instead of snapping.
        let (gw, gh) = Self::glance_target(vm.page);
        moving |= self.glance_ws.step(gw, dt);
        moving |= self.glance_hs.step(gh, dt);
        self.glance_w = self.glance_ws.value;
        self.glance_h = self.glance_hs.value;

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

        moving |= approach(&mut self.drop_amt, if self.drag_files > 0 { 1.0 } else { 0.0 }, 12.0, dt);
        moving |= approach(&mut self.zone_amt, self.drag_zone as f32, 14.0, dt);
        if self.drag_files > 0 {
            moving = true;
        }
        if self.absorb_t > 0.0 {
            self.absorb_t = (self.absorb_t - dt / 0.8).max(0.0);
            moving = true;
        }

        // Recorder waveform: a new bar every 55 ms while recording, decaying after.
        self.voice_acc += dt;
        if self.voice_acc >= 0.055 {
            self.voice_acc = 0.0;
            let lvl = if vm.voice.phase == crate::voice::VoicePhase::Recording { vm.voice.level.powf(0.6) } else { 0.0 };
            self.voice_bars.rotate_left(1);
            self.voice_bars[39] = lvl;
        }
        if vm.voice.phase == crate::voice::VoicePhase::Recording || self.voice_bars.iter().any(|b| *b > 0.0) {
            moving = true;
        }

        let sp = &vm.speed;
        let target = match sp.phase {
            crate::viewmodel::SpeedPhase::Up => sp.up,
            _ => sp.down,
        };
        moving |= approach(&mut self.speed_val, target, 6.0, dt);

        let playing = vm.track.playback == teardrop_core::Playback::Playing;
        self.viz.tick(dt, vm.env.audio_peak, playing);
        if self.viz.active() {
            moving = true;
        }

        self.tick_eyes(dt, vm, s, now_ms);

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
    fn recording_shows_the_face_with_eyes() {
        let now = Utc::now();
        let mut vm = ViewModel::demo(now);
        let mut s = Settings::default();
        let ui = UiState::new(Page::Music);
        let now_ms = now.timestamp_millis() as u64;
        vm.voice.phase = crate::voice::VoicePhase::Recording;
        s.idle_look = IdleLook::Eyes;
        assert_eq!(ui.face_spot(&vm, &s, now_ms), Some(Spot::Rec));
        let with = ui.compact_target(&vm, &s, now_ms).0;
        s.idle_look = IdleLook::Clock;
        assert_eq!(ui.face_spot(&vm, &s, now_ms), None);
        let without = ui.compact_target(&vm, &s, now_ms).0;
        assert!(with > without, "eyes {with} should be wider than plain {without}");
    }

    #[test]
    fn focus_plus_music_shares_the_pill() {
        let now = Utc::now();
        let mut vm = ViewModel::demo(now); // demo has a track playing
        let mut s = Settings::default();
        let ui = UiState::new(Page::Music);
        let now_ms = now.timestamp_millis() as u64;
        s.idle_look = IdleLook::Eyes;
        vm.pomo.start_focus(now);
        assert_eq!(ui.face_spot(&vm, &s, now_ms), Some(Spot::FocusMusic));
        assert!(ui.compact_target(&vm, &s, now_ms).0 > 176.0, "wider than the focus-only pill");
        vm.track.playback = teardrop_core::Playback::Stopped;
        assert_eq!(ui.face_spot(&vm, &s, now_ms), Some(Spot::Focus));
    }

    #[test]
    fn compact_lyric_sizes_capsule() {
        let now = Utc::now();
        let mut vm = ViewModel::demo(now);
        let s = Settings::default();
        let ui = UiState::new(Page::Music);
        let now_ms = now.timestamp_millis() as u64;
        let title_size = ui.compact_target(&ViewModel::demo(now), &s, now_ms);

        // Short line: one subline, capsule keeps the 42px height, whole line fits.
        let line = "Ripples keep the shape of you";
        vm.lyric_lines = teardrop_core::parse_lrc("[00:00.00]Ripples keep the shape of you\n[00:05.00]Surface tension holding true\n");
        vm.lyric_idx = Some(0);
        let (w, h) = ui.compact_target(&vm, &s, now_ms);
        assert_eq!(h, 42.0);
        let chrome = 46.0 + 44.0 + 12.0;
        assert!(w >= chrome + crate::ui::layout::est_w(line, 13.0) * 1.2, "w {w} should fit the whole line");

        // Very long line: width capped, capsule grows, no word dropped.
        let words: Vec<String> = (0..40).map(|i| format!("word{i:02}")).collect();
        let long = words.join(" ");
        vm.lyric_lines = teardrop_core::parse_lrc(&format!("[00:00.00]{long}\n[00:05.00]next line\n"));
        vm.lyric_idx = Some(0);
        let (w, h) = ui.compact_target(&vm, &s, now_ms);
        assert!(w <= crate::ui::layout::LYRIC_MAX_W, "w {w} exceeds cap");
        assert!(h > 36.0, "long lyric should grow the capsule");
        let (subs, ..) = crate::ui::layout::compact_lyric(&vm, &s).unwrap();
        assert!(subs.len() > 1);
        let joined = subs.join(" ");
        for word in &words {
            assert!(joined.contains(word.as_str()), "dropped {word}");
        }

        // Lyrics disabled -> identical to the no-lyrics title size.
        let mut off = s.clone();
        off.lyrics_enabled = false;
        assert_eq!(ui.compact_target(&vm, &off, now_ms), title_size);

        // Empty-text current line (instrumental gap) -> title size.
        vm.lyric_lines = vec![teardrop_core::LyricLine { at_ms: 0, text: "   ".into() }];
        vm.lyric_idx = Some(0);
        assert_eq!(ui.compact_target(&vm, &s, now_ms), title_size);
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
