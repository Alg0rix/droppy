//! The signature teardrop morph.
//!
//! Instead of scaling a rounded rect, the island hangs like a droplet:
//!
//! ```text
//!  t=0 (pill)        t=0.45 (necking)        t=1 (full card)
//!  ▬▬▬▬▬▬            ╲  ▬▬▬▬  ╱                ┌──────────┐
//!                        ╲╱  ╲╱                  │          │
//!                        │ droplet │             │          │
//!                       ╱╲       ╱╲              │          │
//!                      ╱  ╲_____╱  ╲             └──────────┘
//! ```
//!
//! The renderer samples [`DropletGeom::outline`] which returns a closed
//! polygon (evenly spaced points). Direct2D turns it into a path geometry.
//!
//! Phases driven by one `open` spring (0=collapsed, 1=expanded):
//! - 0.00–0.25: pill stretches down, top edge pinned to screen top.
//! - 0.25–0.60: neck pinches in, bulb swells (surface tension).
//! - 0.60–1.00: bulb squares into the card, corners relax to 26px.
//!
//! A second `wobble` spring adds ±6px vertical slosh on the bulb tip.

use serde::{Deserialize, Serialize};

/// Pixel geometry of one frame of the morph.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DropletGeom {
    /// Collapsed pill size.
    pub pill_w: f32,
    pub pill_h: f32,
    /// Glance size: the wide flat pill the island rests at mid-morph.
    pub glance_w: f32,
    pub glance_h: f32,
    /// Fully opened card size.
    pub card_w: f32,
    pub card_h: f32,
    /// Corner radius of the open card.
    pub card_radius: f32,
}

impl Default for DropletGeom {
    fn default() -> Self {
        Self { pill_w: 180.0, pill_h: 36.0, glance_w: 440.0, glance_h: 78.0, card_w: 680.0, card_h: 318.0, card_radius: 26.0 }
    }
}

/// One sampled outline point with a "fullness" hint (1 = bulb tip).
#[derive(Debug, Clone, Copy)]
pub struct OutlinePt {
    pub x: f32,
    pub y: f32,
    pub fullness: f32,
}

impl DropletGeom {
    /// Width at morph position `open` (0..1). The morph is two hops:
    /// pill -> glance pill on the lower half, glance pill -> card above 0.5.
    pub fn width(&self, open: f32) -> f32 {
        let t = open.clamp(-0.15, 1.15);
        if t < 0.5 {
            self.pill_w + (self.glance_w - self.pill_w) * smootherstep(t * 2.0)
        } else {
            self.glance_w + (self.card_w - self.glance_w) * smootherstep((t - 0.5) * 2.0)
        }
    }

    /// Height at morph position `open`.
    pub fn height(&self, open: f32, wobble_px: f32) -> f32 {
        let t = open.clamp(-0.15, 1.15);
        // Droplet hangs: height grows faster than width early on (gravity).
        let hang = t.powf(0.75);
        let h = if hang < 0.5 {
            self.pill_h + (self.glance_h - self.pill_h) * smootherstep(hang * 2.0)
        } else {
            self.glance_h + (self.card_h - self.glance_h) * smootherstep((hang - 0.5) * 2.0)
        };
        h + wobble_px
    }

    /// Neck pinch factor: 1 = straight sides, <1 = pinched waist.
    /// Dips to ~0.62 around t=0.45 then relaxes to 1.0 when square; it also
    /// straightens around t=0.5 so the glance pill rests with flat sides.
    pub fn neck(&self, open: f32) -> f32 {
        let t = open.clamp(0.0, 1.0);
        // Gaussian dip centered at 0.45.
        let dip = (-((t - 0.45) * (t - 0.45)) / (2.0 * 0.0225)).exp();
        let base = 1.0 - 0.38 * dip * (1.0 - t * 0.55);
        let rest = (-((t - 0.5) * (t - 0.5)) / (2.0 * 0.0004)).exp();
        base + (1.0 - base) * rest
    }

    /// Tip roundness: 1 = perfect droplet circle, 0 = squared card.
    pub fn tip_round(&self, open: f32) -> f32 {
        1.0 - smootherstep((open.clamp(0.0, 1.0) - 0.55) / 0.45)
    }

