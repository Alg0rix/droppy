//! One layout function per page. Each fills `a` (the card's content area).

use super::icons::{self, Icon};
use super::layout::{est_w, Ctx};
use super::scene::{Align, Color, Fill, Rect, TextEdit, Weight};
use super::state::{Action, SettingKey};
use super::textedit::TextField;
use super::theme::*;
use crate::viewmodel::{local_naive, mmss, one_line, SpeedPhase};
use chrono::{Datelike, NaiveDate};
use teardrop_core::{apps, format_note_date, AiProvider, AppCategory, EyeStyle, IdleLook, IslandTheme, NoteDateFilter, Page, Phase, Playback};

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
        Page::System => super::system::system(c, a),
        Page::Speed => speed(c, a),
        Page::Usage => usage(c, a),
        Page::Voice => voice(c, a),
        Page::Ai => ai(c, a),
        Page::Chat => chat(c, a),
        Page::Windows => windows_page(c, a),
        Page::Settings => settings(c, a),
    }
}

/// Desktop window switcher: apps grouped, rows activate the window.
fn windows_page(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Windows);
    let n = c.vm.windows.len();
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text("ON THE DESKTOP", Rect::new(a.x + 12.0, a.y, a.w - 130.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    c.text(format!("{n} windows"), Rect::new(a.right() - 118.0, a.y, 118.0, 15.0), 10.5, Weight::Medium, pal().text_3, Align::Right);
    // Launcher field: type to search apps + windows, Enter launches the top hit.
    let field = Rect::new(a.x + 8.0, a.y + 20.0, a.w - 16.0, 28.0);
    c.s.rect(field, 14.0, pal().surface);
    let q = c.ui.input.as_ref().map(|f| f.as_str().to_string()).unwrap_or_default();
    match &c.ui.input {
        Some(f) if !f.is_empty() => {
            let ed = field_edit(c, f, acc);
            c.s.text_edit(f.as_str().to_string(), Rect::new(field.x + 12.0, field.y, field.w - 16.0, field.h), 12.0, Weight::Medium, pal().text, ed);
        }
        Some(_) => {
            c.text("Search apps, files, and windows…", Rect::new(field.x + 12.0, field.y, field.w - 16.0, field.h), 11.5, Weight::Regular, pal().text_3, Align::Left);
            if (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
                c.s.line(field.x + 12.0, field.y + 6.0, field.x + 12.0, field.y + field.h - 6.0, 1.5, acc);
            }
        }
        None => {
            c.text("Search apps, files, and windows…", Rect::new(field.x + 12.0, field.y, field.w - 16.0, field.h), 11.5, Weight::Regular, pal().text_3, Align::Left);
        }
    }
    c.hit(field, Action::WinSearch);
    let mut y = field.bottom() + 8.0;
    if !q.is_empty() {
        let hits = crate::launcher::hits(&q);
        let sel = c.ui.win_sel.min(hits.len().saturating_sub(1));
        // Scroll just enough to keep the keyboard selection on screen.
        let fit = (((a.bottom() - 26.0 - y) / 25.0).floor().max(1.0)) as usize + 1;
        let first = (sel + 1).saturating_sub(fit);
        for (i, h) in hits.iter().enumerate().skip(first) {
            if y > a.bottom() - 26.0 { break; }
            let row = Rect::new(a.x + 8.0, y, a.w - 16.0, 24.0);
            if c.ui.input.is_some() && i == sel {
                c.s.rect(row, 9.0, acc.fade(0.22));
            } else if c.hov(Action::WinLaunch(i as u16)) > 0.01 {
                c.s.rect(row, 9.0, pal().surface_hover);
            }
            let tile = Rect::new(row.x + 4.0, row.y + 3.0, 18.0, 18.0);
            match crate::appicons::icon_for(&h.icon) {
                Some(img) => c.s.image(tile, &img),
                // Letter tile fallback when the shell yields no image.
                None => {
                    c.s.rect(tile, 5.0, acc.fade(0.35));
                    let initial = h.name.chars().next().unwrap_or('?').to_uppercase().collect::<String>();
                    c.text(initial, tile, 10.5, Weight::Semibold, pal().text, Align::Center);
                }
            }
            c.text(h.name.clone(), Rect::new(row.x + 30.0, row.y + 3.0, row.w - 120.0, 17.0), 11.5, Weight::Medium, pal().text, Align::Left);
            c.text(h.tag.clone(), Rect::new(row.right() - 84.0, row.y + 4.0, 80.0, 15.0), 9.5, Weight::Medium, pal().text_3, Align::Right);
            c.hit(row, Action::WinLaunch(i as u16));
            y += 25.0;
        }
    }
    let q_lower = q.to_lowercase();
    let filtered: Vec<(usize, &crate::winreg::WindowInfo)> = c
        .vm
        .windows
        .iter()
        .enumerate()
        .filter(|(_, w)| q.is_empty() || w.title.to_lowercase().contains(&q_lower) || w.app.to_lowercase().contains(&q_lower))
        .collect();
    let row_h = 26.0;
    let scroll = c.ui.scroll_of(Page::Windows);
    // Group consecutive windows of the same exe under an app header.
    let mut last_exe = String::new();
    for (orig, w) in filtered.iter().skip(scroll) {
        if y > a.bottom() - 26.0 { break; }
        if w.exe != last_exe {
            last_exe = w.exe.clone();
            let n = filtered.iter().filter(|(_, x)| x.exe == w.exe).count();
            let tile = Rect::new(a.x + 8.0, y + 3.0, 16.0, 16.0);
            match crate::appicons::icon_for(&w.path) {
                Some(img) => c.s.image(tile, &img),
                None => c.s.rect(tile, 5.0, acc.fade(0.2)),
            }
            c.text(w.app.clone(), Rect::new(a.x + 30.0, y, a.w - 130.0, 22.0), 12.5, Weight::Semibold, pal().text, Align::Left);
            c.text(if n == 1 { "1 window".to_string() } else { format!("{} windows", n) }, Rect::new(a.right() - 120.0, y + 4.0, 112.0, 15.0), 10.0, Weight::Medium, pal().text_3, Align::Right);
            y += 24.0;
            // The app's audio session becomes a slim mixer under its header.
            if let Some(si) = c.vm.audio.iter().position(|s| s.exe == w.exe) {
                let s = &c.vm.audio[si];
                icons::draw(&mut c.s, Icon::Speaker, a.x + 32.0, y + 10.0, 12.0, pal().text_3);
                let track = Rect::new(a.x + 48.0, y + 6.0, a.w - 160.0, 8.0);
                c.s.rect(track, 4.0, pal().surface_hover);
                let fill = (track.w * s.volume).max(4.0);
                c.s.rect(Rect::new(track.x, track.y, fill, track.h), 4.0, if s.muted { pal().text_3 } else { acc });
                c.text(if s.muted { "muted".to_string() } else { format!("{:.0}%", s.volume * 100.0) }, Rect::new(a.right() - 104.0, y + 2.0, 96.0, 15.0), 10.5, Weight::Medium, pal().text_3, Align::Right);
                c.hit(Rect::new(track.x - 4.0, track.y - 5.0, track.w + 8.0, track.h + 10.0), Action::MixerVol(si as u16));
                y += 20.0;
            }
        }
        if y > a.bottom() - 26.0 { break; }
        let row = Rect::new(a.x + 34.0, y, a.w - 42.0, row_h - 4.0);
        if c.hov(Action::WinActivate(*orig as u16)) > 0.01 {
            c.s.rect(row, 9.0, pal().surface_hover);
        }
        c.text(w.title.clone(), Rect::new(row.x + 10.0, row.y + 3.0, row.w - 16.0, 16.0), 11.5, Weight::Regular, pal().text_2, Align::Left);
        c.hit(row, Action::WinActivate(*orig as u16));
        y += row_h;
    }
}

const WHITE: Color = Color::rgb(255, 255, 255);
const ON_ACCENT: Color = Color::rgb(14, 14, 18);

/// Caret/selection overlay for a focused single-line field (blinks on clock_s).
fn field_edit(c: &Ctx, f: &TextField, acc: Color) -> TextEdit {
    let blink = (c.ui.clock_s * 1.8) % 2.0 < 1.0;
    TextEdit { caret: f.caret(), show_caret: blink, sel: f.selection(), caret_color: acc, sel_color: acc.fade(0.35) }
}

fn fmt_ms(ms: u64) -> String {
    format!("{}:{:02}", ms / 60_000, (ms / 1000) % 60)
}

/// Map a click at `x` inside the seek-bar hit rect to 0..=1000 per-mille.
pub fn seek_permille(r: Rect, x: f32) -> u16 {
    let f = ((x - r.x) / r.w.max(1.0)).clamp(0.0, 1.0);
    (f * 1000.0).round() as u16
}

// -------------------------------------------------------------------- music

fn music(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Music);
    let vm = c.vm;
    let t = &vm.track;
    let has = !t.title.is_empty();

    let left_w = (a.w * 0.48).min(320.0);

    // App badge (colored dot + source app name)
    let app_name = if has && !t.app.is_empty() {
        t.app.clone()
    } else {
        "Windows Media".to_string()
    };

    // Top strip: source on the left, position/duration on the right.
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, if app_name.to_lowercase().contains("spotify") { Color::rgb(30, 215, 96) } else if has { acc } else { pal().text_3 });
    let top = if has { format!("NOW PLAYING \u{00b7} {}", app_name.to_uppercase()) } else { "MUSIC".to_string() };
    c.text(top, Rect::new(a.x + 12.0, a.y, a.w - 130.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    if has {
        let pos_str = fmt_ms(t.position_ms);
        let dur_str = fmt_ms(t.duration_ms);
        c.text(format!("{pos_str} / {dur_str}"), Rect::new(a.right() - 118.0, a.y, 118.0, 15.0), 10.5, Weight::Medium, pal().text_3, Align::Right);
    }

    // Album art
    let art_size = 84.0;
    let art = Rect::new(a.x, a.y + 22.0, art_size, art_size);
    if has {
        match crate::media_smtc::art_for(t) {
            Some(img) => {
                c.s.image_rounded(art, 20.0, &img);
                // Darken the bottom so the visualizer stays legible over art.
                let gh = art.h * 0.45;
                c.s.rect(Rect::new(art.x, art.bottom() - gh, art.w, gh), 20.0, Fill::VGrad(Color::rgba(0, 0, 0, 0.0), Color::rgba(0, 0, 0, 0.35)));
            }
            None => {
                let (c1, c2) = art_colors(&t.title);
                c.s.rect(art, 20.0, Fill::DGrad(c1, c2));
                c.s.rect(Rect::new(art.x, art.y, art.w, art.h * 0.55), 20.0, Fill::VGrad(pal().ink(0.22), pal().ink(0.0)));
                icons::draw(&mut c.s, Icon::Note, art.cx(), art.cy() - 6.0, 36.0, WHITE.fade(0.92));
            }
        }
        // Visualizer along the bottom of the tile.
        let n = 12;
        let (bw, gap) = (4.0, 2.5);
        let x0 = art.cx() - (n as f32 * bw + (n - 1) as f32 * gap) / 2.0;
        for (i, b) in c.ui.viz.sample(n).iter().enumerate() {
            let h = 3.0 + b * 24.0;
            c.s.rect(Rect::new(x0 + i as f32 * (bw + gap), art.bottom() - 10.0 - h, bw, h), 2.0, WHITE.fade(0.85));
        }
    } else {
        c.s.rect(art, 20.0, pal().surface);
        icons::draw(&mut c.s, Icon::Note, art.cx(), art.cy(), 36.0, pal().text_3);
    }

    // Track info to the right of album art
    let tx = art.right() + 14.0;
    let tw = (a.x + left_w - tx).max(0.0);
    let title = if has { t.title.clone() } else { "Play something".to_string() };
    c.text(title, Rect::new(tx, a.y + 28.0, tw, 24.0), 17.0, Weight::Semibold, if has { pal().text } else { pal().text_2 }, Align::Left);
    let sub = if !has {
        "Nothing playing".to_string()
    } else if t.album.is_empty() {
        t.display_artist().to_string()
    } else {
        format!("{} \u{00b7} {}", t.display_artist(), t.album)
    };
    c.text(sub, Rect::new(tx, a.y + 54.0, tw, 18.0), 12.0, Weight::Regular, pal().text_2, Align::Left);
    c.text(app_name, Rect::new(tx, a.y + 78.0, tw, 16.0), 11.0, Weight::Semibold, pal().text_3, Align::Left);

    // Seek bar
    let sy = a.y + 126.0;
    let pos_str = fmt_ms(t.position_ms);
    let rem_ms = t.duration_ms.saturating_sub(t.position_ms);
    let rem_str = if has && t.duration_ms > 0 { format!("-{}", fmt_ms(rem_ms)) } else { fmt_ms(t.duration_ms) };

    c.text(pos_str, Rect::new(a.x, sy - 8.0, 36.0, 16.0), 11.0, Weight::Medium, pal().text_3, Align::Left);
    c.text(rem_str, Rect::new(a.x + left_w - 42.0, sy - 8.0, 42.0, 16.0), 11.0, Weight::Medium, pal().text_3, Align::Right);

    let bar_x = a.x + 42.0;
    let bar_w = (left_w - 90.0).max(10.0);
    let bar = Rect::new(bar_x, sy - 2.5, bar_w, 5.0);
    let seekable = has && t.duration_ms > 0;
    let sh = if seekable { c.hov(Action::SeekBar) } else { 0.0 };
    let bh = 5.0 + sh * 3.0;
    let p = t.progress();
    // Seek bar as a live visualizer: each bar's height rides the music,
    // accent up to the play position and dim beyond it.
    let n = (bar.w / 7.0).floor().clamp(12.0, 40.0) as usize;
    let bw = bar.w / n as f32;
    let bars = c.ui.viz.sample(n);
    for (i, b) in bars.iter().enumerate() {
        let bx = bar.x + i as f32 * bw;
        let h = (3.0 + b * (13.0 + sh * 4.0)).max(bh);
        let played = (bx + bw * 0.5 - bar.x) / bar.w <= p;
        let col = if played { acc.mix(WHITE, 0.18) } else { pal().ink(0.16 + 0.10 * sh) };
        c.s.rect(Rect::new(bx + 1.0, sy - h * 0.5, (bw - 2.0).max(1.5), h), (bw - 2.0) * 0.4, col);
    }
    if p > 0.0 || sh > 0.01 {
        c.s.circle(bar.x + bar.w * p, sy, 5.0 + sh * 1.5, WHITE);
    }
    if seekable {
        c.hit(Rect::new(bar.x, sy - 12.0, bar.w, 24.0), Action::SeekBar);
    }

    // Transport buttons centered in the left column
    let cx = a.x + left_w * 0.5;
    let cy = a.y + 172.0;
    c.ghost_icon(Rect::new(cx - 76.0, cy - 20.0, 40.0, 40.0), Action::MediaPrev, Icon::Prev, 19.0, pal().text);
    c.ghost_icon(Rect::new(cx + 36.0, cy - 20.0, 40.0, 40.0), Action::MediaNext, Icon::Next, 19.0, pal().text);
    let pa = Action::MediaPlayPause;
    let r = 26.0 + c.hov(pa) * 1.5;
    let fill = if c.pressed(pa) { WHITE.mix(Color::rgb(0, 0, 0), 0.2) } else { WHITE };
    c.s.circle(cx, cy, r, fill);
    let icon = if t.playback == Playback::Playing { Icon::Pause } else { Icon::Play };
    let nudge = if icon == Icon::Play { 1.5 } else { 0.0 };
    icons::draw(&mut c.s, icon, cx + nudge, cy, 22.0, ON_ACCENT);
    c.hit(Rect::new(cx - 28.0, cy - 28.0, 56.0, 56.0), pa);

    // Right Column: Synced Karaoke Lyrics (auto-wrapped)
    let lx = a.x + left_w + 36.0;
    let lw = (a.right() - lx).max(0.0);

    if !c.set.lyrics_enabled {
        c.text("Lyrics are turned off", Rect::new(lx, a.y + 90.0, lw, 24.0), 13.5, Weight::Regular, pal().text_3, Align::Left);
    } else if let (true, Some(i)) = (has, vm.lyric_idx) {
        let curr_line = vm.lyric_lines.get(i);
        let curr_sublines = curr_line
            .map(|l| wrap_lyrics(&l.text, lw, 17.0, true))
            .unwrap_or_default();
        let curr_count = curr_sublines.len().max(1);

        let prev_line = if i > 0 { vm.lyric_lines.get(i - 1) } else { None };
        let prev_sublines = prev_line
            .map(|l| wrap_lyrics(&l.text, lw, 13.0, false))
            .unwrap_or_default();
        let prev_count = prev_sublines.len().min(2);

        let next_line = vm.lyric_lines.get(i + 1);
        let next_sublines = next_line
            .map(|l| wrap_lyrics(&l.text, lw, 13.0, false))
            .unwrap_or_default();

        let prev_lh = 19.0;
        let curr_lh = 26.0;
        let next_lh = 19.0;

        let curr_y = if curr_count >= 3 {
            a.y + 64.0
        } else if prev_count >= 2 {
            a.y + 86.0
        } else {
            a.y + 80.0
        };

        // Line 1: Previous line (dimmed)
        if prev_count > 0 {
            let prev_start = prev_sublines.len().saturating_sub(prev_count);
            let prev_total_h = prev_count as f32 * prev_lh;
            let mut py = curr_y - 12.0 - prev_total_h;
            for line in &prev_sublines[prev_start..] {
                c.text(line.clone(), Rect::new(lx, py, lw, prev_lh), 13.0, Weight::Regular, pal().text_3, Align::Left);
                py += prev_lh;
            }
        }

        // Line 2: Current active line (wrapped karaoke highlight)
        if let Some(_curr) = curr_line {
            let frac = teardrop_core::line_progress(&vm.lyric_lines, i, t.position_ms);
            let sweeps = karaoke_subline_progress(&curr_sublines, frac);
            let mut cy = curr_y;
            for (subline, sub_frac) in sweeps {
                c.s.text_karaoke(
                    subline.to_string(),
                    Rect::new(lx, cy, lw, curr_lh),
                    17.0,
                    Weight::Bold,
                    pal().text_2,
                    WHITE,
                    sub_frac,
                );
                cy += curr_lh;
            }

            // Line 3: Next line (dimmed)
            let mut ny = cy + 14.0;
            for line in &next_sublines {
                if ny + next_lh > a.bottom() - 10.0 {
                    break;
                }
                c.text(line.clone(), Rect::new(lx, ny, lw, next_lh), 13.0, Weight::Regular, pal().text_3, Align::Left);
                ny += next_lh;
            }
        }
    } else {
        let msg = if has { "No synced lyrics for this track" } else { "Lyrics appear here while music plays" };
        c.text(msg, Rect::new(lx, a.y + 90.0, lw, 24.0), 13.5, Weight::Regular, pal().text_3, Align::Left);
    }
}

/// Wrap lyric line into visual lines that fit `max_w` pixels.
/// Supports whitespace-separated words as well as CJK / long unbroken words by splitting at characters when needed.
pub fn wrap_lyrics(text: &str, max_w: f32, size: f32, bold: bool) -> Vec<String> {
    let factor = if bold { 1.20 } else { 1.14 };
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut cur = String::new();

    let push_word = |out: &mut Vec<String>, cur: &mut String, word: &str| {
        let candidate = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };

        if est_w(&candidate, size) * factor <= max_w {
            *cur = candidate;
        } else if !cur.is_empty() {
            out.push(std::mem::take(cur));
            if est_w(word, size) * factor <= max_w {
                *cur = word.to_string();
            } else {
                let mut sub = String::new();
                for ch in word.chars() {
                    let mut next = sub.clone();
                    next.push(ch);
                    if est_w(&next, size) * factor > max_w && !sub.is_empty() {
                        out.push(sub);
                        sub = ch.to_string();
                    } else {
                        sub = next;
                    }
                }
                *cur = sub;
            }
        } else {
            let mut sub = String::new();
            for ch in word.chars() {
                let mut next = sub.clone();
                next.push(ch);
                if est_w(&next, size) * factor > max_w && !sub.is_empty() {
                    out.push(sub);
                    sub = ch.to_string();
                } else {
                    sub = next;
                }
            }
            *cur = sub;
        }
    };

    for word in text.split_whitespace() {
        push_word(&mut out, &mut cur, word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }

    if out.is_empty() {
        out.push(text.to_string());
    }

    out
}

/// Compute subline sweep progress fractions (0.0..=1.0) across wrapped lines.
pub fn karaoke_subline_progress<'a>(lines: &'a [String], frac: f32) -> Vec<(&'a str, f32)> {
    if lines.is_empty() {
        return Vec::new();
    }
    if lines.len() == 1 {
        return vec![(&lines[0], frac.clamp(0.0, 1.0))];
    }

    let weights: Vec<f32> = lines
        .iter()
        .map(|l| (l.chars().count() as f32).max(1.0))
        .collect();
    let total_weight: f32 = weights.iter().sum();

    let mut result = Vec::with_capacity(lines.len());
    let mut accum = 0.0;
    let frac = frac.clamp(0.0, 1.0);

    for (line, &w) in lines.iter().zip(weights.iter()) {
        let start = accum / total_weight;
        let end = (accum + w) / total_weight;
        let line_frac = if frac <= start {
            0.0
        } else if frac >= end {
            1.0
        } else if end > start {
            ((frac - start) / (end - start)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        result.push((line.as_str(), line_frac));
        accum += w;
    }

    result
}

// -------------------------------------------------------------------- focus

fn focus(c: &mut Ctx, a: Rect) {
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
    c.text(format!("{} / {}", teardrop_core::fmt_glasses(today), teardrop_core::glasses_label(goal)), Rect::new(ctr.0 - 60.0, ctr.1 + 10.0, 120.0, 16.0), 10.5, Weight::Medium, WHITE.fade(0.85), Align::Center);

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
        c.text(letter, Rect::new(bx - 3.0, base + 3.0, bw + 6.0, 13.0), 10.0, if is_today { Weight::Semibold } else { Weight::Regular }, if is_today { pal().text } else { pal().text_3 }, Align::Center);
    }
}

// -------------------------------------------------------------------- shelf

fn ext_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() && e.len() <= 4 => e.to_uppercase(),
        _ => String::new(),
    }
}

