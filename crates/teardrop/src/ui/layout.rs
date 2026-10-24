//! Turns view-model + UI state + morph amount into one frame: a draw list and
//! the clickable regions that match it.

use super::eyes::{self, Paint, Spot};
use super::icons::{self, Icon};
use super::pages;
use super::scene::{Align, Color, Fill, Rect, Scene, Weight};
use super::state::{Action, Hit, UiState};
use super::theme::*;
use crate::viewmodel::{mmss, ViewModel};
use teardrop_core::{DropletGeom, HudKind, IdleLook, Page, Settings};

/// Morph input from the island spring.
#[derive(Debug, Clone, Copy)]
pub struct IslandView {
    pub geom: DropletGeom,
    pub open: f32,
    pub wobble_px: f32,
}

pub struct Frame {
    pub scene: Scene,
    pub hits: Vec<Hit>,
    /// Window size in logical px (island + shadow padding).
    pub w: f32,
    pub h: f32,
}

/// Paint context handed to every page / widget.
pub struct Ctx<'a> {
    pub s: Scene,
    pub hits: Vec<Hit>,
    pub vm: &'a ViewModel,
    pub set: &'a Settings,
    pub ui: &'a UiState,
    pub now_ms: u64,
    /// Register hit regions (off while a page is sliding out).
    pub live: bool,
}

pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rough text width, used only where we must lay out around a string.
pub fn est_w(s: &str, size: f32) -> f32 {
    s.chars()
        .map(|c| match c {
            'm' | 'w' | 'M' | 'W' => 0.80,
            'A'..='Z' => 0.64,
            '0'..='9' => 0.56,
            ' ' | 'i' | 'l' | 'j' | 't' | 'f' | 'r' | '.' | ',' | ':' | ';' | '|' | '!' | '\'' => 0.30,
            _ => 0.53,
        })
        .sum::<f32>()
        * size
}

impl<'a> Ctx<'a> {
    pub fn hit(&mut self, rect: Rect, action: Action) {
        if self.live {
            self.hits.push(Hit { rect, action, text: false });
        }
    }

    pub fn hit_text(&mut self, rect: Rect, action: Action) {
        if self.live {
            self.hits.push(Hit { rect, action, text: true });
        }
    }

    pub fn hov(&self, a: Action) -> f32 {
        self.ui.hover_amt(a)
    }

    pub fn pressed(&self, a: Action) -> bool {
        self.ui.pressed == Some(a)
    }

    pub fn text(&mut self, s: impl Into<String>, r: Rect, size: f32, weight: Weight, color: Color, align: Align) {
        self.s.text(s, r, size, weight, color, align);
    }

    /// Soft-surface background that brightens on hover and press.
    pub fn surface_fill(&self, a: Action) -> Color {
        let h = self.hov(a);
        let base = if self.pressed(a) { SURFACE_PRESS } else { SURFACE.mix(SURFACE_HOVER, h) };
        base
    }

    /// Round icon button.
    pub fn icon_btn(&mut self, r: Rect, a: Action, icon: Icon, size: f32, color: Color) {
        let fill = self.surface_fill(a);
        self.s.rect(r, r.h.min(r.w) / 2.0, fill);
        let c = color.mix(TEXT, self.hov(a) * 0.5);
        icons::draw(&mut self.s, icon, r.cx(), r.cy(), size, c);
        self.hit(r, a);
    }

    /// Icon without a plate (shows a plate only on hover).
    pub fn ghost_icon(&mut self, r: Rect, a: Action, icon: Icon, size: f32, color: Color) {
        let h = self.hov(a);
        if h > 0.01 || self.pressed(a) {
            let f = if self.pressed(a) { SURFACE_PRESS } else { SURFACE_HOVER.fade(h) };
            self.s.rect(r, r.h.min(r.w) / 2.0, f);
        }
        icons::draw(&mut self.s, icon, r.cx(), r.cy(), size, color.mix(TEXT, h * 0.6));
        self.hit(r, a);
    }

