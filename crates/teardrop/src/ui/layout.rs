//! Turns view-model + UI state + morph amount into one frame: a draw list and
//! the clickable regions that match it.

use super::eyes::{self, Paint, Spot};
use super::icons::{self, Icon};
use super::pages;
use super::scene::{Align, Color, Fill, Rect, Scene, Weight};
use super::state::{Action, Hit, UiState};
use super::theme::*;
use crate::viewmodel::{local_naive, mmss, one_line, ViewModel};
use teardrop_core::{DropletGeom, HudKind, IdleLook, IslandStyle, Page, Playback, Settings};

/// Morph input from the island springs: live size + content-layer alphas.
#[derive(Debug, Clone, Copy)]
pub struct IslandView {
    pub geom: DropletGeom,
    /// Derived morph progress 0..1 (shadow strength, HUD checks).
    pub open: f32,
    pub w: f32,
    pub h: f32,
    pub compact_a: f32,
    pub glance_a: f32,
    pub card_a: f32,
}

impl IslandView {
    /// Static frame at morph position `open` (tests, selftest, demo stills).
    pub fn at(geom: DropletGeom, open: f32) -> Self {
        Self {
            geom,
            open,
            w: geom.width(open),
            h: geom.height(open),
            compact_a: 1.0 - smooth(open / 0.28),
            glance_a: 1.0 - smooth(((open - 0.5).abs() - 0.04) / 0.14),
            card_a: smooth((open - 0.80) / 0.18),
        }
    }
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

        if self.pressed(a) {
            pal().surface_press
        } else {
            pal().surface.mix(pal().surface_hover, h)
        }
    }

    /// Round icon button.
    pub fn icon_btn(&mut self, r: Rect, a: Action, icon: Icon, size: f32, color: Color) {
        let fill = self.surface_fill(a);
        self.s.rect(r, r.h.min(r.w) / 2.0, fill);
        let c = color.mix(pal().text, self.hov(a) * 0.5);
        icons::draw(&mut self.s, icon, r.cx(), r.cy(), size, c);
        self.hit(r, a);
    }

    /// Icon without a plate (shows a plate only on hover).
    pub fn ghost_icon(&mut self, r: Rect, a: Action, icon: Icon, size: f32, color: Color) {
        let h = self.hov(a);
        if h > 0.01 || self.pressed(a) {
            let f = if self.pressed(a) { pal().surface_press } else { pal().surface_hover.fade(h) };
            self.s.rect(r, r.h.min(r.w) / 2.0, f);
        }
        icons::draw(&mut self.s, icon, r.cx(), r.cy(), size, color.mix(pal().text, h * 0.6));
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
                self.text(label, r, size, Weight::Semibold, pal().text.fade(0.78 + 0.22 * h), Align::Center);
            }
        }
        self.hit(r, a);
    }

    /// iOS-style switch driven by the animated toggle amount.
    pub fn switch(&mut self, r: Rect, k: super::state::SettingKey, accent: Color) {
        let t = self.ui.toggle_amt(k);
        let a = Action::Toggle(k);
        let track = pal().surface_press.mix(accent, t);
        self.s.rect(r, r.h / 2.0, track);
        let pad = 2.5;
        let d = r.h - pad * 2.0;
        let x = r.x + pad + (r.w - d - pad * 2.0) * t;
        self.s.circle(x + d / 2.0, r.cy(), d / 2.0 + self.hov(a) * 0.6, Color::rgb(255, 255, 255));
        self.hit(r, a);
    }

    pub fn section_title(&mut self, r: Rect, title: &str, sub: &str) {
        self.text(title, Rect::new(r.x, r.y, r.w, 24.0), 17.0, Weight::Semibold, pal().text, Align::Left);
        if !sub.is_empty() {
            self.text(sub, Rect::new(r.x, r.y + 24.0, r.w, 18.0), 12.0, Weight::Regular, pal().text_2, Align::Left);
        }
    }

    /// Centered empty-state block.
    pub fn empty_state(&mut self, r: Rect, icon: Icon, accent: Color, title: &str, sub: &str) {
        let cy = r.cy() - 14.0;
        self.s.circle(r.cx(), cy - 6.0, 28.0, accent.fade(0.12));
        icons::draw(&mut self.s, icon, r.cx(), cy - 6.0, 26.0, accent);
        self.text(title, Rect::new(r.x, cy + 30.0, r.w, 22.0), 14.0, Weight::Semibold, pal().text, Align::Center);
        self.text(sub, Rect::new(r.x, cy + 52.0, r.w, 18.0), 12.0, Weight::Regular, pal().text_3, Align::Center);
    }
}

/// Step through the visible pages only (wheel/arrow cycling).
pub fn step_page(cur: Page, delta: i32, set: &Settings) -> Page {
    let all = visible_pages(set);
    let Some(i) = all.iter().position(|&p| p == cur) else { return all[0] };
    all[((i as i32 + delta).rem_euclid(all.len() as i32)) as usize]
}

/// Pages that actually appear in the nav dock and the page cycle.
pub fn visible_pages(set: &Settings) -> Vec<Page> {
    let v: Vec<Page> = Page::all().iter().copied().filter(|p| !set.hidden_pages.iter().any(|h| h == p.title())).collect();
    if v.is_empty() {
        vec![Page::Music]
    } else {
        v
    }
}

