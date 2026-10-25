//! Turns view-model + UI state + morph amount into one frame: a draw list and
//! the clickable regions that match it.

use super::eyes::{self, Paint, Spot};
use super::icons::{self, Icon};
use super::pages;
use super::scene::{Align, Color, Fill, Rect, Scene, Weight};
use super::state::{Action, Hit, UiState};
use super::theme::*;
use crate::viewmodel::{local_naive, mmss, ViewModel};
use teardrop_core::{DropletGeom, HudKind, IdleLook, Page, Playback, Settings};

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
        Page::Notes => Icon::Notes,
        Page::Clipboard => Icon::Clipboard,
        Page::Today => Icon::Calendar,
        Page::Speed => Icon::Gauge,
        Page::Usage => Icon::Chart,
        Page::Voice => Icon::Mic,
        Page::Ai => Icon::Sparkle,
        Page::Settings => Icon::Gear,
    }
}

fn tab_label_w(p: Page) -> f32 {
    match p {
        Page::Music => 88.0,
        Page::Focus => 86.0,
        Page::Water => 86.0,
        Page::Shelf => 82.0,
        Page::Tasks => 84.0,
        Page::Notes => 82.0,
        Page::Clipboard => 104.0,
        Page::Today => 84.0,
        Page::Speed => 84.0,
        Page::Usage => 84.0,
        Page::Voice => 82.0,
        Page::Ai => 64.0,
        Page::Settings => 36.0,
    }
}

/// Width of an inactive tab: eleven tabs plus the widest label must clear the gear.
pub const TAB_W: f32 = 27.0;
pub const HEADER_H: f32 = 34.0;
pub const HEADER_Y: f32 = 14.0;
pub const CONTENT_TOP: f32 = 64.0;
/// Extra width a level HUD (volume, brightness) gets when the face rides along.
pub const HUD_FACE_W: f32 = 40.0;

/// Collapsed-capsule lyric text size / line height.
pub const LYRIC_SIZE: f32 = 13.0;
pub const LYRIC_LH: f32 = 18.0;
/// Widest the music capsule may grow while showing a lyric line.
pub const LYRIC_MAX_W: f32 = 640.0;

/// Sublines + target capsule size for the current synced lyric, if one shows.
/// None on instrumental gaps, missing/disabled lyrics — callers fall back to
/// the title marquee. The wrap width targets LYRIC_MAX_W, not the animating
/// pill, so sublines never reflow mid-morph.
pub fn compact_lyric(vm: &ViewModel, s: &Settings) -> Option<(Vec<String>, f32, f32)> {
    if !vm.music_active() || !s.lyrics_enabled {
        return None;
    }
    let text = vm.lyric_idx.and_then(|i| vm.lyric_lines.get(i)).map(|l| l.text.trim())?;
    if text.is_empty() {
        return None;
    }
    let left = 46.0; // album art + gap
    let tail = if s.idle_look == IdleLook::Eyes { 44.0 + 42.0 } else { 44.0 } + 12.0;
    let max_text_w = LYRIC_MAX_W - left - tail;
    let sublines = pages::wrap_lyrics(text, max_text_w, LYRIC_SIZE, true);
    let text_w = sublines.iter().map(|l| est_w(l, LYRIC_SIZE) * 1.20).fold(0.0, f32::max);
    let w = (left + text_w + tail).clamp(232.0, LYRIC_MAX_W);
    let h = (sublines.len() as f32 * LYRIC_LH + 18.0).max(36.0);
    Some((sublines, w, h))
}

/// Content area of the open card for a card whose left edge is `card_x`.
pub fn content_rect(card_x: f32, geom: &DropletGeom) -> Rect {
    Rect::new(card_x + 22.0, CONTENT_TOP, geom.card_w - 44.0, geom.card_h - CONTENT_TOP - 18.0)
}

/// Pages drawn as self-contained blobs: no header row, the bottom strip
/// carries clock + page icons + pin/gear instead of the tab bar.
pub fn is_blob_page(p: Page) -> bool {
    p != Page::Settings
}

/// Content area of a blob page: the whole card minus padding and the nav strip.
fn blob_content_rect(card_x: f32, geom: &DropletGeom) -> Rect {
    Rect::new(card_x + 18.0, 14.0, geom.card_w - 36.0, geom.card_h - 14.0 - 44.0)
}

