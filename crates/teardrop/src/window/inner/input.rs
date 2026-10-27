use super::*;
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

/// Last frame's hit rect for `a` — slider drags reference the track's
/// geometry even after the pointer leaves it.
fn hit_rect_of(a: Action) -> Option<crate::ui::scene::Rect> {
    HITS.with(|h| h.borrow().iter().find(|h| h.action == a).map(|h| h.rect))
}

/// Apply a Quick-page slider at window-local `x` (press/drag/release):
/// maps the track position to a level and writes the optimistic value
/// into the view-model so the knob follows the pointer instantly.
fn qs_apply_slider(a: Action, x: f32) {
    let Some(r) = hit_rect_of(a) else { return };
    let pct = (teardrop_core::slider_permille(r.x, r.w, x) / 10) as u8;
    match a {
        Action::QsVol => {
            if volume::set_scalar(pct as f32 / 100.0) {
                LAST_VOL.set(Some((pct, false)));
                VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    vm.quick.volume_pct = pct;
                    if pct > 0 {
                        vm.quick.muted = false;
                    }
                });
            }
        }
        Action::QsBright => {
            if crate::brightness::set(pct) {
                LAST_BRIGHT.set(Some(pct));
                VIEW.with(|v| v.borrow_mut().quick.brightness = Some(pct));
            }
        }
        _ => {}
    }
}

