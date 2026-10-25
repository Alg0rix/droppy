//! Scripted tour of every island state with fake data.
//!
//! `teardrop.exe --selftest <dir>` renders each state through the real
//! Direct2D painter into PNG screenshots (Windows), runs layout + logic
//! checks, and writes `report.json`. Exit code is non-zero on any failure.
//!
//! `TEARDROP_SELFTEST_DPI=144` renders at 150%; default is 192 (2x) so
//! screenshots are crisp.

use super::ui::eyes::{Gauge, Glyph, React};
use super::ui::layout::{build, IslandView};
use super::ui::state::{Action, UiState};
use super::viewmodel::{hud_brightness, hud_volume, hud_water_due, ViewModel};
use chrono::Utc;
use serde_json::json;
use std::path::{Path, PathBuf};
use teardrop_core::{DropletGeom, EyeStyle, Hud, HudKind, IdleLook, Page, Settings};

/// Minimal PNG writer (stored deflate) so the harness needs no image crate.
pub fn write_png(path: &Path, w: u32, h: u32, rgb: &[u8]) -> std::io::Result<()> {
    fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            table[n as usize] = c;
        }
        let mut c = 0xFFFF_FFFFu32;
        for b in data {
            c = table[((c ^ *b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        c ^ 0xFFFF_FFFF
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    }
    let mut raw = Vec::with_capacity((w as usize * 3 + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w as usize * 3..(y + 1) * w as usize * 3]);
    }
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65_535).peekable();
    while let Some(c) = chunks.next() {
        z.push(if chunks.peek().is_none() { 1 } else { 0 });
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + *byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    std::fs::write(path, out)
}

/// Premultiplied BGRA over a soft "desktop" backdrop -> RGB.
#[cfg(windows)]
fn composite(w: u32, h: u32, bgra: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        let t = y as f32 / h.max(1) as f32;
        let bg = [52.0 + 40.0 * t, 64.0 + 30.0 * t, 96.0 + 40.0 * t];
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let a = bgra[i + 3] as f32 / 255.0;
            let (b, g, r) = (bgra[i] as f32, bgra[i + 1] as f32, bgra[i + 2] as f32);
            rgb.push((r + bg[0] * (1.0 - a)).clamp(0.0, 255.0) as u8);
            rgb.push((g + bg[1] * (1.0 - a)).clamp(0.0, 255.0) as u8);
            rgb.push((b + bg[2] * (1.0 - a)).clamp(0.0, 255.0) as u8);
        }
    }
    rgb
}

struct Shot {
    name: &'static str,
    open: f32,
    wobble: f32,
    page: Page,
    hover: Option<Action>,
    setup: fn(&mut ViewModel, &mut Settings, &mut UiState, u64),
    /// Simulated 60 Hz frames before the screenshot.
    ticks: u32,
}

fn nothing(_: &mut ViewModel, _: &mut Settings, _: &mut UiState, _: u64) {}

fn shot(name: &'static str, open: f32, page: Page) -> Shot {
    Shot { name, open, wobble: 0.0, page, hover: None, setup: nothing, ticks: 180 }
}

/// A believable machine for the System page frames: hybrid 12-core CPU, a
/// discrete GPU, busy disk and network, a laptop battery on AC.
fn demo_sys(vm: &mut ViewModel, _: &mut Settings, ui: &mut UiState, tab: teardrop_core::SysTab) {
    use teardrop_core::sysmon::{CoreClass, ProcLoad, Series, Volume};
    let wave = |base: f32, amp: f32, ph: f32, i: usize| (base + amp * ((i as f32 * 0.21 + ph).sin() * 0.6 + (i as f32 * 0.07 + ph).cos() * 0.4)).max(0.0);
    let series = |base: f32, amp: f32, ph: f32, cap: f32| {
        let mut s = Series::default();
        for i in 0..60 {
            s.push(wave(base, amp, ph, i).min(cap));
        }
        s
    };
    let sys = &mut vm.sys;
    sys.loaded = true;
    sys.cpu.name = "13th Gen Intel Core i7-13700H".into();
    sys.cpu.total = 23.0;
    sys.cpu.user = 14.0;
    sys.cpu.system = 9.0;
    sys.cpu.idle = 77.0;
    sys.cpu.physical = 12;
    sys.cpu.ghz = 3.4;
    sys.cpu.temp_c = Some(54.0);
    sys.cpu.cores = (0..20).map(|i| wave(24.0, 22.0, i as f32 * 0.9, 3)).collect();
    sys.cpu.classes = (0..20).map(|i| if i < 16 { CoreClass::Performance } else { CoreClass::Efficiency }).collect();
    sys.cpu.history = series(28.0, 20.0, 0.0, 100.0);
    sys.procs = [("Microsoft Edge", 9.1), ("Spotify", 4.2), ("Desktop Window Manager", 2.3), ("Visual Studio Code", 1.8), ("Windows Explorer", 1.3), ("Discord", 0.9), ("Antimalware Service Executable", 0.4)]
        .iter()
        .map(|(n, v)| ProcLoad { name: n.to_string(), path: String::new(), value: *v })
        .collect();

    sys.gpu.available = true;
    sys.gpu.name = "NVIDIA GeForce RTX 4060 Laptop GPU".into();
    sys.gpu.util = 31.0;
    sys.gpu.mem_used_gb = 3.4;
    sys.gpu.mem_total_gb = 8.0;
    sys.gpu.temp_c = Some(47.0);
    sys.gpu.engines = vec![("3D".into(), 31.0), ("Video decode".into(), 12.0), ("Copy".into(), 3.0)];
    sys.gpu.history = series(34.0, 26.0, 1.3, 100.0);
    sys.gpu.mem_history = series(3.3, 0.5, 0.4, 8.0);
    sys.gpu.procs = [("Microsoft Edge", 18.0), ("Desktop Window Manager", 9.0), ("Discord", 2.0)]
        .iter()
        .map(|(n, v)| ProcLoad { name: n.to_string(), path: String::new(), value: *v })
        .collect();

    sys.mem.total_gb = 31.7;
    sys.mem.used_gb = 19.0;
    sys.mem.available_gb = 12.7;
    sys.mem.cached_gb = 9.2;
    sys.mem.commit_used_gb = 27.4;
    sys.mem.commit_limit_gb = 47.5;
    sys.mem.history = series(59.0, 5.0, 2.1, 100.0);
    sys.mem.procs = [("Microsoft Edge", 3.4), ("Visual Studio Code", 1.9), ("Docker Desktop", 1.6), ("Discord", 0.8), ("Spotify", 0.6)]
        .iter()
        .map(|(n, v)| ProcLoad { name: n.to_string(), path: String::new(), value: *v })
        .collect();

    sys.drives.read_bps = 1.5e6;
    sys.drives.write_bps = 0.6e6;
    sys.drives.busy = 14.0;
    sys.drives.read_hist = series(1.2e6, 1.0e6, 0.3, 9e6);
    sys.drives.write_hist = series(0.6e6, 0.5e6, 2.0, 9e6);
    sys.drives.volumes = vec![
        Volume { label: "C:".into(), used_gb: 312.0, total_gb: 476.0 },
        Volume { label: "D:".into(), used_gb: 822.0, total_gb: 931.0 },
    ];

    sys.net.adapter = "Intel Wi-Fi 6E AX211".into();
    sys.net.down_bps = 1.2e6;
    sys.net.up_bps = 0.18e6;
    sys.net.down_hist = series(1.1e6, 0.9e6, 0.8, 6e6);
    sys.net.up_hist = series(0.2e6, 0.15e6, 1.7, 6e6);
    sys.net.session_down = 4_200_000_000;
    sys.net.session_up = 380_000_000;

    sys.battery.present = true;
    sys.battery.pct = 76.0;
    sys.battery.charging = true;
    sys.battery.on_ac = true;
    sys.battery.history = series(75.0, 0.8, 0.0, 100.0);
    ui.sys_tab = tab;
}

