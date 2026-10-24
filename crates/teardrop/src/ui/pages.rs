//! One layout function per page. Each fills `a` (the card's content area).

use super::icons::{self, Icon};
use super::layout::{est_w, Ctx};
use super::scene::{Align, Color, Fill, Rect, Weight};
use super::state::{Action, SettingKey};
use super::theme::*;
use crate::viewmodel::{local_naive, mmss, one_line, SpeedPhase};
use chrono::{Datelike, NaiveDate};
use teardrop_core::{EyeStyle, IdleLook, Page, Phase, Playback};

pub fn draw(c: &mut Ctx, page: Page, a: Rect) {
    match page {
        Page::Music => music(c, a),
        Page::Focus => focus(c, a),
        Page::Water => water(c, a),
        Page::Shelf => shelf(c, a),
        Page::Tasks => tasks(c, a),
        Page::Clipboard => clipboard(c, a),
        Page::Today => today(c, a),
        Page::Speed => speed(c, a),
        Page::Settings => settings(c, a),
    }
}

const WHITE: Color = Color::rgb(255, 255, 255);
const ON_ACCENT: Color = Color::rgb(14, 14, 18);

fn fmt_ms(ms: u64) -> String {
    format!("{}:{:02}", ms / 60_000, (ms / 1000) % 60)
}

/// Map a click at `x` inside the seek-bar hit rect to 0..=1000 per-mille.
pub fn seek_permille(r: Rect, x: f32) -> u16 {
    let f = ((x - r.x) / r.w.max(1.0)).clamp(0.0, 1.0);
    (f * 1000.0).round() as u16
}

fn grouped(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

// -------------------------------------------------------------------- music

fn music(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Music);
    let vm = c.vm;
    let t = &vm.track;
    let has = !t.title.is_empty();

    let art = Rect::new(a.x, a.y, 108.0, 108.0);
    if has {
        let (c1, c2) = art_colors(&t.title);
        c.s.rect(art, 22.0, Fill::DGrad(c1, c2));
        c.s.rect(Rect::new(art.x, art.y, art.w, art.h * 0.55), 22.0, Fill::VGrad(Color::rgba(255, 255, 255, 0.22), Color::rgba(255, 255, 255, 0.0)));
        icons::draw(&mut c.s, Icon::Note, art.cx(), art.cy() - 8.0, 40.0, WHITE.fade(0.92));
        // Visualizer along the bottom of the tile.
        let n = 14;
        let (bw, gap) = (4.5, 3.0);
        let x0 = art.cx() - (n as f32 * bw + (n - 1) as f32 * gap) / 2.0;
        for (i, b) in c.ui.viz.sample(n).iter().enumerate() {
            let h = 3.0 + b * 30.0;
            c.s.rect(Rect::new(x0 + i as f32 * (bw + gap), art.bottom() - 12.0 - h, bw, h), 2.2, WHITE.fade(0.85));
        }
    } else {
        c.s.rect(art, 22.0, SURFACE);
        icons::draw(&mut c.s, Icon::Note, art.cx(), art.cy(), 40.0, TEXT_3);
    }

    let x = a.x + 126.0;
    let w = a.w - 126.0;
    let chip = if has { t.app.to_uppercase() } else { "NOTHING PLAYING".to_string() };
    c.text(chip, Rect::new(x, a.y + 1.0, w, 16.0), 10.5, Weight::Semibold, TEXT_3, Align::Left);
    let title = if has { t.title.clone() } else { "Play something".to_string() };
    c.text(title, Rect::new(x, a.y + 17.0, w, 28.0), 19.0, Weight::Semibold, if has { TEXT } else { TEXT_2 }, Align::Left);
    let sub = if !has {
        "Any app that uses Windows media controls shows up here".to_string()
    } else if t.album.is_empty() {
        t.display_artist().to_string()
    } else {
        format!("{} \u{00b7} {}", t.display_artist(), t.album)
    };
    c.text(sub, Rect::new(x, a.y + 45.0, w, 20.0), 13.0, Weight::Regular, TEXT_2, Align::Left);

    // Lyrics: current line lights up as it is sung, next line waits dim.
    let lr = Rect::new(x, a.y + 72.0, w, 36.0);
    if !c.set.lyrics_enabled {
        c.text("Lyrics are off", Rect::new(lr.x, lr.y + 9.0, lr.w, 18.0), 12.5, Weight::Regular, TEXT_3, Align::Left);
    } else if let (true, Some(i)) = (has, vm.lyric_idx) {
        if let Some(l) = vm.lyric_lines.get(i) {
            let frac = teardrop_core::line_progress(&vm.lyric_lines, i, t.position_ms);
            c.s.text_karaoke(l.text.clone(), Rect::new(lr.x, lr.y, lr.w, 18.0), 13.5, Weight::Semibold, TEXT_2, TEXT, frac);
        }
        if let Some(l) = vm.lyric_lines.get(i + 1) {
            c.text(l.text.clone(), Rect::new(lr.x, lr.y + 18.0, lr.w, 18.0), 12.5, Weight::Regular, TEXT_3, Align::Left);
        }
    } else {
        let msg = if has { "No synced lyrics for this track" } else { "Lyrics appear here while music plays" };
        c.text(msg, Rect::new(lr.x, lr.y + 9.0, lr.w, 18.0), 12.5, Weight::Regular, TEXT_3, Align::Left);
    }

    // Seek bar. Click to seek once a track with a known length is showing.
    let sy = a.y + 130.0;
    c.text(fmt_ms(t.position_ms), Rect::new(a.x, sy - 9.0, 42.0, 18.0), 11.0, Weight::Medium, TEXT_3, Align::Left);
    c.text(fmt_ms(t.duration_ms), Rect::new(a.right() - 42.0, sy - 9.0, 42.0, 18.0), 11.0, Weight::Medium, TEXT_3, Align::Right);
    let bar = Rect::new(a.x + 50.0, sy - 2.5, a.w - 100.0, 5.0);
    let seekable = has && t.duration_ms > 0;
    let sh = if seekable { c.hov(Action::SeekBar) } else { 0.0 };
    let bh = 5.0 + sh * 3.0;
    let track = Rect::new(bar.x, sy - bh * 0.5, bar.w, bh);
    c.s.rect(track, bh * 0.5, Color::rgba(255, 255, 255, 0.12 + 0.06 * sh));
    let p = t.progress();
    if p > 0.0 {
        c.s.rect(Rect::new(track.x, track.y, (track.w * p).max(bh), bh), bh * 0.5, Fill::DGrad(acc, acc.mix(WHITE, 0.35)));
    }
    if p > 0.0 || sh > 0.01 {
        c.s.circle(track.x + track.w * p, sy, 5.0 + sh * 1.5, WHITE);
    }
    if seekable {
        c.hit(Rect::new(bar.x, sy - 12.0, bar.w, 24.0), Action::SeekBar);
    }

    // Transport.
    let cy = a.y + 178.0;
    let cx = a.cx();
    c.ghost_icon(Rect::new(cx - 92.0, cy - 20.0, 40.0, 40.0), Action::MediaPrev, Icon::Prev, 20.0, TEXT);
    c.ghost_icon(Rect::new(cx + 52.0, cy - 20.0, 40.0, 40.0), Action::MediaNext, Icon::Next, 20.0, TEXT);
    let pa = Action::MediaPlayPause;
    let r = 27.0 + c.hov(pa) * 1.5;
    let fill = if c.pressed(pa) { WHITE.mix(Color::rgb(0, 0, 0), 0.2) } else { WHITE };
    c.s.circle(cx, cy, r, fill);
    let icon = if t.playback == Playback::Playing { Icon::Pause } else { Icon::Play };
    let nudge = if icon == Icon::Play { 1.5 } else { 0.0 };
    icons::draw(&mut c.s, icon, cx + nudge, cy, 24.0, ON_ACCENT);
    c.hit(Rect::new(cx - 28.0, cy - 28.0, 56.0, 56.0), pa);
}