/// Enter / Connect on the Quick page's Wi-Fi password field: send the
/// typed key to the radios worker and put the field away.
pub(super) fn commit_wifi_pass(hwnd: HWND) {
    let (ssid, key) = UI.with(|u| {
        let mut ui = u.borrow_mut();
        let key = ui.input.as_ref().map(|f| f.as_str().to_string()).unwrap_or_default();
        ui.input = None;
        (ui.qs_pass_ssid.take(), key)
    });
    let Some(ssid) = ssid else { return };
    crate::radios::connect_wifi(&ssid, (!key.is_empty()).then_some(key));
    toast(&format!("Connecting to {ssid}"));
    wake(hwnd);
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
        vm.shelf.items.len().checked_sub(1 + i as usize).and_then(|ix| vm.shelf.items.get(ix)).map(|it| it.path.clone())
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
    // A held Quick slider follows the pointer even off the track.
    if wp.0 & MK_LBUTTON.0 as usize != 0 {
        if let Some(a @ (Action::QsVol | Action::QsBright)) = UI.with(|u| u.borrow().pressed) {
            qs_apply_slider(a, x);
        }
    }
    let act = hit_at(x, y).map(|h| h.action);
    UI.with(|u| u.borrow_mut().set_hover(act));
    let over_body = matches!(act, Some(Action::Island) | Some(Action::HudWater));
    let collapsed = ISLAND.with(|s| s.borrow().mode == IslandMode::Collapsed);
    ISLAND.with(|s| {
        let mut st = s.borrow_mut();
        if st.mode == IslandMode::Collapsed {
            // Pre-stretch hints that a glance is coming.
            st.set_hover(over_body);
        } else {
            st.hover = true;
        }
    });
    unsafe {
        if over_body && collapsed {
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
    let in_content = y >= CONTENT_TOP + SETTINGS.with(|s| s.borrow().top_gap());
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
    if matches!(hit_at(LAST_MOUSE.get().0, y).map(|h| h.action), Some(Action::PillGap(_))) {
        perform(hwnd, Action::PillGap(-dir as i8));
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
            Page::Windows => ui::pages::windows_max_scroll(vm.windows.len()),
            Page::Tasks => ui::pages::tasks_max_scroll(vm.tasks.tasks.len()),
            Page::Notes => ui::pages::notes_max_scroll(vm.notes.notes.len()),
            Page::Clipboard => ui::pages::clip_max_scroll(vm.clipboard.items().len()),
            Page::Chat => ui::pages::chat_max_scroll(vm.chat.msgs.len()),
            Page::Quick => {
                let (sec, pass) = UI.with(|u| (u.borrow().qs_section, u.borrow().qs_pass_ssid.is_some()));
                ui::pages::quick_max_scroll(&vm, sec, pass)
            }
            _ => 0,
        }
    });
    if in_content && n > 0 {
        UI.with(|u| u.borrow_mut().scroll_by(page, dir, n));
    } else {
        // Touchpads stream many small deltas per gesture; turn a page only
        // once the accumulated travel passes a notch, and pace the steps.
        let now = now_ms();
        let acc = if now - WHEEL_LAST.get() > 600 { delta as i32 } else { WHEEL_ACC.get() + delta as i32 };
        WHEEL_ACC.set(acc);
        if acc.abs() >= 120 && now - WHEEL_LAST.get() >= 220 {
            let dir = if acc > 0 { -1 } else { 1 };
            WHEEL_ACC.set(0);
            WHEEL_LAST.set(now);
            VIEW.with(|v| {
                let mut vm = v.borrow_mut();
                vm.page = if vm.page == Page::Settings { LAST_TAB.get() } else { ui::layout::step_page(vm.page, dir, &SETTINGS.with(|s| s.borrow().clone())) };
            });
            save_all();
        }
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
        Some((
            false,
            false,
            if page == Page::Ai {
                400
            } else if page == Page::Chat {
                240
            } else {
                120
            },
        ))
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
    } else if page == Page::Windows {
        // New query text: selection restarts at the top hit.
        UI.with(|u| u.borrow_mut().win_sel = 0);
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
            } else if page == Page::Windows {
                // Launch the top search hit.
                perform(hwnd, Action::WinLaunch(UI.with(|u| u.borrow().win_sel) as u16));
                return;
            } else if page == Page::Chat {
                chat_commit_input();
                return;
            } else if page == Page::Quick {
                commit_wifi_pass(hwnd);
                return;
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
    let (ctrl, shift, alt) = unsafe { (GetKeyState(VK_CONTROL.0 as i32) < 0, GetKeyState(VK_SHIFT.0 as i32) < 0, GetKeyState(VK_MENU.0 as i32) < 0) };

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
                        let clean = if editing_note { clip.replace("\r\n", "\n").replace('\r', "\n") } else { clip.lines().collect::<Vec<_>>().join(" ") };
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
            0x26 | 0x28 if page == Page::Windows => {
                // Arrows walk the launcher results (wrapping) instead of the caret.
                let q = UI.with(|u| u.borrow().input.as_ref().map(|f| f.as_str().to_string()).unwrap_or_default());
                let n = crate::launcher::hits(&q).len();
                if n > 0 {
                    UI.with(|u| {
                        let mut ui = u.borrow_mut();
                        let cur = ui.win_sel.min(n - 1);
                        ui.win_sel = if vk == 0x26 { (cur + n - 1) % n } else { (cur + 1) % n };
                    });
                }
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
            vm.page = if vm.page == Page::Settings { LAST_TAB.get() } else { ui::layout::step_page(vm.page, d, &SETTINGS.with(|s| s.borrow().clone())) };
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
        0x56 if page == Page::Shelf => paste_to_shelf(hwnd),
        0x2E if page == Page::Shelf => perform(hwnd, Action::ShelfRemove(0)),
        0x31..=0x36 if page == Page::System => {
            if let Some(t) = teardrop_core::SysTab::ALL.get((vk - 0x31) as usize) {
                perform(hwnd, Action::SysTab(*t));
            }
        }
        0x26 | 0x28 if page == Page::System => {
            let t = UI.with(|u| u.borrow().sys_tab);
            perform(hwnd, Action::SysTab(t.step(if vk == 0x26 { -1 } else { 1 })));
        }
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
        DropEvent::Drop(payload, x, _) => {
            DRAG_OPENED.set(false);
            let zone = drop_zone(x);
            UI.with(|u| u.borrow_mut().drag_files = 0);
            let mut list = payload.paths;
            // Virtual files and dropped pixels land as real files.
            for (name, bytes) in payload.blobs {
                let dir = paths::ensure_dirs().0.join("drops");
                let _ = std::fs::create_dir_all(&dir);
                let p = dir.join(&name);
                if std::fs::write(&p, &bytes).is_ok() {
                    list.push(p.to_string_lossy().into_owned());
                }
            }
            // On the Phone Link half a lone URL/text shares directly.
            if list.is_empty() && zone == 1 {
                if let Some(t) = payload.text {
                    let tr = t.trim().to_string();
                    if tr.starts_with("http://") || tr.starts_with("https://") {
                        crate::share::prepare_link(tr);
                    } else {
                        crate::share::prepare_text(t);
                    }
                    on_share_ready(hwnd, 1, 1);
                    return;
                }
            }
            // On the shelf half a dragged URL or lump of text becomes a
            // tiny file so the shelf (and file-only shares) accept it.
            if list.is_empty() {
                if let Some(t) = payload.text {
                    if let Some(p) = text_to_file(&t) {
                        list.push(p);
                    }
                }
            }
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

/// Ctrl+V on the Shelf page: clipboard files go straight on, clipboard
/// text becomes a .txt/.url drop file.
fn paste_to_shelf(hwnd: HWND) {
    let mut got = false;
    unsafe {
        use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard};
        use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
        if OpenClipboard(hwnd).is_ok() {
            if IsClipboardFormatAvailable(15).is_ok() {
                // CF_HDROP: file copies land on the shelf directly.
                if let Ok(h) = GetClipboardData(15) {
                    let hdrop = windows::Win32::UI::Shell::HDROP(h.0);
                    let count = windows::Win32::UI::Shell::DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
                    let now = Utc::now();
                    VIEW.with(|v| {
                        let mut vm = v.borrow_mut();
                        for i in 0..count.min(24) {
                            let mut buf = [0u16; 520];
                            let len = windows::Win32::UI::Shell::DragQueryFileW(hdrop, i, Some(&mut buf));
                            if len > 0 {
                                vm.shelf.add(&String::from_utf16_lossy(&buf[..len as usize]), now);
                                got = true;
                            }
                        }
                    });
                }
            } else if IsClipboardFormatAvailable(13).is_ok() {
                // CF_UNICODETEXT becomes a drop file.
                if let Ok(h) = GetClipboardData(13) {
                    let hg = windows::Win32::Foundation::HGLOBAL(h.0);
                    let base = GlobalLock(hg);
                    if !base.is_null() {
                        let ptr = base as *const u16;
                        let mut n = 0usize;
                        while *ptr.add(n) != 0 && n < 16384 {
                            n += 1;
                        }
                        let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, n));
                        let _ = GlobalUnlock(hg);
                        if let Some(p) = text_to_file(&s) {
                            VIEW.with(|v| v.borrow_mut().shelf.add(&p, Utc::now()));
                            got = true;
                        }
                    }
                }
            }
            let _ = CloseClipboard();
        }
    }
    if got {
        VIEW.with(|v| v.borrow_mut().page = Page::Shelf);
        toast("Pasted onto the shelf");
        with_eyes(|e| e.react(React::Satisfied));
        save_all();
    } else {
        toast("Nothing on the clipboard to shelve");
    }
    wake(hwnd);
}

/// Persist a dropped text/URL payload as a small file in the data dir:
/// links become `.url` shortcuts, plain text becomes `.txt`. Returns the
/// path so it can ride the shelf/share pipeline like any other file.
fn text_to_file(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let data = paths::ensure_dirs().0;
    let (ext, body) =
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") { ("url", format!("[InternetShortcut]\r\nURL={}\r\n", trimmed)) } else { ("txt", text.to_string()) };
    let path = data.join(format!("drop-{}.{}", now_ms(), ext));
    std::fs::write(&path, body).ok().map(|_| path.to_string_lossy().into_owned())
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
pub(super) unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
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
            apply_hotkeys(hwnd);
            add_tray(hwnd);
            // Eye food: lock/unlock, apps asking for attention, crashes, downloads.
            let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
            if RegisterShellHookWindow(hwnd).as_bool() {
                SHELL_MSG.set(RegisterWindowMessageW(w!("SHELLHOOK")));
            }
            env::spawn_downloads_watcher(hwnd, WM_APP_DOWNLOAD);
            // Live level changes: volume keys and the brightness slider/keys.
            volume::watch(hwnd.0 as isize, WM_APP_VOLUME);
            {
                let raw = hwnd.0 as isize;
                crate::launcher::set_notify(move || unsafe {
                    let _ = PostMessageW(HWND(raw as *mut _), WM_APP_SEARCH, WPARAM(0), LPARAM(0));
                });
                crate::launcher::warm();
                crate::sysmon::set_notify(move || unsafe {
                    let _ = PostMessageW(HWND(raw as *mut _), WM_APP_SEARCH, WPARAM(0), LPARAM(0));
                });
                crate::sysmon::start();
                crate::btdev::set_notify(move || unsafe {
                    let _ = PostMessageW(HWND(raw as *mut _), WM_APP_BT, WPARAM(0), LPARAM(0));
                });
                crate::btdev::start();
                crate::mesh_ble::set_notify(move || unsafe {
                    let _ = PostMessageW(HWND(raw as *mut _), WM_APP_MESH, WPARAM(0), LPARAM(0));
                });
                crate::radios::set_notify(move || unsafe {
                    let _ = PostMessageW(HWND(raw as *mut _), WM_APP_RADIO, WPARAM(0), LPARAM(0));
                });
                crate::radios::start();
            }
            if SETTINGS.with(|s| s.borrow().mesh_chat) {
                crate::chat::start("", now_ms());
            }
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
            // Sliders respond on press, not only on release.
            if let Some(a @ (Action::QsVol | Action::QsBright)) = act {
                qs_apply_slider(a, x);
            }
            let is_text_hit = matches!(
                act,
                Some(Action::TaskInput)
                    | Some(Action::AiInput)
                    | Some(Action::AiFocusModel)
                    | Some(Action::AiFocusBaseUrl)
                    | Some(Action::NoteSearchInput)
                    | Some(Action::WinSearch)
                    | Some(Action::QsPassField)
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
                    ui.qs_pass_ssid = None;
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
                    } else if let Action::MixerVol(i) = p {
                        // Click position on the session's track -> volume.
                        let frac = ((x - r.rect.x) / r.rect.w).clamp(0.0, 1.0);
                        let pid = VIEW.with(|v| v.borrow().audio.get(i as usize).map(|s| s.pid));
                        if let Some(pid) = pid {
                            crate::mixer::set_volume(pid, frac);
                            VIEW.with(|v| v.borrow_mut().audio = crate::mixer::sessions());
                        }
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
                // Bluetooth shows its own card (name + battery) once the watcher
                // sees the link; anything else gets the generic notice.
                crate::btdev::nudge();
                VIEW.with(|v| v.borrow_mut().hud = Some(Hud::transient(HudKind::Device, "Device", "connected", ms, 3000)));
                with_eyes(|e| e.react(React::Perk));
                wake(hwnd);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_HOTKEY => {
            if wp.0 as i32 == SEARCH_HOTKEY_ID {
                hotkey_jump(hwnd, Page::Windows, focus_win_search);
            } else if wp.0 as i32 == NOTE_HOTKEY_ID {
                hotkey_jump(hwnd, Page::Notes, |h| perform(h, Action::NoteNew));
            } else if wp.0 as i32 == AI_HOTKEY_ID {
                hotkey_jump(hwnd, Page::Ai, |h| perform(h, Action::AiInput));
            } else if is_open() {
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
                            // Rest stops at the glance pill; a longer
                            // rest continues into the full island.
                            glance_island(hwnd);
                            let _ = SetTimer(hwnd, T_GLANCE, 850, None);
                        }
                    }
                }
                T_GLANCE => {
                    let _ = KillTimer(hwnd, T_GLANCE);
                    let (x, y) = LAST_MOUSE.get();
                    let still = matches!(hit_at(x, y).map(|h| h.action), Some(Action::Island) | Some(Action::HudWater));
                    let petting = UI.with(|u| u.borrow().eyes.petting());
                    // Dwell continues to the full island only when
                    // open-on-hover is on; otherwise the glance is a peek.
                    let hover_open = SETTINGS.with(|s| s.borrow().open_on_hover);
                    if still && !petting && hover_open {
                        open_island(hwnd, false);
                    }
                }
                T_LEAVE => {
                    let _ = KillTimer(hwnd, T_LEAVE);
                    let _ = KillTimer(hwnd, T_GLANCE);
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
            for (id, ..) in hotkey_table(&Settings::default()) {
                let _ = UnregisterHotKey(hwnd, id);
            }
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
        _ if msg == WM_APP_SEARCH => {
            wake(hwnd);
            LRESULT(0)
        }
        _ if msg == WM_APP_BT => {
            if let Some(dev) = crate::btdev::take() {
                let ms = now_ms();
                VIEW.with(|v| v.borrow_mut().hud = Some(crate::viewmodel::hud_bluetooth(&dev.name, dev.battery, ms)));
                with_eyes(|e| e.react(React::Perk));
                wake(hwnd);
            }
            LRESULT(0)
        }
        _ if msg == WM_APP_RADIO => {
            // A radio toggled or arrived off-thread: copy the snapshot in now.
            let snap = crate::radios::snapshot();
            VIEW.with(|v| {
                let mut vm = v.borrow_mut();
                vm.quick.wifi = snap.wifi;
                vm.quick.bluetooth = snap.bluetooth;
                vm.quick.wifi_ssid = snap.ssid;
            });
            wake(hwnd);
            LRESULT(0)
        }
        _ if msg == WM_APP_MESH => {
            crate::chat::pump();
            sync_chat();
            if let Some((who, text)) = crate::chat::take_alert() {
                let ms = now_ms();
                // HUD shows the real text only when mesh_hud is on;
                // otherwise a generic notice — sender stays visible.
                let body = if SETTINGS.with(|s| s.borrow().mesh_hud) { text } else { "message received".to_string() };
                VIEW.with(|v| v.borrow_mut().hud = Some(crate::viewmodel::hud_chat(&who, &body, ms)));
                with_eyes(|e| e.react(React::Perk));
            }
            wake(hwnd);
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
            // Event-driven registry refresh: create/destroy/activate all
            // change what the switcher page should show.
            if matches!(code, 1 | 2 | 4 | 0x8004) {
                VIEW.with(|v| {
                    let mut vm = v.borrow_mut();
                    vm.windows = crate::winreg::enumerate(hwnd.0 as isize);
                    vm.audio = crate::mixer::sessions();
                });
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