    /// Text pill. `fill` = Some(accent) draws a solid CTA.
    pub fn pill(&mut self, r: Rect, a: Action, label: &str, fill: Option<Color>, size: f32) {
        let h = self.hov(a);
        match fill {
            Some(c) => {
                let c = c.mix(Color::rgb(255, 255, 255), h * 0.18);
                let c = if self.pressed(a) { c.mix(Color::rgb(0, 0, 0), 0.18) } else { c };
                self.s.rect(r, r.h / 2.0, c);
                self.text(label, r, size, Weight::Semibold, Color::rgb(12, 12, 16), Align::Center);
            }
            None => {
                let f = self.surface_fill(a);
                self.s.rect(r, r.h / 2.0, f);
                self.text(label, r, size, Weight::Semibold, TEXT.fade(0.78 + 0.22 * h), Align::Center);
            }
        }
        self.hit(r, a);
    }

    /// iOS-style switch driven by the animated toggle amount.
    pub fn switch(&mut self, r: Rect, k: super::state::SettingKey, accent: Color) {
        let t = self.ui.toggle_amt(k);
        let a = Action::Toggle(k);
        let track = SURFACE_PRESS.mix(accent, t);
        self.s.rect(r, r.h / 2.0, track);
        let pad = 2.5;
        let d = r.h - pad * 2.0;
        let x = r.x + pad + (r.w - d - pad * 2.0) * t;
        self.s.circle(x + d / 2.0, r.cy(), d / 2.0 + self.hov(a) * 0.6, Color::rgb(255, 255, 255));
        self.hit(r, a);
    }

    pub fn section_title(&mut self, r: Rect, title: &str, sub: &str) {
        self.text(title, Rect::new(r.x, r.y, r.w, 24.0), 17.0, Weight::Semibold, TEXT, Align::Left);
        if !sub.is_empty() {
            self.text(sub, Rect::new(r.x, r.y + 24.0, r.w, 18.0), 12.0, Weight::Regular, TEXT_2, Align::Left);
        }
    }

    /// Centered empty-state block.
    pub fn empty_state(&mut self, r: Rect, icon: Icon, accent: Color, title: &str, sub: &str) {
        let cy = r.cy() - 14.0;
        self.s.circle(r.cx(), cy - 6.0, 28.0, accent.fade(0.12));
        icons::draw(&mut self.s, icon, r.cx(), cy - 6.0, 26.0, accent);
        self.text(title, Rect::new(r.x, cy + 30.0, r.w, 22.0), 14.0, Weight::Semibold, TEXT, Align::Center);
        self.text(sub, Rect::new(r.x, cy + 52.0, r.w, 18.0), 12.0, Weight::Regular, TEXT_3, Align::Center);
    }
}

fn tab_icon(p: Page) -> Icon {
    match p {
        Page::Music => Icon::Note,
        Page::Focus => Icon::Timer,
        Page::Water => Icon::Drop,
        Page::Shelf => Icon::Tray,
        Page::Tasks => Icon::List,
        Page::Clipboard => Icon::Clipboard,
        Page::Today => Icon::Calendar,
        Page::Speed => Icon::Gauge,
        Page::Settings => Icon::Gear,
    }
}

fn tab_label_w(p: Page) -> f32 {
    match p {
        Page::Music => 90.0,
        Page::Focus => 88.0,
        Page::Water => 88.0,
        Page::Shelf => 84.0,
        Page::Tasks => 86.0,
        Page::Clipboard => 108.0,
        Page::Today => 86.0,
        Page::Speed => 86.0,
        Page::Settings => 36.0,
    }
}

pub const HEADER_H: f32 = 34.0;
pub const HEADER_Y: f32 = 14.0;
pub const CONTENT_TOP: f32 = 64.0;

/// Content area of the open card for a card whose left edge is `card_x`.
pub fn content_rect(card_x: f32, geom: &DropletGeom) -> Rect {
    Rect::new(card_x + 22.0, CONTENT_TOP, geom.card_w - 44.0, geom.card_h - CONTENT_TOP - 18.0)
}

