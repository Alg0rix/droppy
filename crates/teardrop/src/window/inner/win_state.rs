use super::*;
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

pub(super) fn activate(hwnd: HWND) {
    set_interactive(hwnd, true);
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        if GetForegroundWindow() != hwnd {
            let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let me = GetCurrentThreadId();
            if fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool() {
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(hwnd);
                let _ = AttachThreadInput(me, fg_thread, false);
            }
        }
        let _ = SetFocus(hwnd);
    }
}

/// (id, enabled, modifiers, virtual key, label) for every global hotkey.
pub(super) fn hotkey_table(s: &Settings) -> [(i32, bool, HOT_KEY_MODIFIERS, u32, &'static str); 4] {
    [
        (HOTKEY_ID, s.hotkey_island, MOD_WIN | MOD_SHIFT, HOTKEY_VK, "Win+Shift+T"),
        (SEARCH_HOTKEY_ID, s.hotkey_search, MOD_ALT | MOD_NOREPEAT, 0x20, "Alt+Space"),
        (NOTE_HOTKEY_ID, s.hotkey_note, MOD_ALT | MOD_NOREPEAT, 0x4E, "Alt+N"),
        (AI_HOTKEY_ID, s.hotkey_ai, MOD_ALT | MOD_NOREPEAT, 0x49, "Alt+I"),
    ]
}

/// Re-register the global hotkeys to match settings. Returns the labels of
/// enabled hotkeys another app already owns (Windows refuses duplicates).
pub(super) fn apply_hotkeys(hwnd: HWND) -> Vec<&'static str> {
    let set = SETTINGS.with(|s| s.borrow().clone());
    let mut taken = Vec::new();
    for (id, on, mods, vk, label) in hotkey_table(&set) {
        unsafe {
            let _ = UnregisterHotKey(hwnd, id);
            if on && RegisterHotKey(hwnd, id, mods, vk).is_err() {
                taken.push(label);
            }
        }
    }
    taken
}

/// Switch pages from a hotkey: saves a note being edited, remembers the
/// page we came from for the Settings round trip.
fn goto_page(page: Page) {
    if VIEW.with(|v| v.borrow().page) == Page::Notes && page != Page::Notes {
        finish_note_edit();
    }
    VIEW.with(|v| {
        let mut vm = v.borrow_mut();
        if vm.page != Page::Settings {
            LAST_TAB.set(vm.page);
        }
        vm.page = page;
    });
}

/// Open the island on `page` and run `focus` to put the cursor in its
/// field. Pressing the hotkey again while that field is live closes it.
pub(super) fn hotkey_jump(hwnd: HWND, page: Page, focus: impl FnOnce(HWND)) {
    let live = is_open() && unsafe { GetForegroundWindow() } == hwnd && VIEW.with(|v| v.borrow().page) == page && UI.with(|u| u.borrow().input.is_some());
    if live {
        close_island(hwnd);
        return;
    }
    open_island(hwnd, true);
    goto_page(page);
    focus(hwnd);
}

/// Fresh, focused launcher search field on the Windows page.
pub(super) fn focus_win_search(hwnd: HWND) {
    UI.with(|u| {
        let mut ui = u.borrow_mut();
        ui.input = Some(crate::ui::textedit::TextField::default());
        ui.note_search = None;
        ui.note_edit = None;
        ui.win_sel = 0;
    });
    crate::launcher::warm();
    activate(hwnd);
    wake(hwnd);
}

pub(super) fn open_island(hwnd: HWND, pinned: bool) {
    ISLAND.with(|s| {
        let mut st = s.borrow_mut();
        match st.mode {
            IslandMode::Collapsed | IslandMode::Closing => st.toggle(),
            IslandMode::Glancing | IslandMode::Glance => st.expand(),
            _ => {}
        }
    });
    unsafe {
        let _ = KillTimer(hwnd, T_GLANCE);
    }
    if pinned {
        PINNED.set(true);
        activate(hwnd);
        with_eyes(|e| e.poke());
    }
    wake(hwnd);
}

/// Dwell-hover stops at the glance pill; a longer rest (or a click)
/// continues into the fully open island.
pub(super) fn glance_island(hwnd: HWND) {
    ISLAND.with(|s| s.borrow_mut().glance());
    wake(hwnd);
}

pub(super) fn close_island(hwnd: HWND) {
    ISLAND.with(|s| {
        let mut st = s.borrow_mut();
        if matches!(st.mode, IslandMode::Open | IslandMode::Opening | IslandMode::Glance | IslandMode::Glancing) {
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

pub(super) fn apply_fullscreen_hide(hwnd: HWND) -> bool {
    if !SETTINGS.with(|s| s.borrow().hide_in_fullscreen) {
        SHOW_AT.set(None);
        if HIDDEN.get() {
            HIDDEN.set(false);
            render(hwnd);
            let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
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
            SHOW_AT.set(None);
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
        } else {
            // Unhide only after the foreground has been non-fullscreen
            // for a beat, so a flickering foreground can't blink us.
            let pending = HIDDEN.get() || HIDE_AT.get().is_some();
            let now = now_ms();
            match SHOW_AT.get() {
                None if pending => SHOW_AT.set(Some(now + 700)),
                Some(at) if now >= at && pending => {
                    SHOW_AT.set(None);
                    HIDE_AT.set(None);
                    HIDDEN.set(false);
                    // Paint a fresh frame first so the stale one never flashes up.
                    render(hwnd);
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
                Some(_) if !pending => SHOW_AT.set(None),
                _ => {}
            }
        }
        hide
    }
}

/// The desktop and taskbar cover the monitor too, but they are not a
/// fullscreen app: focusing them must never hide the island.
pub(super) fn is_shell_window(w: HWND) -> bool {
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
