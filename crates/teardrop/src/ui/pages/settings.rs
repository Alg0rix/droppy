use super::*;
// ----------------------------------------------------------------- settings

pub(super) fn settings(c: &mut Ctx, a: Rect) {
    let col_w = (a.w - 36.0) / 2.0;
    let lx = a.x;
    let rx = a.x + col_w + 36.0;

    // LEFT COLUMN: General Toggles
    c.text("General Settings", Rect::new(lx, a.y, col_w, 24.0), 16.0, Weight::Semibold, pal().text, Align::Left);
    let keys = [
        SettingKey::OpenOnHover,
        SettingKey::Lyrics,
        SettingKey::Sounds,
        SettingKey::HideFullscreen,
        SettingKey::StartWithWindows,
        SettingKey::ClipboardHistory,
        SettingKey::WaterReminders,
    ];
    let row_h = 26.0;
    let ty = a.y + 30.0;
    for (i, k) in keys.iter().enumerate() {
        let y = ty + i as f32 * row_h;
        c.text(k.label(), Rect::new(lx, y, col_w - 50.0, row_h - 4.0), 12.0, Weight::Regular, pal().text, Align::Left);
        c.switch(Rect::new(lx + col_w - 42.0, y + 2.0, 42.0, 20.0), *k, acc_on());
    }

    // Global hotkeys: one chip per shortcut, lit = registered. Switch off any
    // that collides with another app.
    let hy = ty + keys.len() as f32 * row_h + 8.0;
    c.text("Hotkeys", Rect::new(lx, hy, 60.0, 16.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    c.text("island \u{00b7} search \u{00b7} note \u{00b7} AI", Rect::new(lx + 62.0, hy, col_w - 62.0, 16.0), 10.0, Weight::Regular, pal().text_3.fade(0.8), Align::Left);
    let hk_w = (col_w - 18.0) / 4.0;
    for (i, k) in [SettingKey::HotkeyIsland, SettingKey::HotkeySearch, SettingKey::HotkeyNote, SettingKey::HotkeyAi].iter().enumerate() {
        let on = k.get(c.set);
        c.pill(Rect::new(lx + i as f32 * (hk_w + 6.0), hy + 20.0, hk_w, 24.0), Action::Toggle(*k), k.label(), on.then_some(acc_on()), 10.0);
    }

    // Island style: glued to the screen edge or floating as a pill.
    let iy = hy + 56.0;
    c.text("Island style", Rect::new(lx, iy, 90.0, 24.0), 12.0, Weight::Medium, pal().text_2, Align::Left);
    if c.set.island_style == IslandStyle::Pill {
        c.text("Gap", Rect::new(lx + col_w - 140.0, iy + 4.0, 26.0, 16.0), 10.5, Weight::Regular, pal().text_3, Align::Right);
        c.icon_btn(Rect::new(lx + col_w - 110.0, iy, 24.0, 24.0), Action::PillGap(-1), Icon::Minus, 11.0, pal().text_2);
        c.text(format!("{} px", c.set.pill_gap), Rect::new(lx + col_w - 84.0, iy + 2.0, 56.0, 24.0), 11.5, Weight::Semibold, pal().text, Align::Center);
        c.icon_btn(Rect::new(lx + col_w - 26.0, iy, 24.0, 24.0), Action::PillGap(1), Icon::Plus, 11.0, pal().text_2);
    }
    let ty2 = iy + 30.0;
    let tw = (col_w - 8.0) / 2.0;
    let th = 80.0;
    style_tile(c, Rect::new(lx, ty2, tw, th), IslandStyle::Anchored, c.set.island_style == IslandStyle::Anchored, c.set.pill_gap as f32);
    style_tile(c, Rect::new(lx + tw + 8.0, ty2, tw, th), IslandStyle::Pill, c.set.island_style == IslandStyle::Pill, c.set.pill_gap as f32);

    // RIGHT COLUMN: Top buttons + Appearance & Mascot
    c.pill(Rect::new(rx + col_w - 60.0, a.y, 60.0, 24.0), Action::Quit, "Quit", None, 11.0);
    update_pill(c, Rect::new(rx, a.y, col_w - 68.0, 24.0));

    // Idle look
    let sy = a.y + 32.0;
    c.text("Idle Look", Rect::new(rx, sy, 68.0, 26.0), 12.0, Weight::Medium, pal().text_2, Align::Left);
    let look = c.vm.idle_look;
    let looks = [(IdleLook::Clock, "Clock"), (IdleLook::Eyes, "Eyes"), (IdleLook::Minimal, "Dot"), (IdleLook::Hidden, "Hidden")].map(|(l, s)| (Action::Idle(l), s, look == l));
    segmented(c, Rect::new(rx + 70.0, sy, col_w - 70.0, 26.0), &looks, 11.5);

    // Island theme: a strip of body swatches, one per IslandTheme.
    let th_y = sy + 34.0;
    c.text("Theme", Rect::new(rx, th_y + 2.0, 68.0, 20.0), 12.0, Weight::Medium, pal().text_2, Align::Left);
    // Swatch band: 72px label on the left, theme name right-aligned at the end.
    let pitch = ((col_w - 72.0 - 22.0) / (IslandTheme::ALL.len() - 1) as f32).min(26.0);
    for (i, t) in IslandTheme::ALL.iter().enumerate() {
        let cx = rx + 72.0 + 11.0 + i as f32 * pitch;
        let cy = th_y + 12.0;
        let act = Action::IslandTheme(*t);
        let tp = palette_for(*t);
        let on = c.set.theme == *t;
        if on {
            c.s.circle(cx, cy, 11.0, pal().surface_press);
        } else {
            let hov = c.hov(act);
            if hov > 0.01 {
                c.s.circle(cx, cy, 11.0, pal().surface_hover.fade(hov));
            }
        }
        c.s.circle(cx, cy, 8.0, Fill::VGrad(tp.body_top, tp.body_bottom));
        c.s.ring(cx, cy, 8.0, 1.0, 0.0, 1.0, tp.rim.fade(tp.rim.a.max(0.4)));
        if on {
            c.s.ring(cx, cy, 10.0, 1.6, 0.0, 1.0, pal().text);
        }
        c.hit(Rect::new(cx - pitch / 2.0, cy - 11.0, pitch, 22.0), act);
    }

    // Eyes Style & Extras (when Eyes active)
    let ey = th_y + 34.0;
    if look == IdleLook::Eyes {
        let style = c.set.eye_style;
        let styles = EyeStyle::ALL.map(|st| (Action::EyeStyle(st), st.label(), st == style));
        segmented(c, Rect::new(rx, ey, col_w, 26.0), &styles, 11.0);

        let chip_w = (col_w - 18.0) / 4.0;
        let my = ey + 32.0;
        let mut cx = rx;
        for k in [SettingKey::MoodRing, SettingKey::WeatherMood, SettingKey::FocusApps] {
            let on = k.get(c.set);
            c.pill(Rect::new(cx, my, chip_w, 24.0), Action::Toggle(k), k.label(), on.then_some(acc_on()), 10.5);
            cx += chip_w + 6.0;
        }
        let bday = match c.set.birthday {
            Some((m, d)) => format!("{} {d}", MONTHS[(m as usize).clamp(1, 12) - 1]),
            None => "Birthday".to_string(),
        };
        c.pill(Rect::new(cx, my, chip_w, 24.0), Action::Birthday, &bday, c.set.birthday.map(|_| accent(Page::Music)), 10.5);
    }

    // Water Goal Stepper
    let gy = if look == IdleLook::Eyes { ey + 64.0 } else { ey + 8.0 };
    c.text("Daily water", Rect::new(rx, gy + 2.0, 75.0, 24.0), 12.0, Weight::Regular, pal().text, Align::Left);
    c.icon_btn(Rect::new(rx + col_w - 110.0, gy, 24.0, 24.0), Action::GoalStep(-1), Icon::Minus, 11.0, pal().text_2);
    c.text(teardrop_core::glasses_label(c.vm.water.goal_ml), Rect::new(rx + col_w - 84.0, gy + 2.0, 56.0, 24.0), 11.5, Weight::Semibold, pal().text, Align::Center);
    c.icon_btn(Rect::new(rx + col_w - 26.0, gy, 24.0, 24.0), Action::GoalStep(1), Icon::Plus, 11.0, pal().text_2);

    // Pages on the island: each icon chips the page in/out of the nav and cycle.
    let py = gy + 36.0;
    c.text("On island", Rect::new(rx, py + 2.0, 75.0, 24.0), 12.0, Weight::Regular, pal().text_2, Align::Left);
    let vis = super::layout::visible_pages(c.set);
    let pitch = ((col_w - 84.0) / Page::all().len() as f32).min(28.0);
    for (i, pg) in Page::all().iter().enumerate() {
        let on = vis.contains(pg);
        c.icon_btn(
            Rect::new(rx + 80.0 + i as f32 * pitch, py, 24.0, 24.0),
            Action::TogglePage(*pg),
            super::layout::tab_icon(*pg),
            13.0,
            if on { accent(*pg) } else { pal().text_3.fade(0.45) },
        );
    }

    // Mesh toggles: BLE radio + HUD text privacy, as two pills.
    let my = py + 36.0;
    let pw = (col_w - 6.0) / 2.0;
    for (i, k) in [SettingKey::MeshChat, SettingKey::MeshHud].iter().enumerate() {
        let lbl = k.label();
        let on = k.get(c.set);
        c.pill(Rect::new(rx + i as f32 * (pw + 6.0), my, pw, 24.0), Action::Toggle(*k), lbl, on.then_some(acc_on()), 10.5);
    }

    // Keyboard hint at bottom
    c.text("Esc closes the island", Rect::new(rx, a.bottom() - 16.0, col_w, 16.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Pill-shaped segmented control; `items` = (action, label, selected).
pub(super) fn segmented(c: &mut Ctx, seg: Rect, items: &[(Action, &str, bool)], size: f32) {
    c.s.rect(seg, seg.h / 2.0, pal().surface);
    let sw = seg.w / items.len() as f32;
    for (i, &(act, label, on)) in items.iter().enumerate() {
        let r = Rect::new(seg.x + i as f32 * sw + 3.0, seg.y + 3.0, sw - 6.0, seg.h - 6.0);
        if on {
            c.s.rect(r, r.h / 2.0, pal().surface_press.mix(WHITE.fade(0.22), 0.4));
        } else if c.hov(act) > 0.01 {
            c.s.rect(r, r.h / 2.0, pal().surface_hover.fade(c.hov(act)));
        }
        c.text(label, r, size, if on { Weight::Semibold } else { Weight::Regular }, if on { pal().text } else { pal().text_2 }, Align::Center);
        c.hit(r, act);
    }
}

fn acc_on() -> Color {
    pal().green
}

/// One "Island style" pick tile: a mini screen showing where the island sits.
fn style_tile(c: &mut Ctx, r: Rect, style: IslandStyle, on: bool, gap: f32) {
    let act = Action::IslandStyle(style);
    c.s.rect(r, 12.0, c.surface_fill(act));
    if on {
        c.s.stroke(r, 12.0, 1.6, acc_on());
    }

    // Mini screen with a bezel along its top edge.
    let scr = Rect::new(r.x + 10.0, r.y + 10.0, r.w - 20.0, r.h - 34.0);
    c.s.rect(scr, 6.0, pal().surface_press);
    c.s.line(scr.x + 4.0, scr.y + 1.0, scr.right() - 4.0, scr.y + 1.0, 2.0, pal().text_3.fade(0.6));

    let iw = scr.w * 0.44;
    let ih = 14.0;
    let icx = scr.cx();
    let fill = Fill::VGrad(pal().body_top, pal().body_bottom);
    match style {
        IslandStyle::Anchored => {
            // Flat top flush with the screen edge, rounded bottom corners.
            let g = DropletGeom { card_radius: 7.0, ..Default::default() };
            let pts = g.outline(icx, scr.y, iw, ih, false);
            c.s.poly(pts.clone(), fill);
            c.s.polyline(pts, 1.0, pal().rim);
        }
        IslandStyle::Pill => {
            let dy = if on { (gap / teardrop_core::PILL_GAP_MAX as f32 * 10.0).clamp(3.0, 10.0) } else { 6.0 };
            let pr = Rect::new(icx - iw / 2.0, scr.y + dy, iw, ih);
            c.s.rect(pr, 7.0, fill);
            c.s.stroke(pr, 7.0, 1.0, pal().rim);
            // Gap marker: a 1px line with 3px end ticks.
            let mc = if on { acc_on() } else { pal().text_3 };
            let mx = pr.right() + 6.0;
            c.s.line(mx, scr.y + 1.0, mx, pr.y, 1.0, mc);
            c.s.line(mx - 1.5, scr.y + 1.0, mx + 1.5, scr.y + 1.0, 1.0, mc);
            c.s.line(mx - 1.5, pr.y, mx + 1.5, pr.y, 1.0, mc);
        }
    }

    let label = match style {
        IslandStyle::Anchored => "Anchored",
        IslandStyle::Pill => "Pill",
    };
    c.text(
        label,
        Rect::new(r.x, scr.bottom() + 4.0, r.w, 16.0),
        11.5,
        if on { Weight::Semibold } else { Weight::Regular },
        if on { pal().text } else { pal().text_2 },
        Align::Center,
    );
    c.hit(r, act);
}