fn header(c: &mut Ctx, card_x: f32, geom: &DropletGeom) {
    let mut x = card_x + 18.0;
    for (i, &p) in Page::all().iter().enumerate() {
        let amt = c.ui.tab_amt[i];
        let w = 36.0 + (tab_label_w(p) - 36.0) * amt;
        let r = Rect::new(x, HEADER_Y, w, HEADER_H);
        let a = Action::Tab(p);
        let h = c.hov(a);
        let acc = accent(p);
        if amt > 0.01 {
            c.s.rect(r, HEADER_H / 2.0, acc.fade(0.17 * amt));
        }
        if h > 0.01 && amt < 0.99 {
            c.s.rect(r, HEADER_H / 2.0, SURFACE_HOVER.fade(h * (1.0 - amt)));
        }
        let ic = TEXT_2.mix(TEXT, h * 0.7).mix(acc, amt);
        icons::draw(&mut c.s, tab_icon(p), r.x + 18.0, r.cy(), 16.0, ic);
        if amt > 0.12 {
            let lr = Rect::new(r.x + 35.0, r.y, (w - 35.0 - 6.0).max(0.0), r.h);
            c.s.text(p.title(), lr, 12.5, Weight::Semibold, acc.fade(smooth((amt - 0.2) / 0.8)), Align::Left);
        }
        c.hit(r, a);
        x += w + 4.0;
    }
    // Settings gear, far right.
    let gr = Rect::new(card_x + geom.card_w - 18.0 - HEADER_H, HEADER_Y, HEADER_H, HEADER_H);
    let on = c.vm.page == Page::Settings;
    let h = c.hov(Action::Gear);
    if on {
        c.s.rect(gr, HEADER_H / 2.0, accent(Page::Settings).fade(0.22));
    } else if h > 0.01 {
        c.s.rect(gr, HEADER_H / 2.0, SURFACE_HOVER.fade(h));
    }
    let ic = if on { TEXT } else { TEXT_2.mix(TEXT, h * 0.7) };
    icons::draw(&mut c.s, Icon::Gear, gr.cx(), gr.cy(), 16.0, ic);
    if c.vm.update.phase == crate::update::UpdatePhase::Available {
        c.s.circle(gr.right() - 6.0, gr.y + 7.0, 4.5, accent(Page::Speed));
    }
    c.hit(gr, Action::Gear);
}

// ---------------------------------------------------------------- collapsed

fn collapsed(c: &mut Ctx, r: Rect, a: f32) {
    if a <= 0.01 {
        return;
    }
    c.s.set_transform(0.0, 0.0, a);
    let cy = r.cy();
    let vm = c.vm;
    if let Some(h) = vm.live_hud(c.now_ms) {
        hud(c, r, h, cy);
    } else if vm.focus_visible() {
        let p = Page::Focus;
        let acc = accent(p);
        let ring = (r.x + 24.0, cy);
        c.s.ring(ring.0, ring.1, 9.0, 2.6, 0.0, 1.0, Color::rgba(255, 255, 255, 0.16));
        c.s.ring(ring.0, ring.1, 9.0, 2.6, 0.0, vm.focus_progress().max(0.02), acc);
        let label = match vm.pomo.phase {
            teardrop_core::Phase::Focus => "Focus",
            teardrop_core::Phase::ShortBreak => "Break",
            _ => "Long break",
        };
        if c.set.idle_look == IdleLook::Eyes {
            face(c, r, Spot::Focus, None);
        } else {
            c.text(label, Rect::new(r.x + 42.0, cy - 10.0, 60.0, 20.0), 12.0, Weight::Medium, TEXT_2, Align::Left);
        }
        let t = mmss(vm.focus_remaining_ms());
        let col = if vm.focus_running() { TEXT } else { TEXT_2 };
        c.text(t, Rect::new(r.right() - 74.0, cy - 10.0, 60.0, 20.0), 14.0, Weight::Semibold, col, Align::Right);
    } else if vm.music_active() {
        let (c1, c2) = art_colors(&vm.track.title);
        let art = Rect::new(r.x + 10.0, cy - 11.0, 22.0, 22.0);
        c.s.rect(art, 6.5, Fill::DGrad(c1, c2));
        let acc = c1.mix(Color::rgb(255, 255, 255), 0.25);
        // Visualizer bars always; with Eyes they sit just left of the face.
        let eyes = c.set.idle_look == IdleLook::Eyes;
        let n = if eyes { 5 } else { 7 };
        let right = if eyes { r.right() - 56.0 } else { r.right() - 14.0 };
        let x0 = right - n as f32 * 5.0;
        for (i, b) in c.ui.viz.sample(n).iter().enumerate() {
            let h = 3.0 + b * 15.0;
            c.s.rect(Rect::new(x0 + i as f32 * 5.0, cy - h / 2.0, 3.0, h), 1.5, acc);
        }
        if eyes {
            face(c, r, Spot::Music, Some(acc));
        }
        // The title stays up the whole time; long ones scroll.
        let tr = Rect::new(art.right() + 10.0, cy - 10.0, (x0 - 10.0) - (art.right() + 10.0), 20.0);
        let line = format!("{} \u{2014} {}", vm.track.title, vm.track.display_artist());
        marquee(c, tr, &line, 12.0, Weight::Medium, TEXT, c.now_ms);
    } else {
        match c.set.idle_look {
            IdleLook::Clock => c.text(vm.clock_text(), r, 13.5, Weight::Semibold, TEXT, Align::Center),
            IdleLook::Eyes => face(c, r, Spot::Idle, None),
            IdleLook::Minimal => c.s.circle(r.cx(), cy, 2.8, TEXT_3),
            IdleLook::Hidden => {}
        }
    }
    c.s.reset_transform();
}

