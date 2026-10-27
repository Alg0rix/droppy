use super::*;
// -------------------------------------------------------------------- focus

pub(super) fn focus(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Focus);
    let vm = c.vm;
    let pomo = &vm.pomo;
    let n = pomo.sessions_before_long.max(1);

    // Top strip: live dot + phase on the left, remaining on the right.
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    let cur = (pomo.completed_focus % n) + if pomo.phase == Phase::Focus { 1 } else { 0 };
    let top = match pomo.phase {
        Phase::Idle => "POMODORO".to_string(),
        Phase::Focus => format!("FOCUS \u{00b7} SESSION {} OF {}", cur.min(n), n),
        Phase::ShortBreak => "SHORT BREAK".into(),
        Phase::LongBreak => "LONG BREAK".into(),
    };
    c.text(top, Rect::new(a.x + 12.0, a.y, a.w - 100.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    let right = match pomo.phase {
        Phase::Focus if vm.focus_running() => format!("{} left", mmss(vm.focus_remaining_ms())),
        Phase::ShortBreak | Phase::LongBreak => format!("break {}", mmss(vm.focus_remaining_ms())),
        _ => String::new(),
    };
    c.text(right, Rect::new(a.right() - 100.0, a.y, 100.0, 15.0), 10.5, Weight::Medium, pal().text_3, Align::Right);

    // Middle: progress ring on the left, session info on the right.
    let mid_y = a.y + 24.0;
    let ring_cx = a.x + 62.0;
    let ring_cy = mid_y + 64.0;
    let rr = 54.0;
    c.s.ring(ring_cx, ring_cy, rr, 8.0, 0.0, 1.0, pal().ink(0.08));
    let prog = vm.focus_progress();
    if prog > 0.003 {
        c.s.ring(ring_cx, ring_cy, rr, 8.0, 0.0, prog, acc);
    }
    c.text(mmss(vm.focus_remaining_ms()), Rect::new(ring_cx - 60.0, ring_cy - 20.0, 120.0, 32.0), 26.0, Weight::Semibold, pal().text, Align::Center);
    let phase = match pomo.phase {
        Phase::Idle => "READY",
        Phase::Focus => "FOCUS",
        Phase::ShortBreak => "SHORT BREAK",
        Phase::LongBreak => "LONG BREAK",
    };
    c.text(phase, Rect::new(ring_cx - 60.0, ring_cy + 13.0, 120.0, 14.0), 9.5, Weight::Semibold, acc, Align::Center);

    let rx = a.x + 140.0;
    let rw = (a.right() - rx).max(10.0);
    let sub = match pomo.phase {
        Phase::Idle => "Ready when you are",
        Phase::Focus if vm.focus_running() => "Stay in the flow",
        Phase::Focus => "Paused",
        _ => "Take a breather",
    };
    c.text(sub, Rect::new(rx, mid_y + 10.0, rw, 22.0), 15.0, Weight::Semibold, pal().text, Align::Left);

    if pomo.phase == Phase::Idle {
        c.icon_btn(Rect::new(rx, mid_y + 40.0, 26.0, 26.0), Action::FocusMinus, Icon::Minus, 11.0, pal().text_2);
        c.text(format!("{} min", pomo.focus_min), Rect::new(rx + 28.0, mid_y + 40.0, 56.0, 26.0), 13.0, Weight::Semibold, pal().text, Align::Center);
        c.icon_btn(Rect::new(rx + 86.0, mid_y + 40.0, 26.0, 26.0), Action::FocusPlus, Icon::Plus, 11.0, pal().text_2);
    } else {
        let done = if pomo.phase == Phase::LongBreak { n } else { pomo.completed_focus % n };
        for i in 0..n {
            let dx = rx + 5.0 + i as f32 * 16.0;
            c.s.circle(dx, mid_y + 48.0, 4.5, if i < done { acc } else { pal().ink(0.14) });
        }
        c.text(format!("{} today", pomo.completed_focus), Rect::new(rx + 5.0 + n as f32 * 16.0 + 6.0, mid_y + 39.0, 120.0, 16.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
    }

    // What the eyes act out for the app in front; click (or wheel) to re-map it.
    let chip_y = mid_y + 74.0;
    if let Some(app) = vm.focus_app(c.set) {
        let user = app.source == apps::Source::User;
        let theme = match (app.theme, user.then(|| apps::override_for(&c.set.app_themes, &app.exe)).flatten()) {
            // A custom theme shows its own name.
            (Some(_), Some(v)) if AppCategory::from_id(v).is_none() => v.to_string(),
            (Some(t), _) => t.cat.label().to_string(),
            (None, _) => "Plain".to_string(),
        };
        let exe: String = if app.exe.chars().count() > 14 { app.exe.chars().take(13).chain(['\u{2026}']).collect() } else { app.exe.clone() };
        let label = format!("{theme} \u{00b7} {exe}");
        c.pill(Rect::new(rx, chip_y, rw.min(210.0), 24.0), Action::FocusAppTheme, &label, user.then_some(acc), 11.0);
    }

    // Primary action + reset, bottom-left of the info column.
    let by = chip_y + 34.0;
    let label = if vm.focus_running() {
        "Pause"
    } else if pomo.remaining_ms_when_paused.is_some() {
        "Resume"
    } else {
        "Start Focus"
    };
    let btn_w = (rw - 46.0).min(170.0);
    c.pill(Rect::new(rx, by, btn_w, 36.0), Action::FocusToggle, label, Some(acc), 13.0);
    c.icon_btn(Rect::new(rx + btn_w + 10.0, by, 36.0, 36.0), Action::FocusReset, Icon::Reset, 15.0, if pomo.phase == Phase::Idle { pal().text_3 } else { pal().text });

    c.text("S start \u{00b7} P pause \u{00b7} R reset", Rect::new(a.x, a.bottom() - 8.0, a.w, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
}