pub fn parent_of(path: &str) -> String {
    let p = path.replace('/', "\\");
    let mut parts: Vec<&str> = p.split('\\').collect();
    parts.pop();
    parts.last().copied().unwrap_or("").to_string()
}

const SHELF_COLS: usize = 2;
const SHELF_ROWS: usize = 3;

pub fn windows_max_scroll(n: usize) -> usize {
    n.saturating_sub(8)
}

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
            c.text(title, Rect::new(z.x, zy + 30.0, z.w, 20.0), 15.0, Weight::Semibold, pal().text.fade(k), Align::Center);
            let line = if hot > 0.5 && n > 1 { format!("{n} files") } else { sub.to_string() };
            c.text(line, Rect::new(z.x, zy + 50.0, z.w, 16.0), 11.5, Weight::Regular, pal().text_2.fade(k), Align::Center);
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
            c.text("Opening share window\u{2026}", Rect::new(a.x, cy + 40.0, a.w, 18.0), 12.5, Weight::Medium, pal().text.fade(u), Align::Center);
        }
    }
}

fn shelf_list(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Shelf);
    let vm = c.vm;
    let n = vm.shelf.items.len();
    let sub = if n == 0 { String::new() } else { format!("{n} file{}", if n == 1 { "" } else { "s" }) };
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text("SHELF \u{00b7} DRAG FILES HERE", Rect::new(a.x + 12.0, a.y, a.w - 140.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    c.text(sub, Rect::new(a.right() - 120.0, a.y, 120.0, 15.0), 11.0, Weight::Regular, pal().text_3, Align::Right);

    let body = Rect::new(a.x, a.y + 22.0, a.w, a.h - 22.0);
    if n == 0 {
        c.s.dashed(body.inset(2.0), 16.0, 1.5, pal().ink(0.16));
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
        let f = if c.pressed(open) { pal().surface_press } else { pal().surface.mix(pal().surface_hover, hv) };
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
        c.text(item.name.clone(), Rect::new(tx, r.y + 8.0, tr_w, 20.0), 13.0, Weight::Medium, pal().text, Align::Left);
        c.text(parent_of(&item.path), Rect::new(tx, r.y + 27.0, tr_w, 18.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
        if hv > 0.05 {
            c.s.set_transform(0.0, 0.0, hv.min(1.0) * 1.0);
            c.ghost_icon(Rect::new(r.right() - 54.0, r.cy() - 12.0, 24.0, 24.0), reveal, Icon::Folder, 13.0, pal().text_2);
            c.ghost_icon(Rect::new(r.right() - 29.0, r.cy() - 12.0, 24.0, 24.0), remove, Icon::Close, 12.0, pal().text_2);
            c.s.set_transform(0.0, 0.0, 1.0);
        }
    }
    if shelf_max_scroll(n) > 0 {
        let tot = shelf_max_scroll(n) as f32 + SHELF_ROWS as f32;
        let track_h = SHELF_ROWS as f32 * (th + 8.0) - 8.0;
        let th_h = track_h * SHELF_ROWS as f32 / tot;
        let ty = body.y + (track_h - th_h) * scroll as f32 / shelf_max_scroll(n) as f32;
        c.s.rect(Rect::new(a.right() + 8.0, ty, 3.0, th_h), 1.5, pal().ink(0.25));
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

    let n = vm.tasks.tasks.len();
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text(format!("TASKS \u{00b7} {} OPEN", vm.tasks.open_count()), Rect::new(a.x + 12.0, a.y, a.w - 20.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);

    // Add field.
    let field = Rect::new(a.x, a.y + 20.0, a.w, 40.0);
    let typing = c.ui.input.is_some();
    c.s.rect(field, 14.0, if typing { pal().surface_hover } else { c.surface_fill(Action::TaskInput) });
    if typing {
        c.s.stroke(field, 14.0, 1.5, acc.fade(0.85));
    }
    icons::draw(&mut c.s, Icon::Plus, field.x + 22.0, field.cy(), 15.0, if typing { acc } else { pal().text_3 });
    let tr = Rect::new(field.x + 42.0, field.y, field.w - 54.0, field.h);
    match &c.ui.input {
        Some(f) if !f.is_empty() => {
            let ed = field_edit(c, f, acc);
            c.s.text_edit(f.as_str().to_string(), tr, 14.0, Weight::Regular, pal().text, ed);
        }
        Some(_) => {
            c.text("What needs doing?", tr, 14.0, Weight::Regular, pal().text_3, Align::Left);
            if (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
                c.s.rect(Rect::new(tr.x, field.cy() - 9.0, 1.6, 18.0), 0.8, acc);
            }
        }
        None => c.text("Add a task", tr, 14.0, Weight::Regular, pal().text_3, Align::Left),
    }
    c.hit_text(field, Action::TaskInput);

    let list_y = a.y + 70.0;
    if n == 0 {
        c.empty_state(Rect::new(a.x, list_y, a.w, a.h - 70.0), Icon::Check, acc, "All clear", "Add something above, or press A");
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
            c.s.rect(r, 10.0, pal().surface.fade(hv));
        }
        let cb = (r.x + 18.0, r.cy());
        if t.done {
            c.s.circle(cb.0, cb.1, 9.5, acc);
            icons::draw(&mut c.s, Icon::Check, cb.0, cb.1, 11.0, ON_ACCENT);
        } else {
            c.s.ring(cb.0, cb.1, 8.5, 1.6, 0.0, 1.0, pal().text_3.mix(acc, c.hov(tog)));
        }
        let tx = r.x + 38.0;
        let right = if hv > 0.05 { 34.0 } else { 10.0 };
        let tr = Rect::new(tx, r.y, r.w - 38.0 - right, r.h);
        let col = if t.done { pal().text_3 } else { pal().text };
        c.text(t.title.clone(), tr, 14.0, Weight::Regular, col, Align::Left);
        if t.done {
            let wd = est_w(&t.title, 14.0).min(tr.w);
            c.s.line(tx, r.cy() + 0.5, tx + wd, r.cy() + 0.5, 1.3, pal().text_3);
        }
        c.hit(Rect::new(r.x, r.y, r.w - 30.0, r.h), tog);
        if hv > 0.05 {
            c.ghost_icon(Rect::new(r.right() - 30.0, r.cy() - 12.0, 24.0, 24.0), del, Icon::Close, 12.0, pal().text_2);
        }
    }
    let open = vm.tasks.open_count();
    let done = n - open;
    c.text(format!("{open} open"), Rect::new(a.x + 2.0, a.bottom() - 20.0, 100.0, 18.0), 11.5, Weight::Medium, pal().text_3, Align::Left);
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

/// Greedy word wrap that keeps explicit newlines (blank lines stay blank),
/// returning the char range each visual line covers in `text` — the ranges
/// used to map caret/selection char indices onto lines.
pub fn wrap_note_ranges(text: &str, width: f32, size: f32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut base = 0usize; // char index where this logical line starts
    for raw in text.split('\n') {
        let raw_len = raw.chars().count();
        if raw.is_empty() {
            out.push((base, base));
        } else {
            let mut line_txt = String::new();
            let mut start: Option<usize> = None;
            let mut end = base;
            let mut wpos = base;
            for word in raw.split(' ') {
                let wlen = word.chars().count();
                let candidate = if line_txt.is_empty() { word.to_string() } else { format!("{line_txt} {word}") };
                if !line_txt.is_empty() && est_w(&candidate, size) > width {
                    out.push((start.unwrap_or(base), end));
                    line_txt = word.to_string();
                    start = Some(wpos);
                } else {
                    if start.is_none() {
                        start = Some(wpos);
                    }
                    line_txt = candidate;
                }
                end = wpos + wlen;
                wpos += wlen + 1; // past the separating space
            }
            out.push((start.unwrap_or(base), end));
        }
        base += raw_len + 1;
    }
    if out.is_empty() {
        out.push((0, 0));
    }
    out
}

/// Slice the visual lines produced by [`wrap_note_ranges`].
fn note_lines_from_ranges(text: &str, ranges: &[(usize, usize)]) -> Vec<String> {
    let ch: Vec<char> = text.chars().collect();
    ranges.iter().map(|&(a, b)| ch[a..b].iter().collect()).collect()
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
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text(format!("NOTES \u{00b7} {}", c.vm.notes.notes.len()), Rect::new(a.x + 12.0, a.y, 200.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    let b = Rect::new(a.x, a.y + 20.0, a.w, a.h - 20.0);
    if c.ui.input.is_some() {
        note_editor(c, b, acc);
    } else {
        note_list(c, b, acc);
    }
}

fn note_list(c: &mut Ctx, a: Rect, acc: Color) {
    let vm = c.vm;
    let new_w = 110.0;
    let filter_w = 230.0;
    let search_w = (a.w - new_w - filter_w - 20.0).max(180.0);

    // Top Bar: [ + New Note ]  [ 🔍 Search... ]  [ Date Filters ]
    let new_btn = Rect::new(a.x, a.y, new_w, 32.0);
    c.s.rect(new_btn, 10.0, acc.fade(0.18));
    icons::draw(&mut c.s, Icon::Plus, new_btn.x + 16.0, new_btn.cy(), 13.0, acc);
    c.text("New note", Rect::new(new_btn.x + 28.0, new_btn.y, new_btn.w - 32.0, new_btn.h), 12.5, Weight::Semibold, acc, Align::Left);
    c.hit(new_btn, Action::NoteNew);

    let search_rect = Rect::new(new_btn.right() + 10.0, a.y, search_w, 32.0);
    let searching = c.ui.note_search.is_some();
    let query = c.ui.note_search.as_ref().map(|f| f.as_str()).unwrap_or("");
    c.s.rect(search_rect, 10.0, if searching { pal().surface_hover } else { pal().surface });
    if searching {
        c.s.stroke(search_rect, 10.0, 1.2, acc.fade(0.85));
    }
    icons::draw(&mut c.s, Icon::Search, search_rect.x + 15.0, search_rect.cy(), 12.0, if searching { acc } else { pal().text_3 });
    let tr = Rect::new(search_rect.x + 32.0, search_rect.y, search_rect.w - 56.0, search_rect.h);
    if !query.is_empty() {
        if let Some(f) = c.ui.note_search.as_ref() {
            let ed = field_edit(c, f, acc);
            c.s.text_edit(f.as_str().to_string(), tr, 13.0, Weight::Regular, pal().text, ed);
        } else {
            c.text(query.to_string(), tr, 13.0, Weight::Regular, pal().text, Align::Left);
        }
        // Clear search button
        let clr_btn = Rect::new(search_rect.right() - 26.0, search_rect.cy() - 10.0, 20.0, 20.0);
        c.ghost_icon(clr_btn, Action::NoteSearchClear, Icon::Close, 11.0, pal().text_3);
    } else if searching {
        c.text("Search notes...", tr, 13.0, Weight::Regular, pal().text_3, Align::Left);
        if (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
            c.s.rect(Rect::new(tr.x, search_rect.cy() - 8.0, 1.6, 16.0), 0.8, acc);
        }
    } else {
        c.text("Search notes...", tr, 13.0, Weight::Regular, pal().text_3, Align::Left);
    }
    c.hit_text(search_rect, Action::NoteSearchInput);

    // Date filter segmented pills
    let filter_rect = Rect::new(a.right() - filter_w, a.y, filter_w, 32.0);
    let date_filter = c.ui.note_date_filter;
    let filters = NoteDateFilter::ALL.map(|f| (Action::NoteDateFilter(f), f.label(), f == date_filter));
    segmented(c, filter_rect, &filters, 11.0);

    // Filter and score notes
    let total_notes = vm.notes.notes.len();
    let mut matched: Vec<(usize, &teardrop_core::Note, i32)> = Vec::new();
    for (idx, note) in vm.notes.notes.iter().enumerate() {
        if !date_filter.matches(note.updated_at, vm.now) {
            continue;
        }
        if !query.is_empty() {
            if let Some(score) = note.match_query(query) {
                matched.push((idx, note, score));
            }
        } else {
            matched.push((idx, note, 0));
        }
    }
    if !query.is_empty() {
        matched.sort_by(|a, b| b.2.cmp(&a.2));
    }

    let n = matched.len();
    let list_y = a.y + 42.0;
    if n == 0 {
        let (title, sub) = if total_notes == 0 {
            ("No notes yet", "Click 'New note' above or press N")
        } else if !query.is_empty() {
            ("No matching notes", "Try a different search keyword or clear the search")
        } else {
            ("No notes in this date range", "Switch date filter to 'All' or write a new note")
        };
        c.empty_state(Rect::new(a.x, list_y, a.w, a.h - 60.0), Icon::Notes, acc, title, sub);
        return;
    }

    let scroll = c.ui.scroll_of(Page::Notes).min(notes_max_scroll(n));
    for slot in 0..NOTE_ROWS {
        let match_idx = scroll + slot;
        let Some(&(orig_idx, note, _)) = matched.get(match_idx) else { break };
        let r = Rect::new(a.x, list_y + slot as f32 * NOTE_ROW_H, a.w, NOTE_ROW_H - 4.0);
        let open = Action::NoteOpen(orig_idx as u16);
        let del = Action::NoteRemove(orig_idx as u16);
        let hv = c.hov(open).max(c.hov(del));
        if hv > 0.01 {
            c.s.rect(r, 10.0, pal().surface.fade(hv));
        }
        let title = teardrop_core::title_of(&note.text);
        let title = if title.is_empty() { "Untitled".to_string() } else { title };
        let preview = teardrop_core::preview_of(&note.text);

        // Date layout on right: Full Date (e.g. "Today, 14:30") + Age ("15m")
        let full_date = format_note_date(note.updated_at, vm.now);
        let right_reserved = if hv > 0.05 { 190.0 } else { 160.0 };

        c.text(
            one_line(&title),
            Rect::new(r.x + 12.0, r.y + 3.0, (r.w - right_reserved - 16.0).max(60.0), 18.0),
            13.5,
            Weight::Semibold,
            pal().text,
            Align::Left,
        );
        if !preview.is_empty() {
            c.text(
                one_line(&preview),
                Rect::new(r.x + 12.0, r.y + 20.0, (r.w - right_reserved - 16.0).max(60.0), 14.0),
                11.0,
                Weight::Regular,
                pal().text_3,
                Align::Left,
            );
        }

        // Full formatted date badge
        let date_w = 110.0;
        let date_x = r.right() - (if hv > 0.05 { 40.0 + date_w } else { 12.0 + date_w });
        c.text(
            full_date,
            Rect::new(date_x, r.y + 4.0, date_w, 15.0),
            11.0,
            Weight::Medium,
            pal().text_2,
            Align::Right,
        );
        // Relative age tag underneath
        c.text(
            note_age(note.updated_at, vm.now),
            Rect::new(date_x, r.y + 19.0, date_w, 13.0),
            10.0,
            Weight::Regular,
            pal().text_3,
            Align::Right,
        );

        c.hit(Rect::new(r.x, r.y, r.w - 36.0, r.h), open);
        if hv > 0.05 {
            c.ghost_icon(Rect::new(r.right() - 32.0, r.cy() - 12.0, 24.0, 24.0), del, Icon::Close, 12.0, pal().text_2);
        }
    }

    let count_text = if n == total_notes {
        if n == 1 { "1 note".to_string() } else { format!("{n} notes") }
    } else {
        format!("{n} of {total_notes} notes ({})", date_filter.label())
    };
    c.text(count_text, Rect::new(a.x + 2.0, a.bottom() - 18.0, 200.0, 16.0), 11.0, Weight::Medium, pal().text_3, Align::Left);
    c.text(
        "N new note \u{00b7} Click search or / to filter \u{00b7} Esc clear",
        Rect::new(a.right() - 320.0, a.bottom() - 18.0, 320.0, 16.0),
        10.5,
        Weight::Regular,
        pal().text_3,
        Align::Right,
    );
}

fn note_editor(c: &mut Ctx, a: Rect, acc: Color) {
    let field = c.ui.input.clone().unwrap_or_default();
    let text = field.as_str().to_string();
    let idx = c.ui.note_edit.as_ref().and_then(|id| c.vm.notes.notes.iter().position(|n| &n.id == id));
    let selected_all = field.all_selected();
    let sel = field.selection();
    let caret = field.caret();

    c.text("Note Editor", Rect::new(a.x, a.y + 2.0, 100.0, 24.0), 13.0, Weight::Semibold, pal().text, Align::Left);

    // Block selection pill action
    let sel_label = if selected_all { "Deselect" } else { "Select All" };
    let sel_fill = if selected_all { Some(acc) } else { None };
    c.pill(Rect::new(a.right() - 212.0, a.y, 74.0, 24.0), Action::NoteSelectAll, sel_label, sel_fill, 11.0);

    if let Some(i) = idx {
        c.pill(Rect::new(a.right() - 132.0, a.y, 64.0, 24.0), Action::NoteRemove(i as u16), "Delete", None, 11.0);
    }
    c.pill(Rect::new(a.right() - 62.0, a.y, 62.0, 24.0), Action::NoteClose, "Done", Some(acc), 11.0);

    let body = Rect::new(a.x, a.y + 32.0, a.w, a.h - 60.0);
    c.s.rect(body, 14.0, pal().surface);
    if selected_all {
        c.s.stroke(body, 14.0, 1.5, acc.fade(0.85));
    }

    let inner_w = body.w - 24.0;
    let line_h = 19.0;
    let ranges = wrap_note_ranges(&text, inner_w, 13.5);
    let lines = note_lines_from_ranges(&text, &ranges);
    let visible = (((body.h - 20.0) / line_h).floor() as usize).max(1);
    // Scroll so the caret's visual line stays in view (tail when at the end).
    let caret_line = ranges.iter().position(|&(la, lb)| caret >= la && caret <= lb).unwrap_or(ranges.len() - 1);
    let mut start = ranges.len().saturating_sub(visible);
    if caret_line < start {
        start = caret_line;
    }
    let blink = (c.ui.clock_s * 1.8) % 2.0 < 1.0;
    let mut y = body.y + 10.0;
    for (line, &(la, lb)) in lines.iter().zip(ranges.iter()).skip(start) {
        let line: String = line.clone();
        let local_sel = sel.and_then(|(sa, sb)| {
            let (a0, b0) = (sa.max(la), sb.min(lb));
            (b0 > a0).then(|| (a0 - la, b0 - la))
        });
        let on_caret_line = caret >= la && caret <= lb;
        let local_caret = if on_caret_line { caret - la } else { 0 };
        if local_sel.is_some() || on_caret_line {
            c.s.text_edit(
                line,
                Rect::new(body.x + 12.0, y, inner_w, line_h),
                13.5,
                Weight::Regular,
                pal().text,
                TextEdit { caret: local_caret, show_caret: blink && on_caret_line, sel: local_sel, caret_color: acc, sel_color: acc.fade(0.35) },
            );
        } else if !line.is_empty() {
            c.text(line, Rect::new(body.x + 12.0, y, inner_w, line_h), 13.5, Weight::Regular, pal().text, Align::Left);
        }
        y += line_h;
        if y > body.bottom() - 4.0 {
            break;
        }
    }
    if text.is_empty() {
        c.text(
            "Start typing \u{2014} Enter starts a new line",
            Rect::new(body.x + 12.0, body.y + 10.0, inner_w, line_h),
            13.5,
            Weight::Regular,
            pal().text_3,
            Align::Left,
        );
        if blink {
            c.s.rect(Rect::new(body.x + 12.0, body.y + 12.0, 1.6, 16.0), 0.8, acc);
        }
    }
    let hint = if sel.is_some() {
        "Selection \u{00b7} Ctrl+C copy \u{00b7} Del clears \u{00b7} Type replaces \u{00b7} Esc collapses"
    } else {
        "Enter adds a line \u{00b7} Ctrl+A select all \u{00b7} Esc when done"
    };
    c.text(hint, Rect::new(a.x + 2.0, a.bottom() - 18.0, a.w, 16.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
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
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text("CLIPBOARD", Rect::new(a.x + 12.0, a.y, 200.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    if !items.is_empty() {
        c.pill(Rect::new(a.right() - 60.0, a.y - 4.0, 60.0, 22.0), Action::ClipClear, "Clear", None, 10.5);
    }
    let body = Rect::new(a.x, a.y + 22.0, a.w, a.h - 22.0);
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
        let r = Rect::new(a.x, body.y + slot as f32 * 36.0, a.w, 32.0);
        let act = Action::ClipCopy(idx as u16);
        let hv = c.hov(act);
        c.s.rect(r, 11.0, if c.pressed(act) { pal().surface_press } else { pal().surface.mix(pal().surface_hover, hv) });
        if idx == 0 {
            c.s.circle(r.x + 14.0, r.cy(), 3.0, acc);
        }
        let tr = Rect::new(r.x + 28.0, r.y, r.w - 28.0 - 44.0, r.h);
        c.text(one_line(s), tr, 13.0, Weight::Regular, pal().text.fade(0.92), Align::Left);
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

    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text(format!("TODAY \u{00b7} {}", date.format("%a %d %b").to_string().to_uppercase()), Rect::new(a.x + 12.0, a.y, 220.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);

    // Month grid (Monday first), shifted by the browsing offset.
    let off_m = c.ui.cal_offset;
    let (shown_y, shown_m) = shown_year_month(date, off_m);
    let cal = Rect::new(a.x, a.y + 20.0, 214.0, a.h - 20.0);
    let has_filter = off_m != 0 || c.ui.cal_selected_day.is_some() || c.ui.cal_selected_event.is_some();
    let title = NaiveDate::from_ymd_opt(shown_y, shown_m, 1).unwrap().format("%B %Y").to_string();
    let title_r = Rect::new(cal.x, cal.y, cal.w - 56.0, 24.0);
    c.text(title, title_r, 14.0, Weight::Semibold, if off_m == 0 { pal().text } else { acc }, Align::Left);
    if has_filter {
        c.hit(title_r, Action::CalToday);
    }
    c.icon_btn(Rect::new(cal.right() - 52.0, cal.y + 1.0, 24.0, 22.0), Action::CalStep(-1), Icon::Prev, 10.0, pal().text_2);
    c.icon_btn(Rect::new(cal.right() - 24.0, cal.y + 1.0, 24.0, 22.0), Action::CalStep(1), Icon::Next, 10.0, pal().text_2);
    let cell_w = cal.w / 7.0;
    for (i, d) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
        c.text(*d, Rect::new(cal.x + i as f32 * cell_w, cal.y + 30.0, cell_w, 16.0), 10.5, Weight::Semibold, pal().text_3, Align::Center);
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
            c.s.circle(r.cx(), r.cy(), 11.5, pal().surface_hover.fade(hv));
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
            pal().text_3.mix(pal().text, hv * 0.5)
        } else {
            pal().text_2.mix(pal().text, hv * 0.6)
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
        c.text(sel_date.format("%A").to_string(), Rect::new(x, a.y, w - 68.0, 30.0), 21.0, Weight::Semibold, pal().text, Align::Left);
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
        c.text(date.format("%A").to_string(), Rect::new(x, a.y, w, 30.0), 23.0, Weight::Semibold, pal().text, Align::Left);
        c.text(date.format("%-d %B").to_string(), Rect::new(x, a.y + 30.0, w, 20.0), 14.0, Weight::Regular, acc, Align::Left);
    }
    agenda(c, Rect::new(x, a.y + 54.0, w, 150.0), &items, acc);
}

fn draw_today_sys_line(c: &mut Ctx, x: f32, y: f32, w: f32) {
    let sys = &c.vm.system;
    let mut line = format!("CPU {:.0}%  \u{00b7}  RAM {:.0}%  \u{00b7}  Disk {:.0}%", sys.cpu_pct, sys.mem_pct(), sys.disk_pct());
    if let Some(b) = sys.battery_pct {
        line.push_str(&format!("  \u{00b7}  {b}%{}", if sys.charging { "\u{26a1}" } else { "" }));
    }
    c.text(line, Rect::new(x, y, w, 16.0), 9.5, Weight::Regular, pal().text_3, Align::Left);
}

/// Upcoming meetings or selected day's events, or the prompt to connect a calendar.
fn agenda(c: &mut Ctx, r: Rect, items: &[&crate::calendar::Event], acc: Color) {
    use crate::calendar::{clock_label, countdown, day_label, time_range_label, FeedState};
    let vm = c.vm;
    let now = vm.now.timestamp();
    if vm.calendar_feeds == 0 {
        c.text("Never miss a meeting", Rect::new(r.x, r.y + 2.0, r.w, 20.0), 13.5, Weight::Semibold, pal().text, Align::Left);
        for (i, l) in ["Paste an iCal link from Google", "Calendar or Outlook and Droppy", "alerts you before each meeting."].iter().enumerate() {
            c.text(*l, Rect::new(r.x, r.y + 26.0 + i as f32 * 15.0, r.w, 15.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
        }
        c.pill(Rect::new(r.x, r.y + 82.0, 150.0, 26.0), Action::CalendarPaste, "Paste calendar link", Some(acc), 11.0);
        c.text("Copy the link first, then press it.", Rect::new(r.x, r.y + 114.0, r.w, 15.0), 10.0, Weight::Regular, pal().text_3, Align::Left);
        return;
    }
    let day_mode = c.ui.cal_selected_day.is_some();
    if items.is_empty() {
        let (msg, col) = match vm.calendar.state {
            FeedState::Loading | FeedState::Idle => ("Loading your calendar\u{2026}".to_string(), pal().text_3),
            FeedState::Failed => (vm.calendar.error.clone(), pal().amber),
            FeedState::Ok if day_mode => ("No events on this day".to_string(), pal().text_3),
            FeedState::Ok => ("No upcoming meetings".to_string(), pal().text_3),
        };
        c.text(msg, Rect::new(r.x, r.y + 6.0, r.w, 18.0), 12.0, Weight::Medium, col, Align::Left);
        if day_mode && vm.calendar.state == FeedState::Ok {
            c.text("Click another date on the calendar or press Upcoming.", Rect::new(r.x, r.y + 26.0, r.w, 15.0), 10.0, Weight::Regular, pal().text_3, Align::Left);
        }
    }
    let scroll = c.ui.scroll_of(Page::Today).min(today_max_scroll(items.len()));
    for (slot, (i, ev)) in items.iter().enumerate().skip(scroll).take(3).enumerate() {
        let row = Rect::new(r.x, r.y + slot as f32 * 38.0, r.w, 35.0);
        let ev_act = Action::CalEvent(i as u8);
        let hv = c.ui.hover_amt(ev_act);
        c.s.rect(row, 9.0, pal().surface.mix(pal().surface_hover, hv * 0.85));
        if hv > 0.01 {
            c.s.stroke(row, 9.0, 1.0, acc.fade(0.35 * hv));
        }
        if i == 0 {
            c.s.rect(Rect::new(row.x, row.y + 7.0, 3.0, row.h - 14.0), 1.5, acc);
        }
        let join = ev.join.is_some();
        let tw = row.w - 20.0 - if join { 48.0 } else { 0.0 };
        c.text(ev.title.clone(), Rect::new(row.x + 12.0, row.y + 3.0, tw, 16.0), 12.0, Weight::Semibold, pal().text, Align::Left);
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
        c.text(format!("{when}{tail}"), Rect::new(row.x + 12.0, row.y + 19.0, tw, 13.0), 10.0, Weight::Regular, pal().text_3, Align::Left);
        let hit_w = if join { row.w - 48.0 } else { row.w };
        c.hit(Rect::new(row.x, row.y, hit_w, row.h), ev_act);
        if join {
            c.pill(Rect::new(row.right() - 46.0, row.cy() - 10.0, 40.0, 20.0), Action::Join(i as u8), "Join", Some(acc), 10.0);
        }
    }
    let fy = r.y + 3.0 * 38.0 + 4.0;
    c.pill(Rect::new(r.x, fy, 66.0, 22.0), Action::CalendarPaste, "Add link", None, 10.5);
    c.pill(Rect::new(r.x + 72.0, fy, 66.0, 22.0), Action::CalendarClear, "Remove", None, 10.5);
    if items.len() > 3 {
        c.text(
            format!("{}\u{2013}{} of {}", scroll + 1, (scroll + 3).min(items.len()), items.len()),
            Rect::new(r.x + 146.0, fy + 3.0, r.w - 146.0, 16.0),
            10.0,
            Weight::Regular,
            pal().text_3,
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
    c.s.rect(card, 12.0, pal().surface);
    c.s.rect(Rect::new(card.x, card.y + 10.0, 3.0, card.h - 20.0), 1.5, acc);

    let ix = card.x + 12.0;
    let iw = card.w - 24.0;
    let mut y = card.y + 8.0;

    for line in wrap_lines(&ev.title, iw, 13.0).into_iter().take(2) {
        c.text(line, Rect::new(ix, y, iw, 17.0), 13.0, Weight::Semibold, pal().text, Align::Left);
        y += 17.0;
    }
    y += 3.0;

    let when = format!("{}  \u{00b7}  {}", full_date_label(ev), time_range_label(ev));
    c.text(when, Rect::new(ix, y, iw, 15.0), 10.5, Weight::Medium, acc, Align::Left);
    y += 17.0;

    if !ev.location.is_empty() {
        c.text(format!("Location: {}", one_line(&ev.location)), Rect::new(ix, y, iw, 14.0), 10.0, Weight::Regular, pal().text_2, Align::Left);
        y += 15.0;
    }

    let bottom_limit = if ev.join.is_some() { card.bottom() - 34.0 } else { card.bottom() - 8.0 };
    let max_desc_lines = ((bottom_limit - y).max(0.0) / 13.5).floor() as usize;
    if max_desc_lines > 0 {
        if !ev.description.is_empty() {
            for line in wrap_lines(&ev.description, iw, 10.0).into_iter().take(max_desc_lines) {
                c.text(line, Rect::new(ix, y, iw, 13.5), 10.0, Weight::Regular, pal().text_3, Align::Left);
                y += 13.5;
            }
        } else if ev.location.is_empty() {
            c.text("No location or notes attached.", Rect::new(ix, y, iw, 14.0), 10.0, Weight::Regular, pal().text_3, Align::Left);
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

// ----------------------------------------------------------------- settings

// -------------------------------------------------------------------- usage

fn usage_bar(c: &mut Ctx, r: Rect, label: &str, w: Option<&crate::usage::Window>, now: i64, acc: Color) {
    c.text(label, Rect::new(r.x, r.y, r.w - 40.0, 14.0), 10.5, Weight::Medium, pal().text_3, Align::Left);
    let bar = Rect::new(r.x, r.y + 20.0, r.w, 6.0);
    c.s.rect(bar, 3.0, pal().ink(0.10));
    match w {
        Some(w) => {
            let used = w.used_now(now);
            let col = if used > 85.0 { pal().red } else if used > 65.0 { pal().amber } else { acc };
            c.text(format!("{used:.0}%"), Rect::new(r.right() - 40.0, r.y - 1.0, 40.0, 16.0), 12.0, Weight::Semibold, pal().text, Align::Right);
            if used > 0.0 {
                c.s.rect(Rect::new(bar.x, bar.y, (bar.w * used / 100.0).max(6.0), 6.0), 3.0, col);
            }
        }
        None => c.text("\u{2014}", Rect::new(r.right() - 40.0, r.y - 1.0, 40.0, 16.0), 12.0, Weight::Semibold, pal().text_3, Align::Right),
    }
}

fn limits_card(c: &mut Ctx, r: Rect, name: &str, l: &crate::usage::Limits, loaded: bool, acc: Color, now: i64) {
    c.s.rect(r, 12.0, pal().surface);
    let head = if l.plan.is_empty() { name.to_string() } else { format!("{name}  \u{00b7}  {}", l.plan) };
    c.text(head, Rect::new(r.x + 12.0, r.y + 7.0, r.w - 24.0, 18.0), 13.0, Weight::Semibold, pal().text, Align::Left);
    if !l.has_data() {
        let msg = if !loaded { "Reading\u{2026}" } else if l.note.is_empty() { "No data yet" } else { l.note.as_str() };
        c.text(msg, Rect::new(r.x + 12.0, r.y + 28.0, r.w - 24.0, 18.0), 11.5, Weight::Regular, pal().text_3, Align::Left);
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
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text("AI USAGE", Rect::new(a.x + 12.0, a.y, 200.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    if u.refreshing {
        c.pill(Rect::new(a.right() - 88.0, a.y - 4.0, 88.0, 22.0), Action::Island, "Updating\u{2026}", None, 10.5);
    } else {
        c.pill(Rect::new(a.right() - 88.0, a.y - 4.0, 88.0, 22.0), Action::UsageRefresh, "Refresh", None, 10.5);
    }
    let w = a.w;
    limits_card(c, Rect::new(a.x, a.y + 24.0, w, 58.0), "Claude Code", &u.claude, u.loaded, acc, now);
    limits_card(c, Rect::new(a.x, a.y + 88.0, w, 58.0), "Codex", &u.codex, u.loaded, acc, now);

    // OpenCode: spend from its local database (no plan limits to read).
    let r = Rect::new(a.x, a.y + 152.0, w, a.h - 152.0);
    c.s.rect(r, 12.0, pal().surface);
    let o = &u.opencode;
    c.text("OpenCode", Rect::new(r.x + 12.0, r.y + 7.0, 120.0, 18.0), 13.0, Weight::Semibold, pal().text, Align::Left);
    if !u.loaded || !o.note.is_empty() {
        let msg = if !u.loaded { "Reading\u{2026}" } else { o.note.as_str() };
        c.text(msg, Rect::new(r.x + 12.0, r.y + 28.0, r.w - 24.0, 18.0), 11.5, Weight::Regular, pal().text_3, Align::Left);
    } else if o.week_tokens == 0 {
        c.text("No OpenCode activity in the last 7 days", Rect::new(r.x + 12.0, r.y + 28.0, r.w - 24.0, 18.0), 11.5, Weight::Regular, pal().text_3, Align::Left);
    } else {
        let money = |v: f64| if v > 0.0 { format!(" \u{00b7} ${v:.2}") } else { String::new() };
        let line = format!("Today {} tokens{}    Week {} tokens{}", crate::usage::compact_count(o.today_tokens), money(o.today_cost), crate::usage::compact_count(o.week_tokens), money(o.week_cost));
        c.text(line, Rect::new(r.x + 12.0, r.y + 26.0, r.w - 24.0, 18.0), 12.0, Weight::Medium, pal().text, Align::Left);
        let top = o.top.iter().map(|s| format!("{} {}", s.label.rsplit('/').next().unwrap_or(&s.label), crate::usage::compact_count(s.tokens))).collect::<Vec<_>>().join("  \u{00b7}  ");
        if !top.is_empty() {
            c.text(top, Rect::new(r.x + 12.0, r.y + 45.0, r.w - 24.0, 16.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
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

    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, if rec { acc } else { pal().text_3 });
    c.text(if rec { "RECORDING" } else { "VOICE RECORDER" }, Rect::new(a.x + 12.0, a.y, 200.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);

    c.text(crate::voice::clock(v.elapsed_ms), Rect::new(a.x, a.y + 20.0, a.w, 34.0), 28.0, Weight::Semibold, if rec { pal().text } else { pal().text_3 }, Align::Center);

    // Waveform of the last few seconds, mirrored around the middle.
    let n = c.ui.voice_bars.len();
    let (bw, gap) = (5.0, 3.0);
    let x0 = cx - (n as f32 * (bw + gap) - gap) / 2.0;
    let mid = a.y + 78.0;
    for (i, b) in c.ui.voice_bars.iter().enumerate() {
        let h = 3.0 + b * 40.0;
        let fade = 0.35 + 0.65 * (i as f32 / n as f32);
        c.s.rect(Rect::new(x0 + i as f32 * (bw + gap), mid - h / 2.0, bw, h), 2.5, if rec { acc.fade(fade) } else { pal().ink(0.12) });
    }

    // Record / stop button.
    let (by, br) = (a.y + 132.0, 32.0);
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
        VoicePhase::Recording => ("Recording\u{2026} tap to stop".to_string(), pal().text_2),
        VoicePhase::Failed => (v.error.clone(), pal().amber),
        VoicePhase::Idle => match &v.saved {
            Some(name) => (format!("Saved to the shelf \u{00b7} {name}"), pal().text_2),
            None => ("Tap to record from your microphone".to_string(), pal().text_3),
        },
    };
    c.text(msg, Rect::new(a.x, a.y + 176.0, a.w, 18.0), 12.0, Weight::Medium, col, Align::Center);
    c.pill(Rect::new(a.right() - 100.0, a.bottom() - 30.0, 100.0, 22.0), Action::VoiceFolder, "Open folder", None, 10.5);
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
    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text("AI CHAT", Rect::new(a.x + 12.0, a.y, 200.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    let a = Rect::new(a.x, a.y + 20.0, a.w, a.h - 20.0);
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
    c.s.rect(badge_r, 12.0, pal().surface.mix(pal().surface_hover, bh));
    let ready = ai.configured || prov == AiProvider::Ollama || prov == AiProvider::OpenAiCompat;
    c.s.circle(badge_r.x + 12.0, badge_r.cy(), 3.8, if ready { pal().green } else { pal().amber });
    let head = format!("{} \u{00b7} {}", prov.label(), model);
    c.text(head, Rect::new(badge_r.x + 22.0, badge_r.y, badge_r.w - 28.0, badge_r.h), 11.5, Weight::Semibold, pal().text, Align::Left);
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
    c.s.rect(card, 12.0, pal().surface);
    match prov {
        AiProvider::ChatGpt => {
            c.text(
                "Sign in with ChatGPT \u{00b7} Codex Subscription OAuth",
                Rect::new(card.x + 12.0, card.y + 6.0, card.w - 24.0, 18.0),
                12.0,
                Weight::Semibold,
                pal().text,
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
                        pal().text_2,
                        Align::Left,
                    );
                    c.pill(Rect::new(card.right() - 66.0, card.y + 26.0, 58.0, 24.0), Action::AiChatGptSignIn, "Cancel", None, 11.0);
                }
            } else if ai.chatgpt_signed_in {
                c.s.circle(card.x + 17.0, card.y + 37.0, 3.8, pal().green);
                c.text(
                    "Connected (DPAPI OAuth token)",
                    Rect::new(card.x + 26.0, card.y + 27.0, card.w - 186.0, 20.0),
                    11.5,
                    Weight::Medium,
                    pal().text_2,
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
                    pal().text_3,
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
                pal().text,
                Align::Left,
            );
            let sub = if ai.configured {
                "Key saved (DPAPI) \u{00b7} Responses / Messages / Chat"
            } else {
                "Copy your OpenCode Go API key, then click Paste Key"
            };
            c.text(sub, Rect::new(card.x + 12.0, card.y + 28.0, card.w - 176.0, 20.0), 11.0, Weight::Regular, if ai.configured { pal().text_2 } else { pal().text_3 }, Align::Left);
            c.pill(Rect::new(card.right() - 158.0, card.y + 25.0, 74.0, 25.0), Action::AiPasteKey, "Paste Key", (!ai.configured).then_some(acc), 11.0);
            c.pill(Rect::new(card.right() - 78.0, card.y + 25.0, 70.0, 25.0), Action::AiRefreshModels, "Sync models", None, 10.5);
        }
        AiProvider::OpenAiCompat => {
            let url_box = Rect::new(card.x + 10.0, card.y + 6.0, card.w - 174.0, 22.0);
            let editing_url = c.ui.input.is_some() && ai.focus_field == AiFocusField::BaseUrl;
            c.s.rect(url_box, 7.0, if editing_url { pal().surface_hover } else { Color::rgba(0, 0, 0, 0.25) });
            if editing_url {
                c.s.stroke(url_box, 7.0, 1.2, acc);
            }
            let url_rect = Rect::new(url_box.x + 8.0, url_box.y, url_box.w - 16.0, url_box.h);
            if editing_url {
                let f = c.ui.input.as_ref().unwrap();
                let ed = field_edit(c, f, acc);
                c.s.text_edit(f.as_str().to_string(), url_rect, 11.0, Weight::Medium, pal().text, ed);
            } else {
                c.text(c.set.active_ai_url().to_string(), url_rect, 11.0, Weight::Medium, pal().text, Align::Left);
            }
            c.hit_text(url_box, Action::AiFocusBaseUrl);
            c.pill(Rect::new(card.right() - 158.0, card.y + 6.0, 72.0, 22.0), Action::AiPasteUrl, "Paste URL", None, 10.5);
            c.pill(Rect::new(card.right() - 80.0, card.y + 6.0, 72.0, 22.0), Action::AiPasteKey, "Paste Key", (!ai.configured).then_some(acc), 10.5);

            let note = if ai.configured {
                "API key saved (DPAPI) \u{00b7} OpenAI / OpenRouter / Groq / vLLM"
            } else {
                "Key optional for local servers \u{00b7} Paste Key for cloud APIs"
            };
            c.text(note, Rect::new(card.x + 12.0, card.y + 32.0, card.w - 86.0, 18.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
            c.pill(Rect::new(card.right() - 68.0, card.y + 31.0, 60.0, 20.0), Action::AiRefreshModels, "Sync", None, 10.0);
        }
        AiProvider::Gemini | AiProvider::Claude => {
            c.text(prov.full_name(), Rect::new(card.x + 12.0, card.y + 6.0, card.w - 110.0, 18.0), 12.0, Weight::Semibold, pal().text, Align::Left);
            let sub = if ai.configured {
                "API key saved (encrypted with Windows DPAPI)"
            } else {
                "Copy your API key to the clipboard, then click Paste Key"
            };
            c.text(sub, Rect::new(card.x + 12.0, card.y + 28.0, card.w - 110.0, 20.0), 11.0, Weight::Regular, if ai.configured { pal().text_2 } else { pal().text_3 }, Align::Left);
            c.pill(Rect::new(card.right() - 90.0, card.y + 15.0, 82.0, 26.0), Action::AiPasteKey, "Paste Key", (!ai.configured).then_some(acc), 11.0);
        }
        AiProvider::Ollama => {
            c.text("Local Ollama \u{00b7} runs offline on your PC", Rect::new(card.x + 12.0, card.y + 5.0, card.w - 24.0, 18.0), 12.0, Weight::Semibold, pal().text, Align::Left);
            let url_box = Rect::new(card.x + 10.0, card.y + 27.0, card.w - 98.0, 22.0);
            let editing_url = c.ui.input.is_some() && ai.focus_field == AiFocusField::BaseUrl;
            c.s.rect(url_box, 7.0, if editing_url { pal().surface_hover } else { Color::rgba(0, 0, 0, 0.25) });
            if editing_url {
                c.s.stroke(url_box, 7.0, 1.2, acc);
            }
            let url_rect = Rect::new(url_box.x + 8.0, url_box.y, url_box.w - 16.0, url_box.h);
            if editing_url {
                let f = c.ui.input.as_ref().unwrap();
                let ed = field_edit(c, f, acc);
                c.s.text_edit(f.as_str().to_string(), url_rect, 11.0, Weight::Medium, pal().text, ed);
            } else {
                c.text(c.set.active_ai_url().to_string(), url_rect, 11.0, Weight::Medium, pal().text, Align::Left);
            }
            c.hit_text(url_box, Action::AiFocusBaseUrl);
            c.pill(Rect::new(card.right() - 82.0, card.y + 27.0, 74.0, 22.0), Action::AiPasteUrl, "Paste URL", None, 10.5);
        }
    }

    // 3. Model selector row: editable model field + Prev/Next + preset chips.
    let my = a.y + 125.0;
    c.text("Model", Rect::new(a.x, my, 42.0, 24.0), 11.5, Weight::Medium, pal().text_2, Align::Left);
    let mbox = Rect::new(a.x + 44.0, my, 134.0, 24.0);
    let editing_model = c.ui.input.is_some() && ai.focus_field == AiFocusField::Model;
    c.s.rect(mbox, 8.0, if editing_model { pal().surface_hover } else { pal().surface });
    if editing_model {
        c.s.stroke(mbox, 8.0, 1.2, acc);
    }
    let model_rect = Rect::new(mbox.x + 8.0, mbox.y, mbox.w - 16.0, mbox.h);
    if editing_model {
        let f = c.ui.input.as_ref().unwrap();
        let ed = field_edit(c, f, acc);
        c.s.text_edit(f.as_str().to_string(), model_rect, 11.5, Weight::Semibold, pal().text, ed);
    } else {
        c.text(c.set.active_ai_model().to_string(), model_rect, 11.5, Weight::Semibold, pal().text, Align::Left);
    }
    c.hit_text(mbox, Action::AiFocusModel);

    c.icon_btn(Rect::new(mbox.right() + 4.0, my, 24.0, 24.0), Action::AiCycleModel(-1), Icon::Prev, 10.0, pal().text_2);
    c.icon_btn(Rect::new(mbox.right() + 30.0, my, 24.0, 24.0), Action::AiCycleModel(1), Icon::Next, 10.0, pal().text_2);

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
        pal().text,
        Align::Left,
    );
    c.switch(Rect::new(a.right() - 42.0, sy, 42.0, 22.0), SettingKey::AiShareDay, acc);

    // 5. Status / footer note.
    let fy = a.bottom() - 18.0;
    let (foot, col) = if !ai.error.is_empty() {
        (ai.error.clone(), pal().amber)
    } else if !ai.status_note.is_empty() {
        (ai.status_note.clone(), acc)
    } else {
        (
            format!(
                "Secrets encrypted with Windows DPAPI \u{00b7} {} learned memor{}",
                ai.memories_count,
                if ai.memories_count == 1 { "y" } else { "ies" }
            ),
            pal().text_3,
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
        c.s.rect(r, 10.0, if active { acc.fade(0.16) } else { pal().surface.mix(pal().surface_hover, hv) });
        if active {
            c.s.circle(r.x + 14.0, r.cy(), 3.5, acc);
        }
        let tx = if active { r.x + 26.0 } else { r.x + 14.0 };
        c.text(item.title.clone(), Rect::new(tx, r.y, r.w - 110.0, r.h), 12.5, if active { Weight::Semibold } else { Weight::Regular }, pal().text, Align::Left);
        c.text(
            format!("{} msg{}", item.msg_count, if item.msg_count == 1 { "" } else { "s" }),
            Rect::new(r.right() - 88.0, r.y, 52.0, r.h),
            11.0,
            Weight::Regular,
            pal().text_3,
            Align::Right,
        );
        c.hit(Rect::new(r.x, r.y, r.w - 32.0, r.h), sel);
        c.ghost_icon(Rect::new(r.right() - 28.0, r.cy() - 12.0, 24.0, 24.0), del, Icon::Close, 11.5, pal().text_2);
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
    c.s.rect(field, 12.0, if typing { pal().surface_hover } else { c.surface_fill(Action::AiInput) });
    if typing {
        c.s.stroke(field, 12.0, 1.4, acc.fade(0.85));
    }
    icons::draw(&mut c.s, Icon::Sparkle, field.x + 16.0, field.cy(), 14.0, if typing { acc } else { pal().text_3 });
    let tr = Rect::new(field.x + 32.0, field.y, field.w - 42.0, field.h);
    let shown = if typing {
        c.ui.input.as_ref().map(|f| f.as_str()).unwrap_or("")
    } else {
        ai.input.as_str()
    };
    if !shown.is_empty() {
        if typing {
            let f = c.ui.input.as_ref().unwrap();
            let ed = field_edit(c, f, acc);
            c.s.text_edit(f.as_str().to_string(), tr, 12.5, Weight::Regular, pal().text, ed);
        } else {
            c.text(shown.to_string(), tr, 12.5, Weight::Regular, pal().text, Align::Left);
        }
    } else {
        let placeholder = if ai.busy {
            "Thinking\u{2026}"
        } else {
            "Ask anything, add a task, or plan your day\u{2026}"
        };
        c.text(placeholder, tr, 12.5, Weight::Regular, pal().text_3, Align::Left);
        if typing && (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
            c.s.rect(Rect::new(tr.x, field.cy() - 8.0, 1.5, 16.0), 0.7, acc);
        }
    }
    c.hit_text(field, Action::AiInput);

    // Send button.
    let send_r = Rect::new(a.right() - 34.0, in_y, 34.0, 34.0);
    let can_send = !shown.trim().is_empty() && !ai.busy;
    c.s.rect(send_r, 17.0, if can_send { acc } else { pal().surface });
    icons::draw(&mut c.s, Icon::Send, send_r.cx(), send_r.cy(), 15.0, if can_send { ON_ACCENT } else { pal().text_3 });
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
            c.text(msg, Rect::new(ban.x + 12.0, ban.y, ban.w - 134.0, ban.h), 11.5, Weight::Medium, pal().text, Align::Left);
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
            c.s.rect(r, 12.0, if c.pressed(act) { pal().surface_press } else { pal().surface.mix(pal().surface_hover, hv) });
            c.text(*title, Rect::new(r.x + 12.0, r.y + 7.0, r.w - 24.0, 18.0), 12.5, Weight::Semibold, pal().text, Align::Left);
            let lines = wrap_lines(prompt, r.w - 24.0, 10.5);
            for (li, l) in lines.iter().take(2).enumerate() {
                c.text(l.clone(), Rect::new(r.x + 12.0, r.y + 26.0 + li as f32 * 13.0, r.w - 24.0, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
            }
            c.hit(r, act);
        }
        return;
    }

    // Pre-layout the conversation so we can bottom-align and scroll cleanly.
    // Assistant replies are rendered as markdown beside a small avatar; the
    // user's messages stay as accent bubbles on the right.
    const GUTTER: f32 = 32.0;
    enum RowItem {
        UserBubble(Vec<String>),
        Assistant { msg_idx: usize, md: std::rc::Rc<super::markdown::Laid> },
        ActionRow { msg_idx: usize, act_idx: usize, label: String, can_undo: bool, undone: bool },
        Thinking,
        ErrorBanner(String),
    }

    let md_w = body.w - GUTTER - 6.0;
    let mut items = Vec::new();
    for (mi, m) in ai.messages.iter().enumerate() {
        match m.role {
            Role::User => {
                items.push(RowItem::UserBubble(wrap_lines(&m.text, body.w * 0.72, 12.0)));
            }
            Role::Assistant => {
                items.push(RowItem::Assistant { msg_idx: mi, md: super::markdown::laid(&m.text, md_w) });
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

    /// Row below an assistant reply that holds its Copy button.
    const COPY_ROW: f32 = 22.0;
    let item_h = |it: &RowItem| -> f32 {
        match it {
            RowItem::UserBubble(lines) => (lines.len() as f32 * 16.0 + 12.0).max(28.0) + 8.0,
            RowItem::Assistant { md, .. } => md.height.max(LH_MIN) + COPY_ROW + 8.0,
            RowItem::ActionRow { .. } => 26.0,
            RowItem::Thinking => 32.0,
            RowItem::ErrorBanner(_) => 28.0,
        }
    };
    const LH_MIN: f32 = 18.0;

    let total_h: f32 = items.iter().map(&item_h).sum();
    let max_scroll = (total_h - body.h).max(0.0);
    let scroll_px = (ai.scroll as f32 * 24.0).min(max_scroll);
    let mut y = if total_h <= body.h {
        body.y
    } else {
        body.bottom() - total_h + scroll_px
    };

    let avatar = |c: &mut Ctx, x: f32, y: f32, pulse: f32| {
        c.s.circle(x + 11.0, y + 11.0, 11.0, acc.fade(0.16));
        icons::draw(&mut c.s, Icon::Sparkle, x + 11.0, y + 11.0, 12.0, acc.fade(pulse));
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
                let bh = h - 8.0;
                let max_line = lines.iter().map(|l| est_w(l, 12.0) * 1.12).fold(40.0_f32, f32::max);
                let bw = (max_line + 24.0).clamp(64.0, body.w * 0.78);
                let br = Rect::new(body.right() - bw, y, bw, bh);
                c.s.rect(br, 14.0, acc.fade(0.2));
                c.s.stroke(br, 14.0, 1.0, acc.fade(0.22));
                for (li, l) in lines.iter().enumerate() {
                    c.text(l.clone(), Rect::new(br.x + 12.0, br.y + 6.0 + li as f32 * 16.0, br.w - 24.0, 16.0), 12.0, Weight::Regular, pal().text, Align::Left);
                }
            }
            RowItem::Assistant { msg_idx, md } => {
                avatar(c, body.x, y + 1.0, 1.0);
                md.draw(&mut c.s, body.x + GUTTER, y + 1.0, md_w, acc);
                let copy_r = Rect::new(body.x + GUTTER - 2.0, y + md.height.max(LH_MIN) + 5.0, 40.0, 18.0);
                if copy_r.y >= body.y && copy_r.bottom() <= body.bottom() {
                    c.pill(copy_r, Action::AiCopyReply(*msg_idx as u16), "Copy", None, 9.5);
                }
            }
            RowItem::ActionRow { msg_idx, act_idx, label, can_undo, undone } => {
                let ar = Rect::new(body.x + GUTTER, y, body.w - GUTTER, 22.0);
                c.s.rect(ar, 8.0, pal().green.fade(0.12));
                icons::draw(&mut c.s, Icon::Check, ar.x + 12.0, ar.cy(), 11.0, pal().green);
                let txt = if *undone { format!("{label} (undone)") } else { label.clone() };
                c.text(txt, Rect::new(ar.x + 24.0, ar.y, ar.w - 84.0, ar.h), 11.0, Weight::Medium, if *undone { pal().text_3 } else { pal().text_2 }, Align::Left);
                if *can_undo {
                    let undo_r = Rect::new(ar.right() - 52.0, ar.y + 2.0, 48.0, 18.0);
                    if undo_r.y >= body.y && undo_r.bottom() <= body.bottom() {
                        c.pill(undo_r, Action::AiUndoAction(*msg_idx as u16, *act_idx as u8), "Undo", None, 9.5);
                    }
                }
            }
            RowItem::Thinking => {
                let t = c.now_ms as f32 / 1000.0;
                let pulse = (t * std::f32::consts::TAU / 1.2).sin() * 0.35 + 0.65;
                avatar(c, body.x, y + 1.0, pulse);
                // Three dots chasing each other.
                for k in 0..3 {
                    let ph = ((t * 1.6 - k as f32 * 0.18) % 1.0).max(0.0);
                    let lift = (ph * std::f32::consts::PI).sin().max(0.0);
                    c.s.circle(body.x + GUTTER + 6.0 + k as f32 * 11.0, y + 12.0 - lift * 3.0, 2.6, pal().text_2.fade(0.45 + 0.55 * lift));
                }
            }
            RowItem::ErrorBanner(err) => {
                let br = Rect::new(body.x, y, body.w, 24.0);
                c.s.rect(br, 8.0, pal().amber.fade(0.14));
                c.text(err.clone(), Rect::new(br.x + 10.0, br.y, br.w - 20.0, br.h), 11.0, Weight::Medium, pal().amber, Align::Left);
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
fn segmented(c: &mut Ctx, seg: Rect, items: &[(Action, &str, bool)], size: f32) {
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


// -------------------------------------------------------------------- chat

/// Max message-scroll in message units (0 = pinned to the newest).
pub fn chat_max_scroll(n: usize) -> usize {
    n.saturating_sub(1)
}

/// BLE mesh chat: peer sidebar + conversation + input (bitchat whitepaper).
fn chat(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Chat);
    let vm = c.vm;
    let chat = &vm.chat;

    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, if chat.radio_on { acc } else { pal().text_3 });
    c.text("MESH CHAT", Rect::new(a.x + 12.0, a.y, 110.0, 15.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
    c.text(chat.status.clone(), Rect::new(a.x + 120.0, a.y, a.w - 120.0, 15.0), 10.5, Weight::Medium, pal().text_3, Align::Right);

    let body = Rect::new(a.x, a.y + 20.0, a.w, a.h - 20.0);
    let side_w = 142.0;
    let side = Rect::new(body.x, body.y, side_w, body.h);
    let main = Rect::new(side.right() + 10.0, body.y, body.w - side_w - 10.0, body.h);

    // -- sidebar ---------------------------------------------------------
    // Our nickname (click to edit; Enter commits via ChatNick).
    let nick_r = Rect::new(side.x, side.y, side.w, 26.0);
    let editing = chat.editing_nick;
    c.s.rect(nick_r, 9.0, if editing { pal().surface_hover } else { c.surface_fill(Action::ChatNick) });
    if editing {
        c.s.stroke(nick_r, 9.0, 1.4, acc.fade(0.85));
    }
    if editing && c.ui.input.is_some() {
        let f = c.ui.input.as_ref().unwrap();
        let ed = field_edit(c, f, acc);
        c.s.text_edit(f.as_str().to_string(), Rect::new(nick_r.x + 10.0, nick_r.y, nick_r.w - 16.0, nick_r.h), 12.0, Weight::Medium, pal().text, ed);
    } else {
        c.text(format!("@{}", chat.nick), Rect::new(nick_r.x + 10.0, nick_r.y, nick_r.w - 56.0, nick_r.h), 12.0, Weight::Semibold, pal().text, Align::Left);
        // Short peer id — compare in person to pin an identity.
        c.text(chat.me.chars().take(4).collect::<String>(), Rect::new(nick_r.right() - 48.0, nick_r.y, 44.0, nick_r.h), 10.0, Weight::Medium, pal().text_3, Align::Right);
    }
    c.hit(nick_r, Action::ChatNick);

    // The public room.
    let room_r = Rect::new(side.x, nick_r.bottom() + 6.0, side.w, 28.0);
    let sel_room = chat.sel.is_none();
    let room_a = Action::ChatSelect(0);
    c.s.rect(room_r, 9.0, if sel_room { acc.fade(0.2) } else { c.surface_fill(room_a) });
    c.s.circle(room_r.x + 13.0, room_r.cy(), 4.0, if sel_room { acc } else { pal().text_3 });
    c.text("room", Rect::new(room_r.x + 24.0, room_r.y, room_r.w - 54.0, room_r.h), 12.0, Weight::Semibold, pal().text, Align::Left);
    if chat.unread_room > 0 {
        let b = Rect::new(room_r.right() - 24.0, room_r.cy() - 8.0, 16.0, 16.0);
        c.s.rect(b, 8.0, acc);
        c.text(format!("{}", chat.unread_room.min(99)), b, 9.5, Weight::Bold, ON_ACCENT, Align::Center);
    }
    c.hit(room_r, room_a);

    // Peers: presence dot + nick (+ lock when a Noise session is up).
    let mut y = room_r.bottom() + 4.0;
    let max_y = side.bottom() - 30.0;
    for (i, p) in chat.peers.iter().enumerate() {
        if y > max_y {
            break;
        }
        let r = Rect::new(side.x, y, side.w, 28.0);
        let sel = chat.sel.as_deref() == Some(p.id.as_str());
        let act = Action::ChatSelect((i + 1) as u16);
        if sel {
            c.s.rect(r, 9.0, acc.fade(0.2));
        } else {
            c.s.rect(r, 9.0, c.surface_fill(act));
        }
        let dot = if p.fresh { acc } else { pal().text_3 };
        c.s.circle(r.x + 13.0, r.cy(), 3.5, dot);
        let name_w = r.w - 24.0 - if p.secure { 20.0 } else { 6.0 } - if p.unread > 0 { 24.0 } else { 4.0 };
        c.text(p.nick.clone(), Rect::new(r.x + 24.0, r.y, name_w, r.h), 12.0, if sel { Weight::Semibold } else { Weight::Regular }, pal().text, Align::Left);
        if p.secure {
            // tiny lock
            let lx = r.right() - 18.0 - if p.unread > 0 { 24.0 } else { 0.0 };
            let ly = r.cy();
            c.s.stroke(Rect::new(lx - 3.5, ly - 2.0, 7.0, 6.5), 2.0, 1.3, acc);
            c.s.ring(lx, ly - 2.5, 2.6, 1.2, 0.5, 0.5, acc);
        }
        if p.unread > 0 {
            let b = Rect::new(r.right() - 24.0, r.cy() - 8.0, 16.0, 16.0);
            c.s.rect(b, 8.0, acc);
            c.text(format!("{}", p.unread.min(99)), b, 9.5, Weight::Bold, ON_ACCENT, Align::Center);
        }
        c.hit(r, act);
        y += 30.0;
    }
    if chat.peers.is_empty() {
        c.text("no peers yet", Rect::new(side.x + 4.0, y + 4.0, side.w - 8.0, 30.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
    }

    // -- conversation -----------------------------------------------------
    let in_y = main.bottom() - 34.0;
    let msg_area = Rect::new(main.x, main.y, main.w, in_y - main.y - 8.0);

    // Title line.
    let secure = chat.sel.as_deref().and_then(|id| chat.peers.iter().find(|p| p.id == id)).map(|p| p.secure).unwrap_or(false);
    let title = if chat.sel.is_none() {
        "mesh room \u{00b7} everyone in range".to_string()
    } else if secure {
        format!("{} \u{00b7} encrypted", chat.sel_nick)
    } else {
        format!("{} \u{00b7} encrypting\u{2026}", chat.sel_nick)
    };
    c.text(title, Rect::new(msg_area.x + 2.0, msg_area.y, msg_area.w - 4.0, 16.0), 11.0, Weight::Semibold, pal().text_3, Align::Left);
    let list = Rect::new(msg_area.x, msg_area.y + 18.0, msg_area.w, msg_area.h - 18.0);

    if chat.msgs.is_empty() {
        let hint = if chat.sel.is_none() { "say hi to the mesh" } else { "no messages yet" };
        c.empty_state(list, Icon::Chat, acc, hint, if chat.peers.is_empty() { "waiting for a droppy in range" } else { "" });
    } else {
        // Bottom-up flow; `scroll` = messages hidden from the end.
        let scroll = c.ui.scroll_of(Page::Chat).min(chat_max_scroll(chat.msgs.len()));
        let mut cy = list.bottom() - 2.0;
        c.s.clip(list);
        for m in chat.msgs.iter().rev().skip(scroll) {
            let lines = wrap_lines(&m.text, list.w - 24.0, 12.5);
            let h = 15.0 + lines.len() as f32 * 15.0 + 7.0;
            cy -= h;
            if cy + h < list.y {
                break;
            }
            let who = if m.mine { "you".to_string() } else { m.who.clone() };
            let when = chrono::DateTime::from_timestamp_millis(m.ts)
                .map(|t| local_naive(t).format("%H:%M").to_string())
                .unwrap_or_default();
            let mut meta = who.clone();
            if m.mine {
                if m.failed {
                    meta.push_str(" \u{00b7} failed");
                } else if m.read {
                    meta.push_str(" \u{00b7} read");
                } else if m.delivered.unwrap_or(false) {
                    meta.push_str(" \u{00b7} sent");
                } else if chat.sel.is_some() {
                    meta.push_str(" \u{00b7} \u{2026}");
                }
            }
            let meta_col = if m.failed { pal().red } else if m.mine { acc } else { pal().text_3 };
            c.text(meta, Rect::new(list.x + 2.0, cy, list.w - 60.0, 14.0), 10.0, Weight::Semibold, meta_col, Align::Left);
            c.text(when, Rect::new(list.right() - 58.0, cy, 56.0, 14.0), 10.0, Weight::Regular, pal().text_3, Align::Right);
            let mut ly = cy + 15.0;
            for l in &lines {
                c.text(l.clone(), Rect::new(list.x + 10.0, ly, list.w - 24.0, 15.0), 12.5, Weight::Regular, pal().text, Align::Left);
                ly += 15.0;
            }
        }
        c.s.pop_clip();
        if scroll > 0 {
            c.text(format!("{} newer", scroll), Rect::new(list.x, list.bottom() - 14.0, list.w, 14.0), 9.5, Weight::Medium, acc, Align::Center);
        }
    }

    // -- input ------------------------------------------------------------
    let field = Rect::new(main.x, in_y, main.w - 40.0, 34.0);
    let typing = c.ui.input.is_some() && !chat.editing_nick;
    c.s.rect(field, 12.0, if typing { pal().surface_hover } else { c.surface_fill(Action::ChatInput) });
    if typing {
        c.s.stroke(field, 12.0, 1.4, acc.fade(0.85));
    }
    icons::draw(&mut c.s, Icon::Chat, field.x + 16.0, field.cy(), 13.0, if typing { acc } else { pal().text_3 });
    let tr = Rect::new(field.x + 34.0, field.y, field.w - 44.0, field.h);
    match &c.ui.input {
        Some(f) if typing && !f.is_empty() => {
            let ed = field_edit(c, f, acc);
            c.s.text_edit(f.as_str().to_string(), tr, 12.5, Weight::Regular, pal().text, ed);
        }
        _ => {
            let ph = if editing {
                "nickname\u{2026}"
            } else if chat.sel.is_some() {
                "dm over noise\u{2026}"
            } else {
                "message the room\u{2026}"
            };
            c.text(ph, tr, 12.5, Weight::Regular, pal().text_3, Align::Left);
            if typing && (c.ui.clock_s * 1.8) % 2.0 < 1.0 {
                c.s.rect(Rect::new(tr.x, field.cy() - 8.0, 1.5, 16.0), 0.7, acc);
            }
        }
    }
    c.hit_text(field, Action::ChatInput);

    let send_r = Rect::new(main.right() - 34.0, in_y, 34.0, 34.0);
    let can_send = c.ui.input.as_ref().map(|f| !f.as_str().trim().is_empty()).unwrap_or(false) && !editing;
    c.s.rect(send_r, 17.0, if can_send { acc } else { pal().surface });
    icons::draw(&mut c.s, Icon::Send, send_r.cx(), send_r.cy(), 15.0, if can_send { ON_ACCENT } else { pal().text_3 });
    c.hit(send_r, Action::ChatSend);
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
        let lines = |s: &str, w: f32| note_lines_from_ranges(s, &wrap_note_ranges(s, w, 14.0));
        // Narrow enough that "alpha beta" cannot share a line.
        let wrapped = lines("alpha beta gamma", 40.0);
        assert!(wrapped.len() >= 2, "{wrapped:?}");
        assert_eq!(wrapped[0], "alpha");
        assert_eq!(wrapped.join(" "), "alpha beta gamma");

        // Blank lines survive, so paragraphs keep their spacing.
        let wrapped = lines("one\n\ntwo", 400.0);
        assert_eq!(wrapped, vec!["one".to_string(), String::new(), "two".to_string()]);

        // Empty text still yields one caret line.
        assert_eq!(lines("", 400.0), vec![String::new()]);
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

    #[test]
    fn note_full_date_and_fuzzy_search() {
        use chrono::{TimeZone, Utc};
        let now = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        let formatted = format_note_date(now, now);
        assert!(formatted.starts_with("Today, "));

        let past = now - chrono::Duration::days(1);
        let formatted_past = format_note_date(past, now);
        assert!(formatted_past.starts_with("Yesterday, "));

        assert!(teardrop_core::fuzzy_score("meet", "Team Meeting Notes").is_some());
        assert!(teardrop_core::fuzzy_score("plan", "Team Meeting Notes").is_none());
    }

    #[test]
    fn lyrics_wrapping_and_karaoke_progress() {
        let short = "Hello world";
        let wrapped_short = wrap_lyrics(short, 300.0, 17.0, true);
        assert_eq!(wrapped_short, vec!["Hello world"]);

        // Long text wraps cleanly into multiple lines
        let long = "This is a very long line of lyrics that definitely exceeds the lyrics column width and should wrap nicely";
        let wrapped_long = wrap_lyrics(long, 200.0, 17.0, true);
        assert!(wrapped_long.len() >= 2, "Expected wrapped lines, got {:?}", wrapped_long);

        // Subline karaoke progress transitions
        let lines = vec!["Hello".to_string(), "world".to_string()];
        // 0.0 -> both 0.0
        let p0 = karaoke_subline_progress(&lines, 0.0);
        assert_eq!(p0[0].1, 0.0);
        assert_eq!(p0[1].1, 0.0);

        // 0.25 -> first line is 0.5, second line is 0.0
        let p25 = karaoke_subline_progress(&lines, 0.25);
        assert!((p25[0].1 - 0.5).abs() < 1e-4);
        assert_eq!(p25[1].1, 0.0);

        // 0.5 -> first line is 1.0, second line is 0.0
        let p50 = karaoke_subline_progress(&lines, 0.5);
        assert_eq!(p50[0].1, 1.0);
        assert_eq!(p50[1].1, 0.0);

        // 0.75 -> first line is 1.0, second line is 0.5
        let p75 = karaoke_subline_progress(&lines, 0.75);
        assert_eq!(p75[0].1, 1.0);
        assert!((p75[1].1 - 0.5).abs() < 1e-4);

        // 1.0 -> both 1.0
        let p100 = karaoke_subline_progress(&lines, 1.0);
        assert_eq!(p100[0].1, 1.0);
        assert_eq!(p100[1].1, 1.0);
    }
}
