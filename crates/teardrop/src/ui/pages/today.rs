use super::*;
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
pub fn active_calendar_events<'a>(vm: &'a crate::viewmodel::ViewModel, selected_day: Option<(i32, u32, u32)>) -> Vec<&'a crate::calendar::Event> {
    match selected_day {
        Some((y, m, d)) => vm.calendar.events_on_day(y, m, d),
        None => vm.calendar.agenda(vm.now.timestamp(), 15),
    }
}

pub fn today_max_scroll(count: usize) -> usize {
    count.saturating_sub(3)
}

pub(super) fn today(c: &mut Ctx, a: Rect) {
    let acc = accent(Page::Today);
    let vm = c.vm;
    let now = local_naive(vm.now);
    let date = now.date();

    c.s.circle(a.x + 4.0, a.y + 7.0, 3.5, acc);
    c.text(
        format!("TODAY \u{00b7} {}", date.format("%a %d %b").to_string().to_uppercase()),
        Rect::new(a.x + 12.0, a.y, 220.0, 15.0),
        10.5,
        Weight::Semibold,
        pal().text_3,
        Align::Left,
    );

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
            if ev.all_day {
                format!("{day} \u{00b7} All day")
            } else {
                format!("{day} {}", clock_label(ev))
            }
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
        c.pill(Rect::new(ix, card.bottom() - 28.0, iw, 22.0), Action::Join(ev_idx as u8), &format!("Join {}", j.platform), Some(acc), 10.5);
    }
}
