use super::*;
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
pub(super) fn sync_position() {
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

pub(super) fn refresh_calendar() {
    let urls = SETTINGS.with(|s| s.borrow().calendar_urls.clone());
    crate::calendar::refresh(urls);
}

/// Open a meeting link, but only one that points at a known meeting host.
pub(super) fn open_meeting(hwnd: HWND, url: Option<String>) {
    match url.filter(|u| crate::calendar::meeting_platform(u).is_some()) {
        Some(u) => shell_open(hwnd, "open", &u, None),
        None => toast("This meeting has no join link"),
    }
}

/// Calendar events into the view model when the worker publishes new ones.
pub(super) fn sync_calendar() {
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
pub(super) fn sync_voice() {
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
pub(super) fn sync_usage() {
    let rev = crate::usage::revision();
    if USAGE_REV.replace(rev) == rev {
        return;
    }
    let snap = crate::usage::snapshot();
    VIEW.with(|v| v.borrow_mut().usage = snap);
}

/// Synchronize AI Chat state and apply any newly returned `[ACTION:...]` directives.
pub(super) fn sync_ai() {
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
                            badges.push(AiActionBadge { label: format!("Added task: {title}"), undo: UndoOp::RemoveAddedTask(id), undone: false });
                            mutated = true;
                        }
                        DoneTask(query) => {
                            let q = query.to_lowercase();
                            if let Some(t) = vm.tasks.tasks.iter_mut().find(|t| !t.done && t.title.to_lowercase().contains(&q)) {
                                t.done = true;
                                let id = t.id.clone();
                                let title = t.title.clone();
                                badges.push(AiActionBadge { label: format!("Completed: {title}"), undo: UndoOp::ReopenTask(id), undone: false });
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
                            badges.push(AiActionBadge { label: format!("Logged +{} of water", teardrop_core::glasses_label(ml)), undo: UndoOp::SubtractWater(ml), undone: false });
                            mutated = true;
                        }
                        Focus(cmd) => {
                            match cmd.as_str() {
                                "pause" => vm.pomo.pause(now),
                                "reset" | "stop" => vm.pomo.stop(),
                                _ => vm.pomo.start_focus(now),
                            }
                            badges.push(AiActionBadge { label: format!("Focus timer: {cmd}"), undo: UndoOp::None, undone: false });
                            mutated = true;
                        }
                        Remember(fact) => {
                            memories.push(fact.clone());
                            badges.push(AiActionBadge { label: format!("Remembered: {fact}"), undo: UndoOp::ForgetMemory(fact), undone: false });
                        }
                        Note(text) => {
                            let title = teardrop_core::title_of(&text);
                            let id = vm.notes.add(&text, now).id.clone();
                            with_eyes(|e| e.noted(text.chars().count()));
                            badges.push(AiActionBadge { label: format!("Saved note: {title}"), undo: UndoOp::RemoveAddedNote(id), undone: false });
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
        let done = (vm.ai.busy && !snap.busy).then_some(snap.error.is_empty());
        vm.ai = snap;
        done
    });
    if let Some(ok) = ai_done {
        with_eyes(|e| e.react(if ok { React::Sparkle } else { React::Confused }));
    }
}

/// Copy the updater's state into the view model when it changes and
/// announce a newly found release once.
pub(super) fn sync_update() {
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
                let changed = vm.track.title != s.title || vm.track.artist != s.artist || vm.track.album != s.album;
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
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            vm.quick.volume_pct = vs.pct;
            vm.quick.muted = vs.muted;
        });
        volume_changed(vs.pct, vs.muted);
    }
}

/// Quick page state: radio snapshot (worker-owned), mic mute, brightness.
/// The two direct reads are cheap but still only sampled every 10 s,
/// or every tick while the page is up.
fn poll_quick(n: u32) {
    let snap = crate::radios::snapshot();
    let page = VIEW.with(|v| v.borrow().page);
    let sec = UI.with(|u| u.borrow().qs_section);
    VIEW.with(|v| {
        let mut vm = v.borrow_mut();
        vm.quick.wifi = snap.wifi;
        vm.quick.bluetooth = snap.bluetooth;
        vm.quick.wifi_ssid = snap.ssid;
        vm.quick.wifi_nets = snap.wifi_nets;
        vm.quick.bt_devs = snap.bt_devs;
        if vm.page == Page::Quick || n.is_multiple_of(10) {
            vm.quick.mic_muted = volume::mic_muted();
            vm.quick.brightness = crate::brightness::get();
        }
    });
    // Bluetooth inquiry is expensive — run it only while the device list
    // is on screen.
    let want_scan = page == Page::Quick && sec == QsSection::Bt;
    if LAST_QS_SCAN.replace(want_scan) != want_scan {
        crate::radios::scan_bt(want_scan);
    }
}

/// From the 1 s poll or the Core Audio callback: raise the HUD and let
/// the face act out the new level. Returns true when something changed.
pub(super) fn volume_changed(pct: u8, muted: bool) -> bool {
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
pub(super) fn brightness_changed(pct: u8) -> bool {
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
    if n.is_multiple_of(3) {
        let (mic, cam) = env::mic_cam();
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            // Our own recorder must not make the eyes suspicious.
            vm.env.mic = mic && vm.voice.phase != crate::voice::VoicePhase::Recording;
            vm.env.camera = cam;
        });
    }
    if n.is_multiple_of(5) {
        let weather_on = SETTINGS.with(|s| s.borrow().weather_mood);
        let (online, quiet) = (env::online(), env::quiet());
        VIEW.with(|v| {
            let mut vm = v.borrow_mut();
            vm.env.offline = !online;
            vm.env.quiet = quiet;
            vm.env.weather = if weather_on { env::weather() } else { Weather::Unknown };
        });
        if weather_on && n.is_multiple_of(1800) {
            env::refresh_weather();
        }
    }
}

/// What kind of app is in front (the eyes act it out). Our own window,
/// menus and the desktop keep the last app.
pub(super) fn poll_focus_app(hwnd: HWND) {
    let enabled = SETTINGS.with(|s| s.borrow().focus_app_eyes);
    if !(eyes_on() && enabled) {
        VIEW.with(|v| v.borrow_mut().env.app = None);
        let why = if !eyes_on() { "off (idle look is not Eyes)" } else { "off (Focus apps is turned off in Settings)" };
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

/// Note a focus-eyes state change in `droppy.log`, for "why don't I see it".
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
pub(super) fn step_app_theme(hwnd: HWND, forward: bool) {
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

pub(super) fn slow_tick(hwnd: HWND) {
    VIEW.with(|v| {
        let mut vm = v.borrow_mut();
        vm.windows = crate::winreg::enumerate(hwnd.0 as isize);
        vm.audio = crate::mixer::sessions();
    });
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
        if n.is_multiple_of(3) {
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
    poll_quick(n);
    if (VIEW.with(|v| v.borrow().page) == Page::Usage && !USAGE_STARTED.replace(true)) || (USAGE_STARTED.get() && n.is_multiple_of(300)) {
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
    if n.is_multiple_of(30) {
        save_all();
    }
    wake(hwnd);
}
