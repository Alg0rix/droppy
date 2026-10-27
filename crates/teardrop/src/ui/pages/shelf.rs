use super::*;
// -------------------------------------------------------------------- shelf

pub(super) fn ext_of(name: &str) -> String {
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
    let rows = n.div_ceil(SHELF_COLS);
    rows.saturating_sub(SHELF_ROWS)
}

pub(super) fn shelf(c: &mut Ctx, a: Rect) {
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