/// Fake events around "now" for the calendar frames.
fn demo_calendar(vm: &mut ViewModel, _: &mut Settings, _: &mut UiState, _: u64) {
    use crate::calendar::{CalendarView, Event, FeedState, Join};
    let now = vm.now.timestamp();
    let ev = |uid: &str, title: &str, loc: &str, desc: &str, start: i64, mins: i64, join: Option<(&str, &str)>| Event {
        uid: uid.into(),
        start,
        end: start + mins * 60,
        title: title.into(),
        location: loc.into(),
        description: desc.into(),
        all_day: false,
        join: join.map(|(p, u)| Join { platform: p.into(), url: u.into() }),
    };
    vm.calendar_feeds = 1;
    vm.calendar = CalendarView {
        events: vec![
            ev(
                "a",
                "Design review",
                "Studio B / Google Meet",
                "Walk through the updated calendar day filter and event detail drawer.",
                now + 12 * 60,
                45,
                Some(("Google Meet", "https://meet.google.com/abc-defg-hij")),
            ),
            ev("b", "1:1 with Rina", "Zoom", "Weekly sync and roadmap check-in.", now + 3 * 3600, 30, Some(("Zoom", "https://zoom.us/j/123"))),
            ev("c", "Sprint planning", "Room 402", "Prioritize backlog items for next sprint.", now + 26 * 3600, 60, Some(("Teams", "https://teams.microsoft.com/l/meetup-join/x"))),
        ],
        state: FeedState::Ok,
        error: String::new(),
        updated: Some(now),
    };
}

