use super::*;
// ---------------------------------------------------------------- clipboard

pub const CLIP_ROWS: usize = 5;

pub fn clip_max_scroll(n: usize) -> usize {
    n.saturating_sub(CLIP_ROWS)
}

pub(super) fn clipboard(c: &mut Ctx, a: Rect) {
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