/// One line of text that fits `r`, or scrolls through it (hold, glide, hold).
fn marquee(c: &mut Ctx, r: Rect, s: &str, size: f32, weight: Weight, color: Color, now_ms: u64) {
    // est_w runs a little short for Segoe UI; pad it so nothing gets ellipsized.
    let tw = est_w(s, size) * 1.15;
    if tw <= r.w {
        c.text(s, r, size, weight, color, Align::Left);
        return;
    }
    let over = tw - r.w + 8.0;
    let (hold, speed) = (1500.0, 34.0);
    let glide = over / speed * 1000.0;
    let cycle = hold * 2.0 + glide;
    let m = (now_ms as f32) % cycle;
    let off = if m < hold { 0.0 } else if m < hold + glide { (m - hold) / 1000.0 * speed } else { over };
    c.s.clip(r);
    c.text(s, Rect::new(r.x - off, r.y, tw + 24.0, r.h), size, weight, color, Align::Left);
    c.s.pop_clip();
}

fn face(c: &mut Ctx, r: Rect, spot: Spot, accent: Option<Color>) {
    let (cx, cy, scale) = eyes::place(spot, r);
    c.s.clip(r);
    c.ui.eyes.draw(&mut c.s, &Paint { cx, cy, scale, bounds: r, style: c.set.eye_style, tint: None, accent, props: spot == Spot::Idle });
    c.s.pop_clip();
}