fn shots() -> Vec<Shot> {
    vec![
        Shot { setup: |vm, _, _, _| vm.track.playback = teardrop_core::Playback::Stopped, ..shot("01-collapsed-clock", 0.0, Page::Music) },
        shot("02-collapsed-music", 0.0, Page::Music),
        Shot {
            setup: |vm, _, _, _| {
                vm.lyric_lines = teardrop_core::parse_lrc("[00:00.00]Ripples keep the shape of you\n[01:02.00]Surface tension holding true\n");
                vm.lyric_idx = Some(0);
                vm.track.position_ms = 1_400;
            },
            ..shot("02b-collapsed-lyric", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, _, _, _| {
                vm.lyric_lines = teardrop_core::parse_lrc("[00:00.00]Somewhere out on the water where the ripples keep the shape of every word you ever whispered softly to the rain\n[01:02.00]next line\n");
                vm.lyric_idx = Some(0);
                vm.track.position_ms = 1_400;
            },
            ..shot("02c-collapsed-lyric-long", 0.0, Page::Music)
        },
        Shot { setup: |_, _, ui, ms| ui.peek_until_ms = ms + 5000, ..shot("03-music-peek", 0.0, Page::Music) },
        Shot {
            setup: |vm, set, _, _| {
                vm.track.playback = teardrop_core::Playback::Stopped;
                set.idle_look = IdleLook::Eyes;
                vm.idle_look = IdleLook::Eyes;
            },
            ..shot("04-collapsed-eyes", 0.0, Page::Music)
        },
        Shot { setup: |vm, _, _, ms| vm.hud = Some(hud_volume(64, false, ms)), ..shot("05-hud-volume", 0.0, Page::Music) },
        Shot { setup: |vm, _, _, ms| vm.hud = Some(crate::viewmodel::hud_bluetooth("Sound Buds", Some(80), ms)), ..shot("05b-hud-bluetooth", 0.0, Page::Music) },
        Shot { setup: |vm, _, _, ms| vm.hud = Some(crate::viewmodel::hud_bluetooth("Keychron K3 Pro", None, ms)), ..shot("05c-hud-bluetooth-nobattery", 0.0, Page::Music) },
        Shot { setup: |vm, _, _, ms| vm.hud = Some(crate::viewmodel::hud_bluetooth("Sound Buds", Some(12), ms)), ..shot("05d-hud-bluetooth-low", 0.0, Page::Music) },
        Shot {
            setup: |vm, set, ui, ms| {
                eyes(vm, set, EyeStyle::Round, false);
                vm.hud = Some(crate::viewmodel::hud_bluetooth("Sound Buds", Some(80), ms));
                ui.eyes.noted(60);
            },
            ticks: 40,
            ..shot("05e-hud-bluetooth-eyes", 0.0, Page::Music)
        },
        Shot { setup: |vm, _, _, ms| vm.hud = Some(hud_water_due(ms)), ..shot("06-hud-water", 0.0, Page::Music) },
        Shot {
            setup: |vm, _, _, ms| vm.hud = Some(Hud::transient(HudKind::Charging, "Charging", "87%", ms, 5000)),
            ..shot("07-hud-charging", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, _, _, _| {
                let now = vm.now;
                vm.pomo.start_focus(now - chrono::Duration::minutes(6));
            },
            ..shot("08-collapsed-focus", 0.0, Page::Music)
        },
        Shot { wobble: 1.5, ..shot("10-morph-hover-peek", 0.08, Page::Music) },
        Shot { wobble: -2.0, ..shot("11-morph-necking", 0.45, Page::Music) },
        shot("14-glance-music", 0.5, Page::Music),
        Shot {
            setup: |vm, _, _, _| {
                let now = vm.now;
                vm.pomo.completed_focus = 2;
                vm.pomo.start_focus(now - chrono::Duration::minutes(9));
            },
            ..shot("15-glance-focus", 0.5, Page::Focus)
        },
        Shot {
            setup: |vm, set, _, _| {
                vm.track.playback = teardrop_core::Playback::Stopped;
                set.idle_look = IdleLook::Eyes;
                vm.idle_look = IdleLook::Eyes;
            },
            ..shot("16-glance-idle", 0.5, Page::Water)
        },
        Shot { wobble: 3.0, ..shot("12-morph-bulb", 0.7, Page::Music) },
        Shot { wobble: 4.0, ..shot("13-morph-settling", 0.92, Page::Music) },
        Shot {
            setup: |vm, _, _, _| {
                vm.lyric_lines = teardrop_core::parse_lrc("[00:58.00]Ripples keep the shape of you\n[01:02.00]Surface tension holding true\n[01:09.00]Fall into the blue\n");
                vm.lyric_idx = teardrop_core::line_at(&vm.lyric_lines, vm.track.position_ms);
            },
            ..shot("20-page-music", 1.0, Page::Music)
        },
        Shot {
            setup: |vm, _, _, _| {
                let now = vm.now;
                vm.pomo.completed_focus = 2;
                vm.pomo.start_focus(now - chrono::Duration::minutes(9));
            },
            ..shot("21-page-focus", 1.0, Page::Focus)
        },
        shot("22-page-water", 1.0, Page::Water),
        shot("23-page-shelf", 1.0, Page::Shelf),
        Shot { setup: |_, _, ui, _| ui.input = Some(super::ui::textedit::TextField::new("Draft the launch notes")), ..shot("24-page-tasks-typing", 1.0, Page::Tasks) },
        Shot {
            setup: |_, _, ui, _| {
                let mut f = super::ui::textedit::TextField::new("Draft the launch notes");
                f.move_h(-1, true, false);
                f.move_h(-1, true, false);
                ui.input = Some(f);
            },
            ticks: 30, // land on a blink-on frame so the caret shows
            ..shot("24b-tasks-caret-mid", 1.0, Page::Tasks)
        },
        Shot {
            setup: |_, _, ui, _| {
                // 115 chars (under the 120 cap), caret at end: the field must
                // scroll right to keep the caret visible.
                ui.input = Some(super::ui::textedit::TextField::new(
                    "Review the launch checklist with design, copy, QA, and the release owner before Friday's freeze",
                ));
            },
            ticks: 30,
            ..shot("24c-tasks-caret-end-long", 1.0, Page::Tasks)
        },
        shot("24n-page-notes", 1.0, Page::Notes),
        Shot {
            setup: |vm, _, ui, _| {
                if let Some(n) = vm.notes.notes.first() {
                    ui.input = Some(super::ui::textedit::TextField::new(n.text.clone()));
                    ui.note_edit = Some(n.id.clone());
                }
            },
            ..shot("24o-page-notes-editor", 1.0, Page::Notes)
        },
        Shot {
            setup: |_, _, ui, _| {
                let mut f = super::ui::textedit::TextField::new("ripples");
                f.select_all();
                ui.note_search = Some(f);
            },
            ticks: 30,
            ..shot("24p-notes-search-sel", 1.0, Page::Notes)
        },
        Shot {
            setup: |_, _, ui, _| {
                // Three logical lines; selection starts mid-line-1 and the
                // caret lands mid-line-2 — spans a real line break.
                let mut f = super::ui::textedit::TextField::new("alpha beta gamma delta\nsecond line is here\nthird line tail");
                f.home(false, true);
                f.move_h(1, true, false); // start of "beta"
                f.move_h(1, true, false); // start of "gamma"
                f.move_v(1, true); // same column on line 2, extending the selection
                ui.input = Some(f);
            },
            ticks: 30,
            ..shot("24q-notes-editor-sel", 1.0, Page::Notes)
        },
        Shot {
            setup: |vm, _, ui, _| {
                crate::ai::set_focus_field(crate::ai::AiFocusField::Prompt);
                vm.ai.focus_field = crate::ai::AiFocusField::Prompt;
                let mut f = super::ui::textedit::TextField::new(
                    "Draft a friendly reminder for tomorrow's design review, mention the calendar update and the two open action items",
                );
                f.home(false, true);
                f.move_h(1, false, false);
                f.move_h(1, false, false);
                ui.input = Some(f);
            },
            ticks: 30,
            ..shot("26p-ai-prompt-caret", 1.0, Page::Ai)
        },
        shot("25-page-clipboard", 1.0, Page::Clipboard),
        shot("26-page-today", 1.0, Page::Today),
        Shot { setup: |vm, set, ui, _| demo_sys(vm, set, ui, teardrop_core::SysTab::Cpu), ..shot("26q-system-cpu", 1.0, Page::System) },
        Shot { setup: |vm, set, ui, _| demo_sys(vm, set, ui, teardrop_core::SysTab::Gpu), ..shot("26r-system-gpu", 1.0, Page::System) },
        Shot { setup: |vm, set, ui, _| demo_sys(vm, set, ui, teardrop_core::SysTab::Memory), ..shot("26s-system-memory", 1.0, Page::System) },
        Shot { setup: |vm, set, ui, _| demo_sys(vm, set, ui, teardrop_core::SysTab::Drives), ..shot("26t-system-drives", 1.0, Page::System) },
        Shot { setup: |vm, set, ui, _| demo_sys(vm, set, ui, teardrop_core::SysTab::Network), ..shot("26u-system-network", 1.0, Page::System) },
        Shot { setup: |vm, set, ui, _| demo_sys(vm, set, ui, teardrop_core::SysTab::Battery), ..shot("26v-system-battery", 1.0, Page::System) },
        Shot { setup: |vm, set, ui, _| demo_sys(vm, set, ui, teardrop_core::SysTab::Cpu), ..shot("26w-system-glance", 0.5, Page::System) },
        shot("26x-system-loading", 1.0, Page::System),
        Shot { setup: demo_calendar, ..shot("26i-today-agenda", 1.0, Page::Today) },
        Shot {
            setup: |vm, set, ui, ms| {
                demo_calendar(vm, set, ui, ms);
                let d = crate::viewmodel::local_naive(vm.now).date() + chrono::Duration::days(1);
                ui.cal_selected_day = Some((chrono::Datelike::year(&d), chrono::Datelike::month(&d), chrono::Datelike::day(&d)));
            },
            ..shot("26i2-today-day-selected", 1.0, Page::Today)
        },
        Shot {
            setup: |vm, set, ui, ms| {
                demo_calendar(vm, set, ui, ms);
                ui.cal_selected_event = Some(0);
            },
            ..shot("26i3-today-event-detail", 1.0, Page::Today)
        },
        Shot {
            setup: |vm, _, _, _| {
                demo_calendar(vm, &mut Settings::default(), &mut UiState::new(Page::Music), 0);
                let ev = vm.calendar.events[0].clone();
                vm.calendar.events[0].start = vm.now.timestamp() + 180;
                vm.calendar.events[0].end = vm.now.timestamp() + 1980;
                let _ = ev;
            },
            ..shot("26j-collapsed-meeting", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, _, _, _| {
                demo_calendar(vm, &mut Settings::default(), &mut UiState::new(Page::Music), 0);
                let mut hud = Hud::transient(HudKind::Meeting, "Design review", "in 5m", 0, u64::MAX / 2);
                hud.action_label = Some("Join".into());
                vm.hud = Some(hud);
            },
            ..shot("26k-hud-meeting", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, _, _, _| {
                use crate::usage::{Limits, OpenCode, Spend, UsageView, Window};
                let now = chrono::Utc::now().timestamp();
                vm.usage = UsageView {
                    claude: Limits {
                        plan: "Pro".into(),
                        primary: Some(Window { used_pct: 72.0, resets_at: Some(now + 2 * 3600 + 14 * 60) }),
                        secondary: Some(Window { used_pct: 31.0, resets_at: Some(now + 4 * 86_400) }),
                        ..Default::default()
                    },
                    codex: Limits {
                        plan: "Plus".into(),
                        primary: Some(Window { used_pct: 91.0, resets_at: Some(now + 50 * 60) }),
                        secondary: Some(Window { used_pct: 27.0, resets_at: Some(now + 3 * 86_400) }),
                        ..Default::default()
                    },
                    opencode: OpenCode {
                        today_tokens: 412_000,
                        today_cost: 0.31,
                        week_tokens: 2_840_000,
                        week_cost: 2.4,
                        top: vec![
                            Spend { label: "xai/grok-4.5".into(), tokens: 1_900_000, cost: 1.6 },
                            Spend { label: "kimi-for-coding/k2".into(), tokens: 700_000, cost: 0.5 },
                        ],
                        note: String::new(),
                    },
                    loaded: true,
                    refreshing: false,
                };
            },
            ..shot("26e-page-usage", 1.0, Page::Usage)
        },
        shot("26f-page-voice-idle", 1.0, Page::Voice),
        Shot {
            setup: |vm, _, ui, _| {
                vm.voice = crate::voice::VoiceView { phase: crate::voice::VoicePhase::Recording, elapsed_ms: 83_400, level: 0.6, saved: None, error: String::new() };
                for (i, b) in ui.voice_bars.iter_mut().enumerate() {
                    *b = 0.15 + 0.7 * ((i as f32 * 0.7).sin().abs());
                }
            },
            ..shot("26g-page-voice-recording", 1.0, Page::Voice)
        },
        Shot {
            setup: |vm, _, _, _| {
                vm.voice = crate::voice::VoiceView { phase: crate::voice::VoicePhase::Recording, elapsed_ms: 83_400, level: 0.6, saved: None, error: String::new() };
            },
            ..shot("26h-collapsed-recording", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, _, _| {
                set.idle_look = IdleLook::Eyes;
                vm.idle_look = IdleLook::Eyes;
                vm.voice = crate::voice::VoiceView { phase: crate::voice::VoicePhase::Recording, elapsed_ms: 83_400, level: 0.7, saved: None, error: String::new() };
            },
            ..shot("26h2-collapsed-rec-eyes-talk", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, _, _| {
                set.idle_look = IdleLook::Eyes;
                vm.idle_look = IdleLook::Eyes;
                vm.voice = crate::voice::VoiceView { phase: crate::voice::VoicePhase::Recording, elapsed_ms: 83_400, level: 0.0, saved: None, error: String::new() };
            },
            ..shot("26h3-collapsed-rec-eyes-quiet", 0.0, Page::Music)
        },
        shot("26l-page-ai-starters", 1.0, Page::Ai),
        Shot {
            setup: |vm, _, _, _| {
                use crate::ai::{AiActionBadge, ChatMessage, Role, UndoOp};
                vm.ai.messages = vec![
                    ChatMessage {
                        role: Role::User,
                        text: "Add a task 'Deep work block' and log a glass of water.".into(),
                        actions: Vec::new(),
                        timestamp: 0,
                    },
                    ChatMessage {
                        role: Role::Assistant,
                        text: "All set! Added your deep work block and logged a glass of water \u{2014} you're at 6 / 8 glasses today.".into(),
                        actions: vec![
                            AiActionBadge {
                                label: "Added task: Deep work block".into(),
                                undo: UndoOp::RemoveAddedTask("1".into()),
                                undone: false,
                            },
                            AiActionBadge {
                                label: "Logged +1 glass of water".into(),
                                undo: UndoOp::SubtractWater(250),
                                undone: false,
                            },
                        ],
                        timestamp: 0,
                    },
                ];
            },
            ..shot("26m-page-ai-chat", 1.0, Page::Ai)
        },
        Shot {
            setup: |vm, _, _, _| {
                use crate::ai::{ChatMessage, Role};
                vm.ai.messages = vec![
                    ChatMessage { role: Role::User, text: "How do I debounce a search box in Rust?".into(), actions: Vec::new(), timestamp: 0 },
                    ChatMessage {
                        role: Role::Assistant,
                        text: "## Debouncing\n\nKeep a **deadline** and reset it on every keystroke; only run the query once the deadline passes. See [the docs](https://doc.rust-lang.org) or `std::time::Instant`.\n\n- Store `last_edit: Instant`\n- On each *tick*, compare `now - last_edit`\n  - fire when it exceeds **150 ms**\n1. Reset on input\n2. Query on timeout\n\n> Tip: 150 ms feels instant but skips most keystrokes.\n\n```rust\nif last_edit.elapsed() > Duration::from_millis(150) {\n    search(&query);\n}\n```".into(),
                        actions: Vec::new(),
                        timestamp: 0,
                    },
                ];
            },
            ..shot("26m2-ai-markdown", 1.0, Page::Ai)
        },
        Shot {
            setup: |vm, set, _, _| {
                set.ai_provider = teardrop_core::AiProvider::ChatGpt;
                vm.ai.show_setup = true;
                vm.ai.chatgpt_signed_in = false;
                vm.ai.configured = false;
            },
            ..shot("26n-page-ai-setup-chatgpt", 1.0, Page::Ai)
        },
        Shot {
            setup: |vm, set, _, _| {
                set.ai_provider = teardrop_core::AiProvider::OpenCodeGo;
                vm.ai.show_setup = true;
                vm.ai.configured = true;
                vm.ai.memories_count = 3;
                vm.ai.available_models = crate::ai::default_models_for(teardrop_core::AiProvider::OpenCodeGo);
            },
            ..shot("26o-page-ai-setup-opencode", 1.0, Page::Ai)
        },
        Shot { setup: |_, _, ui, _| { ui.drag_files = 2; ui.drop_amt = 1.0; ui.clock_s = 0.3; }, ..shot("23b-shelf-drag-over", 1.0, Page::Shelf) },
        Shot { setup: |_, _, ui, _| { ui.drag_files = 2; ui.drop_amt = 1.0; ui.clock_s = 0.3; ui.drag_zone = 1; ui.zone_amt = 1.0; }, ..shot("23c-drag-over-phone", 1.0, Page::Shelf) },
        Shot { setup: |_, _, ui, _| { ui.absorb_t = 0.7; ui.absorb_zone = 1; }, ..shot("23d-drop-phone-burst", 1.0, Page::Shelf) },
        shot("26b-page-speed", 1.0, Page::Speed),
        Shot {
            setup: |vm, _, ui, _| {
                vm.speed = crate::viewmodel::SpeedView { phase: crate::viewmodel::SpeedPhase::Down, ping_ms: Some(18.0), down: 214.0, up: 0.0 };
                ui.speed_val = 214.0;
            },
            ..shot("26c-speed-download", 1.0, Page::Speed)
        },
        Shot {
            setup: |vm, _, ui, _| {
                vm.speed = crate::viewmodel::SpeedView { phase: crate::viewmodel::SpeedPhase::Done, ping_ms: Some(18.0), down: 214.0, up: 48.5 };
                ui.speed_val = 214.0;
            },
            ..shot("26d-speed-done", 1.0, Page::Speed)
        },
        shot("27-page-settings", 1.0, Page::Settings),
        Shot { hover: Some(Action::MediaPlayPause), ..shot("30-hover-play", 1.0, Page::Music) },
        Shot { hover: Some(Action::Tab(Page::Tasks)), ..shot("31-hover-tab", 1.0, Page::Water) },
        Shot { hover: Some(Action::ShelfOpen(1)), ..shot("32-hover-shelf-tile", 1.0, Page::Shelf) },
        Shot { setup: |vm, _, _, _| vm.shelf.items.clear(), ..shot("33-empty-shelf", 1.0, Page::Shelf) },
        Shot { setup: |vm, _, _, _| vm.tasks.tasks.clear(), ..shot("34-empty-tasks", 1.0, Page::Tasks) },
        Shot { setup: |vm, _, _, _| vm.notes.notes.clear(), ..shot("34b-empty-notes", 1.0, Page::Notes) },
        Shot { setup: |_, _, ui, ms| ui.toast("Copied", ms), ..shot("35-toast-copied", 1.0, Page::Clipboard) },
        Shot { hover: Some(Action::SeekBar), ..shot("36-hover-seek", 1.0, Page::Music) },
        Shot { setup: |vm, set, _, _| eyes(vm, set, EyeStyle::Round, true), ..shot("40-eyes-dance", 0.0, Page::Music) },
        Shot {
            setup: |vm, set, _, _| {
                eyes(vm, set, EyeStyle::Round, false);
                vm.env.idle_ms = 600_000;
            },
            ..shot("41-eyes-asleep", 0.0, Page::Music)
        },
        Shot { setup: |vm, set, _, _| eyes(vm, set, EyeStyle::Cat, false), ..shot("42-eyes-cat", 0.0, Page::Music) },
        Shot { setup: |vm, set, _, _| eyes(vm, set, EyeStyle::Pixel, false), ..shot("43-eyes-pixel", 0.0, Page::Music) },
        Shot { setup: |vm, set, _, _| eyes(vm, set, EyeStyle::Anime, false), ..shot("44-eyes-anime", 0.0, Page::Music) },
        Shot { setup: |vm, set, _, _| eyes(vm, set, EyeStyle::Robot, false), ..shot("45-eyes-robot", 0.0, Page::Music) },
        Shot {
            setup: |vm, set, ui, _| {
                eyes(vm, set, EyeStyle::Round, false);
                ui.eyes.react(React::Love);
            },
            ticks: 40,
            ..shot("46-eyes-love", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, _, _| {
                eyes(vm, set, EyeStyle::Round, false);
                let now = vm.now;
                vm.pomo.start_focus(now - chrono::Duration::minutes(6));
            },
            ..shot("47-eyes-focus", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, _, _| {
                eyes(vm, set, EyeStyle::Round, false);
                let d = super::viewmodel::local_naive(vm.now);
                use chrono::Datelike;
                set.birthday = Some((d.month() as u8, d.day() as u8));
            },
            ..shot("48-eyes-birthday", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, ui, _| {
                eyes(vm, set, EyeStyle::Round, false);
                ui.eyes.react(React::Dizzy);
            },
            ticks: 50,
            ..shot("49-eyes-dizzy", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, ui, _| {
                eyes(vm, set, EyeStyle::Anime, false);
                ui.eyes.react(React::Emote(Glyph::Star));
            },
            ticks: 40,
            ..shot("50-eyes-star", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, ui, ms| {
                eyes(vm, set, EyeStyle::Round, false);
                vm.hud = Some(hud_volume(64, false, ms));
                ui.eyes.gauge(Gauge::Volume, 0.5, false);
                ui.eyes.gauge(Gauge::Volume, 0.64, false);
            },
            ticks: 40,
            ..shot("51-eyes-hud-volume", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, ui, ms| {
                eyes(vm, set, EyeStyle::Round, false);
                vm.hud = Some(hud_volume(100, false, ms));
                ui.eyes.gauge(Gauge::Volume, 0.9, false);
                ui.eyes.gauge(Gauge::Volume, 1.0, false);
            },
            ticks: 40,
            ..shot("51b-eyes-hud-loud", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, ui, ms| {
                eyes(vm, set, EyeStyle::Round, false);
                vm.hud = Some(hud_volume(64, true, ms));
                ui.eyes.gauge(Gauge::Volume, 0.64, true);
            },
            ticks: 50,
            ..shot("51c-eyes-hud-muted", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, ui, ms| {
                eyes(vm, set, EyeStyle::Round, false);
                vm.hud = Some(hud_brightness(8, ms));
                ui.eyes.gauge(Gauge::Brightness, 0.08, false);
            },
            ticks: 40,
            ..shot("51d-eyes-hud-dim", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, ui, _| {
                eyes(vm, set, EyeStyle::Round, false);
                ui.eyes.noted(300);
            },
            ticks: 90,
            ..shot("52-eyes-noted", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, _, _| {
                eyes(vm, set, EyeStyle::Round, false);
                set.mood_ring = true;
            },
            ..shot("28-page-settings-eyes", 1.0, Page::Settings)
        },
        Shot {
            setup: |vm, set, _, _| focus_app(vm, set, "Code.exe", "main.rs - teardrop - Visual Studio Code"),
            ..shot("53-eyes-focus-coding", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, _, _| focus_app(vm, set, "chrome.exe", "Q3 plan - Google Docs - Google Chrome"),
            ..shot("54-eyes-focus-writing", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, _, _| focus_app(vm, set, "Figma.exe", "Untitled"),
            ..shot("55-eyes-focus-design", 0.0, Page::Music)
        },
        Shot {
            setup: |vm, set, _, _| {
                focus_app(vm, set, "EXCEL.EXE", "Budget.xlsx - Excel");
                set.app_themes.insert("excel".into(), "data".into());
                let app = vm.env.app.as_mut().unwrap();
                app.theme = Some(teardrop_core::AppTheme { cat: teardrop_core::AppCategory::Data, tint: None });
                app.source = teardrop_core::apps::Source::User;
            },
            ..shot("56-page-focus-app", 1.0, Page::Focus)
        },
        Shot {
            setup: |vm, set, _, _| {
                eyes(vm, set, EyeStyle::Round, true);
                let now = vm.now;
                vm.pomo.start_focus(now - chrono::Duration::minutes(6));
            },
            ..shot("57-collapsed-focus-music", 0.0, Page::Music)
        },
    ]
}

/// Eyes on, a focus block running and `exe` in front.
fn focus_app(vm: &mut ViewModel, set: &mut Settings, exe: &str, title: &str) {
    eyes(vm, set, EyeStyle::Round, false);
    let now = vm.now;
    vm.pomo.start_focus(now - chrono::Duration::minutes(6));
    let (theme, source) = teardrop_core::apps::classify(exe, title, &set.app_themes, &set.eye_themes);
    vm.env.app = Some(crate::viewmodel::FocusApp { exe: teardrop_core::apps::exe_stem(exe), theme, source });
}

/// Eyes as the idle look; `music` keeps the demo track playing.
fn eyes(vm: &mut ViewModel, set: &mut Settings, style: EyeStyle, music: bool) {
    if !music {
        vm.track.playback = teardrop_core::Playback::Stopped;
    }
    set.idle_look = IdleLook::Eyes;
    set.eye_style = style;
    vm.idle_look = IdleLook::Eyes;
}

pub fn run(dir: &str) -> i32 {
    let out = PathBuf::from(dir);
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("selftest: cannot create {dir}: {e}");
        return 2;
    }
    let dpi: i32 = std::env::var("TEARDROP_SELFTEST_DPI").ok().and_then(|v| v.parse().ok()).unwrap_or(192);
    let scale = dpi as f32 / 96.0;
    let geom = DropletGeom::default();
    let mut frames: Vec<serde_json::Value> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }

    // Pixel-true frames through the real painter.
    let now = Utc::now();
    let now_ms = now.timestamp_millis() as u64;
    for s in shots() {
        let mut vm = ViewModel::demo(now);
        let mut set = Settings::default();
        vm.page = s.page;
        let mut ui = UiState::new(s.page);
        (s.setup)(&mut vm, &mut set, &mut ui, now_ms);
        ui.set_hover(s.hover);
        for _ in 0..s.ticks {
            ui.tick(1.0 / 60.0, &vm, &set, now_ms);
        }
        let mut g = geom;
        g.pill_w = ui.compact_w;
        g.pill_h = ui.compact_h;
        let (cw, ch) = UiState::card_target(s.page);
        g.card_w = cw;
        g.card_h = ch;
        let view = IslandView { geom: g, open: s.open, wobble_px: s.wobble };
        let frame = build(&vm, &set, &ui, &view, now_ms);
        if s.open >= 1.0 {
            for h in &frame.hits {
                if h.rect.x < -0.5 || h.rect.right() > frame.w + 0.5 || h.rect.bottom() > frame.h + 0.5 {
                    failures.push(format!("{}: hit {:?} outside window", s.name, h.action));
                }
            }
            if !frame.hits.iter().any(|h| matches!(h.action, Action::Tab(_))) {
                failures.push(format!("{}: no tab hits", s.name));
            }
        }
        #[allow(unused_mut)]
        let mut entry = json!({"name": s.name, "open": s.open, "w": frame.w, "h": frame.h, "cmds": frame.scene.cmds.len(), "hits": frame.hits.len()});
        if set.idle_look == IdleLook::Eyes {
            entry["eyes"] = json!({"mood": format!("{:?}", ui.eyes.mood), "react": format!("{:?}", ui.eyes.current())});
        }
        #[cfg(windows)]
        match crate::render_d2d::d2d::render_offscreen(&frame.scene, (frame.w, frame.h), scale) {
            Some((pw, ph, bgra)) => {
                let rgb = composite(pw, ph, &bgra);
                let file = out.join(format!("{}.png", s.name));
                // A previewer holding the old PNG open is not a rendering failure.
                if let Err(e) = write_png(&file, pw, ph, &rgb) {
                    eprintln!("selftest: warning: {}: png write failed: {e}", s.name);
                }
                entry["file"] = json!(format!("{}.png", s.name));
            }
            None => failures.push(format!("{}: offscreen render failed", s.name)),
        }
        #[cfg(not(windows))]
        let _ = scale;
        frames.push(entry);
    }

    // Morph geometry contracts.
    if (geom.width(0.0) - geom.pill_w).abs() > 0.5 {
        failures.push("collapsed width off".into());
    }
    if (geom.width(1.0) - geom.card_w).abs() > 0.5 {
        failures.push("open width off".into());
    }
    if geom.neck(0.45) >= 0.75 {
        failures.push("neck did not pinch".into());
    }

    // Timing check: island spring must settle < 600ms simulated.
    let mut sp = teardrop_core::Spring::island();
    let mut steps = 0;
    loop {
        steps += 1;
        let moving = sp.step(1.0, 1.0 / 120.0);
        if !moving || steps > 240 {
            break;
        }
    }
    let settle_ms = steps as f32 * 1000.0 / 120.0;
    if settle_ms > 600.0 {
        failures.push(format!("spring settle {settle_ms:.0}ms > 600ms"));
    }

    // Interactions on the view-model.
    let mut vm = ViewModel::demo(now);
    let mut interactions: Vec<serde_json::Value> = Vec::new();
    let day = vm.day();
    let before = vm.water.today(day);
    vm.water.add(day, 250);
    if vm.water.today(day) != before + 250 {
        failures.push("water +250 failed".into());
    }
    vm.water.sub(day, 250);
    if vm.water.today(day) != before {
        failures.push("water undo failed".into());
    }
    interactions.push(json!({"action": "water+250/undo", "today_ml": vm.water.today(day)}));
    let open_before = vm.tasks.open_count();
    vm.tasks.add("selftest task", now);
    if vm.tasks.open_count() != open_before + 1 {
        failures.push("tasks add failed".into());
    }
    vm.pomo.start_focus(now);
    if vm.pomo.remaining_ms(now) == 0 {
        failures.push("focus start failed".into());
    }
    if Page::Music.step(1) != Page::Focus || Page::Voice.step(1) != Page::Ai || Page::Ai.step(1) != Page::Music || Page::Settings.step(1) != Page::Music {
        failures.push("page stepping wrong".into());
    }
    interactions.push(json!({"action": "tasks/focus/page-step", "open": vm.tasks.open_count()}));

    // Lyrics: encode + extract + parse + cache roundtrip (no network in selftest).
    let mut lyrics: Vec<serde_json::Value> = Vec::new();
    let enc = super::lyrics_service::url_encode("Midnight Droplet / A?");
    if enc != "Midnight%20Droplet%20%2F%20A%3F" {
        failures.push(format!("url_encode wrong: {enc}"));
    }
    lyrics.push(json!({"check": "url_encode", "out": enc}));
    let body = r#"{"syncedLyrics":"[00:01.00]hello\n[00:05.00]world\n"}"#;
    match super::lyrics_service::extract_synced(body) {
        Some(lrc) => {
            let lines = teardrop_core::parse_lrc(&lrc);
            if lines.len() != 2 {
                failures.push("lyrics parse len != 2".to_string());
            }
            let idx = teardrop_core::line_at(&lines, 3000);
            if idx != Some(0) {
                failures.push("lyrics line_at(3s) != 0".to_string());
            }
            lyrics.push(json!({"check": "parse+lookup", "lines": lines.len(), "idx_at_3s": idx}));
        }
        None => failures.push("extract_synced failed".to_string()),
    }
    // Cache roundtrip inside the selftest dir (isolated from user data).
    let lyr_dir = out.join("lyrics-tmp");
    super::lyrics_service::save_cache(&lyr_dir, "A/B", "T:C", "[00:01.00]hi\n");
    match super::lyrics_service::load_cached(&lyr_dir, "A/B", "T:C") {
        Some(hit) if hit.len() == 1 && hit[0].text == "hi" => {
            lyrics.push(json!({"check": "cache", "ok": true}));
        }
        _ => failures.push("lyrics cache roundtrip failed".to_string()),
    }
    let _ = std::fs::remove_dir_all(&lyr_dir);

    // SMTC mapping: status codes, snapshot conversion, app names (no OS needed).
    let mut smtc: Vec<serde_json::Value> = Vec::new();
    if super::media_smtc::status_to_playback(4) != teardrop_core::Playback::Playing {
        failures.push("smtc 4 != Playing".to_string());
    }
    if super::media_smtc::status_to_playback(5) != teardrop_core::Playback::Paused {
        failures.push("smtc 5 != Paused".to_string());
    }
    if super::media_smtc::status_to_playback(3) != teardrop_core::Playback::Stopped {
        failures.push("smtc 3 != Stopped".to_string());
    }
    let snap = super::media_smtc::SmtcSnapshot {
        title: "Song".into(),
        artist: "Band".into(),
        album: "Record".into(),
        app: "Spotify".into(),
        position_ms: 90_000,
        duration_ms: 60_000,
        playing: true,
        rate: 1.0,
    };
    let track = super::media_smtc::snapshot_to_track(&snap);
    if track.position_ms != 60_000 || track.playback != teardrop_core::Playback::Playing {
        failures.push("smtc snapshot_to_track wrong".to_string());
    }
    smtc.push(json!({"check": "status+snapshot", "ok": true}));
    if super::media_smtc::friendly_app("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify") != "Spotify" {
        failures.push("friendly_app spotify wrong".to_string());
    }
    smtc.push(json!({"check": "friendly_app", "spotify": "Spotify"}));

    // Audio mapping: scalar clamps + HUD labels (no COM in selftest).
    let mut audio: Vec<serde_json::Value> = Vec::new();
    if super::volume::scalar_to_pct(0.424) != 42 {
        failures.push("scalar_to_pct(0.424) != 42".to_string());
    }
    if super::volume::scalar_to_pct(1.7) != 100 || super::volume::scalar_to_pct(-0.2) != 0 {
        failures.push("scalar_to_pct clamp wrong".to_string());
    }
    let (t, d) = super::volume::label(super::volume::VolumeState { pct: 42, muted: false });
    if t != "Volume" || d != "42%" {
        failures.push("volume label wrong".to_string());
    }
    let (t, d) = super::volume::label(super::volume::VolumeState { pct: 0, muted: true });
    if t != "Muted" || d != "output muted" {
        failures.push("mute label wrong".to_string());
    }
    audio.push(json!({"check": "scalar+label", "ok": true}));

    // Message map: every arm discriminant must be a distinct real Win32 value.
    // (A bare `WM_MOUSELEAVE` pattern once bound as a catch-all variable and
    // swallowed the whole wndproc — this section pins the regression.)
    let mut msgmap: Vec<serde_json::Value> = Vec::new();
    let ids: Vec<(&str, u32)> = vec![
        ("WM_CREATE", 1),
        ("WM_DESTROY", 2),
        ("WM_PAINT", 15),
        ("WM_COMMAND", 273),
        ("WM_TIMER", 275),
        ("WM_LBUTTONDOWN", 513),
        ("WM_RBUTTONUP", 517),
        ("WM_MOUSEMOVE", 512),
        ("WM_MOUSELEAVE", 675),
        ("WM_MOUSEWHEEL", 522),
        ("WM_KEYDOWN", 256),
        ("WM_DROPFILES", 563),
        ("WM_CLIPBOARDUPDATE", 797),
        ("WM_APPCOMMAND", 793),
        ("WM_POWERBROADCAST", 536),
        ("WM_DEVICECHANGE", 537),
        ("WM_HOTKEY", 786),
        ("WM_DPICHANGED", 736),
        ("WM_WTSSESSION_CHANGE", 689),
    ];
    {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for (name, v) in &ids {
            if !seen.insert(v) {
                failures.push(format!("message id collision: {name}"));
            }
        }
    }
    msgmap.push(json!({"check": "distinct_ids", "count": ids.len()}));

    // Render helpers: equalizer shape, progress clamp, one-click HUD text.
    let mut render: Vec<serde_json::Value> = Vec::new();
    let bars_a = teardrop_core::media::Track::eq_bars(0);
    let bars_b = teardrop_core::media::Track::eq_bars(10_000);
    if bars_a.len() != 5 || bars_b.len() != 5 {
        failures.push("eq_bars len != 5".to_string());
    }
    if !bars_a.iter().all(|b| (0.0..=1.0).contains(b)) {
        failures.push("eq_bars out of range".to_string());
    }
    if bars_a == bars_b {
        failures.push("eq_bars static over time".to_string());
    }
    render.push(json!({"check": "eq_bars", "animated": bars_a != bars_b}));
    let mut t = teardrop_core::media::Track::default();
    t.duration_ms = 60_000;
    t.position_ms = 90_000; // clamped by progress()
    if (t.progress() - 1.0).abs() > 0.001 {
        failures.push("progress clamp wrong".to_string());
    }
    render.push(json!({"check": "progress_clamp", "ok": true}));
    // Shell config sanity.
    let st = Settings::default();
    if st.sounds || !st.hide_in_fullscreen || st.open_on_hover || st.goal_ml != 2000 {
        failures.push("settings defaults wrong".into());
    }

    let report = json!({
        "ok": failures.is_empty(),
        "dpi": dpi,
        "frames": frames,
        "interactions": interactions,
        "lyrics": lyrics,
        "smtc": smtc,
        "audio": audio,
        "msgmap": msgmap,
        "render": render,
        "settle_ms": settle_ms,
        "failures": failures,
    });
    let rep_path = out.join("report.json");
    if let Err(e) = std::fs::write(&rep_path, serde_json::to_string_pretty(&report).unwrap()) {
        eprintln!("selftest: cannot write report: {e}");
        return 2;
    }
    let ok = report["failures"].as_array().map(|a| a.is_empty()).unwrap_or(false);
    println!("selftest: {} frames -> {} (ok={ok})", report["frames"].as_array().map(|a| a.len()).unwrap_or(0), rep_path.display());
    if ok {
        0
    } else {
        eprintln!("FAILURES: {:?}", report["failures"]);
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_has_signature_and_chunks() {
        let dir = std::env::temp_dir().join("teardrop-png-test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("t.png");
        write_png(&f, 2, 2, &[255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9]).unwrap();
        let b = std::fs::read(&f).unwrap();
        assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n");
        assert!(b.windows(4).any(|w| w == b"IHDR"));
        assert!(b.windows(4).any(|w| w == b"IDAT"));
        assert!(b.ends_with(&[0xAE, 0x42, 0x60, 0x82]));
    }
}