// -------------------------------------------------------------------- focus

fn focus(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Focus);
    let vm = c.vm;
    let pomo = &vm.pomo;
    let ring = (a.x + 96.0, a.y + 116.0);
    let rr = 80.0;
    c.s.ring(ring.0, ring.1, rr, 10.0, 0.0, 1.0, Color::rgba(255, 255, 255, 0.08));
    let prog = vm.focus_progress();
    if prog > 0.003 {
        c.s.ring(ring.0, ring.1, rr, 10.0, 0.0, prog, acc);
    }
    c.text(mmss(vm.focus_remaining_ms()), Rect::new(ring.0 - 70.0, ring.1 - 30.0, 140.0, 46.0), 38.0, Weight::Semibold, TEXT, Align::Center);
    let phase = match pomo.phase {
        Phase::Idle => "READY",
        Phase::Focus => "FOCUS",
        Phase::ShortBreak => "SHORT BREAK",
        Phase::LongBreak => "LONG BREAK",
    };
    c.text(phase, Rect::new(ring.0 - 70.0, ring.1 + 18.0, 140.0, 18.0), 11.0, Weight::Semibold, acc, Align::Center);

    let x = a.x + 220.0;
    let w = a.w - 220.0;
    c.section_title(Rect::new(x, a.y + 4.0, w, 44.0), "Pomodoro", match pomo.phase {
        Phase::Idle => "Ready when you are",
        Phase::Focus if vm.focus_running() => "Stay with it",
        Phase::Focus => "Paused",
        _ => "Take a breather",
    });

    // Length stepper (only while idle) or session counter.
    let y = a.y + 62.0;
    if pomo.phase == Phase::Idle {
        c.text("Length", Rect::new(x, y, 56.0, 32.0), 12.0, Weight::Regular, TEXT_3, Align::Left);
        c.icon_btn(Rect::new(x + 56.0, y, 32.0, 32.0), Action::FocusMinus, Icon::Minus, 14.0, TEXT_2);
        c.text(format!("{} min", pomo.focus_min), Rect::new(x + 90.0, y, 56.0, 32.0), 14.0, Weight::Semibold, TEXT, Align::Center);
        c.icon_btn(Rect::new(x + 148.0, y, 32.0, 32.0), Action::FocusPlus, Icon::Plus, 14.0, TEXT_2);
    } else {
        let n = pomo.sessions_before_long.max(1);
        let cur = (pomo.completed_focus % n) + if pomo.phase == Phase::Focus { 1 } else { 0 };
        c.text(format!("Session {} of {}", cur.min(n), n), Rect::new(x, y, w, 32.0), 13.0, Weight::Medium, TEXT_2, Align::Left);
    }

    // Cycle dots.
    let n = pomo.sessions_before_long.max(1);
    let done = if pomo.phase == Phase::LongBreak { n } else { pomo.completed_focus % n };
    for i in 0..n {
        let on = i < done;
        let dx = x + 6.0 + i as f32 * 20.0;
        if on {
            c.s.circle(dx, a.y + 120.0, 5.5, acc);
        } else {
            c.s.circle(dx, a.y + 120.0, 5.5, Color::rgba(255, 255, 255, 0.14));
        }
    }
    c.text(format!("{} done", pomo.completed_focus), Rect::new(x + 6.0 + n as f32 * 20.0, a.y + 110.0, 80.0, 20.0), 12.0, Weight::Regular, TEXT_3, Align::Left);

    // Controls.
    let by = a.y + 146.0;
    let label = if vm.focus_running() {
        "Pause"
    } else if pomo.remaining_ms_when_paused.is_some() {
        "Resume"
    } else {
        "Start"
    };
    c.pill(Rect::new(x, by, 130.0, 42.0), Action::FocusToggle, label, Some(acc), 14.0);
    c.icon_btn(Rect::new(x + 142.0, by, 42.0, 42.0), Action::FocusReset, Icon::Reset, 16.0, if pomo.phase == Phase::Idle { TEXT_3 } else { TEXT });
    c.text("S start \u{00b7} P pause \u{00b7} R reset", Rect::new(x, a.y + 204.0, w, 18.0), 11.0, Weight::Regular, TEXT_3, Align::Left);
}

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