pub fn tab_icon(p: Page) -> Icon {
    match p {
        Page::Music => Icon::Note,
        Page::Focus => Icon::Timer,
        Page::Water => Icon::Drop,
        Page::Shelf => Icon::Tray,
        Page::Tasks => Icon::List,
        Page::Notes => Icon::Notes,
        Page::Clipboard => Icon::Clipboard,
        Page::Today => Icon::Calendar,
        Page::System => Icon::Chip,
        Page::Speed => Icon::Gauge,
        Page::Usage => Icon::Chart,
        Page::Voice => Icon::Mic,
        Page::Ai => Icon::Sparkle,
        Page::Chat => Icon::Chat,
        Page::Windows => Icon::Grid,
        Page::Quick => Icon::Sliders,
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
        Page::System => 90.0,
        Page::Speed => 84.0,
        Page::Usage => 84.0,
        Page::Voice => 82.0,
        Page::Windows => 86.0,
        Page::Ai => 64.0,
        Page::Chat => 82.0,
        Page::Quick => 82.0,
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
    c.s.text(local.format(fmt).to_string(), Rect::new(card_x + 18.0, y + 2.0, 110.0, 20.0), 11.0, Weight::Medium, pal().text_3, Align::Left);

    // Page icons centred in the space between the clock and pin/gear,
    // shrinking their pitch on narrow blobs so nothing overlaps.
    let n = visible_pages(c.set).len();
    let left = if narrow { card_x + 62.0 } else { card_x + 150.0 };
    let right = card_x + geom.card_w - 70.0;
    let iw = ((right - left) / n as f32).min(26.0);
    let mut x = left + (right - left - iw * n as f32) / 2.0;
    for &p in visible_pages(c.set).iter() {
        let amt = c.ui.tab_amt[super::state::page_index(p).min(super::state::PAGE_SLOTS - 1)];
        let r = Rect::new(x, y - 3.0, iw - 2.0, 24.0);
        let a = Action::Tab(p);
        let h = c.hov(a);
        if amt > 0.01 {
            c.s.rect(r, 9.0, accent(p).fade(0.22 * amt));
        } else if h > 0.01 {
            c.s.rect(r, 9.0, pal().surface_hover.fade(h));
        }
        icons::draw(&mut c.s, tab_icon(p), r.cx(), r.cy(), (iw - 9.0).clamp(10.0, 13.0), pal().text_3.mix(pal().text, h).mix(accent(p), amt));
        c.hit(r, a);
        x += iw;
    }

    let gr = Rect::new(card_x + geom.card_w - 18.0 - 24.0, y - 3.0, 24.0, 24.0);
    let on = c.vm.page == Page::Settings;
    let h = c.hov(Action::Gear);
    if on || h > 0.01 {
        c.s.rect(gr, 12.0, accent(Page::Settings).fade(if on { 0.22 } else { 0.35 * h }));
    }
    icons::draw(&mut c.s, Icon::Gear, gr.cx(), gr.cy(), 14.0, if on { pal().text } else { pal().text_2.mix(pal().text, h * 0.7) });
    c.hit(gr, Action::Gear);

    let pr = Rect::new(gr.x - 28.0, y, 22.0, 22.0);
    let ph = c.hov(Action::Pin);
    if c.vm.pinned {
        c.s.rect(pr, 11.0, accent(Page::Focus).fade(0.22));
    } else if ph > 0.01 {
        c.s.rect(pr, 11.0, pal().surface_hover.fade(ph));
    }
    icons::draw(&mut c.s, Icon::Pin, pr.cx(), pr.cy(), 12.0, if c.vm.pinned { accent(Page::Focus) } else { pal().text_2.mix(pal().text, ph * 0.7) });
    c.hit(pr, Action::Pin);
}

fn header(c: &mut Ctx, card_x: f32, geom: &DropletGeom) {
    // Live clock and date at the far left (e.g. "Sat 24 Oct  13:49").
    let local = local_naive(c.vm.now);
    let date_str = local.format("%a %d %b  %H:%M").to_string();
    let tr = Rect::new(card_x + 20.0, HEADER_Y + 7.0, 120.0, 20.0);
    c.s.text(date_str, tr, 11.5, Weight::Medium, pal().text_2, Align::Left);

    // Page tabs centred in the card; the active one widens to show its label.
    let widths: Vec<f32> =
        visible_pages(c.set).iter().map(|&p| TAB_W + (tab_label_w(p) - TAB_W) * c.ui.tab_amt[super::state::page_index(p).min(super::state::PAGE_SLOTS - 1)]).collect();
    let total: f32 = widths.iter().sum::<f32>() + (widths.len() - 1) as f32 * 2.0;
    let mut x = card_x + (geom.card_w - total) / 2.0;
    for (i, &p) in visible_pages(c.set).iter().enumerate() {
        let amt = c.ui.tab_amt[super::state::page_index(p).min(super::state::PAGE_SLOTS - 1)];
        let w = widths[i];
        let r = Rect::new(x, HEADER_Y, w, HEADER_H);
        let a = Action::Tab(p);
        let h = c.hov(a);
        let acc = accent(p);
        if amt > 0.01 {
            c.s.rect(r, HEADER_H / 2.0, acc.fade(0.17 * amt));
        }
        if h > 0.01 && amt < 0.99 {
            c.s.rect(r, HEADER_H / 2.0, pal().surface_hover.fade(h * (1.0 - amt)));
        }
        let ic = pal().text_2.mix(pal().text, h * 0.7).mix(acc, amt);
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
        c.s.rect(gr, HEADER_H / 2.0, pal().surface_hover.fade(h));
    }
    let ic = if on { pal().text } else { pal().text_2.mix(pal().text, h * 0.7) };
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
        c.s.rect(pr, (HEADER_H - 6.0) / 2.0, pal().surface_hover.fade(ph));
    }
    let pin_ic = if pin_on { accent(Page::Focus) } else { pal().text_2.mix(pal().text, ph * 0.7) };
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
        c.text(crate::voice::clock(vm.voice.elapsed_ms), Rect::new(r.right() - 74.0, cy - 10.0, 60.0, 20.0), 14.0, Weight::Semibold, pal().text, Align::Right);
        if c.set.idle_look == IdleLook::Eyes {
            face(c, r, Spot::Rec, Some(red));
        }
    } else if let Some(ev) = vm.meeting_soon() {
        let acc = accent(Page::Today);
        icons::draw(&mut c.s, Icon::Calendar, r.x + 22.0, cy, 15.0, acc);
        let tr = Rect::new(r.x + 40.0, cy - 10.0, r.w - 40.0 - 78.0, 20.0);
        marquee(c, tr, &ev.title, 12.0, Weight::Medium, pal().text, c.now_ms);
        let left = ev.start - vm.now.timestamp();
        let col = if left <= 120 { pal().amber } else { pal().text_2 };
        c.text(crate::calendar::countdown(ev, vm.now.timestamp()), Rect::new(r.right() - 74.0, cy - 10.0, 62.0, 20.0), 12.5, Weight::Semibold, col, Align::Right);
    } else if vm.focus_visible() && vm.music_active() {
        // Focus + music share the pill: ring, artwork, title/lyric,
        // visualizer, face and the countdown.
        let acc = accent(Page::Focus);
        let ring = (r.x + 24.0, cy);
        c.s.ring(ring.0, ring.1, 9.0, 2.6, 0.0, 1.0, pal().ink(0.16));
        c.s.ring(ring.0, ring.1, 9.0, 2.6, 0.0, vm.focus_progress().max(0.02), acc);
        let (c1, c2) = art_colors(&vm.track.title);
        let art = Rect::new(r.x + 44.0, cy - 11.0, 22.0, 22.0);
        match crate::media_smtc::art_for(&vm.track) {
            Some(img) => c.s.image_rounded(art, 6.5, &img),
            None => c.s.rect(art, 6.5, Fill::DGrad(c1, c2)),
        }
        let acc_m = c1.mix(Color::rgb(255, 255, 255), 0.25);
        let t = mmss(vm.focus_remaining_ms());
        let col = if vm.focus_running() { pal().text } else { pal().text_2 };
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
                c.s.text_karaoke(subline.to_string(), Rect::new(tx, ly, tw, LYRIC_LH), LYRIC_SIZE, Weight::Semibold, pal().text_2, pal().text, sub_frac);
                ly += LYRIC_LH;
            }
            c.s.pop_clip();
        } else {
            let tr = Rect::new(art.right() + 8.0, cy - 10.0, (x0 - 8.0) - (art.right() + 8.0), 20.0);
            let line = format!("{} \u{2014} {}", vm.track.title, vm.track.display_artist());
            marquee(c, tr, &line, 12.0, Weight::Medium, pal().text, c.now_ms);
        }
    } else if vm.focus_visible() {
        let p = Page::Focus;
        let acc = accent(p);
        let ring = (r.x + 24.0, cy);
        c.s.ring(ring.0, ring.1, 9.0, 2.6, 0.0, 1.0, pal().ink(0.16));
        c.s.ring(ring.0, ring.1, 9.0, 2.6, 0.0, vm.focus_progress().max(0.02), acc);
        let label = match vm.pomo.phase {
            teardrop_core::Phase::Focus => "Focus",
            teardrop_core::Phase::ShortBreak => "Break",
            _ => "Long break",
        };
        if c.set.idle_look == IdleLook::Eyes {
            face(c, r, Spot::Focus, None);
        } else {
            c.text(label, Rect::new(r.x + 42.0, cy - 10.0, 60.0, 20.0), 12.0, Weight::Medium, pal().text_2, Align::Left);
        }
        let t = mmss(vm.focus_remaining_ms());
        let col = if vm.focus_running() { pal().text } else { pal().text_2 };
        c.text(t, Rect::new(r.right() - 74.0, cy - 10.0, 60.0, 20.0), 14.0, Weight::Semibold, col, Align::Right);
    } else if vm.music_active() {
        let (c1, c2) = art_colors(&vm.track.title);
        let art = Rect::new(r.x + 10.0, cy - 11.0, 22.0, 22.0);
        match crate::media_smtc::art_for(&vm.track) {
            Some(img) => c.s.image_rounded(art, 6.5, &img),
            None => c.s.rect(art, 6.5, Fill::DGrad(c1, c2)),
        }
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
                c.s.text_karaoke(subline.to_string(), Rect::new(tx, ly, tw, LYRIC_LH), LYRIC_SIZE, Weight::Semibold, pal().text_2, pal().text, sub_frac);
                ly += LYRIC_LH;
            }
            c.s.pop_clip();
        } else {
            // The title stays up the whole time; long ones scroll.
            let tr = Rect::new(art.right() + 10.0, cy - 10.0, (x0 - 10.0) - (art.right() + 10.0), 20.0);
            let line = format!("{} \u{2014} {}", vm.track.title, vm.track.display_artist());
            marquee(c, tr, &line, 12.0, Weight::Medium, pal().text, c.now_ms);
        }
    } else {
        match c.set.idle_look {
            IdleLook::Clock => c.text(vm.clock_text(), r, 13.5, Weight::Semibold, pal().text, Align::Center),
            IdleLook::Eyes => face(c, r, Spot::Idle, None),
            IdleLook::Minimal => c.s.circle(r.cx(), cy, 2.8, pal().text_3),
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
/// Every page gets its own quick-peek layout; transient activities
/// (recording, imminent meeting) take over regardless of the page.
fn glance(c: &mut Ctx, r: Rect, page: Page) {
    let vm = c.vm;
    let cy = r.cy();

    if vm.voice.phase == crate::voice::VoicePhase::Recording {
        let acc = accent(Page::Voice);
        c.s.circle(r.x + 28.0, cy, 7.0, acc);
        c.text("Recording", Rect::new(r.x + 44.0, cy - 13.0, 160.0, 18.0), 14.5, Weight::Semibold, pal().text, Align::Left);
        c.text(crate::voice::clock(vm.voice.elapsed_ms), Rect::new(r.x + 44.0, cy + 4.0, 160.0, 15.0), 11.0, Weight::Medium, pal().text_3, Align::Left);
        let sq = Rect::new(r.right() - 52.0, cy - 15.0, 30.0, 30.0);
        c.s.rect(sq, 10.0, acc.fade(0.2));
        c.s.rect(sq.inset(9.0), 3.0, acc);
        c.hit(sq, Action::VoiceToggle);
        return;
    }
    if let Some(ev) = vm.meeting_soon() {
        let acc = accent(Page::Today);
        icons::draw(&mut c.s, Icon::Calendar, r.x + 28.0, cy, 19.0, acc);
        c.text(ev.title.clone(), Rect::new(r.x + 48.0, cy - 13.0, r.w - 190.0, 18.0), 13.5, Weight::Semibold, pal().text, Align::Left);
        c.text(crate::calendar::countdown(ev, vm.now.timestamp()), Rect::new(r.x + 48.0, cy + 4.0, 160.0, 15.0), 11.0, Weight::Medium, pal().text_3, Align::Left);
        let pill = Rect::new(r.right() - 86.0, cy - 13.0, 68.0, 26.0);
        c.s.rect(pill, 13.0, acc);
        c.text("Join", pill, 11.5, Weight::Semibold, Color::rgb(8, 16, 32), Align::Center);
        c.hit(pill, Action::MeetingJoin);
        return;
    }

    match page {
        Page::Music => glance_music(c, r, cy),
        Page::Focus => glance_focus(c, r, cy),
        Page::Water => glance_water(c, r, cy),
        Page::Shelf => glance_shelf(c, r, cy),
        Page::Tasks => glance_tasks(c, r, cy),
        Page::Notes => glance_notes(c, r, cy),
        Page::Clipboard => glance_clip(c, r, cy),
        Page::Today => glance_today(c, r, cy),
        Page::System => glance_system(c, r, cy),
        Page::Speed => glance_speed(c, r, cy),
        Page::Usage => glance_usage(c, r, cy),
        Page::Voice => glance_voice(c, r, cy),
        Page::Windows => glance_windows(c, r, cy),
        Page::Quick => glance_quick(c, r, cy),
        Page::Chat => glance_chat(c, r, cy),
        Page::Ai | Page::Settings => glance_ai(c, r, cy),
    }
}

/// Quick peek for the Quick page: radios and levels in one line.
fn glance_quick(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Quick);
    icons::draw(&mut c.s, Icon::Sliders, r.x + 28.0, cy, 17.0, acc);
    let q = &c.vm.quick;
    c.text("Quick settings", Rect::new(r.x + 46.0, cy - 13.0, 200.0, 18.0), 14.0, Weight::Semibold, pal().text, Align::Left);
    let wifi = match q.wifi {
        Some(true) => q.wifi_ssid.clone().unwrap_or_else(|| "Wi-Fi on".into()),
        _ => "Wi-Fi off".into(),
    };
    let bt = if q.bluetooth == Some(true) { "BT on" } else { "BT off" };
    let detail = format!("{wifi} · {bt} · {}%", q.volume_pct);
    c.text(detail, Rect::new(r.x + 46.0, cy + 4.0, r.w - 56.0, 15.0), 11.0, Weight::Medium, pal().text_3, Align::Left);
}

fn glance_chat(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Chat);
    let vm = c.vm;
    icons::draw(&mut c.s, tab_icon(Page::Chat), r.x + 28.0, cy, 17.0, acc);
    let chat = &vm.chat;
    if let Some(m) = chat.msgs.last() {
        let who = if m.mine { "you" } else { m.who.as_str() };
        c.text(format!("{who}: {}", one_line(&m.text)), Rect::new(r.x + 46.0, cy - 10.0, r.w - 190.0, 20.0), 12.5, Weight::Medium, pal().text, Align::Left);
    } else {
        c.text(
            if chat.peers.is_empty() { "no droppys in range" } else { "mesh is quiet" },
            Rect::new(r.x + 46.0, cy - 10.0, 260.0, 20.0),
            13.5,
            Weight::Semibold,
            pal().text,
            Align::Left,
        );
    }
    let n = chat.peers.iter().filter(|p| p.fresh).count();
    let badge = if chat.unread_total > 0 { format!("{} unread", chat.unread_total) } else { format!("{n} peer{}", if n == 1 { "" } else { "s" }) };
    c.text(badge, Rect::new(r.right() - 130.0, cy - 10.0, 118.0, 20.0), 11.5, Weight::Semibold, if chat.unread_total > 0 { acc } else { pal().text_3 }, Align::Right);
    if c.set.idle_look == IdleLook::Eyes {
        face(c, r, Spot::Music, None);
    }
}

