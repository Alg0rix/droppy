//! The System page: live CPU / GPU / memory / drives / network / battery.
//!
//! A strip of six chips (each a live value + sparkline) selects the view; every
//! view shares one three-column rhythm — a summary ring, history charts, and
//! a ranked list — drawn on soft cards so the page reads like the rest of the
//! island rather than a task-manager clone.

use super::icons::{self, Icon};
use super::layout::Ctx;
use super::scene::{Align, Color, Fill, Rect, Weight};
use super::state::Action;
use super::theme::*;
use teardrop_core::sysmon::{self, CoreClass, ProcLoad, Series, SysTab, SysView};
use teardrop_core::Page;

const BLUE: Color = Color::rgb(96, 165, 250);
const INDIGO: Color = Color::rgb(129, 140, 248);
const TEAL: Color = Color::rgb(45, 212, 191);
const VIOLET: Color = Color::rgb(167, 139, 250);
const CYAN: Color = Color::rgb(34, 211, 238);
const LIME: Color = Color::rgb(74, 222, 128);
const GOLD: Color = Color::rgb(251, 191, 36);
const ROSE: Color = Color::rgb(248, 113, 113);

const CHIP_H: f32 = 52.0;
const GAP: f32 = 12.0;

/// Each metric's signature colour; battery shifts with its level.
fn tab_color(t: SysTab, sys: &SysView) -> Color {
    match t {
        SysTab::Cpu => BLUE,
        SysTab::Gpu => LIME,
        SysTab::Memory => VIOLET,
        SysTab::Drives => GOLD,
        SysTab::Network => CYAN,
        SysTab::Battery => {
            if sys.battery.pct <= 15.0 && !sys.battery.charging {
                pal().red
            } else if sys.battery.pct <= 30.0 && !sys.battery.charging {
                pal().amber
            } else {
                pal().green
            }
        }
    }
}

/// Warm a metric colour as it nears saturation.
fn heat(base: Color, pct: f32) -> Color {
    if pct >= 90.0 {
        pal().red
    } else if pct >= 75.0 {
        base.mix(pal().amber, 0.75)
    } else {
        base
    }
}

pub fn system(c: &mut Ctx, a: Rect) {
    let tab = c.ui.sys_tab;
    chips(c, a, tab);
    let body = Rect::new(a.x, a.y + CHIP_H + 10.0, a.w, a.h - CHIP_H - 10.0);
    if !c.vm.sys.loaded {
        c.empty_state(body, Icon::Chip, accent(Page::System), "Reading your system\u{2026}", "Live stats appear in a moment");
        return;
    }
    match tab {
        SysTab::Cpu => cpu_view(c, body),
        SysTab::Gpu => gpu_view(c, body),
        SysTab::Memory => mem_view(c, body),
        SysTab::Drives => drives_view(c, body),
        SysTab::Network => net_view(c, body),
        SysTab::Battery => battery_view(c, body),
    }
}

// ------------------------------------------------------------------ chips