fn water(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Water);
    let vm = c.vm;
    let day = vm.day();
    let today = vm.water.today(day);
    let goal = vm.water.goal_ml.max(1);
    let level = c.ui.water_level;
    let ctr = (a.x + 96.0, a.y + 116.0);
    let r = 82.0;

    c.s.circle(ctr.0, ctr.1, r, Color::rgba(255, 255, 255, 0.045));
    let t = c.ui.clock_s;
    if level > 0.001 {
        c.s.poly(wave_poly(ctr.0, ctr.1, r - 1.0, level, t, 4.0, 2.0), Fill::VGrad(acc.fade(0.45), acc.mix(Color::rgb(0, 0, 0), 0.5).fade(0.55)));
        c.s.poly(wave_poly(ctr.0, ctr.1, r - 1.0, level, t, 3.4, 0.0), Fill::VGrad(acc.fade(0.95), acc.mix(Color::rgb(0, 0, 0), 0.45)));
    }
    c.s.ring(ctr.0, ctr.1, r, 2.5, 0.0, 1.0, Color::rgba(255, 255, 255, 0.16));
    let pct = (vm.water.progress(day) * 100.0).round() as u32;
    c.text(format!("{pct}%"), Rect::new(ctr.0 - 60.0, ctr.1 - 30.0, 120.0, 42.0), 34.0, Weight::Semibold, WHITE, Align::Center);
    c.text(format!("{} / {} ml", grouped(today), grouped(goal)), Rect::new(ctr.0 - 70.0, ctr.1 + 12.0, 140.0, 18.0), 12.0, Weight::Medium, WHITE.fade(0.85), Align::Center);

    let x = a.x + 220.0;
    let w = a.w - 220.0;
    let sub = if today >= goal { "Goal reached \u{2014} nice work".to_string() } else { format!("{} ml to go", grouped(goal - today)) };
    c.section_title(Rect::new(x, a.y + 4.0, w, 44.0), "Hydration", &sub);

    let y = a.y + 54.0;
    for (i, ml) in [150u32, 250, 500].iter().enumerate() {
        c.pill(Rect::new(x + i as f32 * 62.0, y, 56.0, 34.0), Action::WaterAdd(*ml), &format!("+{ml}"), if *ml == 250 { Some(acc) } else { None }, 13.0);
    }
    c.icon_btn(Rect::new(x + 190.0, y, 30.0, 34.0), Action::WaterUndo, Icon::Undo, 14.0, TEXT_2);

    // Seven-day chart against the goal.
    let week = vm.water.week(day);
    let base = a.y + 186.0;
    let ch = 68.0;
    let bw = 22.0;
    let gap = (w - 7.0 * bw) / 6.0;
    for (i, (d, ml)) in week.iter().enumerate() {
        let bx = x + i as f32 * (bw + gap);
        let is_today = i == week.len() - 1;
        c.s.rect(Rect::new(bx, base - ch, bw, ch), 6.0, Color::rgba(255, 255, 255, 0.055));
        let f = (*ml as f32 / goal as f32).clamp(0.0, 1.0);
        if f > 0.0 {
            let hh = (ch * f).max(6.0);
            c.s.rect(Rect::new(bx, base - hh, bw, hh), 6.0, if is_today { acc } else { acc.fade(0.5) });
        }
        let letter: String = d.format("%a").to_string().chars().take(1).collect();
        c.text(letter, Rect::new(bx - 4.0, base + 4.0, bw + 8.0, 16.0), 11.0, if is_today { Weight::Semibold } else { Weight::Regular }, if is_today { TEXT } else { TEXT_3 }, Align::Center);
    }
    c.text("W adds 250 ml from anywhere", Rect::new(x, a.y + 210.0, w, 18.0), 11.0, Weight::Regular, TEXT_3, Align::Left);
}

// -------------------------------------------------------------------- shelf

fn ext_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() && e.len() <= 4 => e.to_uppercase(),
        _ => String::new(),
    }
}

fn parent_of(path: &str) -> String {
    let p = path.replace('/', "\\");
    let mut parts: Vec<&str> = p.split('\\').collect();
    parts.pop();
    parts.last().copied().unwrap_or("").to_string()
}

const SHELF_COLS: usize = 2;
const SHELF_ROWS: usize = 3;

pub fn shelf_max_scroll(n: usize) -> usize {
    let rows = (n + SHELF_COLS - 1) / SHELF_COLS;
    rows.saturating_sub(SHELF_ROWS)
}

fn shelf(c: &mut Ctx, a: Rect) {
    shelf_list(c, a);
    drop_overlay(c, a);
}

/// While a file is carried over the island the shelf is swallowed by inward
/// ripples and a bobbing tray; right after the drop a ring bursts outward.
fn drop_overlay(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Shelf);
    let (amt, u) = (c.ui.drop_amt, c.ui.absorb_t);
    let (cx, cy) = (a.cx(), a.cy());
    if amt > 0.01 {
        let t = c.ui.clock_s;
        c.s.rect(a.inset(-6.0), 20.0, Color::rgba(14, 14, 18, 0.88 * amt));
        c.s.rect(a.inset(-6.0), 20.0, acc.fade(0.10 * amt));
        for k in 0..3 {
            let ph = (t * 0.8 + k as f32 / 3.0).fract();
            let r = Rect::new(a.x, a.y, a.w, a.h).inset(ph * 52.0);
            c.s.stroke(r, (18.0 - ph * 10.0).max(6.0), 2.0, acc.fade((1.0 - ph) * 0.55 * amt));
        }
        let bob = (t * 4.2).sin() * 5.0;
        let pulse = 1.0 + 0.06 * (t * 4.2).sin();
        c.s.circle(cx, cy - 14.0, 40.0 * pulse * amt.max(0.2), acc.fade(0.16 * amt));
        icons::draw(&mut c.s, Icon::Tray, cx, cy - 14.0 + bob, 38.0, acc.fade(amt));
        let n = c.ui.drag_files.max(1);
        let msg = if n == 1 { "Release to add to the shelf".to_string() } else { format!("Release to add {n} files") };
        c.text(msg, Rect::new(a.x, cy + 34.0, a.w, 20.0), 14.0, Weight::Semibold, TEXT.fade(amt), Align::Center);
    }
    if u > 0.0 {
        let p = 1.0 - u;
        let ease = 1.0 - (1.0 - p).powi(3);
        c.s.ring(cx, cy, 24.0 + ease * 150.0, 5.0 * u + 1.0, 0.0, 1.0, acc.fade(u * 0.8));
        c.s.circle(cx, cy, 30.0, acc.fade(u));
        icons::draw(&mut c.s, Icon::Check, cx, cy, 30.0 * (0.7 + 0.3 * ease), ON_ACCENT.fade(u));
    }
}