fn hud(c: &mut Ctx, r: Rect, h: &teardrop_core::Hud, cy: f32) {
    let left = r.x + 15.0;
    match h.kind {
        HudKind::Volume | HudKind::Muted => {
            let muted = h.kind == HudKind::Muted;
            let icon = if muted { Icon::Muted } else { Icon::Speaker };
            icons::draw(&mut c.s, icon, left + 8.0, cy, 17.0, TEXT);
            let bx = r.x + 44.0;
            let bw = r.w - 44.0 - 58.0;
            let track = Rect::new(bx, cy - 3.0, bw, 6.0);
            c.s.rect(track, 3.0, Color::rgba(255, 255, 255, 0.18));
            let p = if muted { 0.0 } else { h.progress.unwrap_or(0.0).clamp(0.0, 1.0) };
            if p > 0.0 {
                c.s.rect(Rect::new(bx, cy - 3.0, (bw * p).max(6.0), 6.0), 3.0, TEXT);
            }
            c.text(h.detail.clone(), Rect::new(r.right() - 56.0, cy - 10.0, 44.0, 20.0), 12.5, Weight::Semibold, TEXT, Align::Right);
        }
        HudKind::Water => {
            let acc = accent(Page::Water);
            icons::draw(&mut c.s, Icon::Drop, left + 8.0, cy, 17.0, acc);
            if h.action_label.is_some() {
                c.text("Time to hydrate", Rect::new(r.x + 42.0, cy - 10.0, r.w - 42.0 - 92.0, 20.0), 13.0, Weight::Medium, TEXT, Align::Left);
                let pill = Rect::new(r.right() - 82.0, cy - 12.0, 70.0, 24.0);
                c.s.rect(pill, 12.0, acc);
                c.text("+250 ml", pill, 11.5, Weight::Semibold, Color::rgb(8, 24, 32), Align::Center);
            } else {
                c.text(h.detail.clone(), Rect::new(r.x + 42.0, cy - 10.0, r.w - 42.0 - 14.0, 20.0), 13.0, Weight::Medium, TEXT, Align::Left);
            }
        }
        _ => {
            let (icon, col) = match h.kind {
                HudKind::Charging => (Icon::Bolt, GREEN),
                HudKind::Bluetooth => (Icon::Headphones, accent(Page::Today)),
                HudKind::Timer => (Icon::Timer, accent(Page::Focus)),
                _ => (Icon::Note, TEXT),
            };
            icons::draw(&mut c.s, icon, left + 8.0, cy, 17.0, col);
            c.text(h.title.clone(), Rect::new(r.x + 42.0, cy - 10.0, 100.0, 20.0), 13.0, Weight::Medium, TEXT, Align::Left);
            c.text(h.detail.clone(), Rect::new(r.right() - 14.0 - 120.0, cy - 10.0, 120.0, 20.0), 12.0, Weight::Regular, TEXT_2, Align::Right);
        }
    }
}

// --------------------------------------------------------------------- build

pub fn build(vm: &ViewModel, set: &Settings, ui: &UiState, isl: &IslandView, now_ms: u64) -> Frame {
    let g = &isl.geom;
    let open = isl.open;
    let bw = g.width(open);
    let bh = g.height(open, isl.wobble_px);
    let w = bw + PAD * 2.0;
    let h = bh + PAD;
    let cx = w / 2.0;
    let body = Rect::new(cx - bw / 2.0, 0.0, bw, bh);

    let mut c = Ctx { s: Scene::new(), hits: Vec::new(), vm, set, ui, now_ms, live: true };

    let pts: Vec<(f32, f32)> = g.outline(cx, 0.0, open, isl.wobble_px, 120).into_iter().map(|p| (p.x, p.y)).collect();
    c.s.body(pts, Fill::VGrad(BODY_TOP, BODY_BOTTOM), RIM, 0.45 + 0.55 * smooth(open));

    // Collapsed activity.
    let ca = 1.0 - smooth(open / 0.28);
    collapsed(&mut c, body, ca);
    if open < 0.5 {
        let a = match vm.live_hud(now_ms) {
            Some(h) if h.kind == HudKind::Water && h.action_label.is_some() => Action::HudWater,
            _ => Action::Island,
        };
        c.hit(body, a);
    }

    // Open card.
    let oa = smooth((open - 0.80) / 0.18);
    if oa > 0.01 {
        let card_x = cx - g.card_w / 2.0;
        c.s.clip(Rect::new(body.x + 1.0, 0.0, body.w - 2.0, body.h - 1.0));
        c.s.set_transform(0.0, 0.0, oa);
        c.live = oa > 0.9;
        header(&mut c, card_x, g);
        c.s.reset_transform();

        let area = content_rect(card_x, g);
        let ease = ui.page_ease();
        let sliding = ui.page_t < 1.0;
        if sliding {
            c.live = false;
            c.s.set_transform(-ui.page_dir * 26.0 * ease, 0.0, oa * (1.0 - ease));
            pages::draw(&mut c, ui.prev_page, area);
        }
        c.live = oa > 0.9 && !sliding;
        let (dx, a) = if sliding { (ui.page_dir * 26.0 * (1.0 - ease), oa * ease) } else { (0.0, oa) };
        c.s.set_transform(dx, 0.0, a);
        pages::draw(&mut c, vm.page, area);
        c.s.reset_transform();

        if let Some((msg, _)) = &ui.toast {
            c.live = false;
            c.s.set_transform(0.0, 0.0, oa);
            let tw = est_w(msg, 12.5) + 28.0;
            let tr = Rect::new(cx - tw / 2.0, g.card_h - 46.0, tw, 28.0);
            c.s.rect(tr, 14.0, Color::rgba(40, 40, 48, 0.96));
            c.s.stroke(tr, 14.0, 1.0, HAIRLINE);
            c.text(msg.clone(), tr, 12.5, Weight::Medium, TEXT, Align::Center);
            c.s.reset_transform();
        }
        c.s.pop_clip();
    }

    Frame { scene: c.s, hits: c.hits, w, h }
}

