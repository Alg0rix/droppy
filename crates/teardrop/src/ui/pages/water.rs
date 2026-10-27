use super::*;
// -------------------------------------------------------------------- water

/// Liquid body clipped to a circle: wave surface on top, circular arc below.
pub fn wave_poly(cx: f32, cy: f32, r: f32, level: f32, t: f32, amp: f32, phase: f32) -> Vec<(f32, f32)> {
    let level = level.clamp(0.0, 1.0);
    let surface = cy + r - 2.0 * r * level;
    let amp = if level <= 0.001 || level >= 0.999 { 0.0 } else { amp };
    let n = 48;
    let half = |x: f32| (r * r - x * x).max(0.0).sqrt();
    let mut pts = Vec::with_capacity(n * 2 + 2);
    for i in 0..=n {
        let x = -r + 2.0 * r * i as f32 / n as f32;
        let y = surface + amp * (x * 0.085 + t * 2.1 + phase).sin() + amp * 0.55 * (x * 0.047 - t * 1.3 + phase).sin();
        let h = half(x);
        pts.push((cx + x, y.clamp(cy - h, cy + h)));
    }
    for i in (0..=n).rev() {
        let x = -r + 2.0 * r * i as f32 / n as f32;
        pts.push((cx + x, cy + half(x)));
    }
    pts
}

pub(super) fn water(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Water);
    let vm = c.vm;
    let day = vm.day();
    let today = vm.water.today(day);
    let goal = vm.water.goal_ml.max(1);
    let level = c.ui.water_level;
    let ctr = (a.x + 66.0, a.y + 92.0);
    let r = 60.0;

    c.s.circle(ctr.0, ctr.1, r, pal().ink(0.045));
    let t = c.ui.clock_s;
    if level > 0.001 {
        c.s.poly(wave_poly(ctr.0, ctr.1, r - 1.0, level, t, 4.0, 2.0), Fill::VGrad(acc.fade(0.45), acc.mix(Color::rgb(0, 0, 0), 0.5).fade(0.55)));
        c.s.poly(wave_poly(ctr.0, ctr.1, r - 1.0, level, t, 3.4, 0.0), Fill::VGrad(acc.fade(0.95), acc.mix(Color::rgb(0, 0, 0), 0.45)));
    }
    c.s.ring(ctr.0, ctr.1, r, 2.5, 0.0, 1.0, pal().ink(0.16));
    let pct = (vm.water.progress(day) * 100.0).round() as u32;
    c.text(format!("{pct}%"), Rect::new(ctr.0 - 50.0, ctr.1 - 22.0, 100.0, 32.0), 24.0, Weight::Semibold, WHITE, Align::Center);
    c.text(
        format!("{} / {}", teardrop_core::fmt_glasses(today), teardrop_core::glasses_label(goal)),
        Rect::new(ctr.0 - 60.0, ctr.1 + 10.0, 120.0, 16.0),
        10.5,
        Weight::Medium,
        WHITE.fade(0.85),
        Align::Center,
    );

    let x = a.x + 148.0;
    let w = a.right() - x;
    let sub = if today >= goal { "Goal reached".to_string() } else { format!("{} to go", teardrop_core::glasses_label(goal - today)) };
    c.section_title(Rect::new(x, a.y + 4.0, w, 40.0), "Hydration", &sub);

    let y = a.y + 50.0;
    for (i, ml) in [125u32, 250, 500].iter().enumerate() {
        c.pill(Rect::new(x + i as f32 * 60.0, y, 54.0, 32.0), Action::WaterAdd(*ml), &teardrop_core::add_label(*ml), if *ml == 250 { Some(acc) } else { None }, 12.5);
    }
    c.icon_btn(Rect::new(x + 184.0, y, 28.0, 32.0), Action::WaterUndo, Icon::Undo, 13.0, pal().text_2);

    // Seven-day chart against the goal.
    let week = vm.water.week(day);
    let base = a.bottom() - 14.0;
    let ch = 44.0;
    let bw = 18.0;
    let gap = (w - 7.0 * bw) / 6.0;
    for (i, (d, ml)) in week.iter().enumerate() {
        let bx = x + i as f32 * (bw + gap);
        let is_today = i == week.len() - 1;
        c.s.rect(Rect::new(bx, base - ch, bw, ch), 5.0, pal().ink(0.055));
        let f = (*ml as f32 / goal as f32).clamp(0.0, 1.0);
        if f > 0.0 {
            let hh = (ch * f).max(5.0);
            c.s.rect(Rect::new(bx, base - hh, bw, hh), 5.0, if is_today { acc } else { acc.fade(0.5) });
        }
        let letter: String = d.format("%a").to_string().chars().take(1).collect();
        c.text(
            letter,
            Rect::new(bx - 3.0, base + 3.0, bw + 6.0, 13.0),
            10.0,
            if is_today { Weight::Semibold } else { Weight::Regular },
            if is_today { pal().text } else { pal().text_3 },
            Align::Center,
        );
    }
}