fn shelf_list(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Shelf);
    let vm = c.vm;
    let n = vm.shelf.items.len();
    let sub = if n == 0 { String::new() } else { format!("{n} file{}", if n == 1 { "" } else { "s" }) };
    c.text("Shelf", Rect::new(a.x, a.y, 120.0, 26.0), 17.0, Weight::Semibold, TEXT, Align::Left);
    c.text(sub, Rect::new(a.right() - 120.0, a.y, 120.0, 26.0), 12.0, Weight::Regular, TEXT_3, Align::Right);

    let body = Rect::new(a.x, a.y + 36.0, a.w, a.h - 36.0);
    if n == 0 {
        c.s.dashed(body.inset(2.0), 16.0, 1.5, Color::rgba(255, 255, 255, 0.16));
        c.empty_state(body, Icon::Tray, acc, "Drop files on the island", "They stay here until you remove them");
        return;
    }
    let scroll = c.ui.scroll_of(Page::Shelf).min(shelf_max_scroll(n));
    let tw = (a.w - 12.0) / 2.0;
    let th = 52.0;
    let items: Vec<_> = vm.shelf.items.iter().rev().collect();
    for slot in 0..(SHELF_COLS * SHELF_ROWS) {
        let idx = scroll * SHELF_COLS + slot;
        let Some(item) = items.get(idx) else { break };
        let (col, row) = (slot % SHELF_COLS, slot / SHELF_COLS);
        let r = Rect::new(a.x + col as f32 * (tw + 12.0), body.y + row as f32 * (th + 8.0), tw, th);
        let open = Action::ShelfOpen(idx as u16);
        let reveal = Action::ShelfReveal(idx as u16);
        let remove = Action::ShelfRemove(idx as u16);
        let hv = c.hov(open).max(c.hov(reveal)).max(c.hov(remove));
        let f = if c.pressed(open) { SURFACE_PRESS } else { SURFACE.mix(SURFACE_HOVER, hv) };
        c.s.rect(r, 14.0, f);
        c.hit(r, open);

        let ext = ext_of(&item.name);
        let badge = Rect::new(r.x + 10.0, r.y + 9.0, 34.0, 34.0);
        let hue = {
            let mut h: u32 = 7;
            for b in ext.bytes() {
                h = h.wrapping_mul(31).wrapping_add(b as u32);
            }
            (h % 360) as f32
        };
        c.s.rect(badge, 10.0, Fill::VGrad(hsl(hue, 0.62, 0.58), hsl(hue, 0.62, 0.42)));
        if ext.is_empty() {
            icons::draw(&mut c.s, Icon::File, badge.cx(), badge.cy(), 17.0, WHITE);
        } else {
            c.text(ext, badge, 9.5, Weight::Bold, WHITE, Align::Center);
        }
        let right_pad = if hv > 0.05 { 56.0 } else { 12.0 };
        let tx = badge.right() + 10.0;
        let tr_w = r.right() - tx - right_pad;
        c.text(item.name.clone(), Rect::new(tx, r.y + 8.0, tr_w, 20.0), 13.0, Weight::Medium, TEXT, Align::Left);
        c.text(parent_of(&item.path), Rect::new(tx, r.y + 27.0, tr_w, 18.0), 11.0, Weight::Regular, TEXT_3, Align::Left);
        if hv > 0.05 {
            c.s.set_transform(0.0, 0.0, hv.min(1.0) * 1.0);
            c.ghost_icon(Rect::new(r.right() - 54.0, r.cy() - 12.0, 24.0, 24.0), reveal, Icon::Folder, 13.0, TEXT_2);
            c.ghost_icon(Rect::new(r.right() - 29.0, r.cy() - 12.0, 24.0, 24.0), remove, Icon::Close, 12.0, TEXT_2);
            c.s.set_transform(0.0, 0.0, 1.0);
        }
    }
    if shelf_max_scroll(n) > 0 {
        let tot = shelf_max_scroll(n) as f32 + SHELF_ROWS as f32;
        let track_h = SHELF_ROWS as f32 * (th + 8.0) - 8.0;
        let th_h = track_h * SHELF_ROWS as f32 / tot;
        let ty = body.y + (track_h - th_h) * scroll as f32 / shelf_max_scroll(n) as f32;
        c.s.rect(Rect::new(a.right() + 8.0, ty, 3.0, th_h), 1.5, Color::rgba(255, 255, 255, 0.25));
    }
}

// -------------------------------------------------------------------- tasks

pub const TASK_ROWS: usize = 5;

pub fn tasks_max_scroll(n: usize) -> usize {
    n.saturating_sub(TASK_ROWS)
}

