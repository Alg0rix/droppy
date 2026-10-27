use super::*;
// ----------------------------------------------------------- actions

fn send_media_key(vk: u8) {
    unsafe {
        keybd_event(vk, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
    }
}

pub(super) fn shell_open(hwnd: HWND, verb: &str, file: &str, params: Option<&str>) {
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

pub(super) fn chime(freq: u32, ms: u32) {
    if SETTINGS.with(|s| s.borrow().sounds) {
        unsafe {
            let _ = Beep(freq, ms);
        }
    }
}

pub(super) fn toast(msg: &str) {
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
        toast(&format!("+{} \u{00b7} {} today", teardrop_core::glasses_label(ml), teardrop_core::glasses_label(total)));
    }
    let goal = VIEW.with(|v| v.borrow().water.goal_ml);
    let reached = total >= goal && total.saturating_sub(ml) < goal;
    with_eyes(|e| e.react(if reached { React::Celebrate } else { React::Satisfied }));
}

/// Enter/Send on the Chat page: nick editor commits a rename, the
/// message field sends to the selected conversation.
pub(super) fn chat_commit_input() {
    let (text, editing_nick) = UI.with(|u| {
        let ui = u.borrow();
        (ui.input.as_ref().map(|f| f.as_str().to_string()).unwrap_or_default(), crate::chat::snapshot().editing_nick)
    });
    if editing_nick {
        crate::chat::set_nick(&text);
        crate::chat::set_editing_nick(false);
        UI.with(|u| u.borrow_mut().input = None);
    } else {
        crate::chat::send(text);
        UI.with(|u| u.borrow_mut().input = Some(crate::ui::textedit::TextField::default()));
    }
    sync_chat();
    wake_if_open();
}

/// Refresh `vm.chat` from the chat service runtime when it changed.
pub(super) fn sync_chat() {
    thread_local! {
        static SEEN: std::cell::Cell<u32> = const { std::cell::Cell::new(u32::MAX) };
    }
    let rev = crate::chat::revision();
    if SEEN.replace(rev) == rev {
        return;
    }
    let snap = crate::chat::snapshot();
    VIEW.with(|v| v.borrow_mut().chat = snap);
}

fn wake_if_open() {
    // Redrawn via the caller's wake().
}

pub(super) fn add_task_from_input() {
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
pub(super) fn commit_note_input() {
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
pub(super) fn finish_note_edit() {
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
    let day_ctx = if set.ai_share_day { Some(VIEW.with(|v| v.borrow().day_context_summary())) } else { None };
    UI.with(|u| u.borrow_mut().input = None);
    crate::ai::send_prompt(prompt, set, day_ctx);
    with_eyes(|e| e.react(React::GotIt));
    sync_ai();
}

pub(super) fn commit_ai_field(send_if_prompt: bool) {
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

pub(super) fn perform(hwnd: HWND, a: Action) {
    let now = Utc::now();
    let ms = now_ms();
    let mut save = true;
    match a {
        Action::Island => {
            open_island(hwnd, true);
            save = false;
        }
        Action::HudWater => add_water(250),
        Action::WinActivate(i) => {
            let target = VIEW.with(|v| v.borrow().windows.get(i as usize).map(|w| w.hwnd));
            if let Some(h) = target {
                let h = HWND(h as *mut _);
                unsafe {
                    let _ = ShowWindow(h, windows::Win32::UI::WindowsAndMessaging::SW_RESTORE);
                    let _ = SetForegroundWindow(h);
                }
                with_eyes(|e| e.react(React::Peek));
            }
            save = false;
        }
        Action::MixerVol(_) => save = false,
        Action::QsWifi => {
            crate::radios::toggle(crate::radios::RadioKind::Wifi);
            with_eyes(|e| e.react(React::Nod));
            save = false;
        }
        Action::QsBt => {
            crate::radios::toggle(crate::radios::RadioKind::Bluetooth);
            with_eyes(|e| e.react(React::Nod));
            save = false;
        }
        Action::QsMute => {
            let muted = VIEW.with(|v| v.borrow().quick.muted);
            if volume::set_muted(!muted) {
                LAST_VOL.set(Some((VIEW.with(|v| v.borrow().quick.volume_pct), !muted)));
                VIEW.with(|v| v.borrow_mut().quick.muted = !muted);
                with_eyes(|e| e.react(if muted { React::Nod } else { React::Shake }));
            }
            save = false;
        }
        Action::QsMicMute => {
            let cur = VIEW.with(|v| v.borrow().quick.mic_muted).unwrap_or(false);
            if volume::set_mic_muted(!cur) {
                VIEW.with(|v| v.borrow_mut().quick.mic_muted = Some(!cur));
                toast(if cur { "Microphone on" } else { "Microphone muted" });
            }
            save = false;
        }
        // Sliders resolve to a level on press/drag, not here.
        Action::QsVol | Action::QsBright => save = false,
        Action::QsLink(l) => {
            shell_open(hwnd, "open", l.uri(), None);
            with_eyes(|e| e.react(React::Salute));
            save = false;
        }
        Action::QsExpand(sec) => {
            let opening = UI.with(|u| {
                let mut ui = u.borrow_mut();
                let opening = ui.qs_section != sec;
                ui.qs_section = if opening { sec } else { QsSection::None };
                ui.qs_pass_ssid = None;
                ui.input = None;
                ui.scroll_by(Page::Quick, -999, 0);
                opening
            });
            match (opening, sec) {
                (true, QsSection::Wifi) => crate::radios::scan_wifi(),
                (true, QsSection::Bt) => crate::radios::scan_bt(true),
                (false, QsSection::Bt) => crate::radios::scan_bt(false),
                _ => {}
            }
            if opening {
                with_eyes(|e| e.react(React::Peek));
            }
            save = false;
        }
        Action::QsWifiNet(i) => {
            let net = VIEW.with(|v| v.borrow().quick.wifi_nets.get(i as usize).cloned());
            if let Some(n) = net {
                if n.connected {
                    crate::radios::disconnect_wifi();
                    toast(&format!("Disconnecting from {}", n.ssid));
                    with_eyes(|e| e.react(React::Shake));
                } else if n.has_profile || !n.secured {
                    crate::radios::connect_wifi(&n.ssid, None);
                    toast(&format!("Connecting to {}", n.ssid));
                    with_eyes(|e| e.react(React::Nod));
                } else {
                    // Secured with no saved profile: collect the key inline.
                    UI.with(|u| {
                        let mut ui = u.borrow_mut();
                        ui.qs_pass_ssid = Some(n.ssid.clone());
                        ui.input = Some(crate::ui::textedit::TextField::default());
                    });
                    activate(hwnd);
                }
            }
            save = false;
        }
        Action::QsPassField => {
            UI.with(|u| {
                let mut ui = u.borrow_mut();
                if ui.input.is_none() {
                    ui.input = Some(crate::ui::textedit::TextField::default());
                }
            });
            activate(hwnd);
            save = false;
        }
        Action::QsWifiConnect => {
            commit_wifi_pass(hwnd);
            with_eyes(|e| e.react(React::Perk));
            save = false;
        }
        Action::QsBtDev(i) => {
            let dev = VIEW.with(|v| v.borrow().quick.bt_devs.get(i as usize).cloned());
            if let Some(d) = dev {
                if !d.paired {
                    crate::radios::pair_bt(&d.id);
                    toast(&format!("Pairing {}", d.name));
                    with_eyes(|e| e.react(React::Nod));
                }
            }
            save = false;
        }
        Action::QsRescan => {
            match UI.with(|u| u.borrow().qs_section) {
                QsSection::Wifi => crate::radios::scan_wifi(),
                QsSection::Bt => {
                    crate::radios::scan_bt(true);
                    crate::radios::nudge();
                }
                QsSection::None => crate::radios::nudge(),
            }
            with_eyes(|e| e.react(React::Radar));
            save = false;
        }
        Action::WinSearch => {
            UI.with(|u| {
                let mut ui = u.borrow_mut();
                if ui.input.is_none() {
                    ui.input = Some(crate::ui::textedit::TextField::default());
                    ui.win_sel = 0;
                }
            });
            crate::launcher::warm();
            activate(hwnd);
            with_eyes(|e| e.react(React::Inspect));
            save = false;
        }
        Action::WinLaunch(i) => {
            let hits = crate::launcher::hits(&UI.with(|u| u.borrow().input.as_ref().map(|f| f.as_str().to_string()).unwrap_or_default()));
            if let Some(h) = hits.get(i as usize) {
                crate::launcher::launch(&h.path);
                UI.with(|u| u.borrow_mut().input = None);
                with_eyes(|e| e.react(React::Zoom));
            }
            save = false;
        }
        Action::TogglePage(p) => {
            SETTINGS.with(|s| {
                let mut st = s.borrow_mut();
                let t = p.title().to_string();
                if let Some(i) = st.hidden_pages.iter().position(|h| h == &t) {
                    st.hidden_pages.remove(i);
                } else {
                    st.hidden_pages.push(t);
                }
            });
            // If the current page just got hidden, move to the first visible one.
            let set = SETTINGS.with(|s| s.borrow().clone());
            if set.hidden_pages.iter().any(|h| h == VIEW.with(|v| v.borrow().page.title())) {
                VIEW.with(|v| v.borrow_mut().page = ui::layout::visible_pages(&set)[0]);
            }
        }
        Action::SysTab(t) => {
            UI.with(|u| u.borrow_mut().sys_tab = t);
            save = false;
        }
        Action::Tab(p) => {
            // Leaving the notes editor closes it (and prunes a blank note).
            if VIEW.with(|v| v.borrow().page) == Page::Notes && p != Page::Notes {
                finish_note_edit();
            }
            if p == Page::Quick {
                // Fresh radio state instead of the up-to-4s-old snapshot.
                crate::radios::nudge();
            }
            VIEW.with(|v| v.borrow_mut().page = p);
        }
        Action::CalStep(d) => {
            UI.with(|u| u.borrow_mut().cal_offset += d as i32);
            with_eyes(|e| e.react(React::Swipe(d > 0)));
            save = false;
        }
        Action::UpdateCheck => {
            crate::update::check();
            with_eyes(|e| e.react(React::Radar));
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
                ui::pages::active_calendar_events(&vm, sel_day).get(i as usize).and_then(|e| e.join.as_ref().map(|j| j.url.clone()))
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
                    let mut out = format!("{}\n{} \u{00b7} {}", ev.title, crate::calendar::full_date_label(ev), crate::calendar::time_range_label(ev));
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
                vm.water.sub(day, teardrop_core::GLASS_ML);
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
            open_island(hwnd, false);
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
            open_island(hwnd, false);
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
            let note = VIEW.with(|v| v.borrow().notes.notes.get(i as usize).map(|n| (n.id.clone(), n.text.clone())));
            if let Some((id, text)) = note {
                NOTE_BEFORE.set(Some(text.clone()));
                UI.with(|u| {
                    let mut ui = u.borrow_mut();
                    ui.input = Some(crate::ui::textedit::TextField::new(text));
                    ui.note_edit = Some(id);
                    ui.note_search = None;
                });
                activate(hwnd);
                with_eyes(|e| e.react(React::Read));
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
                SettingKey::MeshChat => {
                    if on {
                        crate::chat::start("", now_ms());
                    } else {
                        crate::chat::stop();
                    }
                }
                SettingKey::HotkeyIsland | SettingKey::HotkeySearch | SettingKey::HotkeyNote | SettingKey::HotkeyAi => {
                    let taken = apply_hotkeys(hwnd);
                    if on && taken.contains(&k.label()) {
                        toast(&format!("{} is used by another app", k.label()));
                    }
                }
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
        Action::IslandTheme(t) => {
            SETTINGS.with(|s| s.borrow_mut().theme = t);
            with_eyes(|e| e.react(React::Sparkle));
        }
        Action::IslandStyle(st) => {
            SETTINGS.with(|s| s.borrow_mut().island_style = st);
            with_eyes(|e| e.react(React::Sparkle));
        }
        Action::PillGap(d) => {
            SETTINGS.with(|s| {
                let mut st = s.borrow_mut();
                st.pill_gap = (st.pill_gap as i32 + d as i32 * teardrop_core::PILL_GAP_STEP as i32).clamp(0, teardrop_core::PILL_GAP_MAX as i32) as u32;
            });
            with_eyes(|e| e.react(if d > 0 { React::Nod } else { React::Shake }));
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
                vm.water.goal_ml =
                    (vm.water.goal_ml as i32 + teardrop_core::GLASS_ML as i32 * d as i32).clamp(2 * teardrop_core::GLASS_ML as i32, 24 * teardrop_core::GLASS_ML as i32) as u32;
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
            let text = UI.with(|u| u.borrow().input.as_ref().map(|f| f.as_str().to_string())).unwrap_or_else(|| crate::ai::snapshot(prov).input);
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
        Action::ChatSelect(i) => {
            let sel = if i == 0 { None } else { VIEW.with(|v| v.borrow().chat.peers.get(i as usize - 1).map(|p| p.id.clone())) };
            crate::chat::select(sel);
            sync_chat();
            save = false;
        }
        Action::ChatInput => {
            UI.with(|u| u.borrow_mut().input = Some(crate::ui::textedit::TextField::default()));
            crate::chat::set_editing_nick(false);
            activate(hwnd);
            save = false;
        }
        Action::ChatNick => {
            let cur = VIEW.with(|v| v.borrow().chat.nick.clone());
            UI.with(|u| u.borrow_mut().input = Some(crate::ui::textedit::TextField::new(cur)));
            crate::chat::set_editing_nick(true);
            activate(hwnd);
            save = false;
        }
        Action::ChatSend => {
            chat_commit_input();
            with_eyes(|e| e.react(React::Perk));
            save = false;
        }
        Action::Quit => unsafe {
            with_eyes(|e| e.react(React::Goodnight));
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