    /// Sample the closed outline centered at `cx` with the flat top pinned at `top`.
    ///
    /// The silhouette is a rounded rectangle (pill when collapsed, card when
    /// open) whose half-width is modulated by height: the neck pinches in at
    /// the top while the bulb below swells, which reads as a droplet hanging
    /// from the screen edge. Returns a dense, adaptively sampled polygon
    /// (clockwise from the top centre): straight runs every ~5px, corner arcs
    /// in 24 steps, so corners stay round when filled. `n` is the minimum
    /// number of points the caller is happy with.
    pub fn outline(&self, cx: f32, top: f32, open: f32, wobble_px: f32, _n: usize) -> Vec<OutlinePt> {
        use std::f32::consts::{FRAC_PI_2, PI};
        let w = self.width(open);
        let h = self.height(open, wobble_px).max(2.0);
        let neck = self.neck(open);
        let round = self.tip_round(open);
        let o = open.clamp(0.0, 1.0);
        let r_pill = (self.pill_h / 2.0).min(h / 2.0).min(w / 2.0);
        let rb = (r_pill + (self.card_radius - r_pill) * smootherstep(o)).min(h / 2.0).min(w / 2.0).max(1.0);
        
        let (l, r, b) = (cx - w / 2.0, cx + w / 2.0, top + h);
        let mut poly: Vec<(f32, f32)> = Vec::with_capacity(160);
        let straight = |poly: &mut Vec<(f32, f32)>, x0: f32, y0: f32, x1: f32, y1: f32| {
            let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
            let k = (len / 5.0).ceil().max(1.0) as usize;
            for i in 0..k {
                let t = i as f32 / k as f32;
                poly.push((x0 + (x1 - x0) * t, y0 + (y1 - y0) * t));
            }
        };
        let arc = |poly: &mut Vec<(f32, f32)>, ccx: f32, ccy: f32, a0: f32, a1: f32| {
            let k = 24;
            for i in 0..k {
                let a = a0 + (a1 - a0) * i as f32 / k as f32;
                poly.push((ccx + rb * a.cos(), ccy + rb * a.sin()));
            }
        };
        straight(&mut poly, cx, top, r, top);
        straight(&mut poly, r, top, r, b - rb);
        arc(&mut poly, r - rb, b - rb, 0.0, FRAC_PI_2);
        straight(&mut poly, r - rb, b, l + rb, b);
        arc(&mut poly, l + rb, b - rb, FRAC_PI_2, PI);
        straight(&mut poly, l, b - rb, l, top);
        straight(&mut poly, l, top, cx, top);

        // Hanging droplet: narrow where it attaches to the screen edge, swelling
        // into the bulb below. `pinch` is 0 at both rest states, so the pill and
        // the open card stay exact rectangles.
        let pinch = 1.0 - neck;
        for p in poly.iter_mut() {
            let u = ((p.1 - top) / h).clamp(0.0, 1.0);
            let attach = 1.0 - smootherstep(u / 0.6);
            let swell = smootherstep((u - 0.55) / 0.45) * round;
            let k = 1.0 - pinch * attach + swell * 0.045;
            p.0 = cx + (p.0 - cx) * k;
        }

        poly.into_iter()
            .map(|(x, y)| {
                let lower = ((y - top) / h).clamp(0.0, 1.0);
                OutlinePt { x, y: y.max(top), fullness: lower * round + (1.0 - round) * 0.5 }
            })
            .collect()
    }
}

fn smootherstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_is_pill() {
        let g = DropletGeom::default();
        assert!((g.width(0.0) - g.pill_w).abs() < 0.01);
        assert!((g.height(0.0, 0.0) - g.pill_h).abs() < 0.01);
        assert!((g.neck(0.0) - 1.0).abs() < 0.05);
    }

    #[test]
    fn open_is_card() {
        let g = DropletGeom::default();
        assert!((g.width(1.0) - g.card_w).abs() < 0.5);
        assert!((g.height(1.0, 0.0) - g.card_h).abs() < 0.5);
        assert!(g.tip_round(1.0) < 0.05);
        assert!(g.tip_round(0.0) > 0.95);
    }

    #[test]
    fn neck_pinches_midway() {
        let g = DropletGeom::default();
        let mid = g.neck(0.45);
        assert!(mid < 0.75, "expected pinch, got {mid}");
        assert!(g.neck(1.0) > 0.95);
    }

    #[test]
    fn outline_is_closed_and_centered() {
        let g = DropletGeom::default();
        for t in [0.0, 0.25, 0.45, 0.7, 1.0] {
            let pts = g.outline(500.0, 0.0, t, 0.0, 48);
            assert!(pts.len() >= 48);
            let mean_x: f32 = pts.iter().map(|p| p.x).sum::<f32>() / pts.len() as f32;
            assert!((mean_x - 500.0).abs() < 2.0, "t={t} mean {mean_x}");
            assert!(pts.iter().all(|p| p.y >= -0.01));
            assert!(pts.iter().any(|p| p.y < 0.01), "top edge stays flush");
        }
    }
}
