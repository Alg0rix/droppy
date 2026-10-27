//! Teardrop island window state: hover, open amount, HUD priority.
//!
//! Pure-view-state wrapper over `teardrop-core` springs + droplet geometry.
//! The Win32 layer in `window.rs` only forwards input events here and asks
//! for the current width/height each frame.
//!
//! The morph is two independent springs (`w`, `h`) measured in pixels, not
//! one scalar `open` amount, so a direct pill -> card open moves at a
//! constant springy pace and never stalls at the glance size. Content
//! layers (compact / glance / card) fade on their own timers instead of
//! tracking the morph position.

use crate::ui::layout::IslandView;
use teardrop_core::{DropletGeom, Spring};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandMode {
    Collapsed,
    /// Morphing toward the glance pill.
    Glancing,
    /// Resting at the glance pill.
    Glance,
    Opening,
    Open,
    Closing,
}

pub struct IslandState {
    pub geom: DropletGeom,
    /// Island size in px; starts at the collapsed pill.
    pub w: Spring,
    pub h: Spring,
    /// Content layer alphas 0..1 (linear ramps; only one is non-zero).
    pub compact_a: f32,
    pub glance_a: f32,
    pub card_a: f32,
    /// Size when the current transition started (alpha-up gate reference).
    from_w: f32,
    from_h: f32,
    pub hover: bool,
    /// Cooldown on the pre-stretch kick so rubbing across the edge
    /// doesn't re-kick the height spring every pointer event.
    pub stretch_cd: f32,
    pub mode: IslandMode,
}

impl IslandState {
    pub fn new() -> Self {
        let geom = DropletGeom::default();
        Self {
            geom,
            w: Spring::new(geom.pill_w, 900.0, 60.0),
            h: Spring::new(geom.pill_h, 900.0, 60.0),
            compact_a: 1.0,
            glance_a: 0.0,
            card_a: 0.0,
            from_w: geom.pill_w,
            from_h: geom.pill_h,
            hover: false,
            stretch_cd: 0.0,
            mode: IslandMode::Collapsed,
        }
    }

    /// Target size in px for the current mode, read from the (live) geom.
    fn target(&self) -> (f32, f32) {
        match self.mode {
            IslandMode::Collapsed | IslandMode::Closing => (self.geom.pill_w, self.geom.pill_h),
            IslandMode::Glancing | IslandMode::Glance => (self.geom.glance_w, self.geom.glance_h),
            IslandMode::Opening | IslandMode::Open => (self.geom.card_w, self.geom.card_h),
        }
    }

    /// Begin a transition: remember the size it started from so content
    /// fades can key off real progress instead of the morph position.
    fn go(&mut self, m: IslandMode) {
        self.from_w = self.w.value;
        self.from_h = self.h.value;
        self.mode = m;
    }

    pub fn toggle(&mut self) {
        match self.mode {
            IslandMode::Collapsed | IslandMode::Closing => self.go(IslandMode::Opening),
            IslandMode::Glancing | IslandMode::Glance | IslandMode::Opening | IslandMode::Open => self.go(IslandMode::Closing),
        }
    }

    /// Collapsed -> glance pill (the dwell-hover peek).
    pub fn glance(&mut self) {
        match self.mode {
            IslandMode::Collapsed | IslandMode::Closing => self.go(IslandMode::Glancing),
            _ => {}
        }
    }

    /// Glance pill -> fully open card (click or prolonged dwell).
    pub fn expand(&mut self) {
        match self.mode {
            IslandMode::Glancing | IslandMode::Glance => self.go(IslandMode::Opening),
            _ => {}
        }
    }

    pub fn set_hover(&mut self, hover: bool) {
        if hover && !self.hover && self.mode == IslandMode::Collapsed && self.stretch_cd <= 0.0 {
            // Proximity pre-stretch: tiny pull-down before click (magnetic feel).
            self.h.velocity += 60.0;
            self.stretch_cd = 0.6;
        }
        self.hover = hover;
    }

