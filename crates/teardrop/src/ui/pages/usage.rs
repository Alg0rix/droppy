use super::*;
// -------------------------------------------------------------------- usage

fn usage_bar(c: &mut Ctx, r: Rect, label: &str, w: Option<&crate::usage::Window>, now: i64, acc: Color) {
    c.text(label, Rect::new(r.x, r.y, r.w - 40.0, 14.0), 10.5, Weight::Medium, pal().text_3, Align::Left);
    let bar = Rect::new(r.x, r.y + 20.0, r.w, 6.0);
    c.s.rect(bar, 3.0, pal().ink(0.10));
    match w {
        Some(w) => {
            let used = w.used_now(now);
            let col = if used > 85.0 {
                pal().red
            } else if used > 65.0 {
                pal().amber
            } else {
                acc
            };
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
        let msg = if !loaded {
            "Reading\u{2026}"
        } else if l.note.is_empty() {
            "No data yet"
        } else {
            l.note.as_str()
        };
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

pub(super) fn usage(c: &mut Ctx, a: Rect) {
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
        let line = format!(
            "Today {} tokens{}    Week {} tokens{}",
            crate::usage::compact_count(o.today_tokens),
            money(o.today_cost),
            crate::usage::compact_count(o.week_tokens),
            money(o.week_cost)
        );
        c.text(line, Rect::new(r.x + 12.0, r.y + 26.0, r.w - 24.0, 18.0), 12.0, Weight::Medium, pal().text, Align::Left);
        let top =
            o.top.iter().map(|s| format!("{} {}", s.label.rsplit('/').next().unwrap_or(&s.label), crate::usage::compact_count(s.tokens))).collect::<Vec<_>>().join("  \u{00b7}  ");
        if !top.is_empty() {
            c.text(top, Rect::new(r.x + 12.0, r.y + 45.0, r.w - 24.0, 16.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
        }
    }
}