fn glance_music(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Music);
    let t = &c.vm.track;
    let art = Rect::new(r.x + 16.0, cy - 22.0, 44.0, 44.0);
    if t.title.is_empty() {
        c.s.rect(art, 12.0, pal().surface);
        icons::draw(&mut c.s, Icon::Note, art.cx(), art.cy(), 18.0, pal().text_3);
        c.text("Play something", Rect::new(art.right() + 12.0, cy - 10.0, 220.0, 20.0), 14.0, Weight::Semibold, pal().text_2, Align::Left);
    } else {
        match crate::media_smtc::art_for(t) {
            Some(img) => c.s.image_rounded(art, 12.0, &img),
            None => {
                let (c1, c2) = art_colors(&t.title);
                c.s.rect(art, 12.0, Fill::DGrad(c1, c2));
                c.s.rect(Rect::new(art.x, art.y, art.w, art.h * 0.55), 12.0, Fill::VGrad(pal().ink(0.2), pal().ink(0.0)));
                icons::draw(&mut c.s, Icon::Note, art.cx(), art.cy(), 19.0, Color::rgb(255, 255, 255).fade(0.92));
            }
        }
        let tx = art.right() + 12.0;
        let tw = r.right() - 190.0 - tx;
        c.text(t.title.clone(), Rect::new(tx, cy - 20.0, tw, 18.0), 14.0, Weight::Semibold, pal().text, Align::Left);
        c.text(t.display_artist(), Rect::new(tx, cy - 3.0, tw, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
        // Karaoke line under the title when synced lyrics exist.
        if let Some(i) = c.vm.lyric_idx {
            if let Some(l) = c.vm.lyric_lines.get(i) {
                c.text(l.text.clone(), Rect::new(tx, cy + 11.0, tw, 14.0), 11.5, Weight::Semibold, acc, Align::Left);
            }
        }
        // Visualizer + transport on the right.
        let x0 = r.right() - 176.0;
        for (i, b) in c.ui.viz.sample(8).iter().enumerate() {
            let h = 4.0 + b * 16.0;
            c.s.rect(Rect::new(x0 + i as f32 * 6.0, cy - h / 2.0, 3.0, h), 1.5, acc.fade(0.85));
        }
        let px = r.right() - 118.0;
        icons::draw(&mut c.s, Icon::Prev, px, cy, 15.0, pal().text_2);
        c.hit(Rect::new(px - 11.0, cy - 14.0, 22.0, 28.0), Action::MediaPrev);
        let pp = if t.playback == Playback::Playing { Icon::Pause } else { Icon::Play };
        c.s.circle(px + 36.0, cy, 16.0, Color::rgb(255, 255, 255));
        icons::draw(&mut c.s, pp, px + 36.0, cy, 16.0, Color::rgb(20, 20, 26));
        c.hit(Rect::new(px + 19.0, cy - 17.0, 34.0, 34.0), Action::MediaPlayPause);
        icons::draw(&mut c.s, Icon::Next, px + 72.0, cy, 15.0, pal().text_2);
        c.hit(Rect::new(px + 61.0, cy - 14.0, 22.0, 28.0), Action::MediaNext);
    }
}

fn glance_focus(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Focus);
    let vm = c.vm;
    let rcx = r.x + 32.0;
    c.s.ring(rcx, cy, 16.0, 4.5, 0.0, 1.0, pal().ink(0.16));
    let p = vm.focus_progress();
    if p > 0.003 {
        c.s.ring(rcx, cy, 16.0, 4.5, 0.0, p, acc);
    }
    let n = vm.pomo.sessions_before_long.max(1);
    let cur = (vm.pomo.completed_focus % n) + 1;
    let sub = match vm.pomo.phase {
        teardrop_core::Phase::Idle => "Ready when you are".to_string(),
        teardrop_core::Phase::ShortBreak => "Take a breather".to_string(),
        teardrop_core::Phase::LongBreak => "Long break".to_string(),
        _ => format!("Session {} of {}", cur.min(n), n),
    };
    c.text(sub, Rect::new(r.x + 56.0, cy - 9.0, r.w - 250.0, 17.0), 14.0, Weight::Semibold, pal().text, Align::Left);
    c.text(mmss(vm.focus_remaining_ms()), Rect::new(r.right() - 200.0, cy - 16.0, 92.0, 30.0), 24.0, Weight::Semibold, pal().text, Align::Right);
    let label = if vm.focus_running() {
        "Pause"
    } else if vm.pomo.remaining_ms_when_paused.is_some() {
        "Resume"
    } else {
        "Start"
    };
    let pill = Rect::new(r.right() - 98.0, cy - 14.0, 80.0, 28.0);
    c.s.rect(pill, 14.0, acc);
    c.text(label, pill, 12.0, Weight::Semibold, Color::rgb(28, 18, 6), Align::Center);
    c.hit(pill, Action::FocusToggle);
}

