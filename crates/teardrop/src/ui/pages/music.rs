use super::*;
// -------------------------------------------------------------------- music

pub(super) fn music(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Music);
    let vm = c.vm;
    let t = &vm.track;
    let has = !t.title.is_empty();

    let left_w = (a.w * 0.48).min(320.0);

    // App badge (colored dot + source app name)
    let app_name = if has && !t.app.is_empty() { t.app.clone() } else { "Windows Media".to_string() };

    // Top strip: source on the left, position/duration on the right.
    c.s.circle(
        a.x + 4.0,
        a.y + 7.0,
        3.5,
        if app_name.to_lowercase().contains("spotify") {
            Color::rgb(30, 215, 96)
        } else if has {
            acc
        } else {
            pal().text_3
        },
    );
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
        let curr_sublines = curr_line.map(|l| wrap_lyrics(&l.text, lw, 17.0, true)).unwrap_or_default();
        let curr_count = curr_sublines.len().max(1);

        let prev_line = if i > 0 { vm.lyric_lines.get(i - 1) } else { None };
        let prev_sublines = prev_line.map(|l| wrap_lyrics(&l.text, lw, 13.0, false)).unwrap_or_default();
        let prev_count = prev_sublines.len().min(2);

        let next_line = vm.lyric_lines.get(i + 1);
        let next_sublines = next_line.map(|l| wrap_lyrics(&l.text, lw, 13.0, false)).unwrap_or_default();

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
                c.s.text_karaoke(subline.to_string(), Rect::new(lx, cy, lw, curr_lh), 17.0, Weight::Bold, pal().text_2, WHITE, sub_frac);
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
        let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };

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

    let weights: Vec<f32> = lines.iter().map(|l| (l.chars().count() as f32).max(1.0)).collect();
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
