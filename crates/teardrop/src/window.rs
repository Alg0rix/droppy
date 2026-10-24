//! Win32 shell: a layered, per-pixel-alpha popup pinned to the top of the
//! primary monitor. All drawing goes through `ui::build` + the Direct2D
//! painter; this file only owns the window, timers, input and services.
//!
//! Timer model: one animation timer (16 ms while springs move, 40 ms for
//! ambient motion such as the equalizer, off when nothing changes) plus a
//! 1 s service tick. Collapsed + idle means zero wakeups besides the service tick.

#[cfg(windows)]
mod inner {
    use crate::render_d2d::d2d::{render_offscreen, Presenter};
    use crate::services_env::sys as env;
    use crate::services_system::sys::{sample_all, CpuTracker};
    use crate::state::{IslandMode, IslandState};
    use crate::ui::eyes::{Eyes, Gauge, React};
    use crate::ui::layout::{build, hit_test, IslandView, CONTENT_TOP};
    use crate::ui::scene::{Color, Fill, Scene};
    use crate::ui::state::{Action, Hit, SettingKey, UiState};
    use crate::ui::{self, icons};
    use crate::viewmodel::{hud_brightness, hud_volume, hud_water_due, hud_water_logged, local_naive, shift_birthday, FocusApp, ViewModel, Weather};
    use crate::{lyrics_service, media_smtc, paths, shelf_drag, startup, store, volume};
    use chrono::{Datelike, Utc};
    use std::cell::{Cell, RefCell};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Instant;
    use teardrop_core::{apps, line_at, Hud, HudKind, IdleLook, LyricLine, Page, Playback, Settings};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::DwmFlush;
    use windows::Win32::Graphics::Gdi::{
        CreateBitmap, DeleteObject, GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::DataExchange::{
        AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW, RemoveClipboardFormatListener, SetClipboardData,
    };
    use windows::Win32::System::Diagnostics::Debug::Beep;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use crate::drop_target::{DropEvent, DropTarget};
    use windows::Win32::System::Ole::{OleInitialize, RegisterDragDrop};
    use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
    use windows::Win32::System::Power::{RegisterPowerSettingNotification, POWERBROADCAST_SETTING};
    use windows::Win32::System::SystemServices::{GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS, MK_LBUTTON};
    use windows::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, RegisterHotKey, ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, UnregisterHotKey, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, MOD_SHIFT, MOD_WIN, TME_LEAVE, TRACKMOUSEEVENT, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
        VK_MEDIA_PREV_TRACK,
    };
    use windows::Win32::UI::Shell::{
        ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP,
        NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
    };
    use windows::Win32::UI::WindowsAndMessaging::*;

    const TRAY_MSG: u32 = 0x8001;
    const ID_QUIT: usize = 1000;
    const ID_TOGGLE: usize = 1001;
    const ID_SETTINGS: usize = 1002;
    const HOTKEY_ID: i32 = 1;
    const HOTKEY_VK: u32 = 0x54; // T
    /// Upper bound for one note's text (characters, not bytes).
    const NOTE_MAX_CHARS: usize = 4000;
    const CF_UNICODETEXT_U32: u32 = 13;
    const WM_APPCOMMAND_U32: u32 = 793;
    // windows 0.58 keeps WM_MOUSELEAVE under Controls; a bare pattern would
    // bind as a catch-all variable. Match by value.
    const WM_MOUSELEAVE_U32: u32 = 675;
    const WM_WTSSESSION_CHANGE_U32: u32 = 0x02B1;
    const WTS_SESSION_LOCK: u32 = 7;
    const WTS_SESSION_UNLOCK: u32 = 8;
    const WM_APP_DOWNLOAD: u32 = 0x8002;
    const WM_APP_SHARE: u32 = 0x8003;
    const WM_APP_VOLUME: u32 = 0x8004;
    const HSHELL_WINDOWCREATED: u32 = 1;
    const HSHELL_FLASH: u32 = 0x8006;
    const HSHELL_WINDOWACTIVATED: u32 = 4;
    const HSHELL_RUDEAPPACTIVATED: u32 = 0x8004;
    const CF_BITMAP_U32: u32 = 2;
    const CF_DIB_U32: u32 = 8;

    const T_ANIM: usize = 1;
    const T_SLOW: usize = 2;
    const T_HOVER: usize = 3;
    const T_LEAVE: usize = 4;

    static FETCHING: AtomicBool = AtomicBool::new(false);
    static LYRICS_RESULT: Mutex<Option<(String, Vec<LyricLine>)>> = Mutex::new(None);

    thread_local! {
        static ISLAND: RefCell<IslandState> = RefCell::new(IslandState::new());
        static VIEW: RefCell<ViewModel> = RefCell::new(ViewModel::new(Utc::now()));
        static SETTINGS: RefCell<Settings> = RefCell::new(Settings::default());
        static UI: RefCell<UiState> = RefCell::new(UiState::new(Page::Music));
        static HITS: RefCell<Vec<Hit>> = RefCell::new(Vec::new());
        static PRESENTER: RefCell<Option<Presenter>> = RefCell::new(None);
        static CPU: RefCell<CpuTracker> = RefCell::new(CpuTracker::default());
        static LYR_KEY: RefCell<String> = RefCell::new(String::new());
        static SCALE: Cell<f32> = Cell::new(1.0);
        static LAST_TICK: Cell<Option<Instant>> = Cell::new(None);
        static TIMER_MS: Cell<u32> = Cell::new(0);
        static FAST: Cell<bool> = Cell::new(false);
        static ACC: Cell<f32> = Cell::new(0.0);
        static PINNED: Cell<bool> = Cell::new(false);
        static MENU_OPEN: Cell<bool> = Cell::new(false);
        static HIDDEN: Cell<bool> = Cell::new(false);
        static UPDATE_REV: Cell<u32> = Cell::new(u32::MAX);
        static CAL_REV: Cell<u32> = Cell::new(u32::MAX);
        static ALERTED: RefCell<Vec<String>> = RefCell::new(Vec::new());
        static LAST_MEETING_TEXT: RefCell<String> = RefCell::new(String::new());
        static USAGE_REV: Cell<u32> = Cell::new(u32::MAX);
        static USAGE_STARTED: Cell<bool> = Cell::new(false);
        static AI_REV: Cell<u32> = Cell::new(u32::MAX);
        static DRAG_OPENED: Cell<bool> = Cell::new(false);
        static HOVER_PENDING: Cell<bool> = Cell::new(false);
        static LAST_MOUSE: Cell<(f32, f32)> = Cell::new((-1.0, -1.0));
        static PRESS_AT: Cell<Option<(f32, f32)>> = Cell::new(None);
        static SEEK_AT: Cell<Option<u64>> = Cell::new(None);
        static WIN_POS: Cell<(i32, i32)> = Cell::new((0, 0));
        static LAST_TAB: Cell<Page> = Cell::new(Page::Music);
        static TICKS: Cell<u32> = Cell::new(0);
        static LYR_STABLE: Cell<u8> = Cell::new(0);
        static LAST_VOL: Cell<Option<(u8, bool)>> = Cell::new(None);
        static LAST_BRIGHT: Cell<Option<u8>> = Cell::new(None);
        /// Note text when the editor opened, to tell a real edit from a peek.
        static NOTE_BEFORE: RefCell<Option<String>> = RefCell::new(None);
        static HIGH_SURR: Cell<u16> = Cell::new(0);
        static SHELL_MSG: Cell<u32> = Cell::new(0);
        static HOVER_ARM: Cell<(f32, f32)> = Cell::new((-1.0, -1.0));
        static HIDE_AT: Cell<Option<u64>> = Cell::new(None);
        /// Last focus-eyes state written to the log (only changes are logged).
        static FOCUS_LOG: RefCell<String> = RefCell::new(String::new());
    }

    fn with_eyes(f: impl FnOnce(&mut Eyes)) {
        UI.with(|u| f(&mut u.borrow_mut().eyes));
    }

    fn eyes_on() -> bool {
        SETTINGS.with(|s| s.borrow().idle_look == IdleLook::Eyes)
    }

    fn now_ms() -> u64 {
        Utc::now().timestamp_millis() as u64
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn is_open() -> bool {
        ISLAND.with(|s| matches!(s.borrow().mode, IslandMode::Open | IslandMode::Opening))
    }

    fn open_amount() -> f32 {
        ISLAND.with(|s| s.borrow().open.value)
    }

    // ------------------------------------------------------------ timers

    fn set_timer(hwnd: HWND, ms: u32) {
        if TIMER_MS.get() != ms {
            unsafe {
                let _ = SetTimer(hwnd, T_ANIM, ms, None);
            }
            TIMER_MS.set(ms);
        }
    }

    fn kill_timer(hwnd: HWND) {
        if TIMER_MS.get() != 0 {
            unsafe {
                let _ = KillTimer(hwnd, T_ANIM);
            }
            TIMER_MS.set(0);
        }
    }

    /// Something changed: run the vsync-locked frame loop until it settles.
    fn wake(_hwnd: HWND) {
        if !FAST.get() {
            LAST_TICK.set(None);
            FAST.set(true);
        }
    }

    // ------------------------------------------------------------ rendering

    fn render(hwnd: HWND) {
        if HIDDEN.get() {
            return;
        }
        let now = Utc::now();
        let ms = now.timestamp_millis() as u64;
        let scale = SCALE.get();

        // Pointer in window-local logical px (drives the eyes).
        let (wx, wy) = WIN_POS.get();
        let mouse = unsafe {
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_ok() {
                Some(((p.x - wx) as f32 / scale, (p.y - wy) as f32 / scale))
            } else {
                None
            }
        };
        UI.with(|u| u.borrow_mut().mouse = mouse);
        // Cheap per-frame signals for the eyes (the meter only while it dances).
        let peak = if VIEW.with(|v| v.borrow().music_active()) { volume::peak() } else { None };
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            vm.env.idle_ms = env::idle_ms();
            vm.env.lbutton = env::lbutton();
            vm.env.caps = env::caps();
            vm.env.audio_peak = peak;
        });

        let frame = ISLAND.with(|isl| {
            let mut st = isl.borrow_mut();
            UI.with(|u| {
                let ui = u.borrow();
                st.geom.pill_w = ui.compact_w;
                st.geom.pill_h = ui.compact_h;
            });
            let view = IslandView { geom: st.geom, open: st.open.value, wobble_px: st.wobble_px };
            VIEW.with(|v| {
                let mut vm = v.borrow_mut();
                vm.now = now;
                vm.pinned = PINNED.get();
                SETTINGS.with(|s| UI.with(|u| build(&vm, &s.borrow(), &u.borrow(), &view, ms)))
            })
        });

        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let x = ((screen_w as f32 - frame.w * scale) / 2.0).round() as i32;
        WIN_POS.set((x, 0));
        let ok = PRESENTER.with(|p| {
            let mut p = p.borrow_mut();
            if p.is_none() {
                let cap_w = ((720.0 + 2.0 * ui::theme::PAD + 32.0) * scale).ceil() as i32;
                let cap_h = ((340.0 + ui::theme::PAD + 40.0) * scale).ceil() as i32;
                *p = Presenter::new(cap_w, cap_h);
                if let Some(pr) = p.as_ref() {
                    let (_d, cache) = paths::ensure_dirs();
                    store::log_line(&cache, if pr.is_gpu() { "renderer: GPU (D3D11 + Direct2D)" } else { "renderer: CPU fallback" });
                }
            }
            match p.as_mut() {
                Some(pr) => pr.present(hwnd, &frame.scene, (frame.w, frame.h), scale, (x, 0)),
                None => false,
            }
        });
        if ok {
            unsafe {
                if !IsWindowVisible(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
            }
        }
        HITS.with(|h| *h.borrow_mut() = frame.hits);
    }

    /// `TEARDROP_PERF=1` logs average render cost + achieved fps every 120 frames.
    fn perf_note(render_ms: f32) {
        thread_local! {
            static ON: bool = std::env::var_os("TEARDROP_PERF").is_some();
            static ST: Cell<(u32, f32, Option<Instant>)> = Cell::new((0, 0.0, None));
        }
        if !ON.with(|o| *o) {
            return;
        }
        let (mut n, mut sum, start) = ST.get();
        let start = start.unwrap_or_else(Instant::now);
        n += 1;
        sum += render_ms;
        if n >= 120 {
            let secs = start.elapsed().as_secs_f32().max(0.001);
            let (_d, cache) = paths::ensure_dirs();
            store::log_line(&cache, &format!("perf: {:.1} fps, render {:.2} ms avg (last d2d draw {:.2} ms)", n as f32 / secs, sum / n as f32, crate::render_d2d::d2d::PROF_DRAW.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0));
            ST.set((0, 0.0, None));
        } else {
            ST.set((n, sum, Some(start)));
        }
    }