fn tasks(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Tasks);
    let vm = c.vm;

    // Add field.
    let field = Rect::new(a.x, a.y, a.w, 40.0);
    let typing = c.ui.input.is_some();
    c.s.rect(field, 14.0, if typing { SURFACE_HOVER } else { c.surface_fill(Action::TaskInput) });
    if typing {
        c.s.stroke(field, 14.0, 1.5, acc.fade(0.85));
    }
    icons::draw(&mut c.s, Icon::Plus, field.x + 22.0, field.cy(), 15.0, if typing { acc } else { TEXT_3 });
    let tr = Rect::new(field.x + 42.0, field.y, field.w - 54.0, field.h);
    match &c.ui.input {
        Some(s) if !s.is_empty() => {
            c.text(s.clone(), tr, 14.0, Weight::Regular, TEXT, Align::Left);
            if (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
                let cx = (tr.x + est_w(s, 14.0)).min(tr.right() - 4.0);
                c.s.rect(Rect::new(cx + 1.0, field.cy() - 9.0, 1.6, 18.0), 0.8, acc);
            }
        }
        Some(_) => {
            c.text("What needs doing?", tr, 14.0, Weight::Regular, TEXT_3, Align::Left);
            if (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
                c.s.rect(Rect::new(tr.x, field.cy() - 9.0, 1.6, 18.0), 0.8, acc);
            }
        }
        None => c.text("Add a task", tr, 14.0, Weight::Regular, TEXT_3, Align::Left),
    }
    c.hit_text(field, Action::TaskInput);

    let n = vm.tasks.tasks.len();
    let list_y = a.y + 50.0;
    if n == 0 {
        c.empty_state(Rect::new(a.x, list_y, a.w, a.h - 50.0), Icon::Check, acc, "All clear", "Add something above, or press A");
        return;
    }
    let scroll = c.ui.scroll_of(Page::Tasks).min(tasks_max_scroll(n));
    let row_h = 32.0;
    for slot in 0..TASK_ROWS {
        let idx = scroll + slot;
        let Some(t) = vm.tasks.tasks.get(idx) else { break };
        let r = Rect::new(a.x, list_y + slot as f32 * row_h, a.w, row_h - 2.0);
        let tog = Action::TaskToggle(idx as u16);
        let del = Action::TaskRemove(idx as u16);
        let hv = c.hov(tog).max(c.hov(del));
        if hv > 0.01 {
            c.s.rect(r, 10.0, SURFACE.fade(hv));
        }
        let cb = (r.x + 18.0, r.cy());
        if t.done {
            c.s.circle(cb.0, cb.1, 9.5, acc);
            icons::draw(&mut c.s, Icon::Check, cb.0, cb.1, 11.0, ON_ACCENT);
        } else {
            c.s.ring(cb.0, cb.1, 8.5, 1.6, 0.0, 1.0, TEXT_3.mix(acc, c.hov(tog)));
        }
        let tx = r.x + 38.0;
        let right = if hv > 0.05 { 34.0 } else { 10.0 };
        let tr = Rect::new(tx, r.y, r.w - 38.0 - right, r.h);
        let col = if t.done { TEXT_3 } else { TEXT };
        c.text(t.title.clone(), tr, 14.0, Weight::Regular, col, Align::Left);
        if t.done {
            let wd = est_w(&t.title, 14.0).min(tr.w);
            c.s.line(tx, r.cy() + 0.5, tx + wd, r.cy() + 0.5, 1.3, TEXT_3);
        }
        c.hit(Rect::new(r.x, r.y, r.w - 30.0, r.h), tog);
        if hv > 0.05 {
            c.ghost_icon(Rect::new(r.right() - 30.0, r.cy() - 12.0, 24.0, 24.0), del, Icon::Close, 12.0, TEXT_2);
        }
    }
    let open = vm.tasks.open_count();
    let done = n - open;
    c.text(format!("{open} open"), Rect::new(a.x + 2.0, a.bottom() - 20.0, 100.0, 18.0), 11.5, Weight::Medium, TEXT_3, Align::Left);
    if done > 0 {
        c.pill(Rect::new(a.right() - 104.0, a.bottom() - 24.0, 104.0, 24.0), Action::TaskClearDone, &format!("Clear {done} done"), None, 11.0);
    }
}

// ---------------------------------------------------------------- clipboard

pub const CLIP_ROWS: usize = 5;

pub fn clip_max_scroll(n: usize) -> usize {
    n.saturating_sub(CLIP_ROWS)
}

fn clipboard(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Clipboard);
    let vm = c.vm;
    let items = vm.clipboard.items();
    c.text("Clipboard", Rect::new(a.x, a.y, 140.0, 26.0), 17.0, Weight::Semibold, TEXT, Align::Left);
    if !items.is_empty() {
        c.pill(Rect::new(a.right() - 64.0, a.y + 1.0, 64.0, 24.0), Action::ClipClear, "Clear", None, 11.5);
    }
    let body = Rect::new(a.x, a.y + 36.0, a.w, a.h - 36.0);
    if !c.set.clipboard_history {
        c.empty_state(body, Icon::Clipboard, acc, "Clipboard history is off", "Turn it on in Settings");
        return;
    }
    if items.is_empty() {
        c.empty_state(body, Icon::Clipboard, acc, "Nothing copied yet", "Kept in memory only \u{00b7} private items are skipped");
        return;
    }
    let scroll = c.ui.scroll_of(Page::Clipboard).min(clip_max_scroll(items.len()));
    for slot in 0..CLIP_ROWS {
        let idx = scroll + slot;
        let Some(s) = items.get(idx) else { break };
        let r = Rect::new(a.x, body.y + slot as f32 * 40.0, a.w, 34.0);
        let act = Action::ClipCopy(idx as u16);
        let hv = c.hov(act);
        c.s.rect(r, 11.0, if c.pressed(act) { SURFACE_PRESS } else { SURFACE.mix(SURFACE_HOVER, hv) });
        if idx == 0 {
            c.s.circle(r.x + 14.0, r.cy(), 3.0, acc);
        }
        let tr = Rect::new(r.x + 28.0, r.y, r.w - 28.0 - 44.0, r.h);
        c.text(one_line(s), tr, 13.0, Weight::Regular, TEXT.fade(0.92), Align::Left);
        if hv > 0.02 {
            c.text("Copy", Rect::new(r.right() - 48.0, r.y, 40.0, r.h), 11.5, Weight::Semibold, acc.fade(hv), Align::Right);
        }
        c.hit(r, act);
    }
}

// -------------------------------------------------------------------- today

pub fn days_in_month(y: i32, m: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) }.unwrap();
    (next - first).num_days() as u32
}

fn meter(c: &mut Ctx, r: Rect, label: &str, value: &str, frac: f32, col: Color) {
    c.text(label, Rect::new(r.x, r.y, 80.0, 16.0), 12.0, Weight::Regular, TEXT_2, Align::Left);
    c.text(value, Rect::new(r.right() - 80.0, r.y, 80.0, 16.0), 12.0, Weight::Semibold, TEXT, Align::Right);
    let bar = Rect::new(r.x, r.y + 21.0, r.w, 6.0);
    c.s.rect(bar, 3.0, Color::rgba(255, 255, 255, 0.10));
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        c.s.rect(Rect::new(bar.x, bar.y, (bar.w * f).max(6.0), 6.0), 3.0, col);
    }
}