    /// Advance springs and content fades. Returns true while anything moves.
    pub fn tick(&mut self, dt: f32) -> bool {
        let (tw, th) = self.target();
        // Spring feel by phase: grow is springy with a small overshoot,
        // shrink is dead-on, steady states track the live size targets
        // (HUD/lyric/page resizes) quickly without visible double-smoothing.
        let ((kw, dw), (kh, dh)) = match self.mode {
            IslandMode::Glancing | IslandMode::Opening => ((300.0, 26.0), (230.0, 23.0)),
            IslandMode::Closing => ((360.0, 36.0), (420.0, 39.0)),
            IslandMode::Collapsed | IslandMode::Glance | IslandMode::Open => ((900.0, 60.0), (900.0, 60.0)),
        };
        self.w.stiffness = kw;
        self.w.damping = dw;
        self.h.stiffness = kh;
        self.h.damping = dh;
        let mut moving = self.w.step(tw, dt) | self.h.step(th, dt);
        self.stretch_cd = (self.stretch_cd - dt).max(0.0);

        // Content layers: non-target fades out fast; the target fades in
        // once the shape has covered ~55% of the distance it started from.
        let rel = |v: f32, from: f32, to: f32| (v - to).abs() / (from - to).abs().max(1.0);
        let far_enough = rel(self.w.value, self.from_w, tw) <= 0.45 && rel(self.h.value, self.from_h, th) <= 0.45;
        let (tc, tg, td) = match self.mode {
            IslandMode::Collapsed | IslandMode::Closing => (1.0, 0.0, 0.0),
            IslandMode::Glancing | IslandMode::Glance => (0.0, 1.0, 0.0),
            IslandMode::Opening | IslandMode::Open => (0.0, 0.0, 1.0),
        };
        let (down, up) = (dt / 0.09, if far_enough { dt / 0.18 } else { 0.0 });
        self.compact_a = (self.compact_a + if tc > 0.5 { up } else { -down }).clamp(0.0, 1.0);
        self.glance_a = (self.glance_a + if tg > 0.5 { up } else { -down }).clamp(0.0, 1.0);
        self.card_a = (self.card_a + if td > 0.5 { up } else { -down }).clamp(0.0, 1.0);
        moving |= self.compact_a != tc || self.glance_a != tg || self.card_a != td;

        let settled = (self.w.value - tw).abs() < 0.75 && (self.h.value - th).abs() < 0.75 && self.w.velocity.abs() < 20.0 && self.h.velocity.abs() < 20.0;
        match self.mode {
            IslandMode::Glancing if settled => self.mode = IslandMode::Glance,
            IslandMode::Opening if settled => self.mode = IslandMode::Open,
            IslandMode::Closing if settled => {
                self.mode = IslandMode::Collapsed;
                self.w.value = tw;
                self.w.velocity = 0.0;
                self.h.value = th;
                self.h.velocity = 0.0;
            }
            _ => {}
        }
        moving
    }

    /// Derived morph progress 0..1 for consumers that want a scalar
    /// (collapsed = 0, glance = 0.5, open = 1), mapped off the live height.
    pub fn open(&self) -> f32 {
        let p = self.geom.pill_h;
        let g = self.geom.glance_h.max(p + 1.0);
        let c = self.geom.card_h.max(g + 1.0);
        let h = self.h.value;
        let v = if h <= g { 0.5 * (h - p) / (g - p) } else { 0.5 + 0.5 * (h - g) / (c - g) };
        v.clamp(0.0, 1.0)
    }

    #[allow(dead_code)]
    pub fn size(&self) -> (f32, f32) {
        (self.w.value, self.h.value)
    }

    /// Snapshot for the renderer: live size, content alphas, derived progress.
    pub fn view(&self) -> IslandView {
        IslandView { geom: self.geom, open: self.open(), w: self.w.value, h: self.h.value, compact_a: self.compact_a, glance_a: self.glance_a, card_a: self.card_a }
    }
}