/// Bottom strip for blob pages: clock left, page dots centre, pin + gear right.
fn blob_nav(c: &mut Ctx, card_x: f32, geom: &DropletGeom) {
    let y = geom.card_h - 34.0;
    let narrow = geom.card_w < 480.0;
    let local = local_naive(c.vm.now);
    let fmt = if narrow { "%H:%M" } else { "%a %d %b  %H:%M" };
    c.s.text(local.format(fmt).to_string(), Rect::new(card_x + 18.0, y + 2.0, 110.0, 20.0), 11.0, Weight::Medium, TEXT_3, Align::Left);

    // Page icons centred in the space between the clock and pin/gear,
    // shrinking their pitch on narrow blobs so nothing overlaps.
    let n = Page::all().len();
    let left = if narrow { card_x + 62.0 } else { card_x + 150.0 };
    let right = card_x + geom.card_w - 70.0;
    let iw = ((right - left) / n as f32).min(26.0);
    let mut x = left + (right - left - iw * n as f32) / 2.0;
    for (i, &p) in Page::all().iter().enumerate() {
        let amt = c.ui.tab_amt[i];
        let r = Rect::new(x, y - 3.0, iw - 2.0, 24.0);
        let a = Action::Tab(p);
        let h = c.hov(a);
        if amt > 0.01 {
            c.s.rect(r, 9.0, accent(p).fade(0.22 * amt));
        } else if h > 0.01 {
            c.s.rect(r, 9.0, SURFACE_HOVER.fade(h));
        }
        icons::draw(&mut c.s, tab_icon(p), r.cx(), r.cy(), (iw - 9.0).clamp(10.0, 13.0), TEXT_3.mix(TEXT, h).mix(accent(p), amt));
        c.hit(r, a);
        x += iw;
    }

    let gr = Rect::new(card_x + geom.card_w - 18.0 - 24.0, y - 3.0, 24.0, 24.0);
    let on = c.vm.page == Page::Settings;
    let h = c.hov(Action::Gear);
    if on || h > 0.01 {
        c.s.rect(gr, 12.0, accent(Page::Settings).fade(if on { 0.22 } else { 0.35 * h }));
    }
    icons::draw(&mut c.s, Icon::Gear, gr.cx(), gr.cy(), 14.0, if on { TEXT } else { TEXT_2.mix(TEXT, h * 0.7) });
    c.hit(gr, Action::Gear);

    let pr = Rect::new(gr.x - 28.0, y, 22.0, 22.0);
    let ph = c.hov(Action::Pin);
    if c.vm.pinned {
        c.s.rect(pr, 11.0, accent(Page::Focus).fade(0.22));
    } else if ph > 0.01 {
        c.s.rect(pr, 11.0, SURFACE_HOVER.fade(ph));
    }
    icons::draw(&mut c.s, Icon::Pin, pr.cx(), pr.cy(), 12.0, if c.vm.pinned { accent(Page::Focus) } else { TEXT_2.mix(TEXT, ph * 0.7) });
    c.hit(pr, Action::Pin);
}