    /// One frame. Fast mode is driven by the vsync loop in `run`, so frame
    /// pacing follows the display (60/120/144/240 Hz); ambient motion that
    /// doesn't need full rate (equalizer, eyes) runs off a 30 fps timer.
    fn tick(hwnd: HWND) {
        let now = Instant::now();
        let dt = LAST_TICK.get().map(|t| (now - t).as_secs_f32()).unwrap_or(1.0 / 60.0).clamp(0.0005, 0.1);
        LAST_TICK.set(Some(now));
        let ms = now_ms();
        // Fixed 240 Hz physics with a carried remainder: motion speed is identical
        // on every refresh rate and never aliases against the frame time.
        const H: f32 = 1.0 / 240.0;
        let mut acc = ACC.get() + dt;
        let island_moving = ISLAND.with(|s| {
            let mut st = s.borrow_mut();
            let mut moving = false;
            let mut n = 0;
            while acc >= H && n < 48 {
                moving |= st.tick(H);
                acc -= H;
                n += 1;
            }
            if n == 0 {
                moving = true;
            }
            moving
        });
        ACC.set(acc.min(H));
        let open = open_amount();
        sync_position();
        sync_update();
        sync_voice();
        sync_calendar();
        sync_usage();
        sync_ai();
        let speed = crate::speedtest::snapshot();
        let speed_done = VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            let done = vm.speed.phase.running() && speed.phase == crate::viewmodel::SpeedPhase::Done;
            if vm.speed != speed {
                vm.speed = speed;
            }
            done
        });
        if speed_done {
            // Fast line: zoomies. Crawling: defeated.
            let r = match speed.down {
                d if d >= 100.0 => React::Zoom,
                d if d < 10.0 => React::Defeated,
                _ => React::Happy,
            };
            with_eyes(|e| e.react(r));
        }
        let (ui_moving, ambient) = UI.with(|u| {
            VIEW.with(|v| {
                SETTINGS.with(|s| {
                    let mut ui = u.borrow_mut();
                    let (vm, set) = (v.borrow(), s.borrow());
                    ui.island_open = open;
                    let moving = ui.tick(dt, &vm, &set, ms);
                    (moving, ui.ambient(&vm, &set, open, ms))
                })
            })
        });
        let t0 = Instant::now();
        render(hwnd);
        perf_note(t0.elapsed().as_secs_f32() * 1000.0);
        let smooth_ambient = ambient && open > 0.6 && VIEW.with(|v| v.borrow().page) == Page::Water;
        if island_moving || ui_moving || smooth_ambient {
            FAST.set(true);
        } else if ambient {
            FAST.set(false);
            set_timer(hwnd, 33);
        } else {
            FAST.set(false);
            kill_timer(hwnd);
            LAST_TICK.set(None);
            ACC.set(0.0);
        }
    }

    // ------------------------------------------------------- window state

    fn set_interactive(hwnd: HWND, on: bool) {
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let flag = WS_EX_NOACTIVATE.0 as isize;
            let next = if on { ex & !flag } else { ex | flag };
            if next != ex {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
            }
        }
    }

    fn activate(hwnd: HWND) {
        set_interactive(hwnd, true);
        unsafe {
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(hwnd);
        }
    }

    fn open_island(hwnd: HWND, pinned: bool) {
        ISLAND.with(|s| {
            let mut st = s.borrow_mut();
            if matches!(st.mode, IslandMode::Collapsed | IslandMode::Closing) {
                st.toggle();
            }
        });
        if pinned {
            PINNED.set(true);
            activate(hwnd);
            with_eyes(|e| e.poke());
        }
        wake(hwnd);
    }

    fn close_island(hwnd: HWND) {
        ISLAND.with(|s| {
            let mut st = s.borrow_mut();
            if matches!(st.mode, IslandMode::Open | IslandMode::Opening) {
                st.toggle();
            }
        });
        PINNED.set(false);
        // Collapsing mid-note counts as finishing it (same as Esc).
        let editing_note = UI.with(|u| {
            let ui = u.borrow();
            ui.note_edit.is_some() && ui.input.is_some()
        });
        if editing_note {
            finish_note_edit();
        }
        UI.with(|u| u.borrow_mut().input = None);
        with_eyes(|e| e.island_closed());
        unsafe {
            let _ = KillTimer(hwnd, T_LEAVE);
        }
        set_interactive(hwnd, false);
        wake(hwnd);
    }

    fn apply_fullscreen_hide(hwnd: HWND) -> bool {
        if !SETTINGS.with(|s| s.borrow().hide_in_fullscreen) {
            if HIDDEN.get() {
                HIDDEN.set(false);
            }
            return false;
        }
        unsafe {
            let fg = GetForegroundWindow();
            let hide = (|| {
                if fg.0.is_null() || fg == hwnd || is_shell_window(fg) {
                    return false;
                }
                let mut fr = RECT::default();
                if GetWindowRect(fg, &mut fr).is_err() {
                    return false;
                }
                let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
                if mon.0.is_null() {
                    return false;
                }
                let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                if !GetMonitorInfoW(mon, &mut mi).as_bool() {
                    return false;
                }
                let m = mi.rcMonitor;
                fr.left <= m.left && fr.top <= m.top && fr.right >= m.right && fr.bottom >= m.bottom
            })();
            if hide {
                if !HIDDEN.get() {
                    // The eyes glance down at the game before ducking out.
                    let now = now_ms();
                    match HIDE_AT.get() {
                        None if eyes_on() && !is_open() => {
                            HIDE_AT.set(Some(now + 1500));
                            with_eyes(|e| e.react(React::WatchGame));
                            wake(hwnd);
                            return false;
                        }
                        Some(at) if now < at => return false,
                        _ => {}
                    }
                    HIDE_AT.set(None);
                    HIDDEN.set(true);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            } else if HIDE_AT.take().is_some() || HIDDEN.get() {
                HIDDEN.set(false);
                // Paint a fresh frame first so the stale one never flashes up.
                render(hwnd);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            hide
        }
    }

    /// The desktop and taskbar cover the monitor too, but they are not a
    /// fullscreen app: focusing them must never hide the island.
    fn is_shell_window(w: HWND) -> bool {
        unsafe {
            if w == GetShellWindow() || w == GetDesktopWindow() {
                return true;
            }
            let mut buf = [0u16; 64];
            let n = GetClassNameW(w, &mut buf);
            let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
        }
    }

    // ----------------------------------------------------------- persistence

    fn save_all() {
        let (data, _cache) = paths::ensure_dirs();
        SETTINGS.with(|s| {
            VIEW.with(|v| {
                let vm = v.borrow();
                let mut st = s.borrow_mut();
                st.goal_ml = vm.water.goal_ml.max(500);
                st.idle_look = vm.idle_look;
                st.start_with_windows = startup::enabled();
                store::save(&data, "settings", &*st);
                store::save(&data, "page", &vm.page);
                store::save(&data, "water", &vm.water);
                store::save(&data, "tasks", &vm.tasks);
                store::save(&data, "notes", &vm.notes);
                store::save(&data, "shelf", &vm.shelf);
            });
        });
    }

    fn load_all() {
        let (data, _cache) = paths::ensure_dirs();
        SETTINGS.with(|s| {
            let mut st = s.borrow_mut();
            *st = store::load(&data, "settings");
            st.start_with_windows = startup::enabled();
        });
        // Refresh the active provider's catalog in the background so the model
        // picker is never stuck on the curated fallback after a launch.
        let (provider, url) = SETTINGS.with(|s| {
            let st = s.borrow();
            (st.ai_provider, st.active_ai_url().to_string())
        });
        crate::ai::refresh_models(provider, url);
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            let page: Page = store::load(&data, "page");
            vm.page = if page == Page::Settings { Page::Music } else { page };
            vm.water = store::load(&data, "water");
            // First reminder comes one interval after launch, not instantly.
            vm.water.last_reminder = Some(Utc::now());
            vm.tasks = store::load(&data, "tasks");
            vm.notes = store::load(&data, "notes");
            vm.shelf = store::load(&data, "shelf");
            SETTINGS.with(|s| {
                let st = s.borrow();
                vm.water.goal_ml = st.goal_ml.max(500);
                vm.water.reminders_enabled = st.water_reminders;
                vm.idle_look = st.idle_look;
            });
            CPU.with(|c| vm.system = sample_all(&mut c.borrow_mut(), &vm.system));
            UI.with(|u| *u.borrow_mut() = UiState::new(vm.page));
        });
        AI_REV.set(u32::MAX);
        sync_ai();
    }

    // ------------------------------------------------------------- clipboard

    fn clipboard_is_private() -> bool {
        unsafe {
            let fmt = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
            fmt != 0 && IsClipboardFormatAvailable(fmt).is_ok()
        }
    }

    fn read_clipboard_text(owner: HWND) -> Option<String> {
        unsafe {
            if OpenClipboard(owner).is_err() {
                return None;
            }
            let text = (|| {
                if clipboard_is_private() {
                    return None;
                }
                let h: HANDLE = GetClipboardData(CF_UNICODETEXT_U32).ok()?;
                let hg = HGLOBAL(h.0);
                let ptr = GlobalLock(hg) as *const u16;
                if ptr.is_null() {
                    return None;
                }
                let mut len = 0usize;
                while len < 8192 && *ptr.add(len) != 0 {
                    len += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)).trim().to_string();
                let _ = GlobalUnlock(hg);
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            })();
            let _ = CloseClipboard();
            text
        }
    }

    fn write_clipboard_text(owner: HWND, text: &str) -> bool {
        let data = wide(text);
        unsafe {
            if OpenClipboard(owner).is_err() {
                return false;
            }
            let _ = EmptyClipboard();
            let ok = (|| {
                let h = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2).ok()?;
                let p = GlobalLock(h) as *mut u16;
                if p.is_null() {
                    return None;
                }
                std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
                let _ = GlobalUnlock(h);
                SetClipboardData(CF_UNICODETEXT_U32, HANDLE(h.0)).ok()?;
                Some(())
            })()
            .is_some();
            let _ = CloseClipboard();
            ok
        }
    }

    // ----------------------------------------------------------- actions

    fn send_media_key(vk: u8) {
        unsafe {
            keybd_event(vk, 0, KEYBD_EVENT_FLAGS(0), 0);
            keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
        }
    }

    fn shell_open(hwnd: HWND, verb: &str, file: &str, params: Option<&str>) {
        let (v, f) = (wide(verb), wide(file));
        let p = params.map(wide);
        unsafe {
            ShellExecuteW(
                hwnd,
                PCWSTR::from_raw(v.as_ptr()),
                PCWSTR::from_raw(f.as_ptr()),
                p.as_ref().map(|p| PCWSTR::from_raw(p.as_ptr())).unwrap_or(PCWSTR::null()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }

    fn chime(freq: u32, ms: u32) {
        if SETTINGS.with(|s| s.borrow().sounds) {
            unsafe {
                let _ = Beep(freq, ms);
            }
        }
    }

    fn toast(msg: &str) {
        UI.with(|u| u.borrow_mut().toast(msg, now_ms()));
    }

    fn add_water(ml: u32) {
        let now = Utc::now();
        let ms = now_ms();
        let open = is_open();
        with_eyes(|e| e.react(React::Gulp));
        let total = VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            let day = vm.day();
            let total = vm.water.add(day, ml);
            vm.water.snooze(now);
            if !open {
                vm.hud = Some(hud_water_logged(ml, total, ms));
            }
            total
        });
        if open {
            toast(&format!("+{ml} ml \u{00b7} {total} today"));
        }
        let goal = VIEW.with(|v| v.borrow().water.goal_ml);
        let reached = total >= goal && total.saturating_sub(ml) < goal;
        with_eyes(|e| e.react(if reached { React::Celebrate } else { React::Satisfied }));
    }

    fn add_task_from_input() {
        let text = UI.with(|u| u.borrow().input.as_ref().map(|f| f.as_str().to_string())).unwrap_or_default();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        // Easter egg.
        if text.to_lowercase().contains("barrel roll") {
            with_eyes(|e| e.react(React::BarrelRoll));
            UI.with(|u| u.borrow_mut().input = Some(crate::ui::textedit::TextField::default()));
            toast("Wheee! Watch the eyes");
            return;
        }
        VIEW.with(|v| {
            v.borrow_mut().tasks.add(&text, Utc::now());
        });
        UI.with(|u| {
            let mut ui = u.borrow_mut();
            ui.input = Some(crate::ui::textedit::TextField::default());
            // Keep the new row in view.
            let n = VIEW.with(|v| v.borrow().tasks.tasks.len());
            let max = ui::pages::tasks_max_scroll(n);
            ui.scroll_by(Page::Tasks, max as i32, max);
        });
        with_eyes(|e| e.react(React::GotIt));
    }

    /// Write the note editor buffer through to the view-model. Called on every
    /// keystroke, so leaving the page (or the app) never loses text.
    fn commit_note_input() {
        let (edit, text) = UI.with(|u| {
            let ui = u.borrow();
            (ui.note_edit.clone(), ui.input.as_ref().map(|f| f.as_str().to_string()))
        });
        let Some(text) = text else { return };
        match edit {
            Some(id) => {
                VIEW.with(|v| {
                    v.borrow_mut().notes.update(&id, &text, Utc::now());
                });
            }
            None if !text.trim().is_empty() => {
                let id = VIEW.with(|v| v.borrow_mut().notes.add(&text, Utc::now()).id.clone());
                UI.with(|u| u.borrow_mut().note_edit = Some(id));
            }
            None => {}
        }
    }

    /// Leave the editor: commit, drop the note if it ended up blank, clear the field.
    fn finish_note_edit() {
        let (edit, text) = UI.with(|u| {
            let mut ui = u.borrow_mut();
            (ui.note_edit.take(), ui.input.take())
        });
        let before = NOTE_BEFORE.take();
        let Some(id) = edit else { return };
        let text = text.map(|f| f.as_str().to_string()).unwrap_or_default();
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            if text.trim().is_empty() {
                vm.notes.remove(&id);
            } else {
                vm.notes.update(&id, &text, Utc::now());
            }
        });
        // The face reads it over once the island collapses.
        if !text.trim().is_empty() && before.as_deref() != Some(text.as_str()) {
            with_eyes(|e| e.noted(text.chars().count()));
        }
    }

    fn send_ai_prompt(prompt: String) {
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() {
            return;
        }
        let set = SETTINGS.with(|s| s.borrow().clone());
        let day_ctx = if set.ai_share_day {
            Some(VIEW.with(|v| v.borrow().day_context_summary()))
        } else {
            None
        };
        UI.with(|u| u.borrow_mut().input = None);
        crate::ai::send_prompt(prompt, set, day_ctx);
        with_eyes(|e| e.react(React::GotIt));
        sync_ai();
    }

    fn commit_ai_field(send_if_prompt: bool) {
        use crate::ai::AiFocusField;
        let Some(text) = UI.with(|u| u.borrow().input.as_ref().map(|f| f.as_str().to_string())) else { return };
        let prov = SETTINGS.with(|s| s.borrow().ai_provider);
        let focus = crate::ai::snapshot(prov).focus_field;
        match focus {
            AiFocusField::Prompt => {
                crate::ai::input_set(text.clone());
                if send_if_prompt {
                    send_ai_prompt(text);
                }
            }
            AiFocusField::Model => {
                SETTINGS.with(|s| s.borrow_mut().set_active_ai_model(text));
                UI.with(|u| u.borrow_mut().input = None);
                crate::ai::set_focus_field(AiFocusField::Prompt);
                save_all();
            }
            AiFocusField::BaseUrl => {
                SETTINGS.with(|s| s.borrow_mut().set_active_ai_url(text));
                UI.with(|u| u.borrow_mut().input = None);
                crate::ai::set_focus_field(AiFocusField::Prompt);
                save_all();
            }
        }
    }

    fn perform(hwnd: HWND, a: Action) {
        let now = Utc::now();
        let ms = now_ms();
        let mut save = true;
        match a {
            Action::Island => {
                open_island(hwnd, true);
                save = false;
            }
            Action::HudWater => add_water(250),
            Action::Tab(p) => {
                // Leaving the notes editor closes it (and prunes a blank note).
                if VIEW.with(|v| v.borrow().page) == Page::Notes && p != Page::Notes {
                    finish_note_edit();
                }
                VIEW.with(|v| v.borrow_mut().page = p)
            }
            Action::CalStep(d) => {
                UI.with(|u| u.borrow_mut().cal_offset += d as i32);
                with_eyes(|e| e.react(React::Swipe(d > 0)));
                save = false;
            }
            Action::UpdateCheck => {
                crate::update::check();
                save = false;
            }
            Action::UpdateInstall => {
                crate::update::install();
                with_eyes(|e| e.react(React::BarrelRoll));
                save = false;
            }
            Action::CalendarPaste => {
                let text = read_clipboard_text(hwnd);
                match text.as_deref().and_then(crate::calendar::normalize_feed_url) {
                    Some(url) => {
                        let added = SETTINGS.with(|s| {
                            let mut st = s.borrow_mut();
                            if st.calendar_urls.contains(&url) {
                                false
                            } else {
                                st.calendar_urls.push(url);
                                true
                            }
                        });
                        // The link is a secret and the clipboard history already
                        // captured it when it was copied: take it back out.
                        if let Some(t) = &text {
                            VIEW.with(|v| v.borrow_mut().clipboard.remove(t));
                        }
                        refresh_calendar();
                        with_eyes(|e| e.react(if added { React::Happy } else { React::Huh }));
                        toast(if added { "Calendar connected" } else { "That calendar is already connected" });
                    }
                    None => toast("Copy your calendar's iCal link first"),
                }
            }
            Action::CalendarClear => {
                SETTINGS.with(|s| s.borrow_mut().calendar_urls.clear());
                refresh_calendar();
                with_eyes(|e| e.react(React::Poof));
                toast("Calendars removed");
            }
            Action::Join(i) => {
                let sel_day = UI.with(|u| u.borrow().cal_selected_day);
                let url = VIEW.with(|v| {
                    let vm = v.borrow();
                    ui::pages::active_calendar_events(&vm, sel_day)
                        .get(i as usize)
                        .and_then(|e| e.join.as_ref().map(|j| j.url.clone()))
                });
                with_eyes(|e| e.react(React::Salute));
                open_meeting(hwnd, url);
            }
            Action::MeetingJoin => {
                let url = VIEW.with(|v| {
                    let vm = v.borrow();
                    vm.calendar.next(vm.now.timestamp()).and_then(|e| e.join.as_ref().map(|j| j.url.clone()))
                });
                with_eyes(|e| e.react(React::Salute));
                open_meeting(hwnd, url);
            }
            Action::UsageRefresh => {
                crate::usage::refresh();
                save = false;
            }
            Action::VoiceToggle => {
                let was_recording = VIEW.with(|v| v.borrow().voice.phase == crate::voice::VoicePhase::Recording);
                crate::voice::toggle();
                with_eyes(|e| e.react(if was_recording { React::Satisfied } else { React::Perk }));
                save = false;
            }
            Action::VoiceFolder => {
                let dir = crate::voice::recordings_dir();
                let _ = std::fs::create_dir_all(&dir);
                shell_open(hwnd, "open", &dir.to_string_lossy(), None);
                save = false;
            }
            Action::SpeedTest => {
                crate::speedtest::start();
                with_eyes(|e| e.react(React::Perk));
                save = false;
            }
            Action::CalToday => {
                UI.with(|u| {
                    let mut ui = u.borrow_mut();
                    ui.cal_offset = 0;
                    ui.cal_selected_day = None;
                    ui.cal_selected_event = None;
                    ui.scroll[ui::state::page_index(Page::Today)] = 0;
                });
                with_eyes(|e| e.react(React::ClockGlance));
                save = false;
            }
            Action::CalDay(d) => {
                let today = VIEW.with(|v| local_naive(v.borrow().now).date());
                UI.with(|u| {
                    let mut ui = u.borrow_mut();
                    let (y, m) = ui::pages::shown_year_month(today, ui.cal_offset);
                    let target = (y, m, d as u32);
                    if ui.cal_selected_day == Some(target) && ui.cal_selected_event.is_none() {
                        ui.cal_selected_day = None;
                    } else {
                        ui.cal_selected_day = Some(target);
                    }
                    ui.cal_selected_event = None;
                    ui.scroll[ui::state::page_index(Page::Today)] = 0;
                });
                save = false;
            }
            Action::CalEvent(i) => {
                UI.with(|u| u.borrow_mut().cal_selected_event = Some(i as usize));
                with_eyes(|e| e.react(React::Perk));
                save = false;
            }
            Action::CalCloseEvent => {
                UI.with(|u| u.borrow_mut().cal_selected_event = None);
                save = false;
            }
            Action::CalCopyEvent => {
                let text = VIEW.with(|v| {
                    let vm = v.borrow();
                    let (sel_day, sel_ev) = UI.with(|u| {
                        let ui = u.borrow();
                        (ui.cal_selected_day, ui.cal_selected_event)
                    });
                    let items = ui::pages::active_calendar_events(&vm, sel_day);
                    sel_ev.and_then(|idx| items.get(idx)).map(|ev| {
                        let mut out = format!(
                            "{}\n{} \u{00b7} {}",
                            ev.title,
                            crate::calendar::full_date_label(ev),
                            crate::calendar::time_range_label(ev)
                        );
                        if !ev.location.is_empty() {
                            out.push_str(&format!("\nLocation: {}", ev.location));
                        }
                        if let Some(j) = &ev.join {
                            out.push_str(&format!("\n{}: {}", j.platform, j.url));
                        }
                        if !ev.description.is_empty() {
                            out.push_str(&format!("\n\n{}", ev.description));
                        }
                        out
                    })
                });
                if let Some(t) = text {
                    toast(if write_clipboard_text(hwnd, &t) { "Event details copied" } else { "Couldn't copy" });
                    with_eyes(|e| e.react(React::GotIt));
                }
                save = false;
            }
            Action::Gear => VIEW.with(|v| {
                let mut vm = v.borrow_mut();
                if vm.page == Page::Settings {
                    vm.page = LAST_TAB.get();
                } else {
                    LAST_TAB.set(vm.page);
                    vm.page = Page::Settings;
                }
            }),
            Action::Pin => {
                PINNED.set(!PINNED.get());
                with_eyes(|e| e.react(if PINNED.get() { React::Wink } else { React::Nod }));
            }
            Action::MediaPrev => {
                send_media_key(VK_MEDIA_PREV_TRACK.0 as u8);
                VIEW.with(|v| v.borrow_mut().track.position_ms = 0);
                with_eyes(|e| e.skip(false));
            }
            Action::MediaNext => {
                send_media_key(VK_MEDIA_NEXT_TRACK.0 as u8);
                VIEW.with(|v| v.borrow_mut().track.position_ms = 0);
                with_eyes(|e| e.skip(true));
            }
            Action::Seek(p) => {
                let (target, from) = VIEW.with(|v| {
                    let vm = v.borrow();
                    (media_smtc::seek_target_ms(vm.track.duration_ms, p), vm.track.position_ms)
                });
                with_eyes(|e| e.react(React::Swipe(target >= from)));
                media_smtc::seek(target);
                VIEW.with(|v| v.borrow_mut().track.position_ms = target);
                // Hold off stale polled positions while the player catches up.
                SEEK_AT.set(Some(ms));
                save = false;
            }
            // Clicking the bar is turned into a concrete `Seek` in WM_LBUTTONUP.
            Action::SeekBar => save = false,
            Action::MediaPlayPause => {
                send_media_key(VK_MEDIA_PLAY_PAUSE.0 as u8);
                let playing = VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    vm.track.playback = if vm.track.playback == Playback::Playing { Playback::Paused } else { Playback::Playing };
                    vm.track.playback == Playback::Playing
                });
                with_eyes(|e| e.react(if playing { React::Perk } else { React::Huh }));
                save = false;
            }
            Action::FocusToggle => {
                let running = VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    if vm.pomo.ends_at.is_some() {
                        vm.pomo.pause(now);
                        false
                    } else if vm.pomo.remaining_ms_when_paused.is_some() {
                        vm.pomo.resume(now);
                        true
                    } else {
                        vm.pomo.start_focus(now);
                        true
                    }
                });
                with_eyes(|e| e.react(if running { React::Perk } else { React::Huh }));
            }
            Action::FocusReset => {
                VIEW.with(|v| v.borrow_mut().pomo.stop());
                with_eyes(|e| e.react(React::Stretch));
            }
            Action::FocusMinus => {
                VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    vm.pomo.focus_min = vm.pomo.focus_min.saturating_sub(5).max(5);
                });
                with_eyes(|e| e.react(React::Shake));
            }
            Action::FocusPlus => {
                VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    vm.pomo.focus_min = (vm.pomo.focus_min + 5).min(90);
                });
                with_eyes(|e| e.react(React::Nod));
            }
            Action::WaterAdd(ml) => add_water(ml),
            Action::WaterUndo => {
                VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    let day = vm.day();
                    vm.water.sub(day, 250);
                });
                with_eyes(|e| e.react(React::Oops));
            }
            Action::ShelfOpen(i) | Action::ShelfReveal(i) | Action::ShelfRemove(i) => {
                let path = VIEW.with(|v| {
                    let vm = v.borrow();
                    vm.shelf.items.len().checked_sub(1 + i as usize).and_then(|ix| vm.shelf.items.get(ix)).map(|it| it.path.clone())
                });
                if let Some(path) = path {
                    let r = match a {
                        Action::ShelfOpen(_) => React::Perk,
                        Action::ShelfReveal(_) => React::Peek,
                        _ => React::Poof,
                    };
                    with_eyes(|e| e.react(r));
                    match a {
                        Action::ShelfOpen(_) => shell_open(hwnd, "open", &path, None),
                        Action::ShelfReveal(_) => shell_open(hwnd, "open", "explorer.exe", Some(&format!("/select,\"{path}\""))),
                        _ => VIEW.with(|v| {
                            v.borrow_mut().shelf.remove(&path);
                        }),
                    }
                }
            }
            Action::TaskToggle(i) => {
                let status = VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    if let Some(id) = vm.tasks.tasks.get(i as usize).map(|t| t.id.clone()) {
                        vm.tasks.toggle(&id);
                        let is_done = vm.tasks.tasks.iter().find(|t| t.id == id).map(|t| t.done).unwrap_or(false);
                        let all_done = is_done && vm.tasks.tasks.iter().all(|t| t.done);
                        Some((is_done, all_done))
                    } else {
                        None
                    }
                });
                if let Some((is_done, all_done)) = status {
                    let r = if all_done {
                        React::Celebrate
                    } else if is_done {
                        React::Happy
                    } else {
                        React::Oops
                    };
                    with_eyes(|e| e.react(r));
                }
            }
            Action::TaskRemove(i) => {
                let removed = VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    let id = vm.tasks.tasks.get(i as usize).map(|t| t.id.clone());
                    id.is_some_and(|id| vm.tasks.remove(&id))
                });
                if removed {
                    with_eyes(|e| e.react(React::Poof));
                }
            }
            Action::TaskInput => {
                UI.with(|u| {
                    let mut ui = u.borrow_mut();
                    if ui.input.is_none() {
                        ui.input = Some(crate::ui::textedit::TextField::default());
                    }
                });
                activate(hwnd);
                save = false;
            }
            Action::TaskClearDone => {
                VIEW.with(|v| v.borrow_mut().tasks.clear_done());
                with_eyes(|e| e.react(React::Sweep));
            }
            Action::NoteNew => {
                UI.with(|u| {
                    let mut ui = u.borrow_mut();
                    ui.input = Some(crate::ui::textedit::TextField::default());
                    ui.note_edit = None;
                    ui.note_search = None;
                });
                NOTE_BEFORE.set(Some(String::new()));
                activate(hwnd);
                with_eyes(|e| e.react(React::Perk));
                save = false;
            }
            Action::NoteOpen(i) => {
                let note = VIEW.with(|v| {
                    v.borrow().notes.notes.get(i as usize).map(|n| (n.id.clone(), n.text.clone()))
                });
                if let Some((id, text)) = note {
                    NOTE_BEFORE.set(Some(text.clone()));
                    UI.with(|u| {
                        let mut ui = u.borrow_mut();
                        ui.input = Some(crate::ui::textedit::TextField::new(text));
                        ui.note_edit = Some(id);
                        ui.note_search = None;
                    });
                    activate(hwnd);
                }
                save = false;
            }
            Action::NoteRemove(i) => {
                let id = VIEW.with(|v| v.borrow().notes.notes.get(i as usize).map(|n| n.id.clone()));
                if let Some(id) = id {
                    VIEW.with(|v| {
                        v.borrow_mut().notes.remove(&id);
                    });
                    with_eyes(|e| e.react(React::Poof));
                    let editing = UI.with(|u| u.borrow().note_edit.as_deref() == Some(id.as_str()));
                    if editing {
                        UI.with(|u| {
                            let mut ui = u.borrow_mut();
                            ui.input = None;
                            ui.note_edit = None;
                        });
                    }
                }
            }
            Action::NoteClose => {
                finish_note_edit();
                save = false;
            }
            Action::NoteSelectAll => {
                UI.with(|u| {
                    let mut ui = u.borrow_mut();
                    if let Some(f) = ui.input.as_mut() {
                        if f.all_selected() {
                            f.collapse_to_end();
                        } else {
                            f.select_all();
                        }
                    }
                });
                save = false;
            }
            Action::NoteSearchInput => {
                UI.with(|u| {
                    let mut ui = u.borrow_mut();
                    if ui.note_search.is_none() {
                        ui.note_search = Some(crate::ui::textedit::TextField::default());
                    }
                });
                activate(hwnd);
                save = false;
            }
            Action::NoteSearchClear => {
                UI.with(|u| u.borrow_mut().note_search = None);
                save = false;
            }
            Action::NoteDateFilter(f) => {
                UI.with(|u| {
                    let mut ui = u.borrow_mut();
                    ui.note_date_filter = f;
                    ui.scroll[Page::Notes as usize] = 0;
                });
                save = false;
            }
            Action::ClipCopy(i) => {
                let text = VIEW.with(|v| v.borrow().clipboard.items().get(i as usize).cloned());
                if let Some(t) = text {
                    toast(if write_clipboard_text(hwnd, &t) { "Copied" } else { "Couldn't copy" });
                    with_eyes(|e| e.react(React::GotIt));
                }
                save = false;
            }
            Action::ClipClear => {
                VIEW.with(|v| v.borrow_mut().clipboard.clear());
                with_eyes(|e| e.react(React::Sweep));
                save = false;
            }
            Action::Toggle(k) => {
                SETTINGS.with(|s| k.flip(&mut s.borrow_mut()));
                let on = SETTINGS.with(|s| k.get(&s.borrow()));
                with_eyes(|e| e.react(if on { React::Nod } else { React::Shake }));
                match k {
                    SettingKey::StartWithWindows => {
                        startup::set(on);
                    }
                    SettingKey::WaterReminders => VIEW.with(|v| v.borrow_mut().water.reminders_enabled = on),
                    SettingKey::ClipboardHistory if !on => VIEW.with(|v| v.borrow_mut().clipboard.clear()),
                    SettingKey::WeatherMood if on => env::refresh_weather(),
                    _ => {}
                }
            }
            Action::Idle(look) => {
                VIEW.with(|v| v.borrow_mut().idle_look = look);
                SETTINGS.with(|s| s.borrow_mut().idle_look = look);
                if look == IdleLook::Eyes {
                    with_eyes(|e| e.react(React::Wake));
                }
            }
            Action::FocusAppTheme => step_app_theme(hwnd, true),
            Action::EyeStyle(st) => {
                SETTINGS.with(|s| s.borrow_mut().eye_style = st);
                with_eyes(|e| e.react(React::Sparkle));
            }
            Action::Birthday => {
                let has_bday = SETTINGS.with(|s| {
                    let mut st = s.borrow_mut();
                    st.birthday = match st.birthday {
                        Some(_) => None,
                        None => {
                            let d = local_naive(now);
                            Some((d.month() as u8, d.day() as u8))
                        }
                    };
                    st.birthday.is_some()
                });
                if has_bday {
                    with_eyes(|e| e.react(React::Love));
                }
            }
            Action::GoalStep(d) => {
                VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    vm.water.goal_ml = (vm.water.goal_ml as i32 + 250 * d as i32).clamp(500, 6000) as u32;
                });
                with_eyes(|e| e.react(if d > 0 { React::Nod } else { React::Shake }));
            }
            Action::AiInput => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                crate::ai::set_focus_field(crate::ai::AiFocusField::Prompt);
                let cur = crate::ai::snapshot(prov).input;
                UI.with(|u| u.borrow_mut().input = Some(crate::ui::textedit::TextField::new(cur)));
                activate(hwnd);
                sync_ai();
                save = false;
            }
            Action::AiFocusModel => {
                crate::ai::set_focus_field(crate::ai::AiFocusField::Model);
                let cur = SETTINGS.with(|s| s.borrow().active_ai_model().to_string());
                UI.with(|u| u.borrow_mut().input = Some(crate::ui::textedit::TextField::new(cur)));
                activate(hwnd);
                sync_ai();
                save = false;
            }
            Action::AiFocusBaseUrl => {
                crate::ai::set_focus_field(crate::ai::AiFocusField::BaseUrl);
                let cur = SETTINGS.with(|s| s.borrow().active_ai_url().to_string());
                UI.with(|u| u.borrow_mut().input = Some(crate::ui::textedit::TextField::new(cur)));
                activate(hwnd);
                sync_ai();
                save = false;
            }
            Action::AiSend => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                let text = UI
                    .with(|u| u.borrow().input.as_ref().map(|f| f.as_str().to_string()))
                    .unwrap_or_else(|| crate::ai::snapshot(prov).input);
                send_ai_prompt(text);
                save = false;
            }
            Action::AiStarter(idx) => {
                if let Some((_, prompt)) = crate::ai::STARTER_PROMPTS.get(idx as usize) {
                    send_ai_prompt((*prompt).to_string());
                }
                save = false;
            }
            Action::AiNewChat => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                crate::ai::new_chat(prov);
                with_eyes(|e| e.react(React::Perk));
                sync_ai();
                save = false;
            }
            Action::AiToggleHistory => {
                crate::ai::toggle_history();
                sync_ai();
                save = false;
            }
            Action::AiSelectChat(id) => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                crate::ai::select_chat(id, prov);
                sync_ai();
                save = false;
            }
            Action::AiDeleteChat(id) => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                crate::ai::delete_chat(id, prov);
                with_eyes(|e| e.react(React::Poof));
                sync_ai();
                save = false;
            }
            Action::AiToggleSetup => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                crate::ai::toggle_setup(prov);
                sync_ai();
                save = false;
            }
            Action::AiSetProvider(p) => {
                SETTINGS.with(|s| s.borrow_mut().ai_provider = p);
                crate::ai::set_provider(p);
                sync_ai();
            }
            Action::AiCycleModel(delta) => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                let models = crate::ai::snapshot(prov).available_models;
                if !models.is_empty() {
                    let cur = SETTINGS.with(|s| s.borrow().active_ai_model().to_string());
                    let pos = models.iter().position(|m| m == &cur).unwrap_or(0) as i32;
                    let next = (pos + delta as i32).rem_euclid(models.len() as i32) as usize;
                    SETTINGS.with(|s| s.borrow_mut().set_active_ai_model(models[next].clone()));
                }
                sync_ai();
            }
            Action::AiSelectModel(idx) => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                let models = crate::ai::snapshot(prov).available_models;
                if let Some(m) = models.get(idx as usize) {
                    SETTINGS.with(|s| s.borrow_mut().set_active_ai_model(m.clone()));
                }
                sync_ai();
            }
            Action::AiRefreshModels => {
                let (prov, url) = SETTINGS.with(|s| {
                    let st = s.borrow();
                    (st.ai_provider, st.active_ai_url().to_string())
                });
                crate::ai::refresh_models(prov, url);
                sync_ai();
                save = false;
            }
            Action::AiPasteKey => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                match read_clipboard_text(hwnd) {
                    Some(k) if !k.trim().is_empty() => {
                        crate::ai::save_api_key(prov, &k);
                        // Remove the secret API key from the clipboard ring so it is never displayed.
                        VIEW.with(|v| v.borrow_mut().clipboard.remove(&k));
                        toast("API key saved (DPAPI encrypted)");
                    }
                    _ => toast("Copy your API key to the clipboard first"),
                }
                sync_ai();
                save = false;
            }
            Action::AiPasteUrl => {
                match read_clipboard_text(hwnd) {
                    Some(u) if u.starts_with("http://") || u.starts_with("https://") => {
                        SETTINGS.with(|s| s.borrow_mut().set_active_ai_url(u));
                        toast("Endpoint URL updated");
                    }
                    _ => toast("Copy an http:// or https:// URL first"),
                }
                sync_ai();
            }
            Action::AiChatGptSignIn => {
                let busy = crate::ai::snapshot(teardrop_core::AiProvider::ChatGpt).chatgpt_login_busy;
                if busy {
                    crate::ai::cancel_chatgpt_login();
                } else {
                    crate::ai::start_chatgpt_login();
                }
                sync_ai();
                save = false;
            }
            Action::AiChatGptCopyCode => {
                let code = crate::ai::snapshot(teardrop_core::AiProvider::ChatGpt).chatgpt_user_code;
                if !code.is_empty() {
                    let _ = write_clipboard_text(hwnd, &code);
                    crate::ai::open_browser(crate::ai::CHATGPT_DEVICE_VERIFY_URL);
                    toast(&format!("Copied {code} \u{00b7} paste in browser"));
                }
                save = false;
            }
            Action::AiChatGptSignOut => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                crate::ai::sign_out_chatgpt(prov);
                toast("Signed out of ChatGPT");
                sync_ai();
                save = false;
            }
            Action::AiCopyReply(idx) => {
                let text = VIEW.with(|v| v.borrow().ai.messages.get(idx as usize).map(|m| m.text.clone()));
                if let Some(t) = text {
                    toast(if write_clipboard_text(hwnd, &t) { "Reply copied" } else { "Couldn't copy" });
                    with_eyes(|e| e.react(React::GotIt));
                }
                save = false;
            }
            Action::AiUndoAction(msg_idx, act_idx) => {
                let prov = SETTINGS.with(|s| s.borrow().ai_provider);
                if let Some(op) = crate::ai::mark_action_undone(msg_idx as usize, act_idx as usize, prov) {
                    with_eyes(|e| e.react(React::Oops));
                    use crate::ai::UndoOp::*;
                    VIEW.with(|v| {
                        let mut vm = v.borrow_mut();
                        match op {
                            RemoveAddedTask(id) => {
                                vm.tasks.remove(&id);
                            }
                            ReopenTask(id) => {
                                if let Some(t) = vm.tasks.tasks.iter_mut().find(|t| t.id == id) {
                                    t.done = false;
                                }
                            }
                            RestoreDeletedTask(title, was_done) => {
                                vm.tasks.add(&title, now);
                                if was_done {
                                    if let Some(last) = vm.tasks.tasks.last_mut() {
                                        last.done = true;
                                    }
                                }
                            }
                            SubtractWater(ml) => {
                                let day = vm.day();
                                vm.water.sub(day, ml);
                            }
                            ForgetMemory(_) | None => {}
                            RemoveAddedNote(id) => {
                                vm.notes.remove(&id);
                            }
                        }
                    });
                    toast("Action undone");
                    sync_ai();
                } else {
                    save = false;
                }
            }
            Action::Quit => unsafe {
                let _ = DestroyWindow(hwnd);
                return;
            },
        }
        let _ = ms;
        if save {
            save_all();
        }
        wake(hwnd);
    }

    // ----------------------------------------------------------------- tray

    fn load_app_icon() -> Option<HICON> {
        unsafe {
            let inst = GetModuleHandleW(None).ok()?;
            let hinst: windows::Win32::Foundation::HINSTANCE = inst.into();
            let res = LoadImageW(
                hinst,
                windows::core::PCWSTR(1 as *const u16),
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                IMAGE_FLAGS(0),
            );
            if let Ok(handle) = res {
                if !handle.is_invalid() {
                    return Some(HICON(handle.0));
                }
            }
            LoadIconW(hinst, windows::core::PCWSTR(1 as *const u16)).ok()
        }
    }

    fn make_tray_icon() -> Option<HICON> {
        if let Some(icon) = load_app_icon() {
            return Some(icon);
        }
        let mut s = Scene::new();
        s.circle(16.0, 16.0, 15.0, Fill::DGrad(Color::rgb(61, 200, 245), Color::rgb(60, 90, 220)));
        icons::draw(&mut s, icons::Icon::Drop, 16.0, 16.0, 20.0, Color::rgb(255, 255, 255));
        let (pw, ph, mut px) = render_offscreen(&s, (32.0, 32.0), 1.0)?;
        // Premultiplied -> straight alpha for the icon bitmap.
        for p in px.chunks_exact_mut(4) {
            let a = p[3] as f32 / 255.0;
            if a > 0.0 {
                for c in &mut p[..3] {
                    *c = ((*c as f32 / a).min(255.0)) as u8;
                }
            }
        }
        unsafe {
            let color = CreateBitmap(pw as i32, ph as i32, 1, 32, Some(px.as_ptr() as *const _));
            let mask_bits = vec![0u8; (pw as usize / 8) * ph as usize];
            let mask = CreateBitmap(pw as i32, ph as i32, 1, 1, Some(mask_bits.as_ptr() as *const _));
            let info = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
            let icon = CreateIconIndirect(&info).ok();
            let _ = DeleteObject(color);
            let _ = DeleteObject(mask);
            icon
        }
    }

    fn add_tray(hwnd: HWND) {
        unsafe {
            let hicon = load_app_icon().or_else(make_tray_icon).or_else(|| LoadIconW(None, IDI_APPLICATION).ok());
            let Some(hicon) = hicon else { return };
            let mut nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = TRAY_MSG;
            nid.hIcon = hicon;
            let tip = wide("Teardrop");
            nid.szTip[..tip.len().min(127)].copy_from_slice(&tip[..tip.len().min(127)]);
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }
    }

    fn remove_tray(hwnd: HWND) {
        unsafe {
            let nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }

    fn show_menu(hwnd: HWND) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            let items: [(usize, &str); 3] =
                [(ID_TOGGLE, if is_open() { "Collapse island" } else { "Open island" }), (ID_SETTINGS, "Settings"), (ID_QUIT, "Quit Teardrop")];
            for (id, label) in items {
                let l = wide(label);
                let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR::from_raw(l.as_ptr()));
            }
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(hwnd);
            MENU_OPEN.set(true);
            let cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, pt.x, pt.y, 0, hwnd, None);
            MENU_OPEN.set(false);
            let _ = DestroyMenu(menu);
            match cmd.0 as usize {
                ID_QUIT => {
                    let _ = DestroyWindow(hwnd);
                }
                ID_TOGGLE => {
                    if is_open() {
                        close_island(hwnd)
                    } else {
                        open_island(hwnd, true)
                    }
                }
                ID_SETTINGS => {
                    VIEW.with(|v| {
                        let mut vm = v.borrow_mut();
                        if vm.page != Page::Settings {
                            LAST_TAB.set(vm.page);
                            vm.page = Page::Settings;
                        }
                    });
                    open_island(hwnd, true);
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------- services

    fn lyrics_tick() {
        let cache_dir = paths::cache_dir();
        let lyrics_on = SETTINGS.with(|s| s.borrow().lyrics_enabled);
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            if !lyrics_on || vm.track.title.trim().is_empty() {
                vm.lyric_idx = None;
                return;
            }
            let key = format!("{}\0{}\0{}", vm.track.artist, vm.track.title, vm.track.album);
            if let Ok(mut slot) = LYRICS_RESULT.lock() {
                if let Some((k, lines)) = slot.take() {
                    if k == key {
                        vm.lyric_lines = lines;
                    }
                }
            }
            let changed = LYR_KEY.with(|k| {
                if *k.borrow() != key {
                    *k.borrow_mut() = key.clone();
                    true
                } else {
                    false
                }
            });
            if changed {
                LYR_STABLE.set(0);
                vm.lyric_lines = lyrics_service::load_cached(&cache_dir, &vm.track.artist, &vm.track.title).unwrap_or_default();
            } else if vm.lyric_lines.is_empty() {
                let n = LYR_STABLE.get().saturating_add(1);
                LYR_STABLE.set(n);
                // One background attempt once the track proves stable; never blocks the UI.
                if n == 2 && !FETCHING.swap(true, Ordering::SeqCst) {
                    let (artist, title, album) = (vm.track.artist.clone(), vm.track.title.clone(), vm.track.album.clone());
                    let duration_ms = vm.track.duration_ms;
                    std::thread::spawn(move || {
                        let lines = lyrics_service::get_lyrics(&cache_dir, &artist, &title, &album, duration_ms);
                        if let Ok(mut slot) = LYRICS_RESULT.lock() {
                            *slot = Some((key, lines));
                        }
                        FETCHING.store(false, Ordering::SeqCst);
                    });
                }
            }
            vm.lyric_idx = line_at(&vm.lyric_lines, vm.track.position_ms);
        });
    }

    /// Per-frame playback clock: the 2 s snapshots are extrapolated every
    /// frame so the seek bar and synced lyrics move smoothly instead of
    /// stepping once a second.
    fn sync_position() {
        if SEEK_AT.get().is_some() {
            return;
        }
        let Some(pos) = media_smtc::position_now() else { return };
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            if vm.track.playback != Playback::Playing || vm.track.title.trim().is_empty() {
                return;
            }
            vm.track.position_ms = pos.min(vm.track.duration_ms.max(pos));
            if !vm.lyric_lines.is_empty() {
                vm.lyric_idx = line_at(&vm.lyric_lines, pos);
            }
        });
    }

    fn refresh_calendar() {
        let urls = SETTINGS.with(|s| s.borrow().calendar_urls.clone());
        crate::calendar::refresh(urls);
    }

    /// Open a meeting link, but only one that points at a known meeting host.
    fn open_meeting(hwnd: HWND, url: Option<String>) {
        match url.filter(|u| crate::calendar::meeting_platform(u).is_some()) {
            Some(u) => shell_open(hwnd, "open", &u, None),
            None => toast("This meeting has no join link"),
        }
    }

    /// Calendar events into the view model when the worker publishes new ones.
    fn sync_calendar() {
        let feeds = SETTINGS.with(|s| s.borrow().calendar_urls.len());
        VIEW.with(|v| v.borrow_mut().calendar_feeds = feeds);
        let rev = crate::calendar::revision();
        if CAL_REV.replace(rev) == rev {
            return;
        }
        let snap = crate::calendar::snapshot();
        VIEW.with(|v| v.borrow_mut().calendar = snap);
    }

    /// Once a second: raise the "meeting starting" alert and wake a redraw when
    /// the pill's countdown text changes (nothing else animates then).
    fn calendar_tick(hwnd: HWND, ms: u64) {
        let now = Utc::now().timestamp();
        let alert = VIEW.with(|v| {
            let vm = v.borrow();
            ALERTED.with(|seen| crate::calendar::due_alerts(&vm.calendar.events, now, 300, &mut seen.borrow_mut()).into_iter().last().cloned())
        });
        if let Some(ev) = alert {
            let detail = crate::calendar::countdown(&ev, now);
            VIEW.with(|v| {
                let mut hud = Hud::transient(HudKind::Meeting, &ev.title, &detail, ms, 15_000);
                hud.action_label = ev.join.as_ref().map(|_| "Join".to_string());
                v.borrow_mut().hud = Some(hud);
            });
            chime(880, 180);
            with_eyes(|e| e.react(React::Gasp));
            wake(hwnd);
        }
        let text = VIEW.with(|v| v.borrow().meeting_soon().map(|e| crate::calendar::countdown(e, now)).unwrap_or_default());
        if LAST_MEETING_TEXT.with(|t| t.replace(text.clone())) != text {
            wake(hwnd);
        }
    }

    /// Recorder state into the view model; a finished recording goes onto the
    /// shelf so it can be dragged out like any other file.
    fn sync_voice() {
        let snap = crate::voice::snapshot();
        let finished = crate::voice::take_finished();
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            if vm.voice != snap {
                vm.voice = snap;
            }
            if let Some(p) = &finished {
                vm.shelf.add(&p.to_string_lossy(), Utc::now());
                vm.page = Page::Shelf;
            }
        });
        if finished.is_some() {
            toast("Recording saved to the shelf");
            save_all();
        }
    }

    /// Usage numbers into the view model when the worker publishes new ones.
    fn sync_usage() {
        let rev = crate::usage::revision();
        if USAGE_REV.replace(rev) == rev {
            return;
        }
        let snap = crate::usage::snapshot();
        VIEW.with(|v| v.borrow_mut().usage = snap);
    }

    /// Synchronize AI Chat state and apply any newly returned `[ACTION:...]` directives.
    fn sync_ai() {
        let rev = crate::ai::revision();
        if AI_REV.replace(rev) == rev {
            return;
        }
        let prov = SETTINGS.with(|s| s.borrow().ai_provider);
        let pending = crate::ai::take_pending_directives();
        if !pending.is_empty() {
            let now = Utc::now();
            let mut mutated = false;
            for (conv_id, msg_idx, dirs) in pending {
                let mut badges = Vec::new();
                let mut memories = Vec::new();
                VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    for d in dirs {
                        use crate::ai::{AiActionBadge, ParsedDirective::*, UndoOp};
                        match d {
                            AddTask(title) => {
                                let t = vm.tasks.add(&title, now);
                                let id = t.id.clone();
                                badges.push(AiActionBadge {
                                    label: format!("Added task: {title}"),
                                    undo: UndoOp::RemoveAddedTask(id),
                                    undone: false,
                                });
                                mutated = true;
                            }
                            DoneTask(query) => {
                                let q = query.to_lowercase();
                                if let Some(t) = vm.tasks.tasks.iter_mut().find(|t| !t.done && t.title.to_lowercase().contains(&q)) {
                                    t.done = true;
                                    let id = t.id.clone();
                                    let title = t.title.clone();
                                    badges.push(AiActionBadge {
                                        label: format!("Completed: {title}"),
                                        undo: UndoOp::ReopenTask(id),
                                        undone: false,
                                    });
                                    mutated = true;
                                }
                            }
                            DelTask(query) => {
                                let q = query.to_lowercase();
                                if let Some(pos) = vm.tasks.tasks.iter().position(|t| t.title.to_lowercase().contains(&q)) {
                                    let removed = vm.tasks.tasks.remove(pos);
                                    badges.push(AiActionBadge {
                                        label: format!("Deleted: {}", removed.title),
                                        undo: UndoOp::RestoreDeletedTask(removed.title, removed.done),
                                        undone: false,
                                    });
                                    mutated = true;
                                }
                            }
                            LogWater(ml) => {
                                let day = vm.day();
                                vm.water.add(day, ml);
                                vm.water.snooze(now);
                                badges.push(AiActionBadge {
                                    label: format!("Logged +{ml} ml water"),
                                    undo: UndoOp::SubtractWater(ml),
                                    undone: false,
                                });
                                mutated = true;
                            }
                            Focus(cmd) => {
                                match cmd.as_str() {
                                    "pause" => vm.pomo.pause(now),
                                    "reset" | "stop" => vm.pomo.stop(),
                                    _ => vm.pomo.start_focus(now),
                                }
                                badges.push(AiActionBadge {
                                    label: format!("Focus timer: {cmd}"),
                                    undo: UndoOp::None,
                                    undone: false,
                                });
                                mutated = true;
                            }
                            Remember(fact) => {
                                memories.push(fact.clone());
                                badges.push(AiActionBadge {
                                    label: format!("Remembered: {fact}"),
                                    undo: UndoOp::ForgetMemory(fact),
                                    undone: false,
                                });
                            }
                            Note(text) => {
                                let title = teardrop_core::title_of(&text);
                                let id = vm.notes.add(&text, now).id.clone();
                                with_eyes(|e| e.noted(text.chars().count()));
                                badges.push(AiActionBadge {
                                    label: format!("Saved note: {title}"),
                                    undo: UndoOp::RemoveAddedNote(id),
                                    undone: false,
                                });
                                mutated = true;
                            }
                        }
                    }
                });
                crate::ai::record_action_badges(conv_id, msg_idx, badges, memories, prov);
            }
            if mutated {
                save_all();
            }
            AI_REV.set(crate::ai::revision());
        }
        let snap = crate::ai::snapshot(prov);
        let ai_done = VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            let done = (vm.ai.busy && !snap.busy).then(|| snap.error.is_empty());
            vm.ai = snap;
            done
        });
        if let Some(ok) = ai_done {
            with_eyes(|e| e.react(if ok { React::Sparkle } else { React::Confused }));
        }
    }

    /// Copy the updater's state into the view model when it changes and
    /// announce a newly found release once.
    fn sync_update() {
        let rev = crate::update::revision();
        if UPDATE_REV.replace(rev) == rev {
            return;
        }
        let snap = crate::update::snapshot();
        let announce = VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            let was = vm.update.phase;
            let now = snap.phase;
            vm.update = snap.clone();
            now == crate::update::UpdatePhase::Available && was != now
        });
        if announce {
            toast(&format!("Update {} available \u{2014} open Settings", snap.version));
            with_eyes(|e| e.react(React::Sparkle));
        }
    }

    fn poll_smtc() {
        let snap = media_smtc::latest(std::time::Duration::from_secs(15));
        let now = now_ms();
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            match snap {
                Some(s) if !s.title.trim().is_empty() => {
                    let changed =
                        vm.track.title != s.title || vm.track.artist != s.artist || vm.track.album != s.album;
                    if changed {
                        SEEK_AT.set(None);
                    } else if SEEK_AT.get().map(|at| now.saturating_sub(at) >= 2_000).unwrap_or(false) {
                        // The player has had time to apply the seek; trust polls again.
                        SEEK_AT.set(None);
                    }
                    vm.track.title = s.title;
                    vm.track.artist = s.artist;
                    vm.track.album = s.album;
                    vm.track.app = s.app;
                    if s.duration_ms > 0 {
                        vm.track.duration_ms = s.duration_ms;
                        // A snapshot taken just before a seek would yank the bar
                        // back; keep the optimistically-updated position for a bit.
                        if SEEK_AT.get().is_none() {
                            vm.track.position_ms = s.position_ms.min(s.duration_ms);
                        }
                    }
                    vm.track.playback = if s.playing { Playback::Playing } else { Playback::Paused };
                }
                // Player closed / no session: stop pretending something plays.
                _ => {
                    if vm.track.playback == Playback::Playing {
                        vm.track.playback = Playback::Stopped;
                    }
                }
            }
        });
    }

    fn poll_volume() {
        if let Some(vs) = volume::try_read() {
            volume_changed(vs.pct, vs.muted);
        }
    }

    /// From the 1 s poll or the Core Audio callback: raise the HUD and let
    /// the face act out the new level. Returns true when something changed.
    fn volume_changed(pct: u8, muted: bool) -> bool {
        let cur = (pct, muted);
        let prev = LAST_VOL.replace(Some(cur));
        if prev.is_none() || prev == Some(cur) {
            return false;
        }
        let ms = now_ms();
        VIEW.with(|v| v.borrow_mut().hud = Some(hud_volume(pct, muted, ms)));
        if eyes_on() {
            with_eyes(|e| e.gauge(Gauge::Volume, pct as f32 / 100.0, muted));
        }
        true
    }

    /// Display brightness moved (laptop panel, via the power-setting
    /// notification). The first report after registering is just the
    /// current level and stays quiet.
    fn brightness_changed(pct: u8) -> bool {
        let prev = LAST_BRIGHT.replace(Some(pct));
        if prev.is_none() || prev == Some(pct) {
            return false;
        }
        let ms = now_ms();
        VIEW.with(|v| v.borrow_mut().hud = Some(hud_brightness(pct, ms)));
        if eyes_on() {
            with_eyes(|e| e.gauge(Gauge::Brightness, pct as f32 / 100.0, false));
        }
        true
    }

    /// Slower ambient signals, only sampled while the eyes are the idle look.
    fn poll_env(n: u32) {
        if n % 3 == 0 {
            let (mic, cam) = env::mic_cam();
            VIEW.with(|v| {
                let mut vm = v.borrow_mut();
                // Our own recorder must not make the eyes suspicious.
                vm.env.mic = mic && vm.voice.phase != crate::voice::VoicePhase::Recording;
                vm.env.camera = cam;
            });
        }
        if n % 5 == 0 {
            let weather_on = SETTINGS.with(|s| s.borrow().weather_mood);
            let (online, quiet) = (env::online(), env::quiet());
            VIEW.with(|v| {
                let mut vm = v.borrow_mut();
                vm.env.offline = !online;
                vm.env.quiet = quiet;
                vm.env.weather = if weather_on { env::weather() } else { Weather::Unknown };
            });
            if weather_on && n % 1800 == 0 {
                env::refresh_weather();
            }
        }
    }

    /// During a Focus block, what kind of app is in front (the eyes act it
    /// out). Our own window, menus and the desktop keep the last app.
    fn poll_focus_app(hwnd: HWND) {
        let enabled = SETTINGS.with(|s| s.borrow().focus_app_eyes);
        let working = VIEW.with(|v| v.borrow().focus_working());
        if !(eyes_on() && enabled && working) {
            VIEW.with(|v| v.borrow_mut().env.app = None);
            let why = if !working {
                "off (no focus phase running)"
            } else if !eyes_on() {
                "off (idle look is not Eyes)"
            } else {
                "off (Focus apps is turned off in Settings)"
            };
            log_focus(why.to_string());
            return;
        }
        let fg = unsafe { GetForegroundWindow() };
        if fg.0.is_null() || fg == hwnd || is_shell_window(fg) {
            return;
        }
        let Some((exe, title)) = env::window_app(fg) else {
            log_focus("on, but the window in front can't be read (elevated app?)".to_string());
            return;
        };
        let (theme, source) = SETTINGS.with(|s| {
            let s = s.borrow();
            apps::classify(&exe, &title, &s.app_themes, &s.eye_themes)
        });
        let app = FocusApp { exe: apps::exe_stem(&exe), theme, source };
        // Exe name and category only: the window title is never logged.
        log_focus(format!("on, {} -> {} ({:?})", app.exe, theme.map(|t| t.cat.id()).unwrap_or("no theme"), source));
        VIEW.with(|v| v.borrow_mut().env.app = Some(app));
    }

    /// Note a focus-eyes state change in `teardrop.log`, for "why don't I see it".
    fn log_focus(state: String) {
        if FOCUS_LOG.with(|l| *l.borrow() == state) {
            return;
        }
        let (_d, cache) = paths::ensure_dirs();
        store::log_line(&cache, &format!("focus eyes: {state}"));
        FOCUS_LOG.with(|l| *l.borrow_mut() = state);
    }

    /// Step the eye theme for the app in front (Focus page chip: click
    /// forward, wheel either way). Saved as a per-exe mapping.
    fn step_app_theme(hwnd: HWND, forward: bool) {
        let Some(exe) = VIEW.with(|v| v.borrow().env.app.as_ref().map(|a| a.exe.clone())) else { return };
        let (theme, source) = SETTINGS.with(|s| {
            let mut st = s.borrow_mut();
            let themes = st.eye_themes.clone();
            apps::step_override(&mut st.app_themes, &exe, forward, &themes);
            // The island has focus now, so re-classify the remembered exe; the
            // title isn't kept, the next poll in the app refines it.
            apps::classify(&exe, "", &st.app_themes, &st.eye_themes)
        });
        VIEW.with(|v| v.borrow_mut().env.app = Some(FocusApp { exe, theme, source }));
        with_eyes(|e| e.react(React::Sparkle));
        wake(hwnd);
    }

    fn slow_tick(hwnd: HWND) {
        let n = TICKS.get().wrapping_add(1);
        TICKS.set(n);
        if apply_fullscreen_hide(hwnd) {
            return;
        }
        let now = Utc::now();
        let ms = now.timestamp_millis() as u64;
        let remind = SETTINGS.with(|s| s.borrow().water_reminders);
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            vm.now = now;
            if n % 3 == 0 {
                CPU.with(|c| vm.system = sample_all(&mut c.borrow_mut(), &vm.system));
            }
            if vm.pomo.ends_at.is_some() {
                if let Some(next) = vm.pomo.tick(now) {
                    vm.hud = Some(Hud::transient(HudKind::Timer, "Focus", "phase complete", ms, 5000));
                    chime(990, 150);
                    // A focus block done is a win; a break over means back to it.
                    let r = if next == teardrop_core::Phase::Focus { React::Perk } else { React::Celebrate };
                    with_eyes(|e| e.react(r));
                }
            }
            vm.water.reminders_enabled = remind;
            let today_ml = vm.water.today(vm.day());
            if vm.water.reminder_due(now, today_ml) && vm.live_hud(ms).is_none() {
                vm.hud = Some(hud_water_due(ms));
                vm.water.snooze(now);
                chime(880, 120);
                UI.with(|u| u.borrow_mut().eyes.water_due());
            }
        });
        if eyes_on() {
            poll_env(n);
        }
        poll_focus_app(hwnd);
        poll_volume();
        if VIEW.with(|v| v.borrow().page) == Page::Usage && !USAGE_STARTED.replace(true) {
            crate::usage::refresh();
        } else if USAGE_STARTED.get() && n % 300 == 0 {
            crate::usage::refresh();
        }
        calendar_tick(hwnd, ms);
        if n == 10 || n % 900 == 10 {
            refresh_calendar();
        }
        if n == 25 || n % 21_600 == 25 {
            crate::update::check();
        }
        // Cheap: reads the published slot, which extrapolates the position.
        poll_smtc();
        lyrics_tick();
        if n % 30 == 0 {
            save_all();
        }
        wake(hwnd);
    }

    // ---------------------------------------------------------------- input

    fn mouse_xy(lp: LPARAM) -> (f32, f32) {
        let x = (lp.0 & 0xffff) as i16 as f32;
        let y = ((lp.0 >> 16) & 0xffff) as i16 as f32;
        let s = SCALE.get();
        (x / s, y / s)
    }

    fn hit_at(x: f32, y: f32) -> Option<Hit> {
        HITS.with(|h| hit_test(&h.borrow(), x, y))
    }

    /// A shelf tile becomes an OLE drag once the pointer moves far enough with
    /// the button held. A plain click still opens the file.
    fn maybe_start_shelf_drag(hwnd: HWND, wp: WPARAM, x: f32, y: f32) {
        if wp.0 & MK_LBUTTON.0 as usize == 0 {
            return;
        }
        let Some((ox, oy)) = PRESS_AT.get() else { return };
        let Some(Action::ShelfOpen(i)) = UI.with(|u| u.borrow().pressed) else { return };
        let (dx, dy) = (x - ox, y - oy);
        if dx * dx + dy * dy < 6.0 * 6.0 {
            return;
        }
        // The OLE loop owns the mouse from here; never also fire the click.
        PRESS_AT.set(None);
        UI.with(|u| u.borrow_mut().pressed = None);

        let path = VIEW.with(|v| {
            let vm = v.borrow();
            vm.shelf
                .items
                .len()
                .checked_sub(1 + i as usize)
                .and_then(|ix| vm.shelf.items.get(ix))
                .map(|it| it.path.clone())
        });
        let Some(path) = path else { return };

        let _ = unsafe { ReleaseCapture() };
        let _ = shelf_drag::drag(&path);
        wake(hwnd);
    }

    fn on_mouse_move(hwnd: HWND, wp: WPARAM, lp: LPARAM) {
        let (x, y) = mouse_xy(lp);
        LAST_MOUSE.set((x, y));
        maybe_start_shelf_drag(hwnd, wp, x, y);
        let act = hit_at(x, y).map(|h| h.action);
        UI.with(|u| u.borrow_mut().set_hover(act));
        let over_body = matches!(act, Some(Action::Island) | Some(Action::HudWater));
        let collapsed = ISLAND.with(|s| s.borrow().mode == IslandMode::Collapsed);
        let hover_open = SETTINGS.with(|s| s.borrow().open_on_hover);
        ISLAND.with(|s| {
            let mut st = s.borrow_mut();
            if st.mode == IslandMode::Collapsed {
                // The pre-stretch only hints that the island is about to open;
                // with open-on-hover off it would lurch and fade the content
                // for nothing every time the pointer brushes it.
                st.set_hover(over_body && hover_open);
            } else {
                st.hover = true;
            }
        });
        unsafe {
            if over_body && collapsed && hover_open {
                if !HOVER_PENDING.get() {
                    HOVER_PENDING.set(true);
                    HOVER_ARM.set((x, y));
                    let _ = SetTimer(hwnd, T_HOVER, 170, None);
                }
            } else if HOVER_PENDING.get() {
                HOVER_PENDING.set(false);
                let _ = KillTimer(hwnd, T_HOVER);
            }
            let _ = KillTimer(hwnd, T_LEAVE);
            let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            let _ = TrackMouseEvent(&mut tme);
        }
        wake(hwnd);
    }

    fn on_mouse_leave(hwnd: HWND) {
        LAST_MOUSE.set((-1.0, -1.0));
        UI.with(|u| u.borrow_mut().set_hover(None));
        ISLAND.with(|s| s.borrow_mut().set_hover(false));
        unsafe {
            if HOVER_PENDING.get() {
                HOVER_PENDING.set(false);
                let _ = KillTimer(hwnd, T_HOVER);
            }
            if is_open() {
                let _ = SetTimer(hwnd, T_LEAVE, 450, None);
            }
        }
        wake(hwnd);
    }

    fn on_wheel(hwnd: HWND, wp: WPARAM) {
        let delta = ((wp.0 >> 16) & 0xffff) as u16 as i16;
        let dir = if delta > 0 { -1 } else { 1 };
        if !is_open() {
            // Collapsed: the eyes spin with the wheel.
            if eyes_on() {
                with_eyes(|e| e.scrolled(dir));
                wake(hwnd);
            }
            return;
        }
        let (_, y) = LAST_MOUSE.get();
        let page = VIEW.with(|v| v.borrow().page);
        let in_content = y >= CONTENT_TOP;
        if hit_at(LAST_MOUSE.get().0, y).map(|h| h.action) == Some(Action::FocusAppTheme) {
            step_app_theme(hwnd, dir > 0);
            save_all();
            wake(hwnd);
            return;
        }
        if hit_at(LAST_MOUSE.get().0, y).map(|h| h.action) == Some(Action::Birthday) {
            SETTINGS.with(|s| {
                let mut st = s.borrow_mut();
                if let Some(b) = st.birthday {
                    st.birthday = Some(shift_birthday(b, -dir as i64));
                }
            });
            save_all();
            wake(hwnd);
            return;
        }
        if page == Page::Today && in_content && LAST_MOUSE.get().0 < 20.0 + 20.0 + 214.0 + 20.0 {
            UI.with(|u| u.borrow_mut().cal_offset += dir as i32);
            wake(hwnd);
            return;
        }
        if page == Page::Ai && in_content {
            let show_hist = VIEW.with(|v| v.borrow().ai.show_history);
            let show_setup = VIEW.with(|v| v.borrow().ai.show_setup);
            if show_hist {
                let n = VIEW.with(|v| ui::pages::ai_hist_max_scroll(v.borrow().ai.history.len()));
                UI.with(|u| u.borrow_mut().scroll_by(Page::Ai, dir, n));
                wake(hwnd);
                return;
            } else if !show_setup {
                let has_msgs = VIEW.with(|v| !v.borrow().ai.messages.is_empty());
                if has_msgs {
                    crate::ai::scroll_by(-dir);
                    sync_ai();
                    wake(hwnd);
                    return;
                }
            }
        }
        let n = VIEW.with(|v| {
            let vm = v.borrow();
            match page {
                Page::Today => {
                    let (sel_day, in_detail) = UI.with(|u| {
                        let ui = u.borrow();
                        (ui.cal_selected_day, ui.cal_selected_event.is_some())
                    });
                    if in_detail {
                        0
                    } else {
                        ui::pages::today_max_scroll(ui::pages::active_calendar_events(&vm, sel_day).len())
                    }
                }
                Page::Shelf => ui::pages::shelf_max_scroll(vm.shelf.items.len()),
                Page::Tasks => ui::pages::tasks_max_scroll(vm.tasks.tasks.len()),
                Page::Notes => ui::pages::notes_max_scroll(vm.notes.notes.len()),
                Page::Clipboard => ui::pages::clip_max_scroll(vm.clipboard.items().len()),
                _ => 0,
            }
        });
        if in_content && n > 0 {
            UI.with(|u| u.borrow_mut().scroll_by(page, dir, n));
        } else {
            VIEW.with(|v| {
                let mut vm = v.borrow_mut();
                vm.page = if vm.page == Page::Settings { LAST_TAB.get() } else { vm.page.step(dir) };
            });
            save_all();
        }
        wake(hwnd);
    }

    /// The field that's live right now: the multi-line note editor, the note
    /// search box, or the shared single-line input (tasks / AI fields).
    /// Returns (is_note_editor, is_search, char cap).
    fn field_kind() -> Option<(bool, bool, usize)> {
        let page = VIEW.with(|v| v.borrow().page);
        let (has_input, has_search) = UI.with(|u| {
            let ui = u.borrow();
            (ui.input.is_some(), ui.note_search.is_some())
        });
        if page == Page::Notes {
            if has_input {
                Some((true, false, NOTE_MAX_CHARS))
            } else if has_search {
                Some((false, true, 60))
            } else {
                None
            }
        } else if has_input {
            Some((false, false, if page == Page::Ai { 400 } else { 120 }))
        } else {
            None
        }
    }

    /// Read or mutate the live field (same resolution as `field_kind`).
    fn edit_field<R>(page: Page, f: impl FnOnce(&mut crate::ui::textedit::TextField) -> R) -> Option<R> {
        UI.with(|u| {
            let mut ui = u.borrow_mut();
            if page == Page::Notes {
                if let Some(t) = ui.input.as_mut() {
                    return Some(f(t));
                }
                if let Some(t) = ui.note_search.as_mut() {
                    return Some(f(t));
                }
                None
            } else {
                ui.input.as_mut().map(f)
            }
        })
    }

    /// After a field's text changed: push it into the view-model, persist, redraw.
    fn after_edit(hwnd: HWND, page: Page) {
        if page == Page::Notes {
            // Only the editor buffers into the view-model; the query just filters.
            if UI.with(|u| u.borrow().input.is_some()) {
                commit_note_input();
            }
        } else if page == Page::Ai {
            let prov = SETTINGS.with(|s| s.borrow().ai_provider);
            if crate::ai::snapshot(prov).focus_field == crate::ai::AiFocusField::Prompt {
                if let Some(t) = UI.with(|u| u.borrow().input.as_ref().map(|f| f.as_str().to_string())) {
                    crate::ai::input_set(t);
                    sync_ai();
                }
            }
        }
        save_all();
        wake(hwnd);
    }

    fn on_char(hwnd: HWND, code: u16) {
        let Some((editing_note, searching_note, cap)) = field_kind() else { return };
        let page = VIEW.with(|v| v.borrow().page);

        match code {
            13 => {
                // Enter: notes get a newline, AI commits, search keeps the
                // query, tasks add.
                if searching_note {
                    save_all();
                    wake(hwnd);
                    return;
                } else if page == Page::Ai {
                    commit_ai_field(true);
                } else if editing_note {
                    edit_field(page, |f| f.insert("\n", cap));
                    after_edit(hwnd, page);
                    return;
                } else {
                    add_task_from_input();
                }
            }
            c if c >= 32 && c != 127 => {
                let ch = if (0xD800..0xDC00).contains(&c) {
                    HIGH_SURR.set(c);
                    None
                } else if (0xDC00..0xE000).contains(&c) {
                    let hi = HIGH_SURR.replace(0);
                    char::decode_utf16([hi, c]).next().and_then(|r| r.ok())
                } else {
                    char::from_u32(c as u32)
                };
                if let Some(ch) = ch {
                    let s = ch.to_string();
                    edit_field(page, |f| f.insert(&s, cap));
                    after_edit(hwnd, page);
                    return;
                }
            }
            // Everything else (1,3,8,9,10,22,24,27,127) was already handled in
            // on_key — TranslateMessage still emits these WM_CHARs.
            _ => {}
        }
        save_all();
        wake(hwnd);
    }

    fn on_key(hwnd: HWND, vk: u32) -> bool {
        let page = VIEW.with(|v| v.borrow().page);
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
        let (ctrl, shift, alt) = unsafe {
            (
                GetKeyState(VK_CONTROL.0 as i32) < 0,
                GetKeyState(VK_SHIFT.0 as i32) < 0,
                GetKeyState(VK_MENU.0 as i32) < 0,
            )
        };

        if let Some((editing_note, searching_note, cap)) = field_kind() {
            // Ctrl shortcuts — but not with Alt held: AltGr is Ctrl+Alt and
            // must still type characters through WM_CHAR.
            if ctrl && !alt {
                match vk {
                    0x41 => {
                        // Ctrl+A
                        edit_field(page, |f| f.select_all());
                        wake(hwnd);
                        return true;
                    }
                    0x43 | 0x58 => {
                        // Ctrl+C copy / Ctrl+X cut: the selection, else the
                        // whole text (same as the old note behaviour).
                        let (text, sel) = edit_field(page, |f| (f.as_str().to_string(), f.selected_text())).unwrap_or_default();
                        let clip = sel.filter(|s| !s.is_empty()).unwrap_or(text);
                        if !clip.is_empty() {
                            write_clipboard_text(hwnd, &clip);
                            if vk == 0x58 {
                                edit_field(page, |f| {
                                    if f.selection().is_some() {
                                        f.delete(false);
                                    } else {
                                        f.set_text("");
                                    }
                                });
                                after_edit(hwnd, page);
                            } else {
                                wake(hwnd);
                            }
                            toast("Copied");
                            with_eyes(|e| e.react(React::GotIt));
                        }
                        return true;
                    }
                    0x56 => {
                        // Ctrl+V paste.
                        if let Some(clip) = read_clipboard_text(hwnd) {
                            // Notes keep paragraph breaks; other fields join lines.
                            let clean = if editing_note {
                                clip.replace("\r\n", "\n").replace('\r', "\n")
                            } else {
                                clip.lines().collect::<Vec<_>>().join(" ")
                            };
                            edit_field(page, |f| f.insert(&clean, cap));
                            after_edit(hwnd, page);
                        }
                        return true;
                    }
                    _ => {}
                }
            }

            // Editing keys fully handled here; WM_CHAR ignores their echoes.
            let text_changed = match vk {
                0x25 => {
                    edit_field(page, |f| f.move_h(-1, ctrl, shift));
                    false
                }
                0x27 => {
                    edit_field(page, |f| f.move_h(1, ctrl, shift));
                    false
                }
                0x26 => {
                    if editing_note {
                        edit_field(page, |f| f.move_v(-1, shift));
                    } else {
                        edit_field(page, |f| f.home(shift, true));
                    }
                    false
                }
                0x28 => {
                    if editing_note {
                        edit_field(page, |f| f.move_v(1, shift));
                    } else {
                        edit_field(page, |f| f.end(shift, true));
                    }
                    false
                }
                0x24 => {
                    edit_field(page, |f| f.home(shift, ctrl || !editing_note));
                    false
                }
                0x23 => {
                    edit_field(page, |f| f.end(shift, ctrl || !editing_note));
                    false
                }
                0x08 => edit_field(page, |f| f.backspace(ctrl)).unwrap_or(false),
                0x2E => edit_field(page, |f| f.delete(ctrl)).unwrap_or(false),
                0x1B => {
                    if edit_field(page, |f| f.selection().is_some()).unwrap_or(false) {
                        edit_field(page, |f| f.deselect());
                    } else if searching_note {
                        UI.with(|u| u.borrow_mut().note_search = None);
                    } else if editing_note {
                        finish_note_edit();
                    } else if page == Page::Ai {
                        commit_ai_field(false);
                    } else {
                        UI.with(|u| u.borrow_mut().input = None);
                    }
                    wake(hwnd);
                    return true;
                }
                _ => return false,
            };
            if text_changed {
                after_edit(hwnd, page);
            } else {
                wake(hwnd);
            }
            return true;
        }
        if vk == 0x1B {
            if is_open() {
                close_island(hwnd);
                return true;
            }
            return false;
        }
        if !is_open() {
            return false;
        }
        let now = Utc::now();
        let mut handled = true;
        match vk {
            0x25 | 0x27 => VIEW.with(|v| {
                let mut vm = v.borrow_mut();
                let d = if vk == 0x25 { -1 } else { 1 };
                vm.page = if vm.page == Page::Settings { LAST_TAB.get() } else { vm.page.step(d) };
            }),
            0x20 if page == Page::Music => perform(hwnd, Action::MediaPlayPause),
            0x20 if page == Page::Tasks => {
                let first = VIEW.with(|v| v.borrow().tasks.tasks.iter().position(|t| !t.done));
                if let Some(i) = first {
                    perform(hwnd, Action::TaskToggle(i as u16));
                }
            }
            0x4E if page == Page::Music => perform(hwnd, Action::MediaNext),
            0x4E if page == Page::Notes => perform(hwnd, Action::NoteNew),
            0xBF if page == Page::Notes => perform(hwnd, Action::NoteSearchInput),
            0x42 if page == Page::Music => perform(hwnd, Action::MediaPrev),
            0x53 if page == Page::Focus => VIEW.with(|v| v.borrow_mut().pomo.start_focus(now)),
            0x50 if page == Page::Focus => VIEW.with(|v| {
                let mut vm = v.borrow_mut();
                if vm.pomo.ends_at.is_some() {
                    vm.pomo.pause(now);
                } else {
                    vm.pomo.resume(now);
                }
            }),
            0x52 if page == Page::Focus => perform(hwnd, Action::FocusReset),
            0x57 => perform(hwnd, Action::WaterAdd(250)),
            0x41 => {
                VIEW.with(|v| v.borrow_mut().page = Page::Tasks);
                perform(hwnd, Action::TaskInput);
            }
            0x43 if page == Page::Tasks => perform(hwnd, Action::TaskClearDone),
            0x43 if page == Page::Clipboard => perform(hwnd, Action::ClipClear),
            0x2E if page == Page::Shelf => perform(hwnd, Action::ShelfRemove(0)),
            0x31..=0x36 if page == Page::Shelf => {
                let scroll = UI.with(|u| u.borrow().scroll_of(Page::Shelf));
                perform(hwnd, Action::ShelfOpen((scroll * 2 + (vk - 0x31) as usize) as u16));
            }
            0x31..=0x35 if page == Page::Clipboard => {
                let scroll = UI.with(|u| u.borrow().scroll_of(Page::Clipboard));
                perform(hwnd, Action::ClipCopy((scroll + (vk - 0x31) as usize) as u16));
            }
            _ => handled = false,
        }
        if handled {
            save_all();
            wake(hwnd);
        }
        handled
    }

    /// A file drag entered, left or landed on the island (OLE drop target).
    fn drop_event(hwnd: HWND, ev: DropEvent) {
        match ev {
            DropEvent::Enter(n, x, _) => {
                DRAG_OPENED.set(!is_open());
                VIEW.with(|v| v.borrow_mut().page = Page::Shelf);
                let zone = drop_zone(x);
                UI.with(|u| {
                    let mut u = u.borrow_mut();
                    u.drag_files = n;
                    u.drag_zone = zone;
                });
                open_island(hwnd, false);
            }
            DropEvent::Over(x, _) => {
                let zone = drop_zone(x);
                UI.with(|u| u.borrow_mut().drag_zone = zone);
            }
            DropEvent::Leave => {
                UI.with(|u| u.borrow_mut().drag_files = 0);
                if DRAG_OPENED.replace(false) && !PINNED.get() {
                    close_island(hwnd);
                }
            }
            DropEvent::Drop(list, x, _) => {
                DRAG_OPENED.set(false);
                let zone = drop_zone(x);
                UI.with(|u| u.borrow_mut().drag_files = 0);
                if !list.is_empty() {
                    UI.with(|u| {
                        let mut u = u.borrow_mut();
                        u.absorb_t = 1.0;
                        u.absorb_zone = zone;
                    });
                    if zone == 1 {
                        send_to_phone(hwnd, list);
                    } else {
                        on_drop_paths(hwnd, list);
                    }
                }
            }
        }
        wake(hwnd);
    }

    /// 0 = the Shelf half of the card, 1 = the Phone Link half, for a screen x.
    fn drop_zone(screen_x: i32) -> u8 {
        let local = (screen_x - WIN_POS.get().0) as f32 / SCALE.get();
        let mid = ISLAND.with(|s| s.borrow().geom.card_w) / 2.0 + ui::theme::PAD;
        u8::from(local >= mid)
    }

    /// Files dropped on the Phone Link half: resolve them off the UI thread,
    /// then show Windows' share window (Phone Link is a share target) on it.
    fn send_to_phone(hwnd: HWND, list: Vec<String>) {
        let raw = hwnd.0 as isize;
        std::thread::spawn(move || {
            let n = crate::share::prepare(&list);
            unsafe {
                let _ = PostMessageW(HWND(raw as *mut _), WM_APP_SHARE, WPARAM(n), LPARAM(list.len() as isize));
            }
        });
    }

    fn on_share_ready(hwnd: HWND, resolved: usize, dropped: usize) {
        if resolved == 0 {
            toast("Phone Link can only take files, not folders");
            return;
        }
        activate(hwnd);
        match crate::share::show(hwnd) {
            Ok(()) => {
                let skipped = dropped.saturating_sub(resolved);
                toast(&if skipped > 0 { format!("Pick Phone Link \u{2014} {skipped} folder(s) skipped") } else { "Pick Phone Link in the share window".to_string() });
            }
            Err(_) => toast("Couldn't open the share window"),
        }
    }

    fn on_drop_paths(hwnd: HWND, list: Vec<String>) {
        let now = Utc::now();
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            for p in &list {
                vm.shelf.add(p, now);
            }
            vm.page = Page::Shelf;
        });
        toast(&format!("Added {} file{} to the shelf", list.len(), if list.len() == 1 { "" } else { "s" }));
        with_eyes(|e| e.react(React::Satisfied));
        save_all();
        open_island(hwnd, false);
    }

    #[allow(non_snake_case)]
    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_CREATE => {
                load_all();
                crate::calendar::load_cache();
                let (_data, cache) = paths::ensure_dirs();
                store::log_line(&cache, "island created");
                let dpi = GetDpiForWindow(hwnd);
                if dpi >= 96 {
                    SCALE.set(dpi as f32 / 96.0);
                }
                let _ = RegisterDragDrop(hwnd, &DropTarget::new(hwnd, drop_event));
                let _ = AddClipboardFormatListener(hwnd);
                let _ = SetTimer(hwnd, T_SLOW, 1000, None);
                let _ = RegisterHotKey(hwnd, HOTKEY_ID, MOD_WIN | MOD_SHIFT, HOTKEY_VK);
                add_tray(hwnd);
                // Eye food: lock/unlock, apps asking for attention, crashes, downloads.
                let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
                if RegisterShellHookWindow(hwnd).as_bool() {
                    SHELL_MSG.set(RegisterWindowMessageW(w!("SHELLHOOK")));
                }
                env::spawn_downloads_watcher(hwnd, WM_APP_DOWNLOAD);
                // Live level changes: volume keys and the brightness slider/keys.
                volume::watch(hwnd.0 as isize, WM_APP_VOLUME);
                let _ = RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS, DEVICE_NOTIFY_WINDOW_HANDLE);
                if SETTINGS.with(|s| s.borrow().weather_mood) {
                    env::refresh_weather();
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                if (lp.0 & 0xffff) as u32 == HTCLIENT {
                    let (x, y) = LAST_MOUSE.get();
                    let cursor = match hit_at(x, y) {
                        Some(h) if h.text => IDC_IBEAM,
                        Some(_) => IDC_HAND,
                        None => IDC_ARROW,
                    };
                    if let Ok(c) = LoadCursorW(None, cursor) {
                        SetCursor(c);
                    }
                    return LRESULT(1);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_MOUSEMOVE => {
                on_mouse_move(hwnd, wp, lp);
                LRESULT(0)
            }
            _ if msg == WM_MOUSELEAVE_U32 => {
                on_mouse_leave(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = mouse_xy(lp);
                let act = hit_at(x, y).map(|h| h.action);
                PRESS_AT.set(Some((x, y)));
                let is_text_hit = matches!(
                    act,
                    Some(Action::TaskInput) | Some(Action::AiInput) | Some(Action::AiFocusModel) | Some(Action::AiFocusBaseUrl) | Some(Action::NoteSearchInput)
                );
                let page = VIEW.with(|v| v.borrow().page);
                if !is_text_hit && page == Page::Ai {
                    commit_ai_field(false);
                }
                UI.with(|u| {
                    let mut ui = u.borrow_mut();
                    ui.pressed = act;
                    // Clicking away from the field ends text entry.
                    if !is_text_hit && page != Page::Notes {
                        ui.input = None;
                    }
                    if act != Some(Action::NoteSearchInput) && ui.note_search.as_ref().is_some_and(|f| f.is_empty()) {
                        ui.note_search = None;
                    }
                });
                if is_open() {
                    // A click inside a hover-opened card pins it.
                    PINNED.set(true);
                    activate(hwnd);
                }
                SetCapture(hwnd);
                wake(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let _ = ReleaseCapture();
                PRESS_AT.set(None);
                let (x, y) = mouse_xy(lp);
                let released = hit_at(x, y);
                let pressed = UI.with(|u| u.borrow_mut().pressed.take());
                if let (Some(p), Some(r)) = (pressed, released) {
                    if p == r.action {
                        // The bar only knows "clicked"; turn that into a position.
                        if p == Action::SeekBar {
                            perform(hwnd, Action::Seek(ui::pages::seek_permille(r.rect, x)));
                        } else {
                            perform(hwnd, p);
                        }
                    }
                }
                wake(hwnd);
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                show_menu(hwnd);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                on_wheel(hwnd, wp);
                LRESULT(0)
            }
            WM_CHAR => {
                on_char(hwnd, wp.0 as u16);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if on_key(hwnd, (wp.0 & 0xffff) as u32) {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            WM_ACTIVATE => {
                let inactive = (wp.0 & 0xffff) as u32 == 0;
                if inactive && PINNED.get() && is_open() && !MENU_OPEN.get() {
                    close_island(hwnd);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_CLIPBOARDUPDATE => {
                let text = IsClipboardFormatAvailable(CF_UNICODETEXT_U32).is_ok();
                let image = IsClipboardFormatAvailable(CF_DIB_U32).is_ok() || IsClipboardFormatAvailable(CF_BITMAP_U32).is_ok();
                // Office/browsers add a bitmap to plain text copies; only an
                // image-only clipboard (a screenshot) gets the camera flash.
                if image && !text {
                    with_eyes(|e| e.react(React::Flash));
                    wake(hwnd);
                } else if text {
                    with_eyes(|e| e.react(React::GotIt));
                    wake(hwnd);
                }
                if SETTINGS.with(|s| s.borrow().clipboard_history) {
                    if let Some(t) = read_clipboard_text(hwnd) {
                        VIEW.with(|v| v.borrow_mut().clipboard.push(&t));
                        wake(hwnd);
                    }
                }
                LRESULT(0)
            }
            _ if msg == WM_APPCOMMAND_U32 => {
                // Volume keys: the 1 s volume poll raises the HUD; just let Windows handle them.
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            _ if msg == WM_POWERBROADCAST => {
                if wp.0 as u32 == PBT_POWERSETTINGCHANGE {
                    let ps = lp.0 as *const POWERBROADCAST_SETTING;
                    if let Some(ps) = ps.as_ref() {
                        if ps.PowerSetting == GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS && ps.DataLength >= 4 {
                            let pct = std::ptr::read_unaligned(ps.Data.as_ptr() as *const u32).min(100) as u8;
                            if brightness_changed(pct) {
                                wake(hwnd);
                            }
                        }
                    }
                    return LRESULT(1);
                }
                if wp.0 as u32 == 10 {
                    let ms = now_ms();
                    VIEW.with(|v| {
                        let mut vm = v.borrow_mut();
                        CPU.with(|c| vm.system = sample_all(&mut c.borrow_mut(), &vm.system));
                        let pct = vm.system.battery_pct.map(|b| format!("{b}%")).unwrap_or_default();
                        let (title, kind) = if vm.system.charging { ("Charging", HudKind::Charging) } else { ("On battery", HudKind::Charging) };
                        vm.hud = Some(Hud::transient(kind, title, &pct, ms, 3500));
                    });
                    wake(hwnd);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            _ if msg == WM_DEVICECHANGE => {
                if wp.0 as u32 == 0x8000 {
                    let ms = now_ms();
                    VIEW.with(|v| v.borrow_mut().hud = Some(Hud::transient(HudKind::Bluetooth, "Device", "connected", ms, 3000)));
                    with_eyes(|e| e.react(React::Perk));
                    wake(hwnd);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_HOTKEY => {
                if is_open() {
                    close_island(hwnd);
                } else {
                    open_island(hwnd, true);
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let dpi = (wp.0 & 0xffff) as u32;
                if dpi >= 96 {
                    SCALE.set(dpi as f32 / 96.0);
                }
                wake(hwnd);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                wake(hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
                match wp.0 {
                    T_ANIM => tick(hwnd),
                    T_SLOW => slow_tick(hwnd),
                    T_HOVER => {
                        let _ = KillTimer(hwnd, T_HOVER);
                        HOVER_PENDING.set(false);
                        let (x, y) = LAST_MOUSE.get();
                        let still = matches!(hit_at(x, y).map(|h| h.action), Some(Action::Island) | Some(Action::HudWater));
                        if still && !is_open() {
                            // Open only once the pointer rests; a pointer that keeps
                            // moving (or is petting the eyes) re-arms instead.
                            let (ax, ay) = HOVER_ARM.get();
                            let moved = (x - ax).powi(2) + (y - ay).powi(2) > 10.0 * 10.0;
                            let petting = UI.with(|u| u.borrow().eyes.petting());
                            if moved || petting {
                                HOVER_ARM.set((x, y));
                                HOVER_PENDING.set(true);
                                let _ = SetTimer(hwnd, T_HOVER, 170, None);
                            } else {
                                open_island(hwnd, false);
                            }
                        }
                    }
                    T_LEAVE => {
                        let _ = KillTimer(hwnd, T_LEAVE);
                        let pinned = PINNED.get() && GetForegroundWindow() == hwnd;
                        if is_open() && !pinned && !MENU_OPEN.get() {
                            close_island(hwnd);
                        }
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_COMMAND => LRESULT(0),
            // Windows shutting down, or an installer's Restart Manager asking
            // us to close: agree, then actually quit when told it is final.
            WM_QUERYENDSESSION => LRESULT(1),
            WM_ENDSESSION => {
                if wp.0 != 0 {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let (_d, cache) = paths::ensure_dirs();
                store::log_line(&cache, "island destroyed");
                crate::voice::stop_and_wait();
                save_all();
                let _ = RemoveClipboardFormatListener(hwnd);
                let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
                let _ = WTSUnRegisterSessionNotification(hwnd);
                let _ = DeregisterShellHookWindow(hwnd);
                remove_tray(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ if msg == TRAY_MSG => {
                let evt = (lp.0 & 0xffff) as u32;
                if evt == WM_LBUTTONUP {
                    if is_open() {
                        close_island(hwnd);
                    } else {
                        open_island(hwnd, true);
                    }
                } else if evt == WM_RBUTTONUP {
                    show_menu(hwnd);
                }
                LRESULT(0)
            }
            _ if msg == WM_WTSSESSION_CHANGE_U32 => {
                let ev = wp.0 as u32;
                if ev == WTS_SESSION_LOCK || ev == WTS_SESSION_UNLOCK {
                    let locked = ev == WTS_SESSION_LOCK;
                    VIEW.with(|v| v.borrow_mut().env.locked = locked);
                    with_eyes(|e| e.react(if locked { React::Goodnight } else { React::WelcomeBack }));
                    wake(hwnd);
                }
                LRESULT(0)
            }
            _ if msg == WM_APP_VOLUME => {
                if volume_changed(wp.0.min(100) as u8, lp.0 != 0) {
                    wake(hwnd);
                }
                LRESULT(0)
            }
            _ if msg == WM_APP_SHARE => {
                on_share_ready(hwnd, wp.0, lp.0 as usize);
                LRESULT(0)
            }
            _ if msg == WM_APP_DOWNLOAD => {
                with_eyes(|e| e.react(React::Happy));
                wake(hwnd);
                LRESULT(0)
            }
            _ if msg != 0 && msg == SHELL_MSG.get() => {
                // Shell hook traffic is constant (every title change); only wake for ours.
                let code = wp.0 as u32;
                if (code == HSHELL_WINDOWACTIVATED || code == HSHELL_RUDEAPPACTIVATED) && eyes_on() && lp.0 != 0 && lp.0 != hwnd.0 as isize {
                    // Glance toward the window that just came forward.
                    let mut rc = RECT::default();
                    if GetWindowRect(HWND(lp.0 as _), &mut rc).is_ok() {
                        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN).max(1) as f32, GetSystemMetrics(SM_CYSCREEN).max(1) as f32);
                        let cx = (rc.left + rc.right) as f32 / 2.0;
                        let cy = (rc.top + rc.bottom) as f32 / 2.0;
                        with_eyes(|e| e.glance((cx - sw / 2.0) / (sw / 2.0), (cy / sh) * 1.6 - 0.3));
                        wake(hwnd);
                    }
                    // Switch the work theme right away instead of on the next tick.
                    poll_focus_app(hwnd);
                }
                let r = match code {
                    HSHELL_FLASH => Some(React::Perk),
                    HSHELL_WINDOWCREATED if env::is_crash_window(HWND(lp.0 as _)) => Some(React::Shocked),
                    _ => None,
                };
                if let Some(r) = r {
                    with_eyes(|e| e.react(r));
                    wake(hwnd);
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    pub fn run() -> i32 {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            // STA for this UI thread: Core Audio COM reads and the shelf's OLE
            // drag source both need it.
            let _ = OleInitialize(None);
            let inst = GetModuleHandleW(None).unwrap_or_default();
            let cls = w!("TeardropIsland");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: inst.into(),
                lpszClassName: cls,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hIcon: load_app_icon().unwrap_or_default(),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
            let hwnd = match CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0),
                cls,
                w!("Teardrop"),
                WS_POPUP,
                0,
                0,
                16,
                16,
                None,
                None,
                inst,
                None,
            ) {
                Ok(h) => h,
                Err(_) => return 1,
            };
            crate::media_smtc::spawn_poller();
            render(hwnd);
            let mut msg = MSG::default();
            'outer: loop {
                if FAST.get() {
                    // Animating: drain input, draw a frame, then wait for the
                    // compositor's next vsync (DwmFlush follows the monitor's rate).
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        if msg.message == WM_QUIT {
                            break 'outer;
                        }
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    if FAST.get() {
                        tick(hwnd);
                        if DwmFlush().is_err() {
                            std::thread::sleep(std::time::Duration::from_millis(4));
                        }
                    }
                } else {
                    if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        break;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            msg.wParam.0 as i32
        }
    }
}

#[cfg(windows)]
pub use inner::run;

#[cfg(not(windows))]
pub fn run() -> i32 {
    eprintln!("teardrop: the island window runs on Windows 10/11. Try `--selftest <dir>` here.");
    3
}