fn glance_water(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Water);
    let vm = c.vm;
    let day = vm.day();
    let today = vm.water.today(day);
    let goal = vm.water.goal_ml.max(1);
    icons::draw(&mut c.s, Icon::Drop, r.x + 28.0, cy, 19.0, acc);
    c.text(teardrop_core::glasses_label(today), Rect::new(r.x + 46.0, cy - 15.0, 130.0, 24.0), 19.0, Weight::Bold, pal().text, Align::Left);
    c.text(format!("of {}", teardrop_core::glasses_label(goal)), Rect::new(r.x + 46.0, cy + 9.0, 130.0, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
    let bw = r.w - 300.0;
    let bar = Rect::new(r.x + 176.0, cy - 3.0, bw, 6.0);
    c.s.rect(bar, 3.0, pal().ink(0.12));
    let f = vm.water.progress(day).clamp(0.0, 1.0);
    if f > 0.0 {
        c.s.rect(Rect::new(bar.x, bar.y, (bar.w * f).max(6.0), 6.0), 3.0, acc);
    }
    for (i, ml) in [250u32, 500].iter().enumerate() {
        let pill = Rect::new(r.right() - 122.0 + i as f32 * 62.0, cy - 13.0, 56.0, 26.0);
        c.s.rect(pill, 13.0, if *ml == 250 { acc } else { pal().surface_hover });
        c.text(teardrop_core::add_label(*ml), pill, 11.5, Weight::Semibold, if *ml == 250 { Color::rgb(8, 24, 32) } else { pal().text }, Align::Center);
        c.hit(pill, Action::WaterAdd(*ml));
    }
}

fn glance_shelf(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Shelf);
    let vm = c.vm;
    icons::draw(&mut c.s, Icon::Tray, r.x + 28.0, cy, 18.0, acc);
    if let Some(item) = vm.shelf.items.last() {
        c.text(item.name.clone(), Rect::new(r.x + 46.0, cy - 14.0, r.w - 200.0, 18.0), 13.5, Weight::Semibold, pal().text, Align::Left);
        c.text(pages::parent_of(&item.path), Rect::new(r.x + 46.0, cy + 3.0, r.w - 200.0, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
        // The row doubles as the drag-out handle: press and pull the file away.
        c.hit(Rect::new(r.x + 40.0, cy - 20.0, r.w - 190.0, 40.0), Action::ShelfOpen((vm.shelf.items.len() - 1) as u16));
        c.text(format!("{} on the shelf", vm.shelf.items.len()), Rect::new(r.right() - 150.0, cy - 8.0, 140.0, 16.0), 11.0, Weight::Medium, pal().text_3, Align::Right);
    } else {
        c.text("Shelf is empty", Rect::new(r.x + 46.0, cy - 14.0, 200.0, 18.0), 13.5, Weight::Semibold, pal().text_2, Align::Left);
        c.text("Drag files onto the island", Rect::new(r.x + 46.0, cy + 3.0, 240.0, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
    }
    if c.set.idle_look == IdleLook::Eyes {
        face(c, r, Spot::Music, None);
    }
}

fn glance_tasks(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Tasks);
    let vm = c.vm;
    icons::draw(&mut c.s, tab_icon(Page::Tasks), r.x + 28.0, cy, 17.0, acc);
    let open: Vec<_> = vm.tasks.tasks.iter().filter(|t| !t.done).take(2).collect();
    if open.is_empty() {
        c.text("All clear", Rect::new(r.x + 46.0, cy - 10.0, 200.0, 20.0), 14.0, Weight::Semibold, pal().text, Align::Left);
    } else {
        for (i, t) in open.iter().enumerate() {
            let ry = cy - 17.0 + i as f32 * 19.0;
            c.s.ring(r.x + 51.0, ry + 6.0, 6.5, 1.5, 0.0, 1.0, pal().text_3);
            c.text(t.title.clone(), Rect::new(r.x + 66.0, ry, r.w - 240.0, 16.0), 12.5, Weight::Medium, pal().text, Align::Left);
        }
        c.text(format!("{} open", vm.tasks.open_count()), Rect::new(r.right() - 140.0, cy - 22.0, 130.0, 16.0), 11.0, Weight::Medium, pal().text_3, Align::Right);
    }
    let add = Rect::new(r.right() - 108.0, cy + 6.0, 96.0, 24.0);
    c.s.rect(add, 12.0, acc);
    c.text("+ task", add, 11.5, Weight::Semibold, Color::rgb(16, 16, 20), Align::Center);
    c.hit(add, Action::TaskInput);
    if c.set.idle_look == IdleLook::Eyes {
        face(c, r, Spot::Music, None);
    }
}

fn glance_notes(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Notes);
    let vm = c.vm;
    icons::draw(&mut c.s, tab_icon(Page::Notes), r.x + 28.0, cy, 17.0, acc);
    if let Some(note) = vm.notes.notes.last() {
        let (first, rest) = note.text.split_once('\n').unwrap_or((note.text.as_str(), ""));
        c.text(first.to_string(), Rect::new(r.x + 46.0, cy - 14.0, r.w - 190.0, 18.0), 13.5, Weight::Semibold, pal().text, Align::Left);
        c.text(rest.trim().to_string(), Rect::new(r.x + 46.0, cy + 3.0, r.w - 190.0, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
        c.text(format!("{} notes", vm.notes.notes.len()), Rect::new(r.right() - 140.0, cy - 22.0, 130.0, 16.0), 11.0, Weight::Medium, pal().text_3, Align::Right);
    } else {
        c.text("No notes yet", Rect::new(r.x + 46.0, cy - 10.0, 200.0, 20.0), 13.5, Weight::Semibold, pal().text_2, Align::Left);
    }
    let add = Rect::new(r.right() - 108.0, cy + 6.0, 96.0, 24.0);
    c.s.rect(add, 12.0, acc);
    c.text("+ note", add, 11.5, Weight::Semibold, Color::rgb(16, 16, 20), Align::Center);
    c.hit(add, Action::NoteNew);
    if c.set.idle_look == IdleLook::Eyes {
        face(c, r, Spot::Music, None);
    }
}

fn glance_clip(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Clipboard);
    let vm = c.vm;
    icons::draw(&mut c.s, tab_icon(Page::Clipboard), r.x + 28.0, cy, 17.0, acc);
    let items = vm.clipboard.items();
    if let Some(latest) = items.first() {
        let one = latest.split_whitespace().collect::<Vec<_>>().join(" ");
        c.text(one, Rect::new(r.x + 46.0, cy - 14.0, r.w - 190.0, 18.0), 12.5, Weight::Medium, pal().text, Align::Left);
        c.text(format!("{} in history", items.len()), Rect::new(r.x + 46.0, cy + 3.0, 200.0, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
    } else {
        c.text("Nothing copied yet", Rect::new(r.x + 46.0, cy - 10.0, 240.0, 20.0), 13.5, Weight::Semibold, pal().text_2, Align::Left);
    }
    if c.set.idle_look == IdleLook::Eyes {
        face(c, r, Spot::Music, None);
    }
}

fn glance_today(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Today);
    let vm = c.vm;
    icons::draw(&mut c.s, Icon::Calendar, r.x + 28.0, cy, 18.0, acc);
    let next = vm.calendar.agenda(vm.now.timestamp(), 2);
    if let Some(ev) = next.first() {
        c.text(ev.title.clone(), Rect::new(r.x + 46.0, cy - 14.0, r.w - 220.0, 18.0), 13.5, Weight::Semibold, pal().text, Align::Left);
        let tail = next.get(1).map(|e| format!("then {}", e.title)).unwrap_or_default();
        c.text(
            format!("{}   {}", crate::calendar::countdown(ev, vm.now.timestamp()), tail),
            Rect::new(r.x + 46.0, cy + 3.0, r.w - 220.0, 14.0),
            10.5,
            Weight::Regular,
            pal().text_3,
            Align::Left,
        );
        let pill = Rect::new(r.right() - 80.0, cy - 13.0, 64.0, 26.0);
        c.s.rect(pill, 13.0, acc);
        c.text("Join", pill, 11.5, Weight::Semibold, Color::rgb(8, 16, 32), Align::Center);
        c.hit(pill, Action::MeetingJoin);
    } else {
        c.text("No meetings coming up", Rect::new(r.x + 46.0, cy - 10.0, 240.0, 20.0), 13.5, Weight::Semibold, pal().text_2, Align::Left);
    }
}

/// CPU / GPU / memory at a glance: a label, the live value and a thin meter each.
fn glance_system(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::System);
    icons::draw(&mut c.s, tab_icon(Page::System), r.x + 28.0, cy, 17.0, acc);
    let s = &c.vm.sys;
    if !s.loaded {
        c.text("Reading your system\u{2026}", Rect::new(r.x + 52.0, cy - 10.0, 220.0, 20.0), 13.0, Weight::Medium, pal().text_2, Align::Left);
        return;
    }
    let gpu = if s.gpu.available { Some(s.gpu.util) } else { None };
    let items = [("CPU", Some(s.cpu.total), Color::rgb(96, 165, 250)), ("GPU", gpu, Color::rgb(74, 222, 128)), ("MEM", Some(s.mem.pct()), Color::rgb(167, 139, 250))];
    let x0 = r.x + 56.0;
    let w = (r.right() - 18.0 - x0) / items.len() as f32;
    for (i, (label, v, col)) in items.iter().enumerate() {
        let x = x0 + i as f32 * w;
        c.text(*label, Rect::new(x, cy - 22.0, w - 12.0, 13.0), 9.5, Weight::Semibold, pal().text_3, Align::Left);
        let shown = v.map_or("\u{2014}".to_string(), |v| format!("{v:.0}%"));
        c.text(shown, Rect::new(x, cy - 10.0, w - 12.0, 24.0), 18.0, Weight::Bold, pal().text, Align::Left);
        let bar = Rect::new(x, cy + 17.0, w - 18.0, 4.0);
        c.s.rect(bar, 2.0, pal().ink(0.12));
        if let Some(v) = v {
            c.s.rect(Rect::new(bar.x, bar.y, (bar.w * v / 100.0).max(4.0), 4.0), 2.0, *col);
        }
    }
}

fn glance_speed(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Speed);
    let sp = c.vm.speed;
    icons::draw(&mut c.s, tab_icon(Page::Speed), r.x + 28.0, cy, 17.0, acc);
    let idle = sp.phase == crate::viewmodel::SpeedPhase::Idle;
    if idle {
        c.text("Test your connection", Rect::new(r.x + 46.0, cy - 10.0, 220.0, 20.0), 13.5, Weight::Semibold, pal().text_2, Align::Left);
    } else {
        c.text(format!("{:.0}", sp.down), Rect::new(r.x + 46.0, cy - 16.0, 90.0, 26.0), 20.0, Weight::Bold, pal().text, Align::Left);
        c.text("Mbps \u{2193}", Rect::new(r.x + 46.0, cy + 10.0, 90.0, 13.0), 9.5, Weight::Semibold, pal().text_3, Align::Left);
        c.text(format!("{:.0}", sp.up), Rect::new(r.x + 140.0, cy - 16.0, 80.0, 26.0), 20.0, Weight::Bold, pal().text_2, Align::Left);
        c.text("Mbps \u{2191}", Rect::new(r.x + 140.0, cy + 10.0, 80.0, 13.0), 9.5, Weight::Semibold, pal().text_3, Align::Left);
        if let Some(p) = sp.ping_ms {
            c.text(format!("ping {:.0} ms", p), Rect::new(r.x + 226.0, cy - 7.0, 110.0, 15.0), 11.0, Weight::Medium, pal().text_3, Align::Left);
        }
    }
    let pill = Rect::new(r.right() - 96.0, cy - 14.0, 80.0, 28.0);
    c.s.rect(pill, 14.0, acc);
    c.text(if idle { "GO" } else { "Again" }, pill, 12.0, Weight::Semibold, Color::rgb(6, 20, 24), Align::Center);
    c.hit(pill, Action::SpeedTest);
}

fn glance_usage(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Usage);
    let u = &c.vm.usage;
    let now = chrono::Utc::now().timestamp();
    icons::draw(&mut c.s, tab_icon(Page::Usage), r.x + 28.0, cy, 16.0, acc);
    let rows = [("Claude", u.claude.primary.as_ref()), ("Codex", u.codex.primary.as_ref())];
    for (i, (name, w)) in rows.iter().enumerate() {
        let ry = cy - 16.0 + i as f32 * 20.0;
        c.text(*name, Rect::new(r.x + 46.0, ry, 60.0, 14.0), 10.5, Weight::Medium, pal().text_2, Align::Left);
        let bar = Rect::new(r.x + 108.0, ry + 4.0, r.w - 280.0, 5.0);
        c.s.rect(bar, 2.5, pal().ink(0.12));
        if let Some(w) = w {
            let used = w.used_now(now);
            if used > 0.0 {
                let col = if used > 85.0 {
                    Color::rgb(239, 68, 68)
                } else if used > 65.0 {
                    pal().amber
                } else {
                    acc
                };
                c.s.rect(Rect::new(bar.x, bar.y, (bar.w * used / 100.0).max(5.0), 5.0), 2.5, col);
            }
            c.text(format!("{used:.0}%"), Rect::new(bar.right() + 8.0, ry - 1.0, 44.0, 14.0), 10.5, Weight::Semibold, pal().text, Align::Left);
        } else {
            c.text("\u{2014}", Rect::new(bar.right() + 8.0, ry - 1.0, 40.0, 14.0), 10.5, Weight::Semibold, pal().text_3, Align::Left);
        }
    }
    c.text(format!("${:.2} today", u.opencode.today_cost), Rect::new(r.right() - 110.0, cy - 8.0, 100.0, 16.0), 11.0, Weight::Semibold, pal().text_3, Align::Right);
}

fn glance_voice(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Voice);
    icons::draw(&mut c.s, tab_icon(Page::Voice), r.x + 28.0, cy, 18.0, acc);
    c.text("Voice recorder", Rect::new(r.x + 46.0, cy - 14.0, 200.0, 18.0), 13.5, Weight::Semibold, pal().text, Align::Left);
    let sub = match &c.vm.voice.saved {
        Some(name) => format!("Saved \u{00b7} {name}"),
        None => "Tap REC to start".to_string(),
    };
    c.text(sub, Rect::new(r.x + 46.0, cy + 3.0, r.w - 160.0, 14.0), 10.5, Weight::Regular, pal().text_3, Align::Left);
    let btn = Rect::new(r.right() - 84.0, cy - 14.0, 66.0, 28.0);
    c.s.rect(btn, 14.0, acc);
    c.text("REC", btn, 12.0, Weight::Bold, Color::rgb(40, 8, 8), Align::Center);
    c.hit(btn, Action::VoiceToggle);
}

