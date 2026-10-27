use super::*;
// ------------------------------------------------------------ rendering

pub(super) fn render(hwnd: HWND) {
    if HIDDEN.get() {
        return;
    }
    let now = Utc::now();
    let ms = now.timestamp_millis() as u64;
    let scale = SCALE.get();

    // Pointer in window-local logical px (drives the eyes). The eyes
    // compute in un-shifted coords, so undo the pill-mode top gap.
    let (wx, wy) = WIN_POS.get();
    let gap = SETTINGS.with(|s| s.borrow().top_gap());
    let mouse = unsafe {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_ok() {
            Some(((p.x - wx) as f32 / scale, (p.y - wy) as f32 / scale - gap))
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

    // System page: tell the sampler what is on screen, pull in new samples.
    {
        let open = ISLAND.with(|s| s.borrow().open());
        let on = VIEW.with(|v| v.borrow().page) == Page::System && open > 0.05;
        let tab = UI.with(|u| u.borrow().sys_tab);
        crate::sysmon::set_active(on.then_some(tab));
        let rev = crate::sysmon::revision();
        if SYS_REV.replace(rev) != rev {
            let snap = crate::sysmon::snapshot();
            VIEW.with(|v| v.borrow_mut().sys = snap);
        }
    }

    let frame = ISLAND.with(|isl| {
        let mut st = isl.borrow_mut();
        UI.with(|u| {
            let ui = u.borrow();
            st.geom.pill_w = ui.compact_w;
            st.geom.pill_h = ui.compact_h;
            st.geom.card_w = ui.card_w;
            st.geom.card_h = ui.card_h;
            st.geom.glance_w = ui.glance_w;
            st.geom.glance_h = ui.glance_h;
        });
        let view = st.view();
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
            // Size for the largest card (System/Ai are 400 tall); a smaller
            // cap costs ~100 ms recreating the D3D surface on first open.
            let (mut max_cw, mut max_ch) = (0.0f32, 0.0f32);
            for pg in Page::all().iter().chain(std::iter::once(&Page::Settings)) {
                let (cw, ch) = UiState::card_target(*pg);
                max_cw = max_cw.max(cw);
                max_ch = max_ch.max(ch);
            }
            let cap_w = ((max_cw + 2.0 * ui::theme::PAD + 32.0) * scale).ceil() as i32;
            let cap_h = ((max_ch + ui::theme::PAD + 40.0 + teardrop_core::PILL_GAP_MAX as f32) * scale).ceil() as i32;
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
        static ST: Cell<(u32, f32, Option<Instant>)> = const { Cell::new((0, 0.0, None)) };
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
        store::log_line(
            &cache,
            &format!(
                "perf: {:.1} fps, render {:.2} ms avg (last d2d draw {:.2} ms)",
                n as f32 / secs,
                sum / n as f32,
                crate::render_d2d::d2d::PROF_DRAW.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0
            ),
        );
        ST.set((0, 0.0, None));
    } else {
        ST.set((n, sum, Some(start)));
    }
}

/// One frame. Fast mode is driven by the vsync loop in `run`, so frame
/// pacing follows the display (60/120/144/240 Hz); ambient motion that
/// doesn't need full rate (equalizer, eyes) runs off a 30 fps timer.
pub(super) fn tick(hwnd: HWND) {
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