fn chips(c: &mut Ctx, a: Rect, cur: SysTab) {
    let n = SysTab::ALL.len() as f32;
    let w = (a.w - (n - 1.0) * 6.0) / n;
    for (i, &t) in SysTab::ALL.iter().enumerate() {
        let r = Rect::new(a.x + i as f32 * (w + 6.0), a.y, w, CHIP_H);
        let act = Action::SysTab(t);
        let hv = c.hov(act);
        let col = tab_color(t, &c.vm.sys);
        let on = t == cur;
        if on {
            c.s.rect(r, 13.0, col.fade(0.15));
            c.s.stroke(r, 13.0, 1.2, col.fade(0.55));
        } else {
            c.s.rect(r, 13.0, pal().surface.mix(pal().surface_hover, hv));
        }
        let s = &c.vm.sys;
        let (value, hist, max): (String, Vec<&Series>, f32) = if !s.loaded {
            ("\u{2014}".into(), vec![], 100.0)
        } else {
            match t {
                SysTab::Cpu => (format!("{:.0}%", s.cpu.total), vec![&s.cpu.history], 100.0),
                SysTab::Gpu if s.gpu.available => (format!("{:.0}%", s.gpu.util), vec![&s.gpu.history], 100.0),
                SysTab::Gpu => ("\u{2014}".into(), vec![], 100.0),
                SysTab::Memory => (format!("{:.0}%", s.mem.pct()), vec![&s.mem.history], 100.0),
                SysTab::Drives => {
                    let pk = s.drives.read_hist.peak().max(s.drives.write_hist.peak()).max(1.0e6);
                    (sysmon::fmt_rate(s.drives.read_bps + s.drives.write_bps), vec![&s.drives.read_hist, &s.drives.write_hist], pk)
                }
                SysTab::Network => {
                    let pk = s.net.down_hist.peak().max(s.net.up_hist.peak()).max(1.0e5);
                    (sysmon::fmt_rate(s.net.down_bps + s.net.up_bps), vec![&s.net.down_hist, &s.net.up_hist], pk)
                }
                SysTab::Battery if s.battery.present => (format!("{:.0}%", s.battery.pct), vec![&s.battery.history], 100.0),
                SysTab::Battery => ("AC".into(), vec![], 100.0),
            }
        };
        c.text(t.label(), Rect::new(r.x + 10.0, r.y + 6.0, r.w - 20.0, 14.0), 10.0, Weight::Medium, if on { col } else { pal().text_3 }, Align::Left);
        c.text(value, Rect::new(r.x + 10.0, r.y + 19.0, r.w - 20.0, 19.0), 14.0, Weight::Semibold, pal().text, Align::Left);
        let sp = Rect::new(r.x + 10.0, r.bottom() - 13.0, r.w - 20.0, 8.0);
        let multi = hist.len() > 1;
        for (k, h) in hist.iter().enumerate() {
            spark(c, sp, h, max, if multi && k == 1 { col.mix(pal().text, 0.45) } else { col });
        }
        c.hit(r, act);
    }
}

// ---------------------------------------------------------------- widgets

fn card(c: &mut Ctx, r: Rect, title: &str, aside: &str) -> Rect {
    c.s.rect(r, 14.0, pal().surface);
    if !title.is_empty() {
        c.text(title.to_uppercase(), Rect::new(r.x + 12.0, r.y + 8.0, r.w * 0.55, 14.0), 9.5, Weight::Semibold, pal().text_3, Align::Left);
    }
    if !aside.is_empty() {
        c.text(aside, Rect::new(r.x + r.w * 0.4, r.y + 8.0, r.w * 0.6 - 12.0, 14.0), 9.5, Weight::Medium, pal().text_3, Align::Right);
    }
    Rect::new(r.x + 12.0, r.y + 28.0, r.w - 24.0, r.h - 36.0)
}

/// Three columns: summary / charts / list.
fn cols(body: Rect) -> [Rect; 3] {
    let l = 214.0;
    let r = (body.w * 0.30).clamp(170.0, 214.0);
    let m = body.w - l - r - 2.0 * GAP;
    [Rect::new(body.x, body.y, l, body.h), Rect::new(body.x + l + GAP, body.y, m, body.h), Rect::new(body.x + l + m + 2.0 * GAP, body.y, r, body.h)]
}

/// Track ring with stacked coloured arcs (fractions of a full turn).
fn ring(c: &mut Ctx, cx: f32, cy: f32, r: f32, width: f32, parts: &[(f32, Color)]) {
    c.s.ring(cx, cy, r, width, 0.0, 1.0, pal().ink(0.08));
    let mut from = 0.0;
    for (f, col) in parts {
        let to = (from + f.clamp(0.0, 1.0)).min(1.0);
        if to - from > 0.004 {
            c.s.ring(cx, cy, r, width, from, to, *col);
        }
        from = to;
    }
}

fn pts_of(r: Rect, s: &Series, max: f32) -> Vec<(f32, f32)> {
    let v: Vec<f32> = s.iter().collect();
    let dx = r.w / (sysmon::HISTORY - 1) as f32;
    let n = v.len();
    v.iter().enumerate().map(|(i, &x)| (r.right() - (n - 1 - i) as f32 * dx, r.bottom() - (x / max.max(1e-3)).clamp(0.0, 1.0) * (r.h - 2.0) - 1.0)).collect()
}

fn spark(c: &mut Ctx, r: Rect, s: &Series, max: f32, col: Color) {
    let pts = pts_of(r, s, max);
    if pts.len() >= 2 {
        c.s.polyline(pts, 1.3, col.fade(0.9));
    }
}