fn glance_windows(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Windows);
    icons::draw(&mut c.s, tab_icon(Page::Windows), r.x + 28.0, cy, 17.0, acc);
    let n = c.vm.windows.len();
    c.text(format!("{n} windows"), Rect::new(r.x + 46.0, cy - 14.0, 140.0, 18.0), 14.0, Weight::Semibold, pal().text, Align::Left);
    // Up to three foreground-most apps, like the Alt-Tab strip.
    let mut seen: Vec<String> = Vec::new();
    let mut x = r.x + 46.0;
    for w in c.vm.windows.iter().take(12) {
        if seen.contains(&w.exe) {
            continue;
        }
        seen.push(w.exe.clone());
        if seen.len() > 4 {
            break;
        }
        let tw = est_w(&w.app, 11.0) + 18.0;
        let chip = Rect::new(x, cy + 6.0, tw, 22.0);
        c.s.rect(chip, 11.0, pal().surface);
        c.text(w.app.clone(), Rect::new(chip.x + 9.0, chip.y, chip.w - 14.0, chip.h), 11.0, Weight::Medium, pal().text_2, Align::Left);
        x += tw + 6.0;
        if x > r.right() - 30.0 {
            break;
        }
    }
    c.text(format!("{n}"), Rect::new(r.right() - 60.0, cy - 10.0, 46.0, 20.0), 14.0, Weight::Semibold, acc, Align::Right);
}

