use super::*;
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
pub(super) fn note_lines_from_ranges(text: &str, ranges: &[(usize, usize)]) -> Vec<String> {
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

pub(super) fn notes(c: &mut Ctx, a: Rect) {
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

        c.text(one_line(&title), Rect::new(r.x + 12.0, r.y + 3.0, (r.w - right_reserved - 16.0).max(60.0), 18.0), 13.5, Weight::Semibold, pal().text, Align::Left);
        if !preview.is_empty() {
            c.text(one_line(&preview), Rect::new(r.x + 12.0, r.y + 20.0, (r.w - right_reserved - 16.0).max(60.0), 14.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
        }

        // Full formatted date badge
        let date_w = 110.0;
        let date_x = r.right() - (if hv > 0.05 { 40.0 + date_w } else { 12.0 + date_w });
        c.text(full_date, Rect::new(date_x, r.y + 4.0, date_w, 15.0), 11.0, Weight::Medium, pal().text_2, Align::Right);
        // Relative age tag underneath
        c.text(note_age(note.updated_at, vm.now), Rect::new(date_x, r.y + 19.0, date_w, 13.0), 10.0, Weight::Regular, pal().text_3, Align::Right);

        c.hit(Rect::new(r.x, r.y, r.w - 36.0, r.h), open);
        if hv > 0.05 {
            c.ghost_icon(Rect::new(r.right() - 32.0, r.cy() - 12.0, 24.0, 24.0), del, Icon::Close, 12.0, pal().text_2);
        }
    }

    let count_text = if n == total_notes {
        if n == 1 {
            "1 note".to_string()
        } else {
            format!("{n} notes")
        }
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
        c.text("Start typing \u{2014} Enter starts a new line", Rect::new(body.x + 12.0, body.y + 10.0, inner_w, line_h), 13.5, Weight::Regular, pal().text_3, Align::Left);
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
