//! Teardrop island window state: hover, open amount, HUD priority.
//!
//! Pure-view-state wrapper over `teardrop-core` springs + droplet geometry.
//! The Win32 layer in `window.rs` only forwards input events here and asks
//! for the current width/height each frame.

use teardrop_core::{DropletGeom, Spring};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandMode {
    Collapsed,
    /// Morphing toward the glance pill (open ~0.5).
    Glancing,
    /// Resting at the glance pill.
    Glance,
    Opening,
    Open,
    Closing,
}

/// `open` value the glance pill rests at (mid-point of the morph).
pub const GLANCE_T: f32 = 0.5;

pub struct IslandState {
    pub geom: DropletGeom,
    pub open: Spring,
    pub wobble: Spring,
    pub wobble_px: f32,
    pub hover: bool,
    pub mode: IslandMode,
}

impl IslandState {
    pub fn new() -> Self {
        Self {
            geom: DropletGeom::default(),
            open: Spring::island(),
            wobble: Spring::wobble(),
            wobble_px: 0.0,
            hover: false,
            mode: IslandMode::Collapsed,
        }
    }

    pub fn target(&self) -> f32 {
        match self.mode {
            IslandMode::Collapsed | IslandMode::Closing => 0.0,
            IslandMode::Glancing | IslandMode::Glance => GLANCE_T,
            IslandMode::Opening | IslandMode::Open => 1.0,
        }
    }

    pub fn toggle(&mut self) {
        match self.mode {
            IslandMode::Collapsed | IslandMode::Closing => {
                self.mode = IslandMode::Opening;
                // Kick the wobble so the droplet sloshes as it detaches.
                self.wobble.velocity += 3.2;
            }
            IslandMode::Glancing | IslandMode::Glance | IslandMode::Opening | IslandMode::Open => {
                self.mode = IslandMode::Closing;
            }
        }
    }

    /// Collapsed -> glance pill (the dwell-hover peek).
    pub fn glance(&mut self) {
        match self.mode {
            IslandMode::Collapsed | IslandMode::Closing => {
                self.mode = IslandMode::Glancing;
                self.wobble.velocity += 1.6;
            }
            _ => {}
        }
    }

    /// Glance pill -> fully open card (click or prolonged dwell).
    pub fn expand(&mut self) {
        match self.mode {
            IslandMode::Glancing | IslandMode::Glance => {
                self.mode = IslandMode::Opening;
                self.wobble.velocity += 1.6;
            }
            _ => {}
        }
    }

    pub fn set_hover(&mut self, hover: bool) {
        if hover && !self.hover && self.mode == IslandMode::Collapsed {
            // Proximity pre-stretch: tiny pull-down before click (magnetic feel).
            self.open.velocity += 0.35;
        }
        self.hover = hover;
    }

    /// Advance springs. Returns true while anything is still moving.
    pub fn tick(&mut self, dt: f32) -> bool {
        let target = self.target();
        let a = self.open.step(target, dt);
        // Wobble spring always targets 0; velocity kicks make it slosh.
        let b = self.wobble.step(0.0, dt);
        self.wobble_px = (self.wobble.value * 6.0).clamp(-9.0, 12.0);
        let t = self.open.value;
        match self.mode {
            IslandMode::Glancing if (t - GLANCE_T).abs() < 0.015 && self.open.velocity.abs() < 0.15 => {
                self.mode = IslandMode::Glance;
            }
            IslandMode::Opening if t > 0.985 && self.open.velocity.abs() < 0.15 => {
                self.mode = IslandMode::Open;
            }
            IslandMode::Closing if t < 0.015 && self.open.velocity.abs() < 0.15 => {
                self.mode = IslandMode::Collapsed;
                self.open.value = 0.0;
                self.open.velocity = 0.0;
            }
            _ => {}
        }
        a || b
    }

    #[allow(dead_code)]
    pub fn open_t(&self) -> f32 {
        self.open.value.clamp(0.0, 1.0)
    }

    pub fn size(&self) -> (f32, f32) {
        (self.geom.width(self.open.value), self.geom.height(self.open.value, self.wobble_px))
    }

    #[allow(dead_code)]
    /// Outline in window-local coords (top edge at y=0).
    pub fn outline_local(&self, cx: f32, n: usize) -> Vec<(f32, f32)> {
        self.geom
            .outline(cx, 0.0, self.open.value, self.wobble_px, n)
            .into_iter()
            .map(|p| (p.x - (cx - self.size().0 / 2.0), p.y))
            .collect()
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

    #[test]
    fn toggle_opens_and_settles() {
        let mut s = IslandState::new();
        s.toggle();
        assert_eq!(s.mode, IslandMode::Opening);
        for _ in 0..900 {
            s.tick(1.0 / 120.0);
        }
        assert_eq!(s.mode, IslandMode::Open);
        s.toggle();
        for _ in 0..900 {
            s.tick(1.0 / 120.0);
        }
        assert_eq!(s.mode, IslandMode::Collapsed);
    }
}