fn glance_ai(c: &mut Ctx, r: Rect, cy: f32) {
    let acc = accent(Page::Ai);
    icons::draw(&mut c.s, tab_icon(Page::Ai), r.x + 28.0, cy, 18.0, acc);
    let field = Rect::new(r.x + 46.0, cy - 15.0, r.w - 190.0, 30.0);
    c.s.rect(field, 15.0, pal().surface);
    c.s.stroke(field, 15.0, 1.0, acc.fade(0.3));
    c.text("Ask anything\u{2026}", Rect::new(field.x + 14.0, field.y, field.w - 20.0, field.h), 12.5, Weight::Regular, pal().text_3, Align::Left);
    let model = c.set.active_ai_model();
    c.text(model, Rect::new(r.right() - 130.0, cy - 8.0, 116.0, 16.0), 10.5, Weight::Medium, pal().text_3, Align::Right);
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
            icons::draw(&mut c.s, icon, left + 8.0, cy, 17.0, pal().text);
            // With Eyes the face rides along at the right end and watches the bar.
            let eyes = c.set.idle_look == IdleLook::Eyes;
            let tail = if eyes { 58.0 + HUD_FACE_W } else { 58.0 };
            let bx = r.x + 44.0;
            let bw = r.w - 44.0 - tail;
            let track = Rect::new(bx, cy - 3.0, bw, 6.0);
            c.s.rect(track, 3.0, pal().ink(0.18));
            let p = if muted { 0.0 } else { h.progress.unwrap_or(0.0).clamp(0.0, 1.0) };
            if p > 0.0 {
                c.s.rect(Rect::new(bx, cy - 3.0, (bw * p).max(6.0), 6.0), 3.0, pal().text);
            }
            c.text(h.detail.clone(), Rect::new(r.right() - tail + 2.0, cy - 10.0, 44.0, 20.0), 12.5, Weight::Semibold, pal().text, Align::Right);
            if eyes {
                face(c, r, Spot::Hud, None);
            }
        }
        HudKind::Water => {
            let acc = accent(Page::Water);
            icons::draw(&mut c.s, Icon::Drop, left + 8.0, cy, 17.0, acc);
            if h.action_label.is_some() {
                c.text("Time to hydrate", Rect::new(r.x + 42.0, cy - 10.0, r.w - 42.0 - 92.0, 20.0), 13.0, Weight::Medium, pal().text, Align::Left);
                let pill = Rect::new(r.right() - 82.0, cy - 12.0, 70.0, 24.0);
                c.s.rect(pill, 12.0, acc);
                c.text("+1 glass", pill, 11.5, Weight::Semibold, Color::rgb(8, 24, 32), Align::Center);
            } else {
                c.text(h.detail.clone(), Rect::new(r.x + 42.0, cy - 10.0, r.w - 42.0 - 14.0, 20.0), 13.0, Weight::Medium, pal().text, Align::Left);
            }
        }
        HudKind::Meeting => {
            let acc = accent(Page::Today);
            icons::draw(&mut c.s, Icon::Calendar, left + 8.0, cy, 17.0, acc);
            if h.action_label.is_some() {
                let tr = Rect::new(r.x + 42.0, cy - 10.0, r.w - 42.0 - 96.0, 20.0);
                c.text(h.title.clone(), tr, 13.0, Weight::Medium, pal().text, Align::Left);
                let pill = Rect::new(r.right() - 82.0, cy - 12.0, 70.0, 24.0);
                c.s.rect(pill, 12.0, acc);
                c.text("Join", pill, 11.5, Weight::Semibold, Color::rgb(8, 16, 32), Align::Center);
            } else {
                c.text(h.title.clone(), Rect::new(r.x + 42.0, cy - 10.0, r.w - 42.0 - 90.0, 20.0), 13.0, Weight::Medium, pal().text, Align::Left);
                c.text(h.detail.clone(), Rect::new(r.right() - 84.0, cy - 10.0, 72.0, 20.0), 12.0, Weight::Regular, pal().text_2, Align::Right);
            }
        }
        HudKind::Chat => {
            let acc = accent(Page::Chat);
            let eyes = c.set.idle_look == IdleLook::Eyes;
            let tail = if eyes { 14.0 + HUD_FACE_W } else { 14.0 };
            icons::draw(&mut c.s, Icon::Chat, left + 8.0, cy, 17.0, acc);
            let w = r.w - 42.0 - tail;
            c.text(h.title.clone(), Rect::new(r.x + 42.0, cy - 17.0, w, 18.0), 13.0, Weight::Semibold, pal().text, Align::Left);
            c.text(h.detail.clone(), Rect::new(r.x + 42.0, cy + 1.0, w, 16.0), 11.5, Weight::Regular, pal().text_2, Align::Left);
            if eyes {
                face(c, r, Spot::Hud, None);
            }
        }
        HudKind::Bluetooth => {
            // Round blue badge, device name over "Connected", battery at the end.
            let blue = Color::rgb(59, 130, 246);
            let eyes = c.set.idle_look == IdleLook::Eyes;
            let tail = if eyes { 14.0 + HUD_FACE_W } else { 14.0 };
            let badge = Rect::new(r.x + 14.0, cy - 19.0, 38.0, 38.0);
            c.s.circle(badge.cx(), badge.cy(), 19.0, blue);
            icons::draw(&mut c.s, Icon::Headphones, badge.cx(), badge.cy(), 18.0, Color::rgb(255, 255, 255));
            let batt_w = if h.progress.is_some() { 84.0 } else { 0.0 };
            let tr = Rect::new(badge.right() + 12.0, cy - 20.0, r.w - 14.0 - 38.0 - 12.0 - tail - batt_w - 6.0, 22.0);
            c.text(h.title.clone(), tr, 14.0, Weight::Semibold, pal().text, Align::Left);
            c.text(h.detail.clone(), Rect::new(tr.x, cy + 2.0, tr.w, 16.0), 11.5, Weight::Regular, pal().text_3, Align::Left);
            if let Some(p) = h.progress {
                let col = if p <= 0.15 {
                    pal().red
                } else if p <= 0.30 {
                    pal().amber
                } else {
                    pal().green
                };
                let body = Rect::new(r.right() - tail - 80.0, cy - 7.0, 28.0, 14.0);
                c.s.stroke(body, 4.0, 1.4, pal().text_2);
                c.s.rect(Rect::new(body.right() + 1.5, cy - 2.5, 2.4, 5.0), 1.0, pal().text_2);
                let fill = Rect::new(body.x + 2.4, body.y + 2.4, (body.w - 4.8) * p.clamp(0.0, 1.0), body.h - 4.8);
                if fill.w > 0.5 {
                    c.s.rect(fill, 2.0, col);
                }
                c.text(format!("{:.0}%", p * 100.0), Rect::new(body.right() + 8.0, cy - 10.0, 44.0, 20.0), 13.0, Weight::Medium, pal().text, Align::Left);
            }
            if eyes {
                face(c, r, Spot::Hud, None);
            }
        }
        _ => {
            let (icon, col) = match h.kind {
                HudKind::Charging => (Icon::Bolt, pal().green),
                HudKind::Device => (Icon::Chip, accent(Page::System)),
                HudKind::Timer => (Icon::Timer, accent(Page::Focus)),
                _ => (Icon::Note, pal().text),
            };
            icons::draw(&mut c.s, icon, left + 8.0, cy, 17.0, col);
            let eyes = c.set.idle_look == IdleLook::Eyes;
            let tail = if eyes { 14.0 + HUD_FACE_W } else { 14.0 };
            c.text(h.title.clone(), Rect::new(r.x + 42.0, cy - 10.0, 100.0, 20.0), 13.0, Weight::Medium, pal().text, Align::Left);
            c.text(h.detail.clone(), Rect::new(r.right() - tail - 120.0, cy - 10.0, 120.0, 20.0), 12.0, Weight::Regular, pal().text_2, Align::Right);
            if eyes {
                face(c, r, Spot::Hud, None);
            }
        }
    }
}

