//! One layout function per page. Each fills `a` (the card's content area).

use super::icons::{self, Icon};
use super::layout::{self, est_w, Ctx};
use super::markdown;
use super::scene::{Align, Color, Fill, Rect, TextEdit, Weight};
use super::state::{Action, QsLink, QsSection, SettingKey};
use super::textedit::TextField;
use super::theme::*;
use crate::viewmodel::{local_naive, mmss, one_line, SpeedPhase, ViewModel};
use chrono::{Datelike, NaiveDate};
use teardrop_core::{apps, format_note_date, AiProvider, AppCategory, DropletGeom, EyeStyle, IdleLook, IslandStyle, IslandTheme, NoteDateFilter, Page, Phase, Playback};

mod ai;
mod chat;
mod clipboard;
mod focus;
mod music;
mod notes;
mod quick;
mod settings;
mod shelf;
mod speed;
mod tasks;
mod today;
mod usage;
mod voice;
mod water;

pub use ai::*;
pub use chat::*;
pub use clipboard::*;
use focus::*;
pub use music::*;
pub use notes::*;
pub use quick::*;
use settings::*;
pub use shelf::*;
pub use speed::*;
pub use tasks::*;
pub use today::*;
use usage::*;
use voice::*;
pub use water::*;

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
        Page::Quick => quick(c, a),
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
            if y > a.bottom() - 26.0 {
                break;
            }
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
    let filtered: Vec<(usize, &crate::winreg::WindowInfo)> =
        c.vm.windows.iter().enumerate().filter(|(_, w)| q.is_empty() || w.title.to_lowercase().contains(&q_lower) || w.app.to_lowercase().contains(&q_lower)).collect();
    let row_h = 26.0;
    let scroll = c.ui.scroll_of(Page::Windows);
    // Group consecutive windows of the same exe under an app header.
    let mut last_exe = String::new();
    for (orig, w) in filtered.iter().skip(scroll) {
        if y > a.bottom() - 26.0 {
            break;
        }
        if w.exe != last_exe {
            last_exe = w.exe.clone();
            let n = filtered.iter().filter(|(_, x)| x.exe == w.exe).count();
            let tile = Rect::new(a.x + 8.0, y + 3.0, 16.0, 16.0);
            match crate::appicons::icon_for(&w.path) {
                Some(img) => c.s.image(tile, &img),
                None => c.s.rect(tile, 5.0, acc.fade(0.2)),
            }
            c.text(w.app.clone(), Rect::new(a.x + 30.0, y, a.w - 130.0, 22.0), 12.5, Weight::Semibold, pal().text, Align::Left);
            c.text(
                if n == 1 { "1 window".to_string() } else { format!("{} windows", n) },
                Rect::new(a.right() - 120.0, y + 4.0, 112.0, 15.0),
                10.0,
                Weight::Medium,
                pal().text_3,
                Align::Right,
            );
            y += 24.0;
            // The app's audio session becomes a slim mixer under its header.
            if let Some(si) = c.vm.audio.iter().position(|s| s.exe == w.exe) {
                let s = &c.vm.audio[si];
                icons::draw(&mut c.s, Icon::Speaker, a.x + 32.0, y + 10.0, 12.0, pal().text_3);
                let track = Rect::new(a.x + 48.0, y + 6.0, a.w - 160.0, 8.0);
                c.s.rect(track, 4.0, pal().surface_hover);
                let fill = (track.w * s.volume).max(4.0);
                c.s.rect(Rect::new(track.x, track.y, fill, track.h), 4.0, if s.muted { pal().text_3 } else { acc });
                c.text(
                    if s.muted { "muted".to_string() } else { format!("{:.0}%", s.volume * 100.0) },
                    Rect::new(a.right() - 104.0, y + 2.0, 96.0, 15.0),
                    10.5,
                    Weight::Medium,
                    pal().text_3,
                    Align::Right,
                );
                c.hit(Rect::new(track.x - 4.0, track.y - 5.0, track.w + 8.0, track.h + 10.0), Action::MixerVol(si as u16));
                y += 20.0;
            }
        }
        if y > a.bottom() - 26.0 {
            break;
        }
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
