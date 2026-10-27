use super::*;
// -------------------------------------------------------------------- tasks

pub const TASK_ROWS: usize = 5;

pub fn tasks_max_scroll(n: usize) -> usize {
    n.saturating_sub(TASK_ROWS)
}

pub(super) fn tasks(c: &mut Ctx, a: Rect) {
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