impl Default for IslandState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 240.0;

    fn tick_until(s: &mut IslandState, mode: IslandMode, max_s: f32) -> f32 {
        let mut steps = 0;
        while s.mode != mode && steps as f32 * DT < max_s {
            s.tick(DT);
            steps += 1;
        }
        steps as f32 * DT * 1000.0
    }

    #[test]
    fn direct_open_never_stalls() {
        let mut s = IslandState::new();
        let (pw, ph) = (s.geom.pill_w, s.geom.pill_h);
        let (cw, ch) = (s.geom.card_w, s.geom.card_h);
        s.toggle();
        let (mut peak_w, mut peak_h) = (0.0f32, 0.0f32);
        let (mut min_w, mut min_h) = (f32::MAX, f32::MAX);
        for i in 0.. {
            s.tick(DT);
            let t = (i + 1) as f32 * DT;
            peak_w = peak_w.max(s.w.velocity.abs());
            peak_h = peak_h.max(s.h.velocity.abs());
            let done_h = s.h.value >= ph + 0.9 * (ch - ph) || s.mode == IslandMode::Open;
            let done_w = s.w.value >= cw - 0.1 * (cw - pw) || s.mode == IslandMode::Open;
            if t > 0.03 {
                if !done_h {
                    min_h = min_h.min(s.h.velocity.abs() / peak_h.max(1.0));
                }
                if !done_w {
                    min_w = min_w.min(s.w.velocity.abs() / peak_w.max(1.0));
                }
            }
            if done_h && done_w {
                break;
            }
            assert!(t < 1.0, "open never reached 90% ({:.0}ms)", t * 1000.0);
        }
        assert!(min_h >= 0.15, "h velocity dipped to {min_h:.2} of peak");
        assert!(min_w >= 0.15, "w velocity dipped to {min_w:.2} of peak");
    }

    #[test]
    fn direct_open_skips_glance_content() {
        let mut s = IslandState::new();
        let (ph, ch) = (s.geom.pill_h, s.geom.card_h);
        s.toggle();
        while s.mode == IslandMode::Opening {
            s.tick(DT);
            assert_eq!(s.glance_a, 0.0, "glance content flashed on a direct open");
            if s.h.value < ph + 0.5 * (ch - ph) {
                assert_eq!(s.card_a, 0.0, "card content appeared before 50% of the morph");
            }
        }
    }

    #[test]
    fn opens_fast_with_small_overshoot() {
        let mut s = IslandState::new();
        let (pw, ph) = (s.geom.pill_w, s.geom.pill_h);
        let (cw, ch) = (s.geom.card_w, s.geom.card_h);
        s.toggle();
        // Track the overshoot peak across the whole morph + ring-out.
        let (mut ow, mut oh) = (0.0f32, 0.0f32);
        let mut ms = 0.0f32;
        while s.mode != IslandMode::Open && ms < 1000.0 {
            s.tick(DT);
            ms += DT * 1000.0;
            ow = ow.max(s.w.value - cw);
            oh = oh.max(s.h.value - ch);
        }
        assert!(ms <= 600.0, "open took {ms:.0}ms");
        for _ in 0..480 {
            s.tick(DT);
            ow = ow.max(s.w.value - cw);
            oh = oh.max(s.h.value - ch);
        }
        assert!(oh <= 0.06 * (ch - ph), "h overshot by {:.0}px ({:.1}%)", oh, 100.0 * oh / (ch - ph));
        assert!(ow <= 0.06 * (cw - pw), "w overshot by {:.0}px ({:.1}%)", ow, 100.0 * ow / (cw - pw));
    }

    #[test]
    fn close_has_no_bounce() {
        let mut s = IslandState::new();
        s.toggle();
        tick_until(&mut s, IslandMode::Open, 1.0);
        for _ in 0..240 {
            s.tick(DT);
        }
        s.toggle();
        let mut ms = 0.0f32;
        let mut min_h = f32::MAX;
        while s.mode != IslandMode::Collapsed && ms < 600.0 {
            s.tick(DT);
            ms += DT * 1000.0;
            min_h = min_h.min(s.h.value);
        }
        assert!(ms <= 500.0, "close took {ms:.0}ms");
        assert!(min_h >= s.geom.pill_h - 1.0, "h undershot to {min_h}");
        for _ in 0..120 {
            s.tick(DT);
        }
        assert_eq!(s.compact_a, 1.0);
        assert_eq!(s.open(), 0.0);
    }

    #[test]
    fn glance_then_expand() {
        let mut s = IslandState::new();
        s.glance();
        tick_until(&mut s, IslandMode::Glance, 1.0);
        assert_eq!(s.mode, IslandMode::Glance);
        for _ in 0..120 {
            s.tick(DT);
        }
        assert_eq!(s.glance_a, 1.0);
        assert!((s.open() - 0.5).abs() < 0.05, "glance open() = {}", s.open());
        s.expand();
        tick_until(&mut s, IslandMode::Open, 1.0);
        assert_eq!(s.mode, IslandMode::Open);
        for _ in 0..120 {
            s.tick(DT);
        }
        assert_eq!(s.glance_a, 0.0);
        assert_eq!(s.card_a, 1.0);
    }

    #[test]
    fn interrupting_open_mid_flight_stays_smooth() {
        let mut s = IslandState::new();
        s.toggle();
        for _ in 0..29 {
            // ~120ms in.
            s.tick(DT);
        }
        let v_before = s.h.velocity;
        s.toggle();
        assert_eq!(s.h.velocity, v_before, "velocity must not reset on interrupt");
        tick_until(&mut s, IslandMode::Collapsed, 1.0);
        assert_eq!(s.mode, IslandMode::Collapsed);
    }
}