fn header(c: &mut Ctx, card_x: f32, geom: &DropletGeom) {
    // Live clock and date at the far left (e.g. "Sat 24 Oct  13:49").
    let local = local_naive(c.vm.now);
    let date_str = local.format("%a %d %b  %H:%M").to_string();
    let tr = Rect::new(card_x + 20.0, HEADER_Y + 7.0, 120.0, 20.0);
    c.s.text(date_str, tr, 11.5, Weight::Medium, TEXT_2, Align::Left);

    // Page tabs centred in the card; the active one widens to show its label.
    let widths: Vec<f32> = Page::all()
        .iter()
        .enumerate()
        .map(|(i, &p)| TAB_W + (tab_label_w(p) - TAB_W) * c.ui.tab_amt[i])
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + (widths.len() - 1) as f32 * 2.0;
    let mut x = card_x + (geom.card_w - total) / 2.0;
    for (i, &p) in Page::all().iter().enumerate() {
        let amt = c.ui.tab_amt[i];
        let w = widths[i];
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
        icons::draw(&mut c.s, tab_icon(p), r.x + 13.5 + 3.0 * amt, r.cy(), 15.0, ic);
        if amt > 0.12 {
            let lr = Rect::new(r.x + 32.0, r.y, (w - 32.0 - 6.0).max(0.0), r.h);
            c.s.text(p.title(), lr, 12.5, Weight::Semibold, acc.fade(smooth((amt - 0.2) / 0.8)), Align::Left);
        }
        c.hit(r, a);
        x += w + 2.0;
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

    // Pin toggle button, just left of the gear.
    let pr = Rect::new(gr.x - 32.0, HEADER_Y + 3.0, 26.0, HEADER_H - 6.0);
    let ph = c.hov(Action::Pin);
    let pin_on = c.vm.pinned;
    if pin_on {
        c.s.rect(pr, (HEADER_H - 6.0) / 2.0, accent(Page::Focus).fade(0.22));
    } else if ph > 0.01 {
        c.s.rect(pr, (HEADER_H - 6.0) / 2.0, SURFACE_HOVER.fade(ph));
    }
    let pin_ic = if pin_on { accent(Page::Focus) } else { TEXT_2.mix(TEXT, ph * 0.7) };
    icons::draw(&mut c.s, Icon::Pin, pr.cx(), pr.cy(), 13.5, pin_ic);
    c.hit(pr, Action::Pin);
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
    } else if vm.voice.phase == crate::voice::VoicePhase::Recording {
        let red = accent(Page::Voice);
        let pulse = 0.55 + 0.45 * (c.now_ms % 1000) as f32 / 1000.0 * std::f32::consts::PI;
        let pulse = pulse.sin().abs();
        c.s.circle(r.x + 22.0, cy, 8.0 + 3.0 * pulse, red.fade(0.25));
        c.s.circle(r.x + 22.0, cy, 5.0, red);
        c.text("REC", Rect::new(r.x + 38.0, cy - 10.0, 40.0, 20.0), 12.0, Weight::Semibold, red, Align::Left);
        c.text(crate::voice::clock(vm.voice.elapsed_ms), Rect::new(r.right() - 74.0, cy - 10.0, 60.0, 20.0), 14.0, Weight::Semibold, TEXT, Align::Right);
        if c.set.idle_look == IdleLook::Eyes {
            face(c, r, Spot::Rec, Some(red));
        }
    } else if let Some(ev) = vm.meeting_soon() {
        let acc = accent(Page::Today);
        icons::draw(&mut c.s, Icon::Calendar, r.x + 22.0, cy, 15.0, acc);
        let tr = Rect::new(r.x + 40.0, cy - 10.0, r.w - 40.0 - 78.0, 20.0);
        marquee(c, tr, &ev.title, 12.0, Weight::Medium, TEXT, c.now_ms);
        let left = ev.start - vm.now.timestamp();
        let col = if left <= 120 { AMBER } else { TEXT_2 };
        c.text(crate::calendar::countdown(ev, vm.now.timestamp()), Rect::new(r.right() - 74.0, cy - 10.0, 62.0, 20.0), 12.5, Weight::Semibold, col, Align::Right);
    } else if vm.focus_visible() && vm.music_active() {
        // Focus + music share the pill: ring, artwork, title/lyric,
        // visualizer, face and the countdown.
        let acc = accent(Page::Focus);
        let ring = (r.x + 24.0, cy);
        c.s.ring(ring.0, ring.1, 9.0, 2.6, 0.0, 1.0, Color::rgba(255, 255, 255, 0.16));
        c.s.ring(ring.0, ring.1, 9.0, 2.6, 0.0, vm.focus_progress().max(0.02), acc);
        let (c1, c2) = art_colors(&vm.track.title);
        let art = Rect::new(r.x + 44.0, cy - 11.0, 22.0, 22.0);
        c.s.rect(art, 6.5, Fill::DGrad(c1, c2));
        let acc_m = c1.mix(Color::rgb(255, 255, 255), 0.25);
        let t = mmss(vm.focus_remaining_ms());
        let col = if vm.focus_running() { TEXT } else { TEXT_2 };
        c.text(t, Rect::new(r.right() - 74.0, cy - 10.0, 60.0, 20.0), 14.0, Weight::Semibold, col, Align::Right);
        let eyes = c.set.idle_look == IdleLook::Eyes;
        let n = 5;
        let right = if eyes { eyes::place(Spot::FocusMusic, r).0 - 26.0 } else { r.right() - 82.0 };
        let x0 = right - n as f32 * 5.0;
        for (i, b) in c.ui.viz.sample(n).iter().enumerate() {
            let h = 3.0 + b * 15.0;
            c.s.rect(Rect::new(x0 + i as f32 * 5.0, cy - h / 2.0, 3.0, h), 1.5, acc_m);
        }
        if eyes {
            face(c, r, Spot::FocusMusic, Some(acc_m));
        }
        if let Some((sublines, _, _)) = compact_lyric(vm, c.set) {
            // Same karaoke sweep as the music-only capsule, shifted right to
            // make room for the focus ring.
            let frac = teardrop_core::line_progress(&vm.lyric_lines, vm.lyric_idx.unwrap_or(0), vm.track.position_ms);
            let sweeps = pages::karaoke_subline_progress(&sublines, frac);
            let block_h = sublines.len() as f32 * LYRIC_LH;
            let tx = art.right() + 8.0;
            let tw = ((x0 - 8.0) - tx).max(0.0);
            let mut ly = cy - block_h / 2.0;
            c.s.clip(r);
            for (subline, sub_frac) in sweeps {
                c.s.text_karaoke(subline.to_string(), Rect::new(tx, ly, tw, LYRIC_LH), LYRIC_SIZE, Weight::Semibold, TEXT_2, TEXT, sub_frac);
                ly += LYRIC_LH;
            }
            c.s.pop_clip();
        } else {
            let tr = Rect::new(art.right() + 8.0, cy - 10.0, (x0 - 8.0) - (art.right() + 8.0), 20.0);
            let line = format!("{} \u{2014} {}", vm.track.title, vm.track.display_artist());
            marquee(c, tr, &line, 12.0, Weight::Medium, TEXT, c.now_ms);
        }
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
        if let Some((sublines, _, _)) = compact_lyric(vm, c.set) {
            // Synced lyric, karaoke-swept, vertically centered. The sublines
            // come from compact_lyric (wrapped at the target size), never
            // re-wrapped on the animating r.w, so lines don't reflow.
            let frac = teardrop_core::line_progress(&vm.lyric_lines, vm.lyric_idx.unwrap_or(0), vm.track.position_ms);
            let sweeps = pages::karaoke_subline_progress(&sublines, frac);
            let block_h = sublines.len() as f32 * LYRIC_LH;
            let tx = art.right() + 10.0;
            let tw = ((x0 - 10.0) - tx).max(0.0);
            let mut ly = cy - block_h / 2.0;
            c.s.clip(r);
            for (subline, sub_frac) in sweeps {
                c.s.text_karaoke(subline.to_string(), Rect::new(tx, ly, tw, LYRIC_LH), LYRIC_SIZE, Weight::Semibold, TEXT_2, TEXT, sub_frac);
                ly += LYRIC_LH;
            }
            c.s.pop_clip();
        } else {
            // The title stays up the whole time; long ones scroll.
            let tr = Rect::new(art.right() + 10.0, cy - 10.0, (x0 - 10.0) - (art.right() + 10.0), 20.0);
            let line = format!("{} \u{2014} {}", vm.track.title, vm.track.display_artist());
            marquee(c, tr, &line, 12.0, Weight::Medium, TEXT, c.now_ms);
        }
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
    // Scroll only as far as the real text runs: tw carries padding for the fit test.
    let over = (tw / 1.15 * 1.08 - r.w + 10.0).max(8.0);
    let (hold, speed) = (2000.0_f32, 26.0_f32);
    let glide = over / speed * 1000.0;
    let cycle = hold * 2.0 + glide;
    // Reduce the epoch timestamp in integers first: as f32 it only resolves minutes.
    let m = (now_ms % cycle as u64) as f32;
    let off = if m < hold {
        0.0
    } else if m < hold + glide {
        let t = (m - hold) / glide;
        over * t * t * (3.0 - 2.0 * t)
    } else {
        over
    };
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

/// The glance pill: the wide flat capsule the island rests at mid-morph.
/// Shows the live activity with a couple of quick actions; clicking
/// anywhere expands to the full page.
fn glance(c: &mut Ctx, r: Rect) {
    let vm = c.vm;
    let cy = r.cy();

    if vm.voice.phase == crate::voice::VoicePhase::Recording {
        // REC: red dot, elapsed, stop button.
        let acc = accent(Page::Voice);
        c.s.circle(r.x + 28.0, cy, 7.0, acc);
        c.text("Recording", Rect::new(r.x + 44.0, cy - 13.0, 160.0, 18.0), 14.5, Weight::Semibold, TEXT, Align::Left);
        c.text(crate::voice::clock(vm.voice.elapsed_ms), Rect::new(r.x + 44.0, cy + 4.0, 160.0, 15.0), 11.0, Weight::Medium, TEXT_3, Align::Left);
        let sq = Rect::new(r.right() - 52.0, cy - 15.0, 30.0, 30.0);
        c.s.rect(sq, 10.0, acc.fade(0.2));
        c.s.rect(sq.inset(9.0), 3.0, acc);
        c.hit(sq, Action::VoiceToggle);
        return;
    }
    if let Some(ev) = vm.meeting_soon() {
        // Next meeting: icon, title, countdown, Join pill.
        let acc = accent(Page::Today);
        icons::draw(&mut c.s, Icon::Calendar, r.x + 28.0, cy, 19.0, acc);
        c.text(ev.title.clone(), Rect::new(r.x + 48.0, cy - 13.0, r.w - 190.0, 18.0), 13.5, Weight::Semibold, TEXT, Align::Left);
        c.text(crate::calendar::countdown(ev, vm.now.timestamp()), Rect::new(r.x + 48.0, cy + 4.0, 160.0, 15.0), 11.0, Weight::Medium, TEXT_3, Align::Left);
        let pill = Rect::new(r.right() - 86.0, cy - 13.0, 68.0, 26.0);
        c.s.rect(pill, 13.0, acc);
        c.text("Join", pill, 11.5, Weight::Semibold, Color::rgb(8, 16, 32), Align::Center);
        c.hit(pill, Action::MeetingJoin);
        return;
    }
    if vm.focus_visible() {
        // Focus: ring, label + session, big timer, pause/reset.
        let acc = accent(Page::Focus);
        let rcx = r.x + 30.0;
        c.s.ring(rcx, cy, 14.0, 4.0, 0.0, 1.0, Color::rgba(255, 255, 255, 0.16));
        let p = vm.focus_progress();
        if p > 0.003 {
            c.s.ring(rcx, cy, 14.0, 4.0, 0.0, p, acc);
        }
        let n = vm.pomo.sessions_before_long.max(1);
        let cur = (vm.pomo.completed_focus % n) + 1;
        c.text("Focus", Rect::new(r.x + 54.0, cy - 13.0, 150.0, 18.0), 14.0, Weight::Semibold, TEXT, Align::Left);
        let sub = match vm.pomo.phase {
            teardrop_core::Phase::ShortBreak => "Take a breather".to_string(),
            teardrop_core::Phase::LongBreak => "Long break".to_string(),
            _ => format!("Session {} of {}", cur.min(n), n),
        };
        c.text(sub, Rect::new(r.x + 54.0, cy + 4.0, 150.0, 15.0), 11.0, Weight::Medium, TEXT_3, Align::Left);
        c.text(mmss(vm.focus_remaining_ms()), Rect::new(r.right() - 196.0, cy - 17.0, 90.0, 30.0), 24.0, Weight::Semibold, TEXT, Align::Right);
        let label = if vm.focus_running() { "Pause" } else if vm.pomo.remaining_ms_when_paused.is_some() { "Resume" } else { "Start" };
        let pill = Rect::new(r.right() - 96.0, cy - 14.0, 78.0, 28.0);
        c.s.rect(pill, 14.0, acc);
        c.text(label, pill, 12.0, Weight::Semibold, Color::rgb(28, 18, 6), Align::Center);
        c.hit(pill, Action::FocusToggle);
        return;
    }
    if vm.music_active() {
        // Music: note tile, title, artist, viz, transport.
        let acc = accent(Page::Music);
        let t = &vm.track;
        let (c1, c2) = art_colors(&t.title);
        let art = Rect::new(r.x + 16.0, cy - 20.0, 40.0, 40.0);
        c.s.rect(art, 12.0, Fill::DGrad(c1, c2));
        icons::draw(&mut c.s, Icon::Note, art.cx(), art.cy(), 18.0, Color::rgb(255, 255, 255).fade(0.92));
        c.text(t.title.clone(), Rect::new(art.right() + 12.0, cy - 14.0, r.w - 230.0, 18.0), 13.5, Weight::Semibold, TEXT, Align::Left);
        c.text(t.display_artist(), Rect::new(art.right() + 12.0, cy + 4.0, r.w - 230.0, 15.0), 11.0, Weight::Regular, TEXT_3, Align::Left);
        let x0 = r.right() - 160.0;
        for (i, b) in c.ui.viz.sample(8).iter().enumerate() {
            let h = 4.0 + b * 16.0;
            c.s.rect(Rect::new(x0 + i as f32 * 6.0, cy - h / 2.0, 3.0, h), 1.5, acc.fade(0.85));
        }
        let px = r.right() - 108.0;
        icons::draw(&mut c.s, Icon::Prev, px, cy, 15.0, TEXT_2);
        c.hit(Rect::new(px - 11.0, cy - 13.0, 22.0, 26.0), Action::MediaPrev);
        let pp = if vm.track.playback == Playback::Playing { Icon::Pause } else { Icon::Play };
        c.s.circle(px + 36.0, cy, 15.0, Color::rgb(255, 255, 255));
        icons::draw(&mut c.s, pp, px + 36.0, cy, 15.0, Color::rgb(20, 20, 26));
        c.hit(Rect::new(px + 21.0, cy - 15.0, 30.0, 30.0), Action::MediaPlayPause);
        icons::draw(&mut c.s, Icon::Next, px + 66.0, cy, 15.0, TEXT_2);
        c.hit(Rect::new(px + 55.0, cy - 13.0, 22.0, 26.0), Action::MediaNext);
        return;
    }

    // Idle/other pages: page icon, name, one-line status, eyes face on the right.
    let acc = accent(vm.page);
    icons::draw(&mut c.s, tab_icon(vm.page), r.x + 28.0, cy, 18.0, acc);
    c.text(vm.page.title(), Rect::new(r.x + 46.0, cy - 13.0, 140.0, 18.0), 14.0, Weight::Semibold, TEXT, Align::Left);
    let sub = match vm.page {
        Page::Music => "Nothing playing".to_string(),
        Page::Focus => "Ready when you are".to_string(),
        Page::Water => {
            let d = vm.day();
            format!("{} / {} ml", vm.water.today(d), vm.water.goal_ml)
        }
        Page::Tasks => format!("{} open tasks", vm.tasks.open_count()),
        Page::Today => vm.calendar.agenda(vm.now.timestamp(), 1).first().map(|e| e.title.clone()).unwrap_or_else(|| "No meetings".to_string()),
        _ => local_naive(vm.now).format("%a %d %b  %H:%M").to_string(),
    };
    c.text(sub, Rect::new(r.x + 46.0, cy + 4.0, r.w - 160.0, 15.0), 11.0, Weight::Regular, TEXT_3, Align::Left);
    if c.set.idle_look == IdleLook::Eyes {
        face(c, r, Spot::Music, None);
    }
}

fn hud(c: &mut Ctx, r: Rect, h: &teardrop_core::Hud, cy: f32) {
    let left = r.x + 15.0;
    match h.kind {
        HudKind::Volume | HudKind::Muted | HudKind::Brightness => {
            let muted = h.kind == HudKind::Muted;
            let icon = match h.kind {
                HudKind::Muted => Icon::Muted,
                HudKind::Brightness => Icon::Sun,
                _ => Icon::Speaker,
            };
            icons::draw(&mut c.s, icon, left + 8.0, cy, 17.0, TEXT);
            // With Eyes the face rides along at the right end and watches the bar.
            let eyes = c.set.idle_look == IdleLook::Eyes;
            let tail = if eyes { 58.0 + HUD_FACE_W } else { 58.0 };
            let bx = r.x + 44.0;
            let bw = r.w - 44.0 - tail;
            let track = Rect::new(bx, cy - 3.0, bw, 6.0);
            c.s.rect(track, 3.0, Color::rgba(255, 255, 255, 0.18));
            let p = if muted { 0.0 } else { h.progress.unwrap_or(0.0).clamp(0.0, 1.0) };
            if p > 0.0 {
                c.s.rect(Rect::new(bx, cy - 3.0, (bw * p).max(6.0), 6.0), 3.0, TEXT);
            }
            c.text(h.detail.clone(), Rect::new(r.right() - tail + 2.0, cy - 10.0, 44.0, 20.0), 12.5, Weight::Semibold, TEXT, Align::Right);
            if eyes {
                face(c, r, Spot::Hud, None);
            }
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
        HudKind::Meeting => {
            let acc = accent(Page::Today);
            icons::draw(&mut c.s, Icon::Calendar, left + 8.0, cy, 17.0, acc);
            if h.action_label.is_some() {
                let tr = Rect::new(r.x + 42.0, cy - 10.0, r.w - 42.0 - 96.0, 20.0);
                c.text(h.title.clone(), tr, 13.0, Weight::Medium, TEXT, Align::Left);
                let pill = Rect::new(r.right() - 82.0, cy - 12.0, 70.0, 24.0);
                c.s.rect(pill, 12.0, acc);
                c.text("Join", pill, 11.5, Weight::Semibold, Color::rgb(8, 16, 32), Align::Center);
            } else {
                c.text(h.title.clone(), Rect::new(r.x + 42.0, cy - 10.0, r.w - 42.0 - 90.0, 20.0), 13.0, Weight::Medium, TEXT, Align::Left);
                c.text(h.detail.clone(), Rect::new(r.right() - 84.0, cy - 10.0, 72.0, 20.0), 12.0, Weight::Regular, TEXT_2, Align::Right);
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
            let eyes = c.set.idle_look == IdleLook::Eyes;
            let tail = if eyes { 14.0 + HUD_FACE_W } else { 14.0 };
            c.text(h.title.clone(), Rect::new(r.x + 42.0, cy - 10.0, 100.0, 20.0), 13.0, Weight::Medium, TEXT, Align::Left);
            c.text(h.detail.clone(), Rect::new(r.right() - tail - 120.0, cy - 10.0, 120.0, 20.0), 12.0, Weight::Regular, TEXT_2, Align::Right);
            if eyes {
                face(c, r, Spot::Hud, None);
            }
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
    if open < 0.56 {
        let a = match vm.live_hud(now_ms) {
            Some(h) if h.kind == HudKind::Water && h.action_label.is_some() => Action::HudWater,
            Some(h) if h.kind == HudKind::Meeting && h.action_label.is_some() => Action::MeetingJoin,
            _ => Action::Island,
        };
        c.hit(body, a);
    }

    // Glance pill: the middle stop of the morph (~open 0.5). Its content
    // fades in as the collapsed content fades out and hands off to the card.
    let ga = smooth(open / 0.36) * (1.0 - smooth((open - 0.56) / 0.22));
    if ga > 0.01 {
        c.s.clip(Rect::new(body.x + 1.0, 0.0, body.w - 2.0, body.h));
        // Quick actions are only live once the pill is resting at the glance.
        c.live = (open - 0.5).abs() < 0.08;
        c.s.set_transform(0.0, 0.0, ga);
        glance(&mut c, body);
        c.s.reset_transform();
        c.live = true;
        c.s.pop_clip();
    }

    // Open card.
    let oa = smooth((open - 0.80) / 0.18);
    if oa > 0.01 {
        let card_x = cx - g.card_w / 2.0;
        c.s.clip(Rect::new(body.x + 1.0, 0.0, body.w - 2.0, body.h - 1.0));
        c.s.set_transform(0.0, 0.0, oa);
        c.live = oa > 0.9;
        if is_blob_page(vm.page) {
            blob_nav(&mut c, card_x, g);
        } else {
            header(&mut c, card_x, g);
        }
        c.s.reset_transform();

        let area_of = |p: Page| if is_blob_page(p) { blob_content_rect(card_x, g) } else { content_rect(card_x, g) };
        let ease = ui.page_ease();
        let sliding = ui.page_t < 1.0;
        if sliding {
            c.live = false;
            c.s.set_transform(-ui.page_dir * 26.0 * ease, 0.0, oa * (1.0 - ease));
            pages::draw(&mut c, ui.prev_page, area_of(ui.prev_page));
        }
        c.live = oa > 0.9 && !sliding;
        let (dx, a) = if sliding { (ui.page_dir * 26.0 * (1.0 - ease), oa * ease) } else { (0.0, oa) };
        c.s.set_transform(dx, 0.0, a);
        pages::draw(&mut c, vm.page, area_of(vm.page));
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