/// History chart: faint grid, filled lines, and a dot on the newest sample.
fn area(c: &mut Ctx, r: Rect, series: &[(&Series, Color)], max: f32) {
    for k in 1..4 {
        let y = r.y + r.h * k as f32 / 4.0;
        c.s.line(r.x, y, r.right(), y, 1.0, pal().hairline);
    }
    let fill_a = if series.len() > 1 { 0.28 } else { 0.42 };
    for (s, col) in series {
        let pts = pts_of(r, s, max);
        if pts.len() < 2 {
            continue;
        }
        let mut poly = pts.clone();
        poly.push((r.right(), r.bottom()));
        poly.push((pts[0].0, r.bottom()));
        c.s.poly(poly, Fill::VGrad(col.fade(fill_a), col.fade(0.02)));
        c.s.polyline(pts.clone(), 1.7, *col);
        let (x, y) = *pts.last().unwrap();
        c.s.circle(x, y, 3.0, *col);
        c.s.circle(x, y, 5.5, col.fade(0.22));
    }
}

fn row(c: &mut Ctx, r: Rect, swatch: Option<Color>, label: &str, value: &str) {
    let mut lx = r.x;
    if let Some(col) = swatch {
        c.s.rect(Rect::new(r.x, r.cy() - 4.0, 8.0, 8.0), 2.5, col);
        lx += 15.0;
    }
    c.text(label, Rect::new(lx, r.y, r.w * 0.62 - (lx - r.x), r.h), 11.0, Weight::Regular, pal().text_2, Align::Left);
    c.text(value, Rect::new(r.x + r.w * 0.5, r.y, r.w * 0.5, r.h), 11.0, Weight::Semibold, pal().text, Align::Right);
}

/// Labelled progress bar with the value on the right.
fn bar(c: &mut Ctx, r: Rect, label: &str, value: &str, frac: f32, col: Color) {
    c.text(label, Rect::new(r.x, r.y, r.w * 0.6, 14.0), 10.5, Weight::Medium, pal().text_2, Align::Left);
    c.text(value, Rect::new(r.x + r.w * 0.4, r.y, r.w * 0.6, 14.0), 10.5, Weight::Semibold, pal().text, Align::Right);
    let t = Rect::new(r.x, r.y + 18.0, r.w, 6.0);
    c.s.rect(t, 3.0, pal().ink(0.09));
    if frac > 0.0 {
        c.s.rect(Rect::new(t.x, t.y, (t.w * frac.clamp(0.0, 1.0)).max(6.0), 6.0), 3.0, col);
    }
}

fn app_tile(c: &mut Ctx, r: Rect, p: &ProcLoad) {
    let icon = if p.path.is_empty() { None } else { crate::appicons::icon_for(&p.path) };
    match icon {
        Some(img) => c.s.image(r, &img),
        None => {
            let (a, b) = art_colors(&p.name);
            c.s.rect(r, 5.0, Fill::DGrad(a, b));
            let initial = p.name.chars().next().unwrap_or('?').to_uppercase().collect::<String>();
            c.text(initial, r, 10.0, Weight::Semibold, WHITE, Align::Center);
        }
    }
}

const WHITE: Color = Color::rgb(255, 255, 255);