// --------------------------------------------------------------------- build

pub fn build(vm: &ViewModel, set: &Settings, ui: &UiState, isl: &IslandView, now_ms: u64) -> Frame {
    set_theme(set.theme);
    let g = &isl.geom;
    let open = isl.open;
    let (bw, bh) = (isl.w, isl.h);
    let w = bw + PAD * 2.0;
    let h = bh + PAD;
    let cx = w / 2.0;
    let body = Rect::new(cx - bw / 2.0, 0.0, bw, bh);
    // Ease-out on the content layers so fades decelerate into place.
    let eo = |a: f32| 1.0 - (1.0 - a).powi(3);

    let mut c = Ctx { s: Scene::new(), hits: Vec::new(), vm, set, ui, now_ms, live: true };

    let pts: Vec<(f32, f32)> = g.outline(cx, 0.0, bw, bh, set.island_style == IslandStyle::Pill);
    c.s.body(pts, Fill::VGrad(pal().body_top, pal().body_bottom), pal().rim, (0.45 + 0.55 * smooth(open)) * pal().shadow, pal().sheen);

    // Collapsed activity.
    let ca = eo(isl.compact_a);
    collapsed(&mut c, body, ca);
    if isl.card_a < 0.5 {
        let a = match vm.live_hud(now_ms) {
            Some(h) if h.kind == HudKind::Water && h.action_label.is_some() => Action::HudWater,
            Some(h) if h.kind == HudKind::Meeting && h.action_label.is_some() => Action::MeetingJoin,
            _ => Action::Island,
        };
        c.hit(body, a);
    }

    // Glance pill: the middle stop of the morph. Its content fades in as the
    // collapsed content fades out and hands off to the card.
    let ga = eo(isl.glance_a);
    if ga > 0.01 {
        c.s.clip(Rect::new(body.x + 1.0, 0.0, body.w - 2.0, body.h));
        // Quick actions are only live once the pill is resting at the glance.
        let ease = ui.page_ease();
        let eb = ui.page_ease_back();
        let sliding = ui.page_t < 1.0;
        let dy = -5.0 * (1.0 - ga);
        if sliding {
            c.live = false;
            c.s.set_transform(-ui.page_dir * 26.0 * eb, -(ease * 5.0) + dy, ga * (1.0 - ease));
            glance(&mut c, body, ui.prev_page);
        }
        c.live = isl.glance_a > 0.9 && !sliding;
        let (dx, a) = if sliding { (ui.page_dir * 26.0 * (1.0 - eb), ga * ease.min(1.0)) } else { (0.0, ga) };
        c.s.set_transform(dx, if sliding { (1.0 - ease) * 5.0 + dy } else { dy }, a);
        glance(&mut c, body, vm.page);
        c.s.reset_transform();
        c.live = true;
        c.s.pop_clip();
    }

    // Open card.
    let oa = eo(isl.card_a);
    if oa > 0.01 {
        let card_x = cx - g.card_w / 2.0;
        c.s.clip(Rect::new(body.x + 1.0, 0.0, body.w - 2.0, body.h - 1.0));
        c.s.set_transform(0.0, -8.0 * (1.0 - oa), oa);
        c.live = isl.card_a > 0.9;
        if is_blob_page(vm.page) {
            blob_nav(&mut c, card_x, g);
        } else {
            header(&mut c, card_x, g);
        }
        c.s.reset_transform();

        let area_of = |p: Page| if is_blob_page(p) { blob_content_rect(card_x, g) } else { content_rect(card_x, g) };
        let ease = ui.page_ease();
        let sliding = ui.page_t < 1.0;
        let dy = -8.0 * (1.0 - oa);
        if sliding {
            c.live = false;
            c.s.set_transform(-ui.page_dir * 26.0 * ui.page_ease_back(), -(ease * 5.0) + dy, oa * (1.0 - ease));
            pages::draw(&mut c, ui.prev_page, area_of(ui.prev_page));
        }
        c.live = isl.card_a > 0.9 && !sliding;
        let (dx, a) = if sliding { (ui.page_dir * 26.0 * (1.0 - ui.page_ease_back()), oa * ease.min(1.0)) } else { (0.0, oa) };
        c.s.set_transform(dx, if sliding { (1.0 - ease) * 5.0 + dy } else { dy }, a);
        pages::draw(&mut c, vm.page, area_of(vm.page));
        c.s.reset_transform();

        if let Some((msg, _)) = &ui.toast {
            c.live = false;
            c.s.set_transform(0.0, 0.0, oa);
            let tw = est_w(msg, 12.5) + 28.0;
            let tr = Rect::new(cx - tw / 2.0, g.card_h - 46.0, tw, 28.0);
            c.s.rect(tr, 14.0, Color::rgba(40, 40, 48, 0.96));
            c.s.stroke(tr, 14.0, 1.0, pal().hairline);
            c.text(msg.clone(), tr, 12.5, Weight::Medium, pal().text, Align::Center);
            c.s.reset_transform();
        }
        c.s.pop_clip();
    }

    // Pill mode floats the whole island `gap` px below the screen edge.
    let gap = set.top_gap();
    if gap > 0.0 {
        c.s.shift_y(gap);
        for hit in c.hits.iter_mut() {
            hit.rect.y += gap;
        }
    }
    Frame { scene: c.s, hits: c.hits, w, h: h + gap }
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
        // The System page only draws its live meters once a sample has landed.
        vm.sys.loaded = true;
        let set = Settings::default();
        let mut ui = UiState::new(page);
        let now_ms = now.timestamp_millis() as u64;
        for _ in 0..120 {
            ui.tick(1.0 / 60.0, &vm, &set, now_ms);
        }
        // Mirror window.rs: the morph geometry follows the animated sizes.
        let geom = DropletGeom {
            pill_w: ui.compact_w,
            pill_h: ui.compact_h,
            glance_w: ui.glance_w,
            glance_h: ui.glance_h,
            card_w: ui.card_w,
            card_h: ui.card_h,
            ..DropletGeom::default()
        };
        let isl = IslandView::at(geom, open);
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
    fn pill_style_floats_the_frame() {
        let now = Utc::now();
        let vm = ViewModel::demo(now);
        let now_ms = now.timestamp_millis() as u64;
        let mut set = Settings::default();
        let ui = UiState::new(Page::Music);
        let isl = IslandView::at(DropletGeom::default(), 0.0);
        let anchored = build(&vm, &set, &ui, &isl, now_ms);
        set.island_style = IslandStyle::Pill;
        set.pill_gap = 20;
        let pill = build(&vm, &set, &ui, &isl, now_ms);
        let ya = anchored.hits.iter().find(|h| h.action == Action::Island).unwrap().rect.y;
        let yp = pill.hits.iter().find(|h| h.action == Action::Island).unwrap().rect.y;
        assert_eq!((ya, yp), (0.0, 20.0));
        assert!((pill.h - anchored.h - 20.0).abs() < 0.01);
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

    /// No page may cover its glance with a click target of its own: that
    /// swallows the click that expands the island (the System glance once
    /// did, so its pill could not be opened).
    #[test]
    fn no_glance_covers_the_pill_with_its_own_hit() {
        for &p in Page::all() {
            let f = frame(p, 0.5);
            let pill = (f.w - 2.0 * PAD) * (f.h - PAD);
            for h in &f.hits {
                let area = h.rect.w * h.rect.h;
                assert!(h.action == Action::Island || area < 0.6 * pill, "{p:?} glance has a {:?} hit covering {:.0}% of the pill", h.action, 100.0 * area / pill);
            }
            assert!(f.hits.iter().any(|h| h.action == Action::Island), "{p:?} glance lost its expand hit");
        }
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