fn today(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Today);
    let vm = c.vm;
    let now = local_naive(vm.now);
    let date = now.date();

    // Month grid (Monday first), shifted by the browsing offset.
    let off_m = c.ui.cal_offset;
    let idx = date.year() * 12 + date.month0() as i32 + off_m;
    let (shown_y, shown_m) = (idx.div_euclid(12), idx.rem_euclid(12) as u32 + 1);
    let cal = Rect::new(a.x, a.y, 214.0, a.h);
    let title = NaiveDate::from_ymd_opt(shown_y, shown_m, 1).unwrap().format("%B %Y").to_string();
    let title_r = Rect::new(cal.x, cal.y, cal.w - 56.0, 24.0);
    c.text(title, title_r, 14.0, Weight::Semibold, if off_m == 0 { TEXT } else { acc }, Align::Left);
    if off_m != 0 {
        c.hit(title_r, Action::CalToday);
    }
    c.icon_btn(Rect::new(cal.right() - 52.0, cal.y + 1.0, 24.0, 22.0), Action::CalStep(-1), Icon::Prev, 10.0, TEXT_2);
    c.icon_btn(Rect::new(cal.right() - 24.0, cal.y + 1.0, 24.0, 22.0), Action::CalStep(1), Icon::Next, 10.0, TEXT_2);
    let cell_w = cal.w / 7.0;
    for (i, d) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
        c.text(*d, Rect::new(cal.x + i as f32 * cell_w, cal.y + 30.0, cell_w, 16.0), 10.5, Weight::Semibold, TEXT_3, Align::Center);
    }
    let first = NaiveDate::from_ymd_opt(shown_y, shown_m, 1).unwrap();
    let off = first.weekday().num_days_from_monday() as usize;
    for d in 1..=days_in_month(shown_y, shown_m) {
        let slot = off + d as usize - 1;
        let (col, row) = (slot % 7, slot / 7);
        let r = Rect::new(cal.x + col as f32 * cell_w, cal.y + 50.0 + row as f32 * 25.0, cell_w, 24.0);
        let is_today = off_m == 0 && d == date.day();
        if is_today {
            c.s.circle(r.cx(), r.cy(), 12.0, acc);
        }
        let wk = col >= 5;
        c.text(d.to_string(), r, 12.0, if is_today { Weight::Bold } else { Weight::Regular }, if is_today { ON_ACCENT } else if wk { TEXT_3 } else { TEXT_2 }, Align::Center);
    }

    // Day + system.
    let x = a.x + 246.0;
    let w = a.w - 246.0;
    c.text(date.format("%A").to_string(), Rect::new(x, a.y, w, 30.0), 23.0, Weight::Semibold, TEXT, Align::Left);
    c.text(date.format("%-d %B").to_string(), Rect::new(x, a.y + 30.0, w, 20.0), 14.0, Weight::Regular, acc, Align::Left);
    let sys = &vm.system;
    let y0 = a.y + 74.0;
    let hot = |p: f32| if p > 85.0 { RED } else if p > 65.0 { AMBER } else { acc };
    meter(c, Rect::new(x, y0, w, 28.0), "CPU", &format!("{:.0}%", sys.cpu_pct), sys.cpu_pct / 100.0, hot(sys.cpu_pct));
    meter(c, Rect::new(x, y0 + 40.0, w, 28.0), "Memory", &format!("{:.1} / {:.0} GB", sys.mem_used_gb, sys.mem_total_gb), sys.mem_pct() / 100.0, hot(sys.mem_pct()));
    meter(c, Rect::new(x, y0 + 80.0, w, 28.0), "Disk", &format!("{:.0}%", sys.disk_pct()), sys.disk_pct() / 100.0, hot(sys.disk_pct()));
    match sys.battery_pct {
        Some(b) => {
            let col = if sys.charging { GREEN } else if b <= 20 { RED } else { acc };
            let label = if sys.charging { "Battery \u{00b7} charging" } else { "Battery" };
            meter(c, Rect::new(x, y0 + 120.0, w, 28.0), label, &format!("{b}%"), b as f32 / 100.0, col);
        }
        None => {
            c.text("Power", Rect::new(x, y0 + 120.0, 80.0, 16.0), 12.0, Weight::Regular, TEXT_2, Align::Left);
            c.text("Plugged in", Rect::new(x + w - 100.0, y0 + 120.0, 100.0, 16.0), 12.0, Weight::Semibold, TEXT, Align::Right);
        }
    }
}

// -------------------------------------------------------------------- speed

/// Arc fill (0..1) for `v` Mbit/s on a fixed log scale (1 Gbps = full), so the
/// same speed always fills the same amount and the gauge never rescales mid-test.
pub fn gauge_frac(v: f32) -> f32 {
    ((1.0 + v.max(0.0) / 5.0).ln() / (1.0 + 1000.0_f32 / 5.0).ln()).clamp(0.0, 1.0)
}

