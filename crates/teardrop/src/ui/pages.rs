//! One layout function per page. Each fills `a` (the card's content area).

use super::icons::{self, Icon};
use super::layout::{est_w, Ctx};
use super::scene::{Align, Color, Fill, Rect, Weight};
use super::state::{Action, SettingKey};
use super::theme::*;
use crate::viewmodel::{local_naive, mmss, one_line, SpeedPhase};
use chrono::{Datelike, NaiveDate};
use teardrop_core::{AiProvider, EyeStyle, IdleLook, Page, Phase, Playback};

pub fn draw(c: &mut Ctx, page: Page, a: Rect) {
    match page {
        Page::Music => music(c, a),
        Page::Focus => focus(c, a),
        Page::Water => water(c, a),
        Page::Shelf => shelf(c, a),
        Page::Tasks => tasks(c, a),
        Page::Notes => notes(c, a),
        Page::Clipboard => clipboard(c, a),
        Page::Today => today(c, a),
        Page::Speed => speed(c, a),
        Page::Usage => usage(c, a),
        Page::Voice => voice(c, a),
        Page::Ai => ai(c, a),
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

    let left_w = (a.w * 0.48).min(320.0);

    // Album art
    let art_size = 92.0;
    let art = Rect::new(a.x, a.y + 4.0, art_size, art_size);
    if has {
        let (c1, c2) = art_colors(&t.title);
        c.s.rect(art, 20.0, Fill::DGrad(c1, c2));
        c.s.rect(Rect::new(art.x, art.y, art.w, art.h * 0.55), 20.0, Fill::VGrad(Color::rgba(255, 255, 255, 0.22), Color::rgba(255, 255, 255, 0.0)));
        icons::draw(&mut c.s, Icon::Note, art.cx(), art.cy() - 6.0, 36.0, WHITE.fade(0.92));
        // Visualizer along the bottom of the tile.
        let n = 12;
        let (bw, gap) = (4.0, 2.5);
        let x0 = art.cx() - (n as f32 * bw + (n - 1) as f32 * gap) / 2.0;
        for (i, b) in c.ui.viz.sample(n).iter().enumerate() {
            let h = 3.0 + b * 24.0;
            c.s.rect(Rect::new(x0 + i as f32 * (bw + gap), art.bottom() - 10.0 - h, bw, h), 2.0, WHITE.fade(0.85));
        }
    } else {
        c.s.rect(art, 20.0, SURFACE);
        icons::draw(&mut c.s, Icon::Note, art.cx(), art.cy(), 36.0, TEXT_3);
    }

    // Track info to the right of album art
    let tx = art.right() + 14.0;
    let tw = (a.x + left_w - tx).max(0.0);
    let title = if has { t.title.clone() } else { "Play something".to_string() };
    c.text(title, Rect::new(tx, a.y + 10.0, tw, 26.0), 18.0, Weight::Semibold, if has { TEXT } else { TEXT_2 }, Align::Left);
    let sub = if !has {
        "Nothing playing".to_string()
    } else if t.album.is_empty() {
        t.display_artist().to_string()
    } else {
        format!("{} \u{00b7} {}", t.display_artist(), t.album)
    };
    c.text(sub, Rect::new(tx, a.y + 38.0, tw, 20.0), 13.0, Weight::Regular, TEXT_2, Align::Left);

    // App badge (colored dot + source app name)
    let app_name = if has && !t.app.is_empty() {
        t.app.clone()
    } else {
        "Windows Media".to_string()
    };
    let dot_col = if app_name.to_lowercase().contains("spotify") {
        Color::rgb(30, 215, 96)
    } else if has {
        acc
    } else {
        TEXT_3
    };
    c.s.circle(tx + 4.0, a.y + 68.0, 3.5, dot_col);
    c.text(app_name, Rect::new(tx + 14.0, a.y + 60.0, tw - 14.0, 16.0), 11.5, Weight::Semibold, TEXT_3, Align::Left);

    // Seek bar
    let sy = a.y + 128.0;
    let pos_str = fmt_ms(t.position_ms);
    let rem_ms = t.duration_ms.saturating_sub(t.position_ms);
    let rem_str = if has && t.duration_ms > 0 { format!("-{}", fmt_ms(rem_ms)) } else { fmt_ms(t.duration_ms) };

    c.text(pos_str, Rect::new(a.x, sy - 8.0, 36.0, 16.0), 11.0, Weight::Medium, TEXT_3, Align::Left);
    c.text(rem_str, Rect::new(a.x + left_w - 42.0, sy - 8.0, 42.0, 16.0), 11.0, Weight::Medium, TEXT_3, Align::Right);

    let bar_x = a.x + 42.0;
    let bar_w = (left_w - 90.0).max(10.0);
    let bar = Rect::new(bar_x, sy - 2.5, bar_w, 5.0);
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

    // Transport buttons centered in the left column
    let cx = a.x + left_w * 0.5;
    let cy = a.y + 178.0;
    c.ghost_icon(Rect::new(cx - 76.0, cy - 20.0, 40.0, 40.0), Action::MediaPrev, Icon::Prev, 19.0, TEXT);
    c.ghost_icon(Rect::new(cx + 36.0, cy - 20.0, 40.0, 40.0), Action::MediaNext, Icon::Next, 19.0, TEXT);
    let pa = Action::MediaPlayPause;
    let r = 26.0 + c.hov(pa) * 1.5;
    let fill = if c.pressed(pa) { WHITE.mix(Color::rgb(0, 0, 0), 0.2) } else { WHITE };
    c.s.circle(cx, cy, r, fill);
    let icon = if t.playback == Playback::Playing { Icon::Pause } else { Icon::Play };
    let nudge = if icon == Icon::Play { 1.5 } else { 0.0 };
    icons::draw(&mut c.s, icon, cx + nudge, cy, 22.0, ON_ACCENT);
    c.hit(Rect::new(cx - 28.0, cy - 28.0, 56.0, 56.0), pa);

    // Right Column: Synced Karaoke Lyrics
    let lx = a.x + left_w + 36.0;
    let lw = (a.right() - lx).max(0.0);

    if !c.set.lyrics_enabled {
        c.text("Lyrics are turned off", Rect::new(lx, a.y + 90.0, lw, 24.0), 13.5, Weight::Regular, TEXT_3, Align::Left);
    } else if let (true, Some(i)) = (has, vm.lyric_idx) {
        // Line 1: Previous line (dimmed)
        if i > 0 {
            if let Some(prev) = vm.lyric_lines.get(i - 1) {
                c.text(prev.text.clone(), Rect::new(lx, a.y + 46.0, lw, 24.0), 13.5, Weight::Regular, TEXT_3, Align::Left);
            }
        }
        // Line 2: Current active line (karaoke highlighted)
        if let Some(curr) = vm.lyric_lines.get(i) {
            let frac = teardrop_core::line_progress(&vm.lyric_lines, i, t.position_ms);
            c.s.text_karaoke(curr.text.clone(), Rect::new(lx, a.y + 82.0, lw, 36.0), 18.0, Weight::Bold, TEXT_2, WHITE, frac);
        }
        // Line 3: Next line (dimmed)
        if let Some(next) = vm.lyric_lines.get(i + 1) {
            c.text(next.text.clone(), Rect::new(lx, a.y + 128.0, lw, 24.0), 13.5, Weight::Regular, TEXT_3, Align::Left);
        }
    } else {
        let msg = if has { "No synced lyrics for this track" } else { "Lyrics appear here while music plays" };
        c.text(msg, Rect::new(lx, a.y + 90.0, lw, 24.0), 13.5, Weight::Regular, TEXT_3, Align::Left);
    }
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

/// Phone Link blue.
const PHONE: Color = Color::rgb(56, 160, 255);

/// While files are carried over the island the card splits in two drop zones,
/// Shelf (left) and Phone Link (right); the one under the pointer lights up
/// with inward ripples and a bobbing icon. After the drop a ring bursts out in
/// the colour of the zone that took it.
fn drop_overlay(c: &mut Ctx, a: Rect) {
    let (amt, u) = (c.ui.drop_amt, c.ui.absorb_t);
    let t = c.ui.clock_s;
    if amt > 0.01 {
        let zw = (a.w - 12.0) / 2.0;
        let phone_w = c.ui.zone_amt.clamp(0.0, 1.0);
        c.s.rect(a.inset(-6.0), 20.0, Color::rgba(14, 14, 18, 0.9 * amt));
        let n = c.ui.drag_files.max(1);
        let zones = [
            (Rect::new(a.x, a.y, zw, a.h), accent(Page::Shelf), Icon::Tray, "Shelf", "Keep it here", 1.0 - phone_w),
            (Rect::new(a.x + zw + 12.0, a.y, zw, a.h), PHONE, Icon::Phone, "Phone Link", "Send to your phone", phone_w),
        ];
        for (z, col, icon, title, sub, hot) in zones {
            let k = amt * (0.35 + 0.65 * hot);
            c.s.rect(z, 18.0, col.fade(0.05 + 0.10 * hot * amt));
            c.s.stroke(z, 18.0, 1.0 + 1.5 * hot, col.fade((0.25 + 0.6 * hot) * amt));
            if hot > 0.05 {
                for i in 0..3 {
                    let ph = (t * 0.8 + i as f32 / 3.0).fract();
                    c.s.stroke(z.inset(ph * 36.0), (18.0 - ph * 8.0).max(6.0), 2.0, col.fade((1.0 - ph) * 0.5 * hot * amt));
                }
            }
            let bob = (t * 4.2).sin() * 4.0 * hot;
            let (zx, zy) = (z.cx(), z.cy() - 16.0);
            c.s.circle(zx, zy, (30.0 + 4.0 * hot) * amt.max(0.2), col.fade(0.14 * k));
            icons::draw(&mut c.s, icon, zx, zy + bob, 32.0, col.fade(k));
            c.text(title, Rect::new(z.x, zy + 30.0, z.w, 20.0), 15.0, Weight::Semibold, TEXT.fade(k), Align::Center);
            let line = if hot > 0.5 && n > 1 { format!("{n} files") } else { sub.to_string() };
            c.text(line, Rect::new(z.x, zy + 50.0, z.w, 16.0), 11.5, Weight::Regular, TEXT_2.fade(k), Align::Center);
        }
    }
    if u > 0.0 {
        let (cx, cy) = (a.cx(), a.cy());
        let phone = c.ui.absorb_zone == 1;
        let col = if phone { PHONE } else { accent(Page::Shelf) };
        let p = 1.0 - u;
        let ease = 1.0 - (1.0 - p).powi(3);
        c.s.ring(cx, cy, 24.0 + ease * 150.0, 5.0 * u + 1.0, 0.0, 1.0, col.fade(u * 0.8));
        c.s.circle(cx, cy, 30.0, col.fade(u));
        let icon = if phone { Icon::Phone } else { Icon::Check };
        icons::draw(&mut c.s, icon, cx, cy, 28.0 * (0.7 + 0.3 * ease), ON_ACCENT.fade(u));
        if phone {
            c.text("Opening share window\u{2026}", Rect::new(a.x, cy + 40.0, a.w, 18.0), 12.5, Weight::Medium, TEXT.fade(u), Align::Center);
        }
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

// -------------------------------------------------------------------- notes

pub const NOTE_ROWS: usize = 4;
const NOTE_ROW_H: f32 = 40.0;

pub fn notes_max_scroll(n: usize) -> usize {
    n.saturating_sub(NOTE_ROWS)
}

/// Greedy word wrap that keeps explicit newlines (blank lines stay blank).
pub fn wrap_note_lines(text: &str, width: f32, size: f32) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split('\n') {
        if raw.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        for word in raw.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && est_w(&candidate, size) > width {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Short "when" label for a note row: now, 5m, 3h, 2d, then the date.
pub fn note_age(updated: chrono::DateTime<chrono::Utc>, now: chrono::DateTime<chrono::Utc>) -> String {
    let secs = (now - updated).num_seconds().max(0);
    if secs < 60 {
        "now".into()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else if secs < 7 * 86_400 {
        format!("{}d", secs / 86_400)
    } else {
        updated.with_timezone(&chrono::Local).format("%b %d").to_string()
    }
}

fn notes(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Notes);
    if c.ui.input.is_some() {
        note_editor(c, a, acc);
    } else {
        note_list(c, a, acc);
    }
}

fn note_list(c: &mut Ctx, a: Rect, acc: Color) {
    let vm = c.vm;
    let field = Rect::new(a.x, a.y, a.w, 40.0);
    c.s.rect(field, 14.0, c.surface_fill(Action::NoteNew));
    icons::draw(&mut c.s, Icon::Plus, field.x + 22.0, field.cy(), 15.0, TEXT_3);
    c.text("Write a note", Rect::new(field.x + 42.0, field.y, field.w - 54.0, field.h), 14.0, Weight::Regular, TEXT_3, Align::Left);
    c.hit(field, Action::NoteNew);

    let n = vm.notes.notes.len();
    let list_y = a.y + 50.0;
    if n == 0 {
        c.empty_state(Rect::new(a.x, list_y, a.w, a.h - 50.0), Icon::Notes, acc, "No notes yet", "Click above or press N");
        return;
    }
    let scroll = c.ui.scroll_of(Page::Notes).min(notes_max_scroll(n));
    for slot in 0..NOTE_ROWS {
        let idx = scroll + slot;
        let Some(note) = vm.notes.notes.get(idx) else { break };
        let r = Rect::new(a.x, list_y + slot as f32 * NOTE_ROW_H, a.w, NOTE_ROW_H - 4.0);
        let open = Action::NoteOpen(idx as u16);
        let del = Action::NoteRemove(idx as u16);
        let hv = c.hov(open).max(c.hov(del));
        if hv > 0.01 {
            c.s.rect(r, 10.0, SURFACE.fade(hv));
        }
        let title = teardrop_core::title_of(&note.text);
        let title = if title.is_empty() { "Untitled".to_string() } else { title };
        let preview = teardrop_core::preview_of(&note.text);
        let right = if hv > 0.05 { 60.0 } else { 48.0 };
        c.text(
            one_line(&title),
            Rect::new(r.x + 10.0, r.y + 3.0, r.w - 20.0 - right, 18.0),
            13.5,
            Weight::Semibold,
            TEXT,
            Align::Left,
        );
        if !preview.is_empty() {
            c.text(
                one_line(&preview),
                Rect::new(r.x + 10.0, r.y + 20.0, r.w - 20.0 - right, 14.0),
                11.0,
                Weight::Regular,
                TEXT_3,
                Align::Left,
            );
        }
        c.text(
            note_age(note.updated_at, vm.now),
            Rect::new(r.right() - 48.0, r.y, 38.0, r.h),
            10.5,
            Weight::Regular,
            TEXT_3,
            Align::Right,
        );
        c.hit(Rect::new(r.x, r.y, r.w - 32.0, r.h), open);
        if hv > 0.05 {
            c.ghost_icon(Rect::new(r.right() - 30.0, r.cy() - 12.0, 24.0, 24.0), del, Icon::Close, 12.0, TEXT_2);
        }
    }
    let count = if n == 1 { "1 note".to_string() } else { format!("{n} notes") };
    c.text(count, Rect::new(a.x + 2.0, a.bottom() - 20.0, 120.0, 18.0), 11.5, Weight::Medium, TEXT_3, Align::Left);
}

fn note_editor(c: &mut Ctx, a: Rect, acc: Color) {
    let text = c.ui.input.clone().unwrap_or_default();
    let idx = c.ui.note_edit.as_ref().and_then(|id| c.vm.notes.notes.iter().position(|n| &n.id == id));

    c.text("Note", Rect::new(a.x, a.y, 60.0, 24.0), 12.0, Weight::Medium, TEXT_2, Align::Left);
    if let Some(i) = idx {
        c.pill(Rect::new(a.right() - 132.0, a.y, 64.0, 24.0), Action::NoteRemove(i as u16), "Delete", None, 11.0);
    }
    c.pill(Rect::new(a.right() - 62.0, a.y, 62.0, 24.0), Action::NoteClose, "Done", Some(acc), 11.0);

    let body = Rect::new(a.x, a.y + 32.0, a.w, a.h - 60.0);
    c.s.rect(body, 14.0, SURFACE);
    let inner_w = body.w - 24.0;
    let line_h = 19.0;
    let lines = wrap_note_lines(&text, inner_w, 13.5);
    let visible = (((body.h - 20.0) / line_h).floor() as usize).max(1);
    let start = lines.len().saturating_sub(visible);
    let mut y = body.y + 10.0;
    for line in lines.iter().skip(start) {
        if !line.is_empty() {
            c.text(line.clone(), Rect::new(body.x + 12.0, y, inner_w, line_h), 13.5, Weight::Regular, TEXT, Align::Left);
        }
        y += line_h;
    }
    if text.is_empty() {
        c.text(
            "Start typing \u{2014} Enter starts a new line",
            Rect::new(body.x + 12.0, body.y + 10.0, inner_w, line_h),
            13.5,
            Weight::Regular,
            TEXT_3,
            Align::Left,
        );
    }
    let blink = (c.ui.clock_s * 1.8) % 2.0 < 1.0;
    let mut y = body.y + 10.0;
    for (i, line) in lines.iter().skip(start).enumerate() {
        let last = i + 1 == lines.len() - start;
        if !line.is_empty() {
            // The live field: let the renderer put the caret right after the
            // last glyph instead of guessing a width.
            if last && blink {
                c.s.text_caret(
                    line.clone(),
                    Rect::new(body.x + 12.0, y, inner_w, line_h),
                    13.5,
                    Weight::Regular,
                    TEXT,
                    Align::Left,
                    acc,
                );
            } else {
                c.text(line.clone(), Rect::new(body.x + 12.0, y, inner_w, line_h), 13.5, Weight::Regular, TEXT, Align::Left);
            }
        }
        y += line_h;
    }
    if text.is_empty() {
        c.text(
            "Start typing \u{2014} Enter starts a new line",
            Rect::new(body.x + 12.0, body.y + 10.0, inner_w, line_h),
            13.5,
            Weight::Regular,
            TEXT_3,
            Align::Left,
        );
    }
    // Caret on an empty line (new note, or a trailing newline) has no glyph
    // to measure, so draw it at the line start.
    let last_line = lines.last().cloned().unwrap_or_default();
    if blink && last_line.is_empty() {
        let row = (lines.len() - start).saturating_sub(1) as f32;
        let cy = body.y + 10.0 + row * line_h;
        c.s.rect(Rect::new(body.x + 12.0, cy + 2.0, 1.6, 16.0), 0.8, acc);
    }
    c.text(
        "Enter adds a line \u{00b7} Esc when done",
        Rect::new(a.x + 2.0, a.bottom() - 18.0, a.w, 16.0),
        11.0,
        Weight::Regular,
        TEXT_3,
        Align::Left,
    );
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

/// Year and 1-based month for the calendar month offset from `today`.
pub fn shown_year_month(today: NaiveDate, cal_offset: i32) -> (i32, u32) {
    let idx = today.year() * 12 + today.month0() as i32 + cal_offset;
    (idx.div_euclid(12), idx.rem_euclid(12) as u32 + 1)
}

/// Active list of events on `Page::Today`: either the events occurring on
/// `selected_day` or the upcoming agenda when no day is selected.
pub fn active_calendar_events<'a>(
    vm: &'a crate::viewmodel::ViewModel,
    selected_day: Option<(i32, u32, u32)>,
) -> Vec<&'a crate::calendar::Event> {
    match selected_day {
        Some((y, m, d)) => vm.calendar.events_on_day(y, m, d),
        None => vm.calendar.agenda(vm.now.timestamp(), 15),
    }
}

pub fn today_max_scroll(count: usize) -> usize {
    count.saturating_sub(3)
}

fn today(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Today);
    let vm = c.vm;
    let now = local_naive(vm.now);
    let date = now.date();

    // Month grid (Monday first), shifted by the browsing offset.
    let off_m = c.ui.cal_offset;
    let (shown_y, shown_m) = shown_year_month(date, off_m);
    let cal = Rect::new(a.x, a.y, 214.0, a.h);
    let has_filter = off_m != 0 || c.ui.cal_selected_day.is_some() || c.ui.cal_selected_event.is_some();
    let title = NaiveDate::from_ymd_opt(shown_y, shown_m, 1).unwrap().format("%B %Y").to_string();
    let title_r = Rect::new(cal.x, cal.y, cal.w - 56.0, 24.0);
    c.text(title, title_r, 14.0, Weight::Semibold, if off_m == 0 { TEXT } else { acc }, Align::Left);
    if has_filter {
        c.hit(title_r, Action::CalToday);
    }
    c.icon_btn(Rect::new(cal.right() - 52.0, cal.y + 1.0, 24.0, 22.0), Action::CalStep(-1), Icon::Prev, 10.0, TEXT_2);
    c.icon_btn(Rect::new(cal.right() - 24.0, cal.y + 1.0, 24.0, 22.0), Action::CalStep(1), Icon::Next, 10.0, TEXT_2);
    let cell_w = cal.w / 7.0;
    for (i, d) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
        c.text(*d, Rect::new(cal.x + i as f32 * cell_w, cal.y + 30.0, cell_w, 16.0), 10.5, Weight::Semibold, TEXT_3, Align::Center);
    }
    let busy = crate::calendar::busy_days(&vm.calendar.events, shown_y, shown_m);
    let first = NaiveDate::from_ymd_opt(shown_y, shown_m, 1).unwrap();
    let off = first.weekday().num_days_from_monday() as usize;
    for d in 1..=days_in_month(shown_y, shown_m) {
        let slot = off + d as usize - 1;
        let (col, row) = (slot % 7, slot / 7);
        let r = Rect::new(cal.x + col as f32 * cell_w, cal.y + 50.0 + row as f32 * 25.0, cell_w, 24.0);
        let act = Action::CalDay(d as u8);
        let hv = c.ui.hover_amt(act);
        let is_today = off_m == 0 && d == date.day();
        let is_selected = c.ui.cal_selected_day == Some((shown_y, shown_m, d));
        if is_today {
            c.s.circle(r.cx(), r.cy(), 12.0, acc);
            if is_selected {
                c.s.ring(r.cx(), r.cy(), 13.3, 1.5, 0.0, std::f32::consts::TAU, ON_ACCENT.fade(0.75));
            }
        } else if is_selected {
            c.s.circle(r.cx(), r.cy(), 12.0, acc.fade(0.24));
            c.s.ring(r.cx(), r.cy(), 11.2, 1.5, 0.0, std::f32::consts::TAU, acc);
        } else if hv > 0.01 {
            c.s.circle(r.cx(), r.cy(), 11.5, SURFACE_HOVER.fade(hv));
        }
        if busy.contains(&d) {
            c.s.circle(r.cx(), r.y + r.h - 2.5, 1.9, if is_today { ON_ACCENT } else { acc });
        }
        let wk = col >= 5;
        let weight = if is_today || is_selected { Weight::Bold } else { Weight::Regular };
        let fg = if is_today {
            ON_ACCENT
        } else if is_selected {
            acc
        } else if wk {
            TEXT_3.mix(TEXT, hv * 0.5)
        } else {
            TEXT_2.mix(TEXT, hv * 0.6)
        };
        c.text(d.to_string(), r, 12.0, weight, fg, Align::Center);
        c.hit(r, act);
    }

    // Day + agenda/detail + a one-line system summary.
    let x = a.x + 232.0;
    let w = a.w - 232.0;
    let items = active_calendar_events(vm, c.ui.cal_selected_day);
    if let Some(ev_idx) = c.ui.cal_selected_event {
        if let Some(&ev) = items.get(ev_idx) {
            event_detail(c, Rect::new(x, a.y, w, a.h - 20.0), ev_idx, ev, acc);
            draw_today_sys_line(c, x, a.bottom() - 16.0, w);
            return;
        }
    }
    if let Some((sy, sm, sd)) = c.ui.cal_selected_day {
        let sel_date = NaiveDate::from_ymd_opt(sy, sm, sd).unwrap_or(date);
        c.text(sel_date.format("%A").to_string(), Rect::new(x, a.y, w - 68.0, 30.0), 21.0, Weight::Semibold, TEXT, Align::Left);
        c.pill(Rect::new(x + w - 64.0, a.y + 4.0, 64.0, 22.0), Action::CalToday, "Upcoming", None, 10.0);
        let sub = if sel_date == date {
            format!("{} \u{00b7} Today", sel_date.format("%-d %B"))
        } else if sy == date.year() {
            sel_date.format("%-d %B").to_string()
        } else {
            sel_date.format("%-d %B %Y").to_string()
        };
        c.text(sub, Rect::new(x, a.y + 30.0, w, 20.0), 14.0, Weight::Regular, acc, Align::Left);
    } else {
        c.text(date.format("%A").to_string(), Rect::new(x, a.y, w, 30.0), 23.0, Weight::Semibold, TEXT, Align::Left);
        c.text(date.format("%-d %B").to_string(), Rect::new(x, a.y + 30.0, w, 20.0), 14.0, Weight::Regular, acc, Align::Left);
    }
    agenda(c, Rect::new(x, a.y + 58.0, w, 158.0), &items, acc);
    draw_today_sys_line(c, x, a.bottom() - 16.0, w);
}

fn draw_today_sys_line(c: &mut Ctx, x: f32, y: f32, w: f32) {
    let sys = &c.vm.system;
    let mut line = format!("CPU {:.0}%  \u{00b7}  RAM {:.0}%  \u{00b7}  Disk {:.0}%", sys.cpu_pct, sys.mem_pct(), sys.disk_pct());
    if let Some(b) = sys.battery_pct {
        line.push_str(&format!("  \u{00b7}  {b}%{}", if sys.charging { "\u{26a1}" } else { "" }));
    }
    c.text(line, Rect::new(x, y, w, 16.0), 9.5, Weight::Regular, TEXT_3, Align::Left);
}

/// Upcoming meetings or selected day's events, or the prompt to connect a calendar.
fn agenda(c: &mut Ctx, r: Rect, items: &[&crate::calendar::Event], acc: Color) {
    use crate::calendar::{clock_label, countdown, day_label, time_range_label, FeedState};
    let vm = c.vm;
    let now = vm.now.timestamp();
    if vm.calendar_feeds == 0 {
        c.text("Never miss a meeting", Rect::new(r.x, r.y + 2.0, r.w, 20.0), 13.5, Weight::Semibold, TEXT, Align::Left);
        for (i, l) in ["Paste an iCal link from Google", "Calendar or Outlook and Teardrop", "alerts you before each meeting."].iter().enumerate() {
            c.text(*l, Rect::new(r.x, r.y + 26.0 + i as f32 * 15.0, r.w, 15.0), 10.5, Weight::Regular, TEXT_3, Align::Left);
        }
        c.pill(Rect::new(r.x, r.y + 82.0, 150.0, 26.0), Action::CalendarPaste, "Paste calendar link", Some(acc), 11.0);
        c.text("Copy the link first, then press it.", Rect::new(r.x, r.y + 114.0, r.w, 15.0), 10.0, Weight::Regular, TEXT_3, Align::Left);
        return;
    }
    let day_mode = c.ui.cal_selected_day.is_some();
    if items.is_empty() {
        let (msg, col) = match vm.calendar.state {
            FeedState::Loading | FeedState::Idle => ("Loading your calendar\u{2026}".to_string(), TEXT_3),
            FeedState::Failed => (vm.calendar.error.clone(), AMBER),
            FeedState::Ok if day_mode => ("No events on this day".to_string(), TEXT_3),
            FeedState::Ok => ("No upcoming meetings".to_string(), TEXT_3),
        };
        c.text(msg, Rect::new(r.x, r.y + 6.0, r.w, 18.0), 12.0, Weight::Medium, col, Align::Left);
        if day_mode && vm.calendar.state == FeedState::Ok {
            c.text("Click another date on the calendar or press Upcoming.", Rect::new(r.x, r.y + 26.0, r.w, 15.0), 10.0, Weight::Regular, TEXT_3, Align::Left);
        }
    }
    let scroll = c.ui.scroll_of(Page::Today).min(today_max_scroll(items.len()));
    for (slot, (i, ev)) in items.iter().enumerate().skip(scroll).take(3).enumerate() {
        let row = Rect::new(r.x, r.y + slot as f32 * 44.0, r.w, 40.0);
        let ev_act = Action::CalEvent(i as u8);
        let hv = c.ui.hover_amt(ev_act);
        c.s.rect(row, 10.0, SURFACE.mix(SURFACE_HOVER, hv * 0.85));
        if hv > 0.01 {
            c.s.stroke(row, 10.0, 1.0, acc.fade(0.35 * hv));
        }
        if i == 0 {
            c.s.rect(Rect::new(row.x, row.y + 8.0, 3.0, row.h - 16.0), 1.5, acc);
        }
        let join = ev.join.is_some();
        let tw = row.w - 20.0 - if join { 48.0 } else { 0.0 };
        c.text(ev.title.clone(), Rect::new(row.x + 12.0, row.y + 4.0, tw, 18.0), 12.5, Weight::Semibold, TEXT, Align::Left);
        let when = if day_mode {
            time_range_label(ev)
        } else {
            let day = day_label(ev, now);
            if ev.all_day { format!("{day} \u{00b7} All day") } else { format!("{day} {}", clock_label(ev)) }
        };
        let tail = match (&ev.join, i) {
            (Some(j), _) => format!(" \u{00b7} {}", j.platform),
            (None, _) if !ev.location.is_empty() => format!(" \u{00b7} {}", one_line(&ev.location)),
            (None, 0) if !ev.all_day => format!(" \u{00b7} {}", countdown(ev, now)),
            _ => String::new(),
        };
        c.text(format!("{when}{tail}"), Rect::new(row.x + 12.0, row.y + 22.0, tw, 14.0), 10.5, Weight::Regular, TEXT_3, Align::Left);
        let hit_w = if join { row.w - 48.0 } else { row.w };
        c.hit(Rect::new(row.x, row.y, hit_w, row.h), ev_act);
        if join {
            c.pill(Rect::new(row.right() - 46.0, row.cy() - 11.0, 40.0, 22.0), Action::Join(i as u8), "Join", Some(acc), 10.5);
        }
    }
    let fy = r.y + 3.0 * 44.0 + 4.0;
    c.pill(Rect::new(r.x, fy, 64.0, 20.0), Action::CalendarPaste, "Add link", None, 10.0);
    c.pill(Rect::new(r.x + 70.0, fy, 64.0, 20.0), Action::CalendarClear, "Remove", None, 10.0);
    if items.len() > 3 {
        c.text(
            format!("{}\u{2013}{} of {}", scroll + 1, (scroll + 3).min(items.len()), items.len()),
            Rect::new(r.x + 140.0, fy + 2.0, r.w - 140.0, 16.0),
            9.5,
            Weight::Regular,
            TEXT_3,
            Align::Right,
        );
    }
}

/// Full detail card for a clicked calendar event on `Page::Today`.
fn event_detail(c: &mut Ctx, r: Rect, ev_idx: usize, ev: &crate::calendar::Event, acc: Color) {
    use crate::calendar::{countdown, full_date_label, time_range_label};
    c.pill(Rect::new(r.x, r.y, 58.0, 22.0), Action::CalCloseEvent, "\u{2190} Back", None, 10.5);
    c.pill(Rect::new(r.right() - 54.0, r.y, 54.0, 22.0), Action::CalCopyEvent, "Copy", None, 10.5);
    let status = if ev.all_day { "All day".to_string() } else { countdown(ev, c.vm.now.timestamp()) };
    c.text(status, Rect::new(r.x + 62.0, r.y + 3.0, r.w - 122.0, 16.0), 10.5, Weight::Semibold, acc, Align::Right);

    let card = Rect::new(r.x, r.y + 28.0, r.w, r.h - 28.0);
    c.s.rect(card, 12.0, SURFACE);
    c.s.rect(Rect::new(card.x, card.y + 10.0, 3.0, card.h - 20.0), 1.5, acc);

    let ix = card.x + 12.0;
    let iw = card.w - 24.0;
    let mut y = card.y + 8.0;

    for line in wrap_lines(&ev.title, iw, 13.0).into_iter().take(2) {
        c.text(line, Rect::new(ix, y, iw, 17.0), 13.0, Weight::Semibold, TEXT, Align::Left);
        y += 17.0;
    }
    y += 3.0;

    let when = format!("{}  \u{00b7}  {}", full_date_label(ev), time_range_label(ev));
    c.text(when, Rect::new(ix, y, iw, 15.0), 10.5, Weight::Medium, acc, Align::Left);
    y += 17.0;

    if !ev.location.is_empty() {
        c.text(format!("Location: {}", one_line(&ev.location)), Rect::new(ix, y, iw, 14.0), 10.0, Weight::Regular, TEXT_2, Align::Left);
        y += 15.0;
    }

    let bottom_limit = if ev.join.is_some() { card.bottom() - 34.0 } else { card.bottom() - 8.0 };
    let max_desc_lines = ((bottom_limit - y).max(0.0) / 13.5).floor() as usize;
    if max_desc_lines > 0 {
        if !ev.description.is_empty() {
            for line in wrap_lines(&ev.description, iw, 10.0).into_iter().take(max_desc_lines) {
                c.text(line, Rect::new(ix, y, iw, 13.5), 10.0, Weight::Regular, TEXT_3, Align::Left);
                y += 13.5;
            }
        } else if ev.location.is_empty() {
            c.text("No location or notes attached.", Rect::new(ix, y, iw, 14.0), 10.0, Weight::Regular, TEXT_3, Align::Left);
        }
    }

    if let Some(j) = &ev.join {
        c.pill(
            Rect::new(ix, card.bottom() - 28.0, iw, 22.0),
            Action::Join(ev_idx as u8),
            &format!("Join {}", j.platform),
            Some(acc),
            10.5,
        );
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

// -------------------------------------------------------------------- usage

fn usage_bar(c: &mut Ctx, r: Rect, label: &str, w: Option<&crate::usage::Window>, now: i64, acc: Color) {
    c.text(label, Rect::new(r.x, r.y, r.w - 40.0, 14.0), 10.5, Weight::Medium, TEXT_3, Align::Left);
    let bar = Rect::new(r.x, r.y + 20.0, r.w, 6.0);
    c.s.rect(bar, 3.0, Color::rgba(255, 255, 255, 0.10));
    match w {
        Some(w) => {
            let used = w.used_now(now);
            let col = if used > 85.0 { RED } else if used > 65.0 { AMBER } else { acc };
            c.text(format!("{used:.0}%"), Rect::new(r.right() - 40.0, r.y - 1.0, 40.0, 16.0), 12.0, Weight::Semibold, TEXT, Align::Right);
            if used > 0.0 {
                c.s.rect(Rect::new(bar.x, bar.y, (bar.w * used / 100.0).max(6.0), 6.0), 3.0, col);
            }
        }
        None => c.text("\u{2014}", Rect::new(r.right() - 40.0, r.y - 1.0, 40.0, 16.0), 12.0, Weight::Semibold, TEXT_3, Align::Right),
    }
}

fn limits_card(c: &mut Ctx, r: Rect, name: &str, l: &crate::usage::Limits, loaded: bool, acc: Color, now: i64) {
    c.s.rect(r, 12.0, SURFACE);
    let head = if l.plan.is_empty() { name.to_string() } else { format!("{name}  \u{00b7}  {}", l.plan) };
    c.text(head, Rect::new(r.x + 12.0, r.y + 7.0, r.w - 24.0, 18.0), 13.0, Weight::Semibold, TEXT, Align::Left);
    if !l.has_data() {
        let msg = if !loaded { "Reading\u{2026}" } else if l.note.is_empty() { "No data yet" } else { l.note.as_str() };
        c.text(msg, Rect::new(r.x + 12.0, r.y + 28.0, r.w - 24.0, 18.0), 11.5, Weight::Regular, TEXT_3, Align::Left);
        return;
    }
    let half = (r.w - 36.0) / 2.0;
    let reset = |w: &Option<crate::usage::Window>, name: &str| match w.as_ref().and_then(|w| w.resets_at) {
        Some(t) if t > now => format!("{name} \u{00b7} resets {}", crate::usage::reset_text(t, now)),
        _ => name.to_string(),
    };
    usage_bar(c, Rect::new(r.x + 12.0, r.y + 28.0, half, 30.0), &reset(&l.primary, "5-hour"), l.primary.as_ref(), now, acc);
    usage_bar(c, Rect::new(r.x + 24.0 + half, r.y + 28.0, half, 30.0), &reset(&l.secondary, "Weekly"), l.secondary.as_ref(), now, acc);
}

fn usage(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Usage);
    let u = &c.vm.usage;
    let now = chrono::Utc::now().timestamp();
    c.text("Usage", Rect::new(a.x, a.y, 120.0, 26.0), 17.0, Weight::Semibold, TEXT, Align::Left);
    if u.refreshing {
        c.pill(Rect::new(a.right() - 96.0, a.y + 1.0, 96.0, 24.0), Action::Island, "Updating\u{2026}", None, 11.5);
    } else {
        c.pill(Rect::new(a.right() - 96.0, a.y + 1.0, 96.0, 24.0), Action::UsageRefresh, "Refresh", None, 11.5);
    }
    let w = a.w;
    limits_card(c, Rect::new(a.x, a.y + 34.0, w, 62.0), "Claude Code", &u.claude, u.loaded, acc, now);
    limits_card(c, Rect::new(a.x, a.y + 102.0, w, 62.0), "Codex", &u.codex, u.loaded, acc, now);

    // OpenCode: spend from its local database (no plan limits to read).
    let r = Rect::new(a.x, a.y + 170.0, w, a.h - 170.0);
    c.s.rect(r, 12.0, SURFACE);
    let o = &u.opencode;
    c.text("OpenCode", Rect::new(r.x + 12.0, r.y + 7.0, 120.0, 18.0), 13.0, Weight::Semibold, TEXT, Align::Left);
    if !u.loaded || !o.note.is_empty() {
        let msg = if !u.loaded { "Reading\u{2026}" } else { o.note.as_str() };
        c.text(msg, Rect::new(r.x + 12.0, r.y + 28.0, r.w - 24.0, 18.0), 11.5, Weight::Regular, TEXT_3, Align::Left);
    } else if o.week_tokens == 0 {
        c.text("No OpenCode activity in the last 7 days", Rect::new(r.x + 12.0, r.y + 28.0, r.w - 24.0, 18.0), 11.5, Weight::Regular, TEXT_3, Align::Left);
    } else {
        let money = |v: f64| if v > 0.0 { format!(" \u{00b7} ${v:.2}") } else { String::new() };
        let line = format!("Today {} tokens{}    Week {} tokens{}", crate::usage::compact_count(o.today_tokens), money(o.today_cost), crate::usage::compact_count(o.week_tokens), money(o.week_cost));
        c.text(line, Rect::new(r.x + 12.0, r.y + 26.0, r.w - 24.0, 18.0), 12.0, Weight::Medium, TEXT, Align::Left);
        let top = o.top.iter().map(|s| format!("{} {}", s.label.rsplit('/').next().unwrap_or(&s.label), crate::usage::compact_count(s.tokens))).collect::<Vec<_>>().join("  \u{00b7}  ");
        if !top.is_empty() {
            c.text(top, Rect::new(r.x + 12.0, r.y + 45.0, r.w - 24.0, 16.0), 10.5, Weight::Regular, TEXT_3, Align::Left);
        }
    }
}

// -------------------------------------------------------------------- voice

fn voice(c: &mut Ctx, a: Rect) {
    use crate::voice::VoicePhase;
    let acc = accent(Page::Voice);
    let v = &c.vm.voice;
    let rec = v.phase == VoicePhase::Recording;
    let cx = a.cx();

    c.text(crate::voice::clock(v.elapsed_ms), Rect::new(a.x, a.y, a.w, 34.0), 28.0, Weight::Semibold, if rec { TEXT } else { TEXT_3 }, Align::Center);

    // Waveform of the last few seconds, mirrored around the middle.
    let n = c.ui.voice_bars.len();
    let (bw, gap) = (5.0, 3.0);
    let x0 = cx - (n as f32 * (bw + gap) - gap) / 2.0;
    let mid = a.y + 66.0;
    for (i, b) in c.ui.voice_bars.iter().enumerate() {
        let h = 3.0 + b * 40.0;
        let fade = 0.35 + 0.65 * (i as f32 / n as f32);
        c.s.rect(Rect::new(x0 + i as f32 * (bw + gap), mid - h / 2.0, bw, h), 2.5, if rec { acc.fade(fade) } else { Color::rgba(255, 255, 255, 0.12) });
    }

    // Record / stop button.
    let (by, br) = (a.y + 134.0, 34.0);
    let act = Action::VoiceToggle;
    let hv = c.hov(act);
    if rec {
        let pulse = ((c.now_ms % 1400) as f32 / 1400.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
        c.s.circle(cx, by, br + 6.0 + 4.0 * pulse, acc.fade(0.16));
        c.s.circle(cx, by, br + hv * 2.0, acc);
        c.s.rect(Rect::new(cx - 11.0, by - 11.0, 22.0, 22.0), 5.0, WHITE);
    } else {
        c.s.circle(cx, by, br + 6.0, acc.fade(0.10 + 0.10 * hv));
        c.s.ring(cx, by, br, 3.0, 0.0, 1.0, acc);
        c.s.circle(cx, by, 14.0 + hv * 2.0, acc);
    }
    c.hit(Rect::new(cx - br, by - br, br * 2.0, br * 2.0), act);

    // Status line.
    let (msg, col) = match v.phase {
        VoicePhase::Recording => ("Recording\u{2026} tap to stop".to_string(), TEXT_2),
        VoicePhase::Failed => (v.error.clone(), AMBER),
        VoicePhase::Idle => match &v.saved {
            Some(name) => (format!("Saved to the shelf \u{00b7} {name}"), TEXT_2),
            None => ("Tap to record from your microphone".to_string(), TEXT_3),
        },
    };
    c.text(msg, Rect::new(a.x, a.y + 184.0, a.w, 18.0), 12.0, Weight::Medium, col, Align::Center);
    c.pill(Rect::new(a.right() - 112.0, a.bottom() - 26.0, 112.0, 24.0), Action::VoiceFolder, "Open folder", None, 11.0);
}

// ----------------------------------------------------------------------- ai

/// Word-wrap `text` into visual lines whose estimated pixel width fits `max_w`.
pub fn wrap_lines(text: &str, max_w: f32, size: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.lines() {
        let para = para.trim_end();
        if para.is_empty() {
            continue;
        }
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let candidate = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if est_w(&candidate, size) * 1.12 <= max_w || cur.is_empty() {
                cur = candidate;
            } else {
                out.push(cur);
                cur = word.to_string();
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

pub const AI_HIST_ROWS: usize = 5;

pub fn ai_hist_max_scroll(n: usize) -> usize {
    n.saturating_sub(AI_HIST_ROWS)
}

fn ai(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Ai);
    let ai = &c.vm.ai;
    let prov = c.set.ai_provider;
    let model = c.set.active_ai_model();

    // Top bar: Provider/Model badge on the left, New / History / Setup pills on the right.
    let setup_r = Rect::new(a.right() - 52.0, a.y, 52.0, 24.0);
    let hist_r = Rect::new(setup_r.x - 5.0 - 54.0, a.y, 54.0, 24.0);
    let new_r = Rect::new(hist_r.x - 5.0 - 44.0, a.y, 44.0, 24.0);
    c.pill(new_r, Action::AiNewChat, "New", None, 11.0);
    c.pill(hist_r, Action::AiToggleHistory, "History", ai.show_history.then_some(acc), 11.0);
    c.pill(setup_r, Action::AiToggleSetup, "Setup", ai.show_setup.then_some(acc), 11.0);

    let badge_r = Rect::new(a.x, a.y, new_r.x - a.x - 8.0, 24.0);
    let bh = c.hov(Action::AiToggleSetup);
    c.s.rect(badge_r, 12.0, SURFACE.mix(SURFACE_HOVER, bh));
    let ready = ai.configured || prov == AiProvider::Ollama || prov == AiProvider::OpenAiCompat;
    c.s.circle(badge_r.x + 12.0, badge_r.cy(), 3.8, if ready { GREEN } else { AMBER });
    let head = format!("{} \u{00b7} {}", prov.label(), model);
    c.text(head, Rect::new(badge_r.x + 22.0, badge_r.y, badge_r.w - 28.0, badge_r.h), 11.5, Weight::Semibold, TEXT, Align::Left);
    c.hit(badge_r, Action::AiToggleSetup);

    if ai.show_setup {
        ai_setup(c, a, acc);
    } else if ai.show_history {
        ai_history(c, a, acc);
    } else {
        ai_chat(c, a, acc);
    }
}

fn ai_setup(c: &mut Ctx, a: Rect, acc: Color) {
    use crate::ai::AiFocusField;
    let ai = &c.vm.ai;
    let prov = c.set.ai_provider;

    // 1. Provider segmented control (6 providers).
    let seg_r = Rect::new(a.x, a.y + 30.0, a.w, 26.0);
    let items = AiProvider::ALL.map(|p| (Action::AiSetProvider(p), p.short_label(), p == prov));
    segmented(c, seg_r, &items, 11.0);

    // 2. Provider auth / endpoint card.
    let card = Rect::new(a.x, a.y + 62.0, a.w, 56.0);
    c.s.rect(card, 12.0, SURFACE);
    match prov {
        AiProvider::ChatGpt => {
            c.text(
                "Sign in with ChatGPT \u{00b7} Codex Subscription OAuth",
                Rect::new(card.x + 12.0, card.y + 6.0, card.w - 24.0, 18.0),
                12.0,
                Weight::Semibold,
                TEXT,
                Align::Left,
            );
            if ai.chatgpt_login_busy {
                if !ai.chatgpt_user_code.is_empty() {
                    c.text(
                        format!("Code: {}", ai.chatgpt_user_code),
                        Rect::new(card.x + 12.0, card.y + 27.0, card.w - 176.0, 22.0),
                        13.5,
                        Weight::Bold,
                        acc,
                        Align::Left,
                    );
                    c.pill(Rect::new(card.right() - 156.0, card.y + 26.0, 92.0, 24.0), Action::AiChatGptCopyCode, "Copy & Open", Some(acc), 11.0);
                    c.pill(Rect::new(card.right() - 58.0, card.y + 26.0, 50.0, 24.0), Action::AiChatGptSignIn, "Cancel", None, 11.0);
                } else {
                    c.text(
                        "Requesting device code from auth.openai.com\u{2026}",
                        Rect::new(card.x + 12.0, card.y + 28.0, card.w - 86.0, 20.0),
                        11.5,
                        Weight::Regular,
                        TEXT_2,
                        Align::Left,
                    );
                    c.pill(Rect::new(card.right() - 66.0, card.y + 26.0, 58.0, 24.0), Action::AiChatGptSignIn, "Cancel", None, 11.0);
                }
            } else if ai.chatgpt_signed_in {
                c.s.circle(card.x + 17.0, card.y + 37.0, 3.8, GREEN);
                c.text(
                    "Connected (DPAPI OAuth token)",
                    Rect::new(card.x + 26.0, card.y + 27.0, card.w - 186.0, 20.0),
                    11.5,
                    Weight::Medium,
                    TEXT_2,
                    Align::Left,
                );
                c.pill(Rect::new(card.right() - 152.0, card.y + 26.0, 80.0, 24.0), Action::AiRefreshModels, "Sync models", None, 10.5);
                c.pill(Rect::new(card.right() - 66.0, card.y + 26.0, 58.0, 24.0), Action::AiChatGptSignOut, "Sign out", None, 10.5);
            } else {
                c.text(
                    "Use your ChatGPT Plus, Pro, or Team subscription",
                    Rect::new(card.x + 12.0, card.y + 28.0, card.w - 166.0, 20.0),
                    11.5,
                    Weight::Regular,
                    TEXT_3,
                    Align::Left,
                );
                c.pill(
                    Rect::new(card.right() - 148.0, card.y + 25.0, 140.0, 25.0),
                    Action::AiChatGptSignIn,
                    "Sign in with ChatGPT",
                    Some(acc),
                    11.0,
                );
            }
        }
        AiProvider::OpenCodeGo => {
            c.text(
                "OpenCode Go \u{00b7} https://opencode.ai/zen/go/v1",
                Rect::new(card.x + 12.0, card.y + 6.0, card.w - 24.0, 18.0),
                12.0,
                Weight::Semibold,
                TEXT,
                Align::Left,
            );
            let sub = if ai.configured {
                "Key saved (DPAPI) \u{00b7} Responses / Messages / Chat"
            } else {
                "Copy your OpenCode Go API key, then click Paste Key"
            };
            c.text(sub, Rect::new(card.x + 12.0, card.y + 28.0, card.w - 176.0, 20.0), 11.0, Weight::Regular, if ai.configured { TEXT_2 } else { TEXT_3 }, Align::Left);
            c.pill(Rect::new(card.right() - 158.0, card.y + 25.0, 74.0, 25.0), Action::AiPasteKey, "Paste Key", (!ai.configured).then_some(acc), 11.0);
            c.pill(Rect::new(card.right() - 78.0, card.y + 25.0, 70.0, 25.0), Action::AiRefreshModels, "Sync models", None, 10.5);
        }
        AiProvider::OpenAiCompat => {
            let url_box = Rect::new(card.x + 10.0, card.y + 6.0, card.w - 174.0, 22.0);
            let editing_url = c.ui.input.is_some() && ai.focus_field == AiFocusField::BaseUrl;
            c.s.rect(url_box, 7.0, if editing_url { SURFACE_HOVER } else { Color::rgba(0, 0, 0, 0.25) });
            if editing_url {
                c.s.stroke(url_box, 7.0, 1.2, acc);
            }
            let shown_url = if editing_url {
                c.ui.input.as_deref().unwrap_or("")
            } else {
                c.set.active_ai_url()
            };
            c.text(shown_url.to_string(), Rect::new(url_box.x + 8.0, url_box.y, url_box.w - 16.0, url_box.h), 11.0, Weight::Medium, TEXT, Align::Left);
            c.hit_text(url_box, Action::AiFocusBaseUrl);
            c.pill(Rect::new(card.right() - 158.0, card.y + 6.0, 72.0, 22.0), Action::AiPasteUrl, "Paste URL", None, 10.5);
            c.pill(Rect::new(card.right() - 80.0, card.y + 6.0, 72.0, 22.0), Action::AiPasteKey, "Paste Key", (!ai.configured).then_some(acc), 10.5);

            let note = if ai.configured {
                "API key saved (DPAPI) \u{00b7} OpenAI / OpenRouter / Groq / vLLM"
            } else {
                "Key optional for local servers \u{00b7} Paste Key for cloud APIs"
            };
            c.text(note, Rect::new(card.x + 12.0, card.y + 32.0, card.w - 86.0, 18.0), 10.5, Weight::Regular, TEXT_3, Align::Left);
            c.pill(Rect::new(card.right() - 68.0, card.y + 31.0, 60.0, 20.0), Action::AiRefreshModels, "Sync", None, 10.0);
        }
        AiProvider::Gemini | AiProvider::Claude => {
            c.text(prov.full_name(), Rect::new(card.x + 12.0, card.y + 6.0, card.w - 110.0, 18.0), 12.0, Weight::Semibold, TEXT, Align::Left);
            let sub = if ai.configured {
                "API key saved (encrypted with Windows DPAPI)"
            } else {
                "Copy your API key to the clipboard, then click Paste Key"
            };
            c.text(sub, Rect::new(card.x + 12.0, card.y + 28.0, card.w - 110.0, 20.0), 11.0, Weight::Regular, if ai.configured { TEXT_2 } else { TEXT_3 }, Align::Left);
            c.pill(Rect::new(card.right() - 90.0, card.y + 15.0, 82.0, 26.0), Action::AiPasteKey, "Paste Key", (!ai.configured).then_some(acc), 11.0);
        }
        AiProvider::Ollama => {
            c.text("Local Ollama \u{00b7} runs offline on your PC", Rect::new(card.x + 12.0, card.y + 5.0, card.w - 24.0, 18.0), 12.0, Weight::Semibold, TEXT, Align::Left);
            let url_box = Rect::new(card.x + 10.0, card.y + 27.0, card.w - 98.0, 22.0);
            let editing_url = c.ui.input.is_some() && ai.focus_field == AiFocusField::BaseUrl;
            c.s.rect(url_box, 7.0, if editing_url { SURFACE_HOVER } else { Color::rgba(0, 0, 0, 0.25) });
            if editing_url {
                c.s.stroke(url_box, 7.0, 1.2, acc);
            }
            let shown_url = if editing_url {
                c.ui.input.as_deref().unwrap_or("")
            } else {
                c.set.active_ai_url()
            };
            c.text(shown_url.to_string(), Rect::new(url_box.x + 8.0, url_box.y, url_box.w - 16.0, url_box.h), 11.0, Weight::Medium, TEXT, Align::Left);
            c.hit_text(url_box, Action::AiFocusBaseUrl);
            c.pill(Rect::new(card.right() - 82.0, card.y + 27.0, 74.0, 22.0), Action::AiPasteUrl, "Paste URL", None, 10.5);
        }
    }

    // 3. Model selector row: editable model field + Prev/Next + preset chips.
    let my = a.y + 125.0;
    c.text("Model", Rect::new(a.x, my, 42.0, 24.0), 11.5, Weight::Medium, TEXT_2, Align::Left);
    let mbox = Rect::new(a.x + 44.0, my, 134.0, 24.0);
    let editing_model = c.ui.input.is_some() && ai.focus_field == AiFocusField::Model;
    c.s.rect(mbox, 8.0, if editing_model { SURFACE_HOVER } else { SURFACE });
    if editing_model {
        c.s.stroke(mbox, 8.0, 1.2, acc);
    }
    let shown_model = if editing_model {
        c.ui.input.as_deref().unwrap_or("")
    } else {
        c.set.active_ai_model()
    };
    c.text(shown_model.to_string(), Rect::new(mbox.x + 8.0, mbox.y, mbox.w - 16.0, mbox.h), 11.5, Weight::Semibold, TEXT, Align::Left);
    c.hit_text(mbox, Action::AiFocusModel);

    c.icon_btn(Rect::new(mbox.right() + 4.0, my, 24.0, 24.0), Action::AiCycleModel(-1), Icon::Prev, 10.0, TEXT_2);
    c.icon_btn(Rect::new(mbox.right() + 30.0, my, 24.0, 24.0), Action::AiCycleModel(1), Icon::Next, 10.0, TEXT_2);

    let mut cx = mbox.right() + 58.0;
    for (idx, m) in ai.available_models.iter().take(2).enumerate() {
        let rem = a.right() - cx;
        if rem < 60.0 {
            break;
        }
        let w = (est_w(m, 10.5) * 1.12 + 20.0).clamp(60.0, (rem - 2.0).min(118.0));
        let is_cur = m == c.set.active_ai_model();
        c.pill(Rect::new(cx, my + 1.0, w, 22.0), Action::AiSelectModel(idx as u16), m, is_cur.then_some(acc), 10.0);
        cx += w + 4.0;
    }

    // 4. Share my day toggle + learned memories badge.
    let sy = a.y + 156.0;
    c.text(
        "Share my day (calendar, tasks, water, focus, PC stats)",
        Rect::new(a.x, sy, a.w - 52.0, 22.0),
        12.0,
        Weight::Regular,
        TEXT,
        Align::Left,
    );
    c.switch(Rect::new(a.right() - 42.0, sy, 42.0, 22.0), SettingKey::AiShareDay, acc);

    // 5. Status / footer note.
    let fy = a.bottom() - 18.0;
    let (foot, col) = if !ai.error.is_empty() {
        (ai.error.clone(), AMBER)
    } else if !ai.status_note.is_empty() {
        (ai.status_note.clone(), acc)
    } else {
        (
            format!(
                "Secrets encrypted with Windows DPAPI \u{00b7} {} learned memor{}",
                ai.memories_count,
                if ai.memories_count == 1 { "y" } else { "ies" }
            ),
            TEXT_3,
        )
    };
    c.text(foot, Rect::new(a.x, fy, a.w, 18.0), 10.5, Weight::Medium, col, Align::Left);
}

fn ai_history(c: &mut Ctx, a: Rect, acc: Color) {
    let ai = &c.vm.ai;
    let body = Rect::new(a.x, a.y + 32.0, a.w, a.h - 32.0);
    if ai.history.is_empty() {
        c.empty_state(body, Icon::Sparkle, acc, "No conversations yet", "Send a message to start chatting");
        return;
    }
    let scroll = c.ui.scroll_of(Page::Ai).min(ai_hist_max_scroll(ai.history.len()));
    for slot in 0..AI_HIST_ROWS {
        let idx = scroll + slot;
        let Some(item) = ai.history.get(idx) else { break };
        let r = Rect::new(a.x, body.y + slot as f32 * 38.0, a.w, 34.0);
        let sel = Action::AiSelectChat(item.id);
        let del = Action::AiDeleteChat(item.id);
        let hv = c.hov(sel).max(c.hov(del));
        let active = item.id == ai.active_id;
        c.s.rect(r, 10.0, if active { acc.fade(0.16) } else { SURFACE.mix(SURFACE_HOVER, hv) });
        if active {
            c.s.circle(r.x + 14.0, r.cy(), 3.5, acc);
        }
        let tx = if active { r.x + 26.0 } else { r.x + 14.0 };
        c.text(item.title.clone(), Rect::new(tx, r.y, r.w - 110.0, r.h), 12.5, if active { Weight::Semibold } else { Weight::Regular }, TEXT, Align::Left);
        c.text(
            format!("{} msg{}", item.msg_count, if item.msg_count == 1 { "" } else { "s" }),
            Rect::new(r.right() - 88.0, r.y, 52.0, r.h),
            11.0,
            Weight::Regular,
            TEXT_3,
            Align::Right,
        );
        c.hit(Rect::new(r.x, r.y, r.w - 32.0, r.h), sel);
        c.ghost_icon(Rect::new(r.right() - 28.0, r.cy() - 12.0, 24.0, 24.0), del, Icon::Close, 11.5, TEXT_2);
    }
}

fn ai_chat(c: &mut Ctx, a: Rect, acc: Color) {
    use crate::ai::{AiFocusField, Role, STARTER_PROMPTS};
    let ai = &c.vm.ai;
    let prov = c.set.ai_provider;

    // Prompt input bar at the bottom.
    let in_y = a.bottom() - 34.0;
    let field = Rect::new(a.x, in_y, a.w - 40.0, 34.0);
    let typing = c.ui.input.is_some() && ai.focus_field == AiFocusField::Prompt;
    c.s.rect(field, 12.0, if typing { SURFACE_HOVER } else { c.surface_fill(Action::AiInput) });
    if typing {
        c.s.stroke(field, 12.0, 1.4, acc.fade(0.85));
    }
    icons::draw(&mut c.s, Icon::Sparkle, field.x + 16.0, field.cy(), 14.0, if typing { acc } else { TEXT_3 });
    let tr = Rect::new(field.x + 32.0, field.y, field.w - 42.0, field.h);
    let shown = if typing {
        c.ui.input.as_deref().unwrap_or("")
    } else {
        ai.input.as_str()
    };
    if !shown.is_empty() {
        c.text(shown.to_string(), tr, 12.5, Weight::Regular, TEXT, Align::Left);
        if typing && (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
            let cx = (tr.x + est_w(shown, 12.5)).min(tr.right() - 4.0);
            c.s.rect(Rect::new(cx + 1.0, field.cy() - 8.0, 1.5, 16.0), 0.7, acc);
        }
    } else {
        let placeholder = if ai.busy {
            "Thinking\u{2026}"
        } else {
            "Ask anything, add a task, or plan your day\u{2026}"
        };
        c.text(placeholder, tr, 12.5, Weight::Regular, TEXT_3, Align::Left);
        if typing && (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
            c.s.rect(Rect::new(tr.x, field.cy() - 8.0, 1.5, 16.0), 0.7, acc);
        }
    }
    c.hit_text(field, Action::AiInput);

    // Send button.
    let send_r = Rect::new(a.right() - 34.0, in_y, 34.0, 34.0);
    let can_send = !shown.trim().is_empty() && !ai.busy;
    c.s.rect(send_r, 17.0, if can_send { acc } else { SURFACE });
    icons::draw(&mut c.s, Icon::Send, send_r.cx(), send_r.cy(), 15.0, if can_send { ON_ACCENT } else { TEXT_3 });
    c.hit(send_r, Action::AiSend);

    let body = Rect::new(a.x, a.y + 30.0, a.w, in_y - (a.y + 30.0) - 6.0);

    // Empty chat state: optional setup banner + 4 starter suggestion cards.
    if ai.messages.is_empty() && !ai.busy {
        let mut grid_y = body.y;
        let mut card_h = 68.0;
        if !ai.configured && prov != AiProvider::Ollama && prov != AiProvider::OpenAiCompat {
            let ban = Rect::new(body.x, body.y, body.w, 32.0);
            c.s.rect(ban, 10.0, acc.fade(0.12));
            let msg = match prov {
                AiProvider::ChatGpt => "Sign in with your ChatGPT subscription to start",
                _ => "Add your API key in Setup to start chatting",
            };
            c.text(msg, Rect::new(ban.x + 12.0, ban.y, ban.w - 134.0, ban.h), 11.5, Weight::Medium, TEXT, Align::Left);
            let (cta_act, cta_lbl) = if prov == AiProvider::ChatGpt {
                (Action::AiChatGptSignIn, "Sign in")
            } else {
                (Action::AiToggleSetup, "Open Setup")
            };
            c.pill(Rect::new(ban.right() - 112.0, ban.y + 4.0, 104.0, 24.0), cta_act, cta_lbl, Some(acc), 11.0);
            grid_y += 38.0;
            card_h = 56.0;
        }
        let cw = (body.w - 10.0) / 2.0;
        for (i, (title, prompt)) in STARTER_PROMPTS.iter().enumerate() {
            let (col, row) = (i % 2, i / 2);
            let r = Rect::new(body.x + col as f32 * (cw + 10.0), grid_y + row as f32 * (card_h + 8.0), cw, card_h);
            let act = Action::AiStarter(i as u8);
            let hv = c.hov(act);
            c.s.rect(r, 12.0, if c.pressed(act) { SURFACE_PRESS } else { SURFACE.mix(SURFACE_HOVER, hv) });
            c.text(*title, Rect::new(r.x + 12.0, r.y + 7.0, r.w - 24.0, 18.0), 12.5, Weight::Semibold, TEXT, Align::Left);
            let lines = wrap_lines(prompt, r.w - 24.0, 10.5);
            for (li, l) in lines.iter().take(2).enumerate() {
                c.text(l.clone(), Rect::new(r.x + 12.0, r.y + 26.0 + li as f32 * 13.0, r.w - 24.0, 14.0), 10.5, Weight::Regular, TEXT_3, Align::Left);
            }
            c.hit(r, act);
        }
        return;
    }

    // Pre-layout conversation bubbles so we can bottom-align and scroll cleanly.
    enum RowItem {
        UserBubble(Vec<String>),
        AssistantBubble { msg_idx: usize, lines: Vec<String> },
        ActionRow { msg_idx: usize, act_idx: usize, label: String, can_undo: bool, undone: bool },
        Thinking,
        ErrorBanner(String),
    }

    let mut items = Vec::new();
    for (mi, m) in ai.messages.iter().enumerate() {
        match m.role {
            Role::User => {
                items.push(RowItem::UserBubble(wrap_lines(&m.text, body.w * 0.72, 12.0)));
            }
            Role::Assistant => {
                items.push(RowItem::AssistantBubble {
                    msg_idx: mi,
                    lines: wrap_lines(&m.text, body.w - 64.0, 12.0),
                });
                for (ai_idx, act) in m.actions.iter().enumerate() {
                    items.push(RowItem::ActionRow {
                        msg_idx: mi,
                        act_idx: ai_idx,
                        label: act.label.clone(),
                        can_undo: act.can_undo(),
                        undone: act.undone,
                    });
                }
            }
        }
    }
    if ai.busy {
        items.push(RowItem::Thinking);
    }
    if !ai.error.is_empty() {
        items.push(RowItem::ErrorBanner(ai.error.clone()));
    }

    let item_h = |it: &RowItem| -> f32 {
        match it {
            RowItem::UserBubble(lines) => (lines.len() as f32 * 16.0 + 12.0).max(28.0) + 5.0,
            RowItem::AssistantBubble { lines, .. } => (lines.len() as f32 * 16.0 + 14.0).max(30.0) + 5.0,
            RowItem::ActionRow { .. } => 26.0,
            RowItem::Thinking => 28.0,
            RowItem::ErrorBanner(_) => 28.0,
        }
    };

    let total_h: f32 = items.iter().map(&item_h).sum();
    let max_scroll = (total_h - body.h).max(0.0);
    let scroll_px = (ai.scroll as f32 * 24.0).min(max_scroll);
    let mut y = if total_h <= body.h {
        body.y
    } else {
        body.bottom() - total_h + scroll_px
    };

    c.s.clip(body);
    for it in &items {
        let h = item_h(it);
        if y + h < body.y - 2.0 {
            y += h;
            continue;
        }
        if y > body.bottom() + 2.0 {
            break;
        }
        match it {
            RowItem::UserBubble(lines) => {
                let bh = h - 5.0;
                let max_line = lines.iter().map(|l| est_w(l, 12.0) * 1.12).fold(40.0_f32, f32::max);
                let bw = (max_line + 22.0).clamp(64.0, body.w * 0.78);
                let br = Rect::new(body.right() - bw, y, bw, bh);
                c.s.rect(br, 11.0, acc.fade(0.22));
                for (li, l) in lines.iter().enumerate() {
                    c.text(l.clone(), Rect::new(br.x + 11.0, br.y + 6.0 + li as f32 * 16.0, br.w - 22.0, 16.0), 12.0, Weight::Regular, TEXT, Align::Left);
                }
            }
            RowItem::AssistantBubble { msg_idx, lines } => {
                let bh = h - 5.0;
                let br = Rect::new(body.x, y, body.w, bh);
                c.s.rect(br, 11.0, SURFACE);
                for (li, l) in lines.iter().enumerate() {
                    c.text(l.clone(), Rect::new(br.x + 11.0, br.y + 7.0 + li as f32 * 16.0, br.w - 58.0, 16.0), 12.0, Weight::Regular, TEXT, Align::Left);
                }
                let copy_r = Rect::new(br.right() - 44.0, br.y + 5.0, 38.0, 20.0);
                if copy_r.y >= body.y && copy_r.bottom() <= body.bottom() {
                    c.pill(copy_r, Action::AiCopyReply(*msg_idx as u16), "Copy", None, 9.5);
                }
            }
            RowItem::ActionRow { msg_idx, act_idx, label, can_undo, undone } => {
                let ar = Rect::new(body.x + 8.0, y, body.w - 8.0, 22.0);
                c.s.rect(ar, 8.0, GREEN.fade(0.12));
                icons::draw(&mut c.s, Icon::Check, ar.x + 12.0, ar.cy(), 11.0, GREEN);
                let txt = if *undone { format!("{label} (undone)") } else { label.clone() };
                c.text(txt, Rect::new(ar.x + 24.0, ar.y, ar.w - 84.0, ar.h), 11.0, Weight::Medium, if *undone { TEXT_3 } else { TEXT_2 }, Align::Left);
                if *can_undo {
                    let undo_r = Rect::new(ar.right() - 52.0, ar.y + 2.0, 48.0, 18.0);
                    if undo_r.y >= body.y && undo_r.bottom() <= body.bottom() {
                        c.pill(undo_r, Action::AiUndoAction(*msg_idx as u16, *act_idx as u8), "Undo", None, 9.5);
                    }
                }
            }
            RowItem::Thinking => {
                let pulse = ((c.now_ms % 1200) as f32 / 1200.0 * std::f32::consts::TAU).sin() * 0.35 + 0.65;
                let br = Rect::new(body.x, y, 124.0, 24.0);
                c.s.rect(br, 11.0, SURFACE);
                icons::draw(&mut c.s, Icon::Sparkle, br.x + 14.0, br.cy(), 12.0, acc.fade(pulse));
                c.text("Thinking\u{2026}", Rect::new(br.x + 28.0, br.y, br.w - 32.0, br.h), 11.5, Weight::Medium, TEXT_2.fade(pulse), Align::Left);
            }
            RowItem::ErrorBanner(err) => {
                let br = Rect::new(body.x, y, body.w, 24.0);
                c.s.rect(br, 8.0, AMBER.fade(0.14));
                c.text(err.clone(), Rect::new(br.x + 10.0, br.y, br.w - 20.0, br.h), 11.0, Weight::Medium, AMBER, Align::Left);
            }
        }
        y += h;
    }
    c.s.pop_clip();
}

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

    #[test]
    fn note_wrap_keeps_paragraph_breaks_and_wraps_long_lines() {
        // Narrow enough that "alpha beta" cannot share a line.
        let lines = wrap_note_lines("alpha beta gamma", 40.0, 14.0);
        assert!(lines.len() >= 2, "{lines:?}");
        assert_eq!(lines[0], "alpha");
        assert_eq!(lines.join(" "), "alpha beta gamma");

        // Blank lines survive, so paragraphs keep their spacing.
        let lines = wrap_note_lines("one\n\ntwo", 400.0, 14.0);
        assert_eq!(lines, vec!["one".to_string(), String::new(), "two".to_string()]);

        // Empty text still yields one caret line.
        assert_eq!(wrap_note_lines("", 400.0, 14.0), vec![String::new()]);
    }

    #[test]
    fn note_age_is_short_and_readable() {
        use chrono::{TimeZone, Utc};
        let now = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        let ago = |secs: i64| note_age(now - chrono::Duration::seconds(secs), now);
        assert_eq!(ago(5), "now");
        assert_eq!(ago(90), "1m");
        assert_eq!(ago(3 * 3600 + 120), "3h");
        assert_eq!(ago(2 * 86_400 + 60), "2d");
        assert_eq!(note_age(now, now), "now");
    }
}
