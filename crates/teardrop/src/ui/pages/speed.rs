use super::*;
// -------------------------------------------------------------------- speed

/// Arc fill (0..1) for `v` Mbit/s on a fixed log scale (1 Gbps = full), so the
/// same speed always fills the same amount and the gauge never rescales mid-test.
pub fn gauge_frac(v: f32) -> f32 {
    ((1.0 + v.max(0.0) / 5.0).ln() / (1.0 + 1000.0_f32 / 5.0).ln()).clamp(0.0, 1.0)
}

pub(super) fn speed(c: &mut Ctx, a: Rect) {
    use std::f32::consts::{FRAC_PI_2, TAU};
    const DOWN: Color = Color::rgb(34, 211, 238);
    const UP: Color = Color::rgb(192, 132, 252);
    let sp = c.vm.speed;
    let idle = sp.phase == SpeedPhase::Idle;
    let tint = match sp.phase {
        SpeedPhase::Up => UP,
        SpeedPhase::Ping | SpeedPhase::Idle => accent(Page::Speed),
        _ => DOWN,
    };
    let shown = c.ui.speed_val;

    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, accent(Page::Speed));
    c.text("SPEEDTEST", Rect::new(a.x + 12.0, a.y, a.w - 40.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);

    // Dial: 270 degrees open at the bottom, log scale with Ookla-style ticks.
    let (cx, cy, r) = (a.x + 104.0, a.y + 112.0, 80.0);
    let (f0, span) = (0.625_f32, 0.75_f32);
    c.s.ring(cx, cy, r, 9.0, f0, f0 + span, pal().ink(0.08));
    let frac = gauge_frac(shown);
    if !idle && frac > 0.004 {
        c.s.ring(cx, cy, r, 9.0, f0, f0 + span * frac, tint);
    }
    let at = |f: f32, rad: f32| {
        let ang = (f0 + span * f) * TAU - FRAC_PI_2;
        (cx + rad * ang.cos(), cy + rad * ang.sin())
    };
    for t in [0.0_f32, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0] {
        let f = gauge_frac(t);
        let (x1, y1) = at(f, r - 9.0);
        let (x2, y2) = at(f, r - 14.0);
        c.s.line(x1, y1, x2, y2, 1.5, pal().text_3);
        let (lx, ly) = at(f, r - 26.0);
        c.text(format!("{t:.0}"), Rect::new(lx - 16.0, ly - 7.0, 32.0, 14.0), 9.5, Weight::Medium, pal().text_3, Align::Center);
    }

    if idle {
        // Round GO button in the middle of the dial.
        let go = Action::SpeedTest;
        let h = c.hov(go);
        let rad = 36.0 + h * 2.0;
        c.s.circle(cx, cy, rad + 6.0, tint.fade(0.10 + 0.10 * h));
        c.s.ring(cx, cy, rad, 3.0, 0.0, 1.0, tint);
        c.text("GO", Rect::new(cx - 36.0, cy - 14.0, 72.0, 28.0), 22.0, Weight::Bold, tint, Align::Center);
        c.hit(Rect::new(cx - rad, cy - rad, rad * 2.0, rad * 2.0), go);
    } else {
        // Needle + hub, then the live number in the open bottom of the dial.
        let (nx, ny) = at(frac, r - 22.0);
        c.s.line(cx, cy, nx, ny, 3.0, WHITE);
        c.s.circle(cx, cy, 6.5, WHITE);
        c.s.circle(cx, cy, 3.0, tint);
        let big = if shown >= 100.0 { format!("{shown:.0}") } else { format!("{shown:.1}") };
        c.text(big, Rect::new(cx - 52.0, cy + 18.0, 104.0, 40.0), 32.0, Weight::Semibold, pal().text, Align::Center);
        let cap = match sp.phase {
            SpeedPhase::Ping => "Pinging\u{2026}",
            SpeedPhase::Down => "Mbps \u{00b7} download",
            SpeedPhase::Up => "Mbps \u{00b7} upload",
            SpeedPhase::Done => "Mbps \u{00b7} download",
            _ => "Offline?",
        };
        let col = if sp.phase == SpeedPhase::Failed { pal().amber } else { pal().text_2 };
        c.text(cap, Rect::new(cx - 60.0, cy + 56.0, 120.0, 16.0), 11.0, Weight::Medium, col, Align::Center);
    }

    // Result cards.
    let x = a.x + 246.0;
    let w = a.w - 246.0;
    let reached = |p: SpeedPhase| match p {
        SpeedPhase::Ping => sp.ping_ms.is_some(),
        SpeedPhase::Down => matches!(sp.phase, SpeedPhase::Down | SpeedPhase::Up | SpeedPhase::Done),
        _ => matches!(sp.phase, SpeedPhase::Up | SpeedPhase::Done),
    };
    let live = |p: SpeedPhase| sp.phase == p;
    let mbit = |v: f32| if v >= 100.0 { format!("{v:.0}") } else { format!("{v:.1}") };
    let rows = [
        ("PING", "\u{25CF}", pal().amber, SpeedPhase::Ping, sp.ping_ms.map(|p| format!("{p:.0}")), "ms"),
        ("DOWNLOAD", "\u{2193}", DOWN, SpeedPhase::Down, Some(mbit(sp.down)), "Mbps"),
        ("UPLOAD", "\u{2191}", UP, SpeedPhase::Up, Some(mbit(sp.up)), "Mbps"),
    ];
    for (i, (label, glyph, col, phase, value, unit)) in rows.iter().enumerate() {
        let ry = a.y + 20.0 + i as f32 * 50.0;
        let card = Rect::new(x, ry, w, 44.0);
        c.s.rect(card, 12.0, pal().surface);
        if live(*phase) {
            c.s.stroke(card, 12.0, 1.5, col.fade(0.7));
        }
        let on = reached(*phase);
        c.s.circle(card.x + 20.0, card.cy(), 11.0, col.fade(if on { 0.20 } else { 0.08 }));
        c.text(*glyph, Rect::new(card.x + 9.0, card.cy() - 10.0, 22.0, 20.0), 12.0, Weight::Bold, if on { *col } else { pal().text_3 }, Align::Center);
        c.text(*label, Rect::new(card.x + 38.0, card.y + 4.0, w - 46.0, 13.0), 9.0, Weight::Semibold, pal().text_3, Align::Left);
        match (on, value) {
            (true, Some(v)) => {
                c.text(v.clone(), Rect::new(card.x + 38.0, card.y + 17.0, w - 46.0, 22.0), 16.0, Weight::Semibold, pal().text, Align::Left);
                c.text(*unit, Rect::new(card.x + 42.0 + est_w(v, 16.0) * 1.08, card.y + 22.0, 50.0, 15.0), 10.0, Weight::Regular, pal().text_2, Align::Left);
            }
            _ => c.text("\u{2014}", Rect::new(card.x + 38.0, card.y + 17.0, w - 46.0, 22.0), 16.0, Weight::Semibold, pal().text_3, Align::Left),
        }
    }

    // Start / again.
    let pr = Rect::new(x, a.bottom() - 30.0, w, 28.0);
    match sp.phase {
        SpeedPhase::Idle => c.text("Press GO to start", pr, 12.0, Weight::Medium, pal().text_3, Align::Center),
        p if p.running() => c.pill(pr, Action::Island, "Testing\u{2026}", None, 12.5),
        _ => c.pill(pr, Action::SpeedTest, "Test again", Some(tint), 12.5),
    }
}