fn speed(c: &mut Ctx, a: Rect) {
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

    // Dial: 270 degrees open at the bottom, log scale with Ookla-style ticks.
    let (cx, cy, r) = (a.x + 112.0, a.y + 104.0, 84.0);
    let (f0, span) = (0.625_f32, 0.75_f32);
    c.s.ring(cx, cy, r, 9.0, f0, f0 + span, Color::rgba(255, 255, 255, 0.08));
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
        c.s.line(x1, y1, x2, y2, 1.5, TEXT_3);
        let (lx, ly) = at(f, r - 26.0);
        c.text(format!("{t:.0}"), Rect::new(lx - 16.0, ly - 7.0, 32.0, 14.0), 9.5, Weight::Medium, TEXT_3, Align::Center);
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
        c.text(big, Rect::new(cx - 52.0, cy + 18.0, 104.0, 40.0), 32.0, Weight::Semibold, TEXT, Align::Center);
        let cap = match sp.phase {
            SpeedPhase::Ping => "Pinging\u{2026}",
            SpeedPhase::Down => "Mbps \u{00b7} download",
            SpeedPhase::Up => "Mbps \u{00b7} upload",
            SpeedPhase::Done => "Mbps \u{00b7} download",
            _ => "Offline?",
        };
        let col = if sp.phase == SpeedPhase::Failed { AMBER } else { TEXT_2 };
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
        ("PING", "\u{25CF}", AMBER, SpeedPhase::Ping, sp.ping_ms.map(|p| format!("{p:.0}")), "ms"),
        ("DOWNLOAD", "\u{2193}", DOWN, SpeedPhase::Down, Some(mbit(sp.down)), "Mbps"),
        ("UPLOAD", "\u{2191}", UP, SpeedPhase::Up, Some(mbit(sp.up)), "Mbps"),
    ];
    for (i, (label, glyph, col, phase, value, unit)) in rows.iter().enumerate() {
        let ry = a.y + 2.0 + i as f32 * 52.0;
        let card = Rect::new(x, ry, w, 46.0);
        c.s.rect(card, 12.0, SURFACE);
        if live(*phase) {
            c.s.stroke(card, 12.0, 1.5, col.fade(0.7));
        }
        let on = reached(*phase);
        c.s.circle(card.x + 22.0, card.cy(), 12.0, col.fade(if on { 0.20 } else { 0.08 }));
        c.text(*glyph, Rect::new(card.x + 10.0, card.cy() - 10.0, 24.0, 20.0), 13.0, Weight::Bold, if on { *col } else { TEXT_3 }, Align::Center);
        c.text(*label, Rect::new(card.x + 42.0, card.y + 5.0, w - 50.0, 14.0), 9.5, Weight::Semibold, TEXT_3, Align::Left);
        match (on, value) {
            (true, Some(v)) => {
                c.text(v.clone(), Rect::new(card.x + 42.0, card.y + 19.0, w - 50.0, 24.0), 18.0, Weight::Semibold, TEXT, Align::Left);
                c.text(*unit, Rect::new(card.x + 46.0 + est_w(v, 18.0) * 1.08, card.y + 25.0, 50.0, 16.0), 11.0, Weight::Regular, TEXT_2, Align::Left);
            }
            _ => c.text("\u{2014}", Rect::new(card.x + 42.0, card.y + 19.0, w - 50.0, 24.0), 18.0, Weight::Semibold, TEXT_3, Align::Left),
        }
    }

    // Start / again.
    let pr = Rect::new(x, a.bottom() - 34.0, w, 32.0);
    match sp.phase {
        SpeedPhase::Idle => c.text("Press GO to start", pr, 12.0, Weight::Medium, TEXT_3, Align::Center),
        p if p.running() => c.pill(pr, Action::Island, "Testing\u{2026}", None, 12.5),
        _ => c.pill(pr, Action::SpeedTest, "Test again", Some(tint), 12.5),
    }
}

// ----------------------------------------------------------------- settings

/// "Check for updates" / "Update to vX" button for the GitHub release check.
fn update_pill(c: &mut Ctx, r: Rect) {
    use crate::update::UpdatePhase::*;
    let u = &c.vm.update;
    let acc = accent(Page::Speed);
    match u.phase {
        Idle | Failed => {
            let label = if u.phase == Failed { "Update failed \u{2014} retry" } else { "Check for updates" };
            let act = if u.phase == Failed && !u.version.is_empty() { Action::UpdateInstall } else { Action::UpdateCheck };
            c.pill(r, act, label, None, 11.0);
        }
        UpToDate => c.pill(r, Action::UpdateCheck, &format!("Up to date \u{00b7} v{}", crate::update::CURRENT), None, 11.0),
        Checking => c.pill(r, Action::Island, "Checking\u{2026}", None, 11.0),
        Available => c.pill(r, Action::UpdateInstall, &format!("Update to v{}", u.version), Some(acc), 11.5),
        Downloading => c.pill(r, Action::Island, "Downloading\u{2026}", Some(acc.fade(0.45)), 11.0),
    }
}