/// Ranked process list with a proportional bar behind each row.
fn proc_list(c: &mut Ctx, r: Rect, items: &[ProcLoad], fmt: fn(f32) -> String, col: Color, empty: &str) {
    if items.is_empty() {
        c.text(empty, Rect::new(r.x, r.y + 4.0, r.w, 16.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
        return;
    }
    let max = items.iter().map(|p| p.value).fold(0.0, f32::max).max(1e-3);
    let rh = 26.0;
    for (i, p) in items.iter().enumerate() {
        let y = r.y + i as f32 * rh;
        if y + rh > r.bottom() + 1.0 {
            break;
        }
        let row = Rect::new(r.x - 4.0, y, r.w + 8.0, rh - 3.0);
        c.s.rect(Rect::new(row.x, row.y, (row.w * p.value / max).max(8.0), row.h), 8.0, col.fade(0.13));
        app_tile(c, Rect::new(r.x + 2.0, y + 3.0, 17.0, 17.0), p);
        c.text(p.name.clone(), Rect::new(r.x + 26.0, y, r.w - 26.0 - 46.0, rh - 3.0), 11.0, Weight::Medium, pal().text, Align::Left);
        c.text(fmt(p.value), Rect::new(r.right() - 46.0, y, 46.0, rh - 3.0), 10.5, Weight::Medium, pal().text_2, Align::Right);
    }
}

fn pct1(v: f32) -> String {
    if v >= 10.0 {
        format!("{v:.0}%")
    } else {
        format!("{v:.1}%")
    }
}

fn small_pill(c: &mut Ctx, r: Rect, label: &str, col: Color) {
    c.s.rect(r, r.h / 2.0, col.fade(0.14));
    c.text(label, r, 10.5, Weight::Semibold, col, Align::Center);
}

// -------------------------------------------------------------------- CPU

fn cpu_view(c: &mut Ctx, body: Rect) {
    let [l, m, r] = cols(body);
    let cpu = c.vm.sys.cpu.clone();
    let acc = BLUE;

    // Summary ring.
    let inner = card(c, l, "Processor", "");
    let rad = 46.0;
    let (cx, cy) = (inner.cx(), inner.y + rad + 6.0);
    ring(c, cx, cy, rad, 10.0, &[(cpu.user / 100.0, heat(acc, cpu.total)), (cpu.system / 100.0, ROSE)]);
    c.text(format!("{:.0}%", cpu.total), Rect::new(cx - 40.0, cy - 17.0, 80.0, 28.0), 24.0, Weight::Semibold, pal().text, Align::Center);
    c.text("in use", Rect::new(cx - 40.0, cy + 10.0, 80.0, 14.0), 10.0, Weight::Regular, pal().text_3, Align::Center);
    let mut y = cy + rad + 12.0;
    c.text(
        if cpu.name.is_empty() { "Processor".to_string() } else { cpu.name.clone() },
        Rect::new(inner.x - 4.0, y, inner.w + 8.0, 14.0),
        10.5,
        Weight::Medium,
        pal().text_2,
        Align::Center,
    );
    y += 15.0;
    let threads = cpu.cores.len();
    let sub = if cpu.physical > 0 { format!("{} cores \u{00b7} {threads} threads", cpu.physical) } else { format!("{threads} threads") };
    c.text(sub, Rect::new(inner.x, y, inner.w, 13.0), 10.0, Weight::Regular, pal().text_3, Align::Center);
    y += 19.0;
    // Temperature and clock chips.
    let mut chips: Vec<(String, Color)> = Vec::new();
    if let Some(t) = cpu.temp_c {
        chips.push((
            format!("{t:.0}\u{00b0}C"),
            if t >= 85.0 {
                pal().red
            } else if t >= 70.0 {
                pal().amber
            } else {
                pal().green
            },
        ));
    }
    if cpu.ghz > 0.0 {
        chips.push((format!("{:.2} GHz", cpu.ghz), INDIGO));
    }
    let n = chips.len().max(1) as f32;
    let cw = (inner.w - (n - 1.0) * 6.0) / n;
    for (i, (label, col)) in chips.iter().enumerate() {
        small_pill(c, Rect::new(inner.x + i as f32 * (cw + 6.0), y, cw, 22.0), label, *col);
    }
    if !chips.is_empty() {
        y += 30.0;
    }
    for (swatch, label, v) in [(ROSE, "System", cpu.system), (acc, "User", cpu.user), (pal().ink(0.28), "Idle", cpu.idle)] {
        if y + 16.0 > inner.bottom() + 6.0 {
            break;
        }
        row(c, Rect::new(inner.x, y, inner.w, 16.0), Some(swatch), label, &format!("{v:.0}%"));
        y += 17.0;
    }

    // History chart + per-core bars.
    let top_h = (body.h * 0.5).round();
    let top = Rect::new(m.x, m.y, m.w, top_h - 4.0);
    let aside = format!("peak {:.0}%  \u{00b7}  avg {:.0}%", cpu.history.peak(), cpu.history.avg());
    let ch = card(c, top, "Utilisation \u{00b7} 60 s", &aside);
    area(c, ch, &[(&cpu.history, acc)], 100.0);

    let bot = Rect::new(m.x, m.y + top_h + 4.0, m.w, body.h - top_h - 4.0);
    let hybrid = cpu.is_hybrid();
    let aside = if hybrid {
        format!("E {:.0}%  \u{00b7}  P {:.0}%", cpu.class_load(CoreClass::Efficiency).unwrap_or(0.0), cpu.class_load(CoreClass::Performance).unwrap_or(0.0))
    } else {
        String::new()
    };
    let inner = card(c, bot, "Per core", &aside);
    let n = cpu.cores.len().max(1);
    let gap = if n > 24 { 1.5 } else { 3.0 };
    let bw = ((inner.w - gap * (n as f32 - 1.0)) / n as f32).max(2.0);
    for (i, &load) in cpu.cores.iter().enumerate() {
        let x = inner.x + i as f32 * (bw + gap);
        let col = match cpu.classes.get(i) {
            Some(CoreClass::Efficiency) => TEAL,
            Some(CoreClass::Performance) => INDIGO,
            _ => acc,
        };
        c.s.rect(Rect::new(x, inner.y, bw, inner.h), bw.min(3.0), pal().ink(0.07));
        let h = (inner.h * load / 100.0).max(2.0);
        c.s.rect(Rect::new(x, inner.bottom() - h, bw, h), bw.min(3.0), heat(col, load));
    }

    // Top processes.
    let inner = card(c, r, "Top processes", "CPU");
    let items = c.vm.sys.procs.clone();
    proc_list(c, inner, &items, pct1, acc, "Reading processes\u{2026}");
}

// -------------------------------------------------------------------- GPU

fn gpu_view(c: &mut Ctx, body: Rect) {
    let g = c.vm.sys.gpu.clone();
    if !g.available {
        c.empty_state(body, Icon::Chip, LIME, "No GPU detected", "Windows reported no hardware graphics adapter");
        return;
    }
    let [l, m, r] = cols(body);
    let inner = card(c, l, "Graphics", "");
    let rad = 46.0;
    let (cx, cy) = (inner.cx(), inner.y + rad + 6.0);
    ring(c, cx, cy, rad, 10.0, &[(g.util / 100.0, heat(LIME, g.util))]);
    c.text(format!("{:.0}%", g.util), Rect::new(cx - 40.0, cy - 17.0, 80.0, 28.0), 24.0, Weight::Semibold, pal().text, Align::Center);
    c.text("busy", Rect::new(cx - 40.0, cy + 10.0, 80.0, 14.0), 10.0, Weight::Regular, pal().text_3, Align::Center);
    let mut y = cy + rad + 12.0;
    c.text(g.name.clone(), Rect::new(inner.x - 4.0, y, inner.w + 8.0, 14.0), 10.5, Weight::Medium, pal().text_2, Align::Center);
    y += 24.0;
    if let Some(t) = g.temp_c {
        small_pill(c, Rect::new(inner.x, y, inner.w, 22.0), &format!("{t:.0}\u{00b0}C"), if t >= 85.0 { pal().red } else { pal().green });
        y += 30.0;
    }
    bar(
        c,
        Rect::new(inner.x, y, inner.w, 26.0),
        "Video memory",
        &format!("{} / {}", sysmon::fmt_gb(g.mem_used_gb), sysmon::fmt_gb(g.mem_total_gb)),
        g.mem_pct() / 100.0,
        heat(VIOLET, g.mem_pct()),
    );

    let top_h = (body.h * 0.5).round();
    let ch = card(c, Rect::new(m.x, m.y, m.w, top_h - 4.0), "Utilisation \u{00b7} 60 s", &format!("peak {:.0}%", g.history.peak()));
    area(c, ch, &[(&g.history, LIME)], 100.0);
    let ch = card(c, Rect::new(m.x, m.y + top_h + 4.0, m.w, body.h - top_h - 4.0), "Video memory \u{00b7} 60 s", &sysmon::fmt_gb(g.mem_used_gb));
    area(c, ch, &[(&g.mem_history, VIOLET)], g.mem_total_gb.max(0.5));

    let inner = card(c, r, "Engines", "");
    if g.engines.is_empty() {
        c.text("Idle", Rect::new(inner.x, inner.y + 2.0, inner.w, 16.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
    }
    let mut y = inner.y;
    for (name, v) in g.engines.iter().take(4) {
        bar(c, Rect::new(inner.x, y, inner.w, 26.0), name, &format!("{v:.0}%"), v / 100.0, LIME);
        y += 31.0;
    }
    let y0 = (inner.y + 4.0 * 31.0 + 4.0).max(y + 4.0);
    c.text("BY PROCESS", Rect::new(inner.x, y0, inner.w, 12.0), 9.0, Weight::Semibold, pal().text_3, Align::Left);
    proc_list(c, Rect::new(inner.x, y0 + 16.0, inner.w, inner.bottom() - y0 - 16.0), &g.procs, pct1, LIME, "No GPU activity");
}

// ----------------------------------------------------------------- memory

fn mem_view(c: &mut Ctx, body: Rect) {
    let [l, m, r] = cols(body);
    let mem = c.vm.sys.mem.clone();
    let inner = card(c, l, "Memory", "");
    let rad = 46.0;
    let (cx, cy) = (inner.cx(), inner.y + rad + 6.0);
    ring(c, cx, cy, rad, 10.0, &[(mem.pct() / 100.0, heat(VIOLET, mem.pct()))]);
    c.text(format!("{:.0}%", mem.pct()), Rect::new(cx - 40.0, cy - 17.0, 80.0, 28.0), 24.0, Weight::Semibold, pal().text, Align::Center);
    c.text(format!("of {}", sysmon::fmt_gb(mem.total_gb)), Rect::new(cx - 44.0, cy + 10.0, 88.0, 14.0), 10.0, Weight::Regular, pal().text_3, Align::Center);
    let mut y = cy + rad + 14.0;
    for (swatch, label, v) in
        [(VIOLET, "In use", sysmon::fmt_gb(mem.used_gb)), (pal().ink(0.28), "Available", sysmon::fmt_gb(mem.available_gb)), (INDIGO, "Cached", sysmon::fmt_gb(mem.cached_gb))]
    {
        row(c, Rect::new(inner.x, y, inner.w, 17.0), Some(swatch), label, &v);
        y += 19.0;
    }

    let top_h = (body.h * 0.55).round();
    let ch = card(c, Rect::new(m.x, m.y, m.w, top_h - 4.0), "In use \u{00b7} 60 s", &format!("peak {:.0}%", mem.history.peak()));
    area(c, ch, &[(&mem.history, VIOLET)], 100.0);
    let inner = card(c, Rect::new(m.x, m.y + top_h + 4.0, m.w, body.h - top_h - 4.0), "Commit charge", "");
    let frac = if mem.commit_limit_gb > 0.0 { mem.commit_used_gb / mem.commit_limit_gb } else { 0.0 };
    bar(
        c,
        Rect::new(inner.x, inner.y + 2.0, inner.w, 26.0),
        "Committed",
        &format!("{} / {}", sysmon::fmt_gb(mem.commit_used_gb), sysmon::fmt_gb(mem.commit_limit_gb)),
        frac,
        heat(INDIGO, frac * 100.0),
    );
    c.text("Virtual memory the system has promised to apps", Rect::new(inner.x, inner.y + 34.0, inner.w, 28.0), 10.0, Weight::Regular, pal().text_3, Align::Left);

    let inner = card(c, r, "Top processes", "Memory");
    proc_list(c, inner, &mem.procs, sysmon::fmt_gb, VIOLET, "Reading processes\u{2026}");
}

// ----------------------------------------------------------------- drives

fn drives_view(c: &mut Ctx, body: Rect) {
    let [l, m, r] = cols(body);
    let d = c.vm.sys.drives.clone();
    let inner = card(c, l, "Disk activity", "");
    let rad = 46.0;
    let (cx, cy) = (inner.cx(), inner.y + rad + 6.0);
    ring(c, cx, cy, rad, 10.0, &[(d.busy / 100.0, heat(GOLD, d.busy))]);
    c.text(format!("{:.0}%", d.busy), Rect::new(cx - 40.0, cy - 17.0, 80.0, 28.0), 24.0, Weight::Semibold, pal().text, Align::Center);
    c.text("active time", Rect::new(cx - 44.0, cy + 10.0, 88.0, 14.0), 10.0, Weight::Regular, pal().text_3, Align::Center);
    let mut y = cy + rad + 16.0;
    for (swatch, label, v) in [(GOLD, "Read", d.read_bps), (GOLD.mix(pal().text, 0.5), "Write", d.write_bps)] {
        row(c, Rect::new(inner.x, y, inner.w, 20.0), Some(swatch), label, &sysmon::fmt_rate(v));
        y += 23.0;
    }
    y += 6.0;
    c.s.line(inner.x, y, inner.right(), y, 1.0, pal().hairline);
    y += 8.0;
    for (label, v) in [("Peak read", d.read_hist.peak()), ("Peak write", d.write_hist.peak())] {
        row(c, Rect::new(inner.x, y, inner.w, 18.0), None, label, &sysmon::fmt_rate(v));
        y += 20.0;
    }

    let pk = d.read_hist.peak().max(d.write_hist.peak()).max(1.0e6) * 1.15;
    let ch = card(c, Rect::new(m.x, m.y, m.w, body.h), "Throughput \u{00b7} 60 s", &format!("peak {}", sysmon::fmt_rate(pk / 1.15)));
    let chart = Rect::new(ch.x, ch.y, ch.w, ch.h - 52.0);
    area(c, chart, &[(&d.read_hist, GOLD), (&d.write_hist, GOLD.mix(pal().text, 0.55))], pk);
    let ly = chart.bottom() + 10.0;
    row(c, Rect::new(ch.x, ly, ch.w, 16.0), Some(GOLD), "Read \u{00b7} avg", &sysmon::fmt_rate(d.read_hist.avg()));
    row(c, Rect::new(ch.x, ly + 17.0, ch.w, 16.0), Some(GOLD.mix(pal().text, 0.55)), "Write \u{00b7} avg", &sysmon::fmt_rate(d.write_hist.avg()));

    let inner = card(c, r, "Volumes", "");
    if d.volumes.is_empty() {
        c.text("No fixed drives", Rect::new(inner.x, inner.y + 2.0, inner.w, 16.0), 11.0, Weight::Regular, pal().text_3, Align::Left);
    }
    for (i, v) in d.volumes.iter().take(6).enumerate() {
        bar(
            c,
            Rect::new(inner.x, inner.y + i as f32 * 36.0, inner.w, 28.0),
            &v.label,
            &format!("{} / {}", sysmon::fmt_gb(v.used_gb), sysmon::fmt_gb(v.total_gb)),
            v.pct() / 100.0,
            heat(GOLD, v.pct()),
        );
    }
}

// ---------------------------------------------------------------- network

fn net_view(c: &mut Ctx, body: Rect) {
    let [l, m, r] = cols(body);
    let n = c.vm.sys.net.clone();
    let inner = card(c, l, "Throughput", "");
    let mut y = inner.y + 6.0;
    for (arrow, label, v, col) in [("\u{2193}", "Download", n.down_bps, CYAN), ("\u{2191}", "Upload", n.up_bps, CYAN.mix(pal().text, 0.5))] {
        c.s.circle(inner.x + 14.0, y + 22.0, 14.0, col.fade(0.16));
        c.text(arrow, Rect::new(inner.x, y + 8.0, 28.0, 28.0), 15.0, Weight::Bold, col, Align::Center);
        c.text(label, Rect::new(inner.x + 36.0, y + 2.0, inner.w - 36.0, 14.0), 10.0, Weight::Medium, pal().text_3, Align::Left);
        c.text(sysmon::fmt_rate(v), Rect::new(inner.x + 36.0, y + 16.0, inner.w - 36.0, 24.0), 18.0, Weight::Semibold, pal().text, Align::Left);
        y += 62.0;
    }
    c.s.line(inner.x, y - 4.0, inner.right(), y - 4.0, 1.0, pal().hairline);
    row(c, Rect::new(inner.x, y + 2.0, inner.w, 18.0), None, "Combined", &sysmon::fmt_rate(n.down_bps + n.up_bps));
    if !n.adapter.is_empty() {
        c.text(n.adapter.clone(), Rect::new(inner.x, inner.bottom() - 16.0, inner.w, 14.0), 10.0, Weight::Regular, pal().text_3, Align::Left);
    }

    let pk = n.down_hist.peak().max(n.up_hist.peak()).max(1.0e5) * 1.15;
    let ch = card(c, Rect::new(m.x, m.y, m.w, body.h), "Traffic \u{00b7} 60 s", &format!("peak {}", sysmon::fmt_rate(pk / 1.15)));
    let chart = Rect::new(ch.x, ch.y, ch.w, ch.h - 52.0);
    area(c, chart, &[(&n.down_hist, CYAN), (&n.up_hist, CYAN.mix(pal().text, 0.55))], pk);
    let ly = chart.bottom() + 10.0;
    row(c, Rect::new(ch.x, ly, ch.w, 16.0), Some(CYAN), "Download", &sysmon::fmt_rate(n.down_bps));
    row(c, Rect::new(ch.x, ly + 17.0, ch.w, 16.0), Some(CYAN.mix(pal().text, 0.55)), "Upload", &sysmon::fmt_rate(n.up_bps));

    let inner = card(c, r, "Session", "since launch");
    let mut y = inner.y + 2.0;
    for (label, v) in [
        ("Downloaded", sysmon::fmt_bytes(n.session_down)),
        ("Uploaded", sysmon::fmt_bytes(n.session_up)),
        ("Avg down", sysmon::fmt_rate(n.down_hist.avg())),
        ("Avg up", sysmon::fmt_rate(n.up_hist.avg())),
        ("Peak down", sysmon::fmt_rate(n.down_hist.peak())),
        ("Peak up", sysmon::fmt_rate(n.up_hist.peak())),
    ] {
        row(c, Rect::new(inner.x, y, inner.w, 20.0), None, label, &v);
        y += 25.0;
    }
}

// ---------------------------------------------------------------- battery

fn battery_view(c: &mut Ctx, body: Rect) {
    let b = c.vm.sys.battery.clone();
    if !b.present {
        c.empty_state(body, Icon::Bolt, pal().green, "No battery", "This PC runs on mains power");
        return;
    }
    let [l, m, r] = cols(body);
    let col = tab_color(SysTab::Battery, &c.vm.sys);
    let inner = card(c, l, "Battery", "");
    let rad = 46.0;
    let (cx, cy) = (inner.cx(), inner.y + rad + 6.0);
    ring(c, cx, cy, rad, 10.0, &[(b.pct / 100.0, col)]);
    c.text(format!("{:.0}%", b.pct), Rect::new(cx - 40.0, cy - 17.0, 80.0, 28.0), 24.0, Weight::Semibold, pal().text, Align::Center);
    if b.charging {
        icons::draw(&mut c.s, Icon::Bolt, cx, cy + 17.0, 12.0, col);
    } else {
        c.text("remaining", Rect::new(cx - 40.0, cy + 10.0, 80.0, 14.0), 10.0, Weight::Regular, pal().text_3, Align::Center);
    }
    let status = if b.charging {
        "Charging"
    } else if b.on_ac {
        "Plugged in"
    } else {
        "On battery"
    };
    small_pill(c, Rect::new(inner.x, cy + rad + 16.0, inner.w, 22.0), status, col);

    let inner = card(c, m, "Power", "");
    let time = match (b.charging, b.mins_left) {
        (false, Some(mn)) => sysmon::fmt_mins(mn),
        _ => "\u{2014}".to_string(),
    };
    let mut y = inner.y + 4.0;
    for (label, v) in [
        ("Source", if b.on_ac { "AC power" } else { "Battery" }.to_string()),
        ("Status", status.to_string()),
        ("Time left", time),
        ("Battery saver", if b.saver { "On" } else { "Off" }.to_string()),
        ("Charge", format!("{:.0}%", b.pct)),
    ] {
        row(c, Rect::new(inner.x, y, inner.w, 22.0), None, label, &v);
        y += 30.0;
    }

    let inner = card(c, r, "Level", "");
    battery_glyph(c, inner, b.pct, b.charging, col);
}

/// A tall battery: terminal cap, body, and a fill that rises with the charge.
fn battery_glyph(c: &mut Ctx, area: Rect, pct: f32, charging: bool, col: Color) {
    let (bw, bh) = (62.0, (area.h - 26.0).min(150.0));
    let body = Rect::new(area.cx() - bw / 2.0, area.y + 12.0 + (area.h - 26.0 - bh) / 2.0, bw, bh);
    c.s.rect(Rect::new(body.cx() - 10.0, body.y - 7.0, 20.0, 8.0), 3.0, pal().ink(0.22));
    c.s.rect(body, 14.0, pal().ink(0.07));
    c.s.stroke(body, 14.0, 2.0, pal().ink(0.22));
    let inner = Rect::new(body.x + 5.0, body.y + 5.0, body.w - 10.0, body.h - 10.0);
    let h = (inner.h * pct / 100.0).clamp(0.0, inner.h);
    if h > 1.0 {
        c.s.rect(Rect::new(inner.x, inner.bottom() - h, inner.w, h), 10.0, Fill::VGrad(col, col.mix(Color::rgb(0, 0, 0), 0.35)));
    }
    if charging {
        icons::draw(&mut c.s, Icon::Bolt, body.cx(), body.cy(), 22.0, WHITE.fade(0.92));
    }
    c.text(format!("{pct:.0}%"), Rect::new(area.x, body.bottom() + 8.0, area.w, 18.0), 13.0, Weight::Semibold, pal().text, Align::Center);
}
