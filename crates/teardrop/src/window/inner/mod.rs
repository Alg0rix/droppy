mod actions;
mod clipboard;
mod input;
mod persist;
mod render;
mod services;
mod timers;
mod tray;
mod win_state;

use actions::*;
use clipboard::*;
use input::*;
use persist::*;
use render::*;
use services::*;
use timers::*;
use tray::*;
use win_state::*;

use crate::drop_target::{DropEvent, DropTarget};
use crate::render_d2d::d2d::{render_offscreen, Presenter};
use crate::services_env::sys as env;
use crate::services_system::sys::{sample_all, CpuTracker};
use crate::state::{IslandMode, IslandState};
use crate::ui::eyes::{Eyes, Gauge, React};
use crate::ui::layout::{build, hit_test, CONTENT_TOP};
use crate::ui::scene::{Color, Fill, Scene};
use crate::ui::state::{Action, Hit, QsSection, SettingKey, UiState};
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
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject, GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
    RemoveClipboardFormatListener, SetClipboardData,
};
use windows::Win32::System::Diagnostics::Debug::Beep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{OleInitialize, RegisterDragDrop};
use windows::Win32::System::Power::{RegisterPowerSettingNotification, POWERBROADCAST_SETTING};
use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows::Win32::System::SystemServices::{GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS, MK_LBUTTON};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, RegisterHotKey, ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, UnregisterHotKey, HOT_KEY_MODIFIERS, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, MOD_ALT,
    MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, TME_LEAVE, TRACKMOUSEEVENT, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK,
};
use windows::Win32::UI::Shell::{ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows::Win32::UI::WindowsAndMessaging::*;

const TRAY_MSG: u32 = 0x8001;
const ID_QUIT: usize = 1000;
const ID_TOGGLE: usize = 1001;
const ID_SETTINGS: usize = 1002;
const HOTKEY_ID: i32 = 1;
const HOTKEY_VK: u32 = 0x54; // T
/// Alt+Space: open straight into the launcher search.
const SEARCH_HOTKEY_ID: i32 = 2;
/// Alt+N: new note. Alt+I: AI prompt.
const NOTE_HOTKEY_ID: i32 = 3;
const AI_HOTKEY_ID: i32 = 4;
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
/// Launcher search results / app icons finished on a worker thread.
const WM_APP_SEARCH: u32 = 0x8005;
/// A Bluetooth device connected (btdev worker thread).
const WM_APP_BT: u32 = 0x8007;
const WM_APP_MESH: u32 = 0x8008;
const WM_APP_RADIO: u32 = 0x8009;
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
const T_GLANCE: usize = 5;

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
    static SYS_REV: Cell<u64> = Cell::new(u64::MAX);
    static DRAG_OPENED: Cell<bool> = Cell::new(false);
    static HOVER_PENDING: Cell<bool> = Cell::new(false);
    static LAST_MOUSE: Cell<(f32, f32)> = Cell::new((-1.0, -1.0));
    /// Wheel page-step damping: accumulated delta and last step time so
    /// touchpad momentum doesn't flick through pages.
    static WHEEL_ACC: Cell<i32> = Cell::new(0);
    static WHEEL_LAST: Cell<u64> = Cell::new(0);
    static PRESS_AT: Cell<Option<(f32, f32)>> = Cell::new(None);
    static SEEK_AT: Cell<Option<u64>> = Cell::new(None);
    static WIN_POS: Cell<(i32, i32)> = Cell::new((0, 0));
    static LAST_TAB: Cell<Page> = Cell::new(Page::Music);
    static TICKS: Cell<u32> = Cell::new(0);
    static LYR_STABLE: Cell<u8> = Cell::new(0);
    static LAST_VOL: Cell<Option<(u8, bool)>> = Cell::new(None);
    static LAST_BRIGHT: Cell<Option<u8>> = Cell::new(None);
    static LAST_QS_SCAN: Cell<bool> = Cell::new(false);
    /// Note text when the editor opened, to tell a real edit from a peek.
    static NOTE_BEFORE: RefCell<Option<String>> = RefCell::new(None);
    static HIGH_SURR: Cell<u16> = Cell::new(0);
    static SHELL_MSG: Cell<u32> = Cell::new(0);
    static HOVER_ARM: Cell<(f32, f32)> = Cell::new((-1.0, -1.0));
    static HIDE_AT: Cell<Option<u64>> = Cell::new(None);
    static SHOW_AT: Cell<Option<u64>> = Cell::new(None);
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
    ISLAND.with(|s| matches!(s.borrow().mode, IslandMode::Open | IslandMode::Opening | IslandMode::Glance | IslandMode::Glancing))
}

fn open_amount() -> f32 {
    ISLAND.with(|s| s.borrow().open())
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
            w!("Droppy"),
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