fn settings(c: &mut Ctx, a: Rect) {
    c.text("Settings", Rect::new(a.x, a.y, 160.0, 26.0), 17.0, Weight::Semibold, TEXT, Align::Left);
    c.pill(Rect::new(a.right() - 70.0, a.y + 1.0, 70.0, 24.0), Action::Quit, "Quit", None, 11.5);
    update_pill(c, Rect::new(a.right() - 70.0 - 8.0 - 150.0, a.y + 1.0, 150.0, 24.0));

    let col_w = (a.w - 28.0) / 2.0;
    let left = [SettingKey::OpenOnHover, SettingKey::Lyrics, SettingKey::WaterReminders, SettingKey::ClipboardHistory];
    let right = [SettingKey::Sounds, SettingKey::HideFullscreen, SettingKey::StartWithWindows];
    let ty = a.y + 34.0;
    let row = 30.0;
    for (ci, keys) in [&left[..], &right[..]].iter().enumerate() {
        let x = a.x + ci as f32 * (col_w + 28.0);
        for (i, k) in keys.iter().enumerate() {
            let y = ty + i as f32 * row;
            c.text(k.label(), Rect::new(x, y, col_w - 52.0, row - 4.0), 13.0, Weight::Regular, TEXT, Align::Left);
            c.switch(Rect::new(x + col_w - 42.0, y + 2.0, 42.0, 22.0), *k, acc_on());
        }
    }
    // Daily goal stepper sits as the 4th row of the right column.
    let gx = a.x + col_w + 28.0;
    let gy = ty + 3.0 * row;
    c.text("Water goal", Rect::new(gx, gy, 90.0, row - 4.0), 13.0, Weight::Regular, TEXT, Align::Left);
    c.icon_btn(Rect::new(gx + col_w - 118.0, gy, 26.0, 26.0), Action::GoalStep(-1), Icon::Minus, 12.0, TEXT_2);
    c.text(format!("{:.2} L", c.vm.water.goal_ml as f32 / 1000.0), Rect::new(gx + col_w - 88.0, gy, 56.0, 26.0), 12.5, Weight::Semibold, TEXT, Align::Center);
    c.icon_btn(Rect::new(gx + col_w - 28.0, gy, 26.0, 26.0), Action::GoalStep(1), Icon::Plus, 12.0, TEXT_2);

    // Idle look segmented control.
    let sy = ty + 4.0 * row + 6.0;
    c.text("Idle look", Rect::new(a.x, sy, 90.0, 30.0), 13.0, Weight::Regular, TEXT, Align::Left);
    let look = c.vm.idle_look;
    let looks = [(IdleLook::Clock, "Clock"), (IdleLook::Eyes, "Eyes"), (IdleLook::Minimal, "Dot"), (IdleLook::Hidden, "Hidden")].map(|(l, s)| (Action::Idle(l), s, look == l));
    segmented(c, Rect::new(a.x + 100.0, sy, a.w - 100.0, 30.0), &looks, 12.5);

    if look != IdleLook::Eyes {
        c.text("Win+Shift+T toggles the island \u{00b7} Esc closes it", Rect::new(a.x, a.bottom() - 16.0, a.w, 16.0), 11.0, Weight::Regular, TEXT_3, Align::Left);
        return;
    }
    // Eyes: style + extras.
    let ey = sy + 36.0;
    let style = c.set.eye_style;
    let styles = EyeStyle::ALL.map(|st| (Action::EyeStyle(st), st.label(), st == style));
    let seg_w = 232.0;
    segmented(c, Rect::new(a.x, ey, seg_w, 28.0), &styles, 11.5);
    let chip_w = (a.w - seg_w - 8.0 - 2.0 * 6.0) / 3.0;
    let mut x = a.x + seg_w + 8.0;
    for k in [SettingKey::MoodRing, SettingKey::WeatherMood] {
        let on = k.get(c.set);
        c.pill(Rect::new(x, ey + 1.0, chip_w, 26.0), Action::Toggle(k), k.label(), on.then_some(acc_on()), 11.0);
        x += chip_w + 6.0;
    }
    let bday = match c.set.birthday {
        Some((m, d)) => format!("{} {d}", MONTHS[(m as usize).clamp(1, 12) - 1]),
        None => "Birthday".to_string(),
    };
    c.pill(Rect::new(x, ey + 1.0, chip_w, 26.0), Action::Birthday, &bday, c.set.birthday.map(|_| accent(Page::Music)), 11.0);
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Pill-shaped segmented control; `items` = (action, label, selected).
fn segmented(c: &mut Ctx, seg: Rect, items: &[(Action, &str, bool)], size: f32) {
    c.s.rect(seg, seg.h / 2.0, SURFACE);
    let sw = seg.w / items.len() as f32;
    for (i, &(act, label, on)) in items.iter().enumerate() {
        let r = Rect::new(seg.x + i as f32 * sw + 3.0, seg.y + 3.0, sw - 6.0, seg.h - 6.0);
        if on {
            c.s.rect(r, r.h / 2.0, SURFACE_PRESS.mix(WHITE.fade(0.22), 0.4));
        } else if c.hov(act) > 0.01 {
            c.s.rect(r, r.h / 2.0, SURFACE_HOVER.fade(c.hov(act)));
        }
        c.text(label, r, size, if on { Weight::Semibold } else { Weight::Regular }, if on { TEXT } else { TEXT_2 }, Align::Center);
        c.hit(r, act);
    }
}

fn acc_on() -> Color {
    GREEN
}

#[cfg(test)]
mod tests {
    #[test]
    fn gauge_scale_is_fixed_and_monotonic() {
        assert_eq!(super::gauge_frac(0.0), 0.0);
        assert!((super::gauge_frac(1000.0) - 1.0).abs() < 1e-4);
        assert_eq!(super::gauge_frac(5000.0), 1.0);
        let (a, b, c) = (super::gauge_frac(10.0), super::gauge_frac(100.0), super::gauge_frac(500.0));
        assert!(a < b && b < c);
    }

    use super::*;

    #[test]
    fn wave_stays_inside_circle() {
        for level in [0.05, 0.3, 0.5, 0.9] {
            for t in [0.0, 1.3, 7.7] {
                for (x, y) in wave_poly(100.0, 100.0, 80.0, level, t, 4.0, 0.0) {
                    let d = ((x - 100.0).powi(2) + (y - 100.0).powi(2)).sqrt();
                    assert!(d <= 80.5, "pt escaped circle: {d}");
                }
            }
        }
    }

    #[test]
    fn wave_level_orders_surface() {
        let top = |lvl: f32| wave_poly(0.0, 0.0, 80.0, lvl, 0.0, 0.0, 0.0).iter().map(|p| p.1).fold(f32::MAX, f32::min);
        assert!(top(0.8) < top(0.4));
    }

    #[test]
    fn month_lengths() {
        assert_eq!(days_in_month(2026, 10), 31);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    #[test]
    fn grouped_numbers() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(950), "950");
        assert_eq!(grouped(1250), "1,250");
        assert_eq!(grouped(12500), "12,500");
    }

    #[test]
    fn extension_and_parent() {
        assert_eq!(ext_of("report.final.pdf"), "PDF");
        assert_eq!(ext_of("Makefile"), "");
        assert_eq!(ext_of(".gitignore"), "");
        assert_eq!(parent_of("C:\\Work\\spec.pdf"), "Work");
    }

    #[test]
    fn scroll_bounds() {
        assert_eq!(shelf_max_scroll(6), 0);
        assert_eq!(shelf_max_scroll(7), 1);
        assert_eq!(tasks_max_scroll(5), 0);
        assert_eq!(tasks_max_scroll(9), 4);
    }

    #[test]
    fn seek_permille_maps_and_clamps() {
        let r = Rect::new(100.0, 0.0, 200.0, 24.0);
        assert_eq!(seek_permille(r, 100.0), 0);
        assert_eq!(seek_permille(r, 200.0), 500);
        assert_eq!(seek_permille(r, 300.0), 1000);
        assert_eq!(seek_permille(r, 0.0), 0, "left of the bar clamps to the start");
        assert_eq!(seek_permille(r, 999.0), 1000, "right of the bar clamps to the end");
    }
}