pub fn hit_test(hits: &[Hit], x: f32, y: f32) -> Option<Hit> {
    hits.iter().rev().find(|h| h.rect.contains(x, y)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn frame(page: Page, open: f32) -> Frame {
        let now = Utc::now();
        let mut vm = ViewModel::demo(now);
        vm.page = page;
        let set = Settings::default();
        let mut ui = UiState::new(page);
        let now_ms = now.timestamp_millis() as u64;
        for _ in 0..120 {
            ui.tick(1.0 / 60.0, &vm, &set, now_ms);
        }
        let isl = IslandView { geom: DropletGeom::default(), open, wobble_px: 0.0 };
        build(&vm, &set, &ui, &isl, now_ms)
    }

    #[test]
    fn every_page_draws_and_registers_hits_when_open() {
        for &p in Page::all().iter().chain([Page::Settings].iter()) {
            let f = frame(p, 1.0);
            assert!(f.scene.cmds.len() > 20, "{p:?} drew only {} cmds", f.scene.cmds.len());
            assert!(f.hits.iter().any(|h| matches!(h.action, Action::Tab(_))), "{p:?} has no tabs");
            assert!(f.hits.iter().any(|h| h.action == Action::Gear));
        }
    }

    #[test]
    fn collapsed_exposes_one_island_hit_and_no_tabs() {
        let f = frame(Page::Music, 0.0);
        assert!(f.hits.iter().any(|h| h.action == Action::Island));
        assert!(!f.hits.iter().any(|h| matches!(h.action, Action::Tab(_))));
    }

    #[test]
    fn hits_stay_inside_the_window() {
        for &p in Page::all().iter().chain([Page::Settings].iter()) {
            let f = frame(p, 1.0);
            for h in &f.hits {
                assert!(h.rect.x >= 0.0 && h.rect.right() <= f.w + 0.5, "{p:?} {:?} x-overflow {:?} vs {}", h.action, h.rect, f.w);
                assert!(h.rect.y >= 0.0 && h.rect.bottom() <= f.h, "{p:?} {:?} y-overflow {:?} vs {}", h.action, h.rect, f.h);
            }
        }
    }

    #[test]
    fn midway_morph_has_no_interactive_content() {
        let f = frame(Page::Music, 0.4);
        assert!(!f.hits.iter().any(|h| matches!(h.action, Action::Tab(_) | Action::MediaPlayPause)));
    }

    #[test]
    fn hit_test_prefers_topmost() {
        let hits = vec![
            Hit { rect: Rect::new(0.0, 0.0, 100.0, 100.0), action: Action::Island, text: false },
            Hit { rect: Rect::new(10.0, 10.0, 20.0, 20.0), action: Action::Gear, text: false },
        ];
        assert_eq!(hit_test(&hits, 15.0, 15.0).map(|h| h.action), Some(Action::Gear));
        assert_eq!(hit_test(&hits, 60.0, 60.0).map(|h| h.action), Some(Action::Island));
        assert!(hit_test(&hits, 200.0, 200.0).is_none());
    }
}
