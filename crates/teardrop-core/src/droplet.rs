//! The island morph: a rounded rect driven by two size springs.
//!
//! The live window animates `w` and `h` independently (see `IslandState` in
//! the app crate), so a direct pill -> card open never stalls at the glance
//! size the way a single scalar morph did:
//!
//! ```text
//!  pill                glance                card
//!  ▬▬▬▬▬        ▬▬▬▬▬▬▬▬▬▬▬▬▬▬        ┌──────────────┐
//!                                     │              │
//!                                     └──────────────┘
//! ```
//!
//! The renderer samples [`DropletGeom::outline`] with the spring's current
//! pixel size, which returns a closed, evenly sampled polygon that Direct2D
//! turns into a path geometry. `width`/`height` are static samplers of the
//! same pill -> glance -> card staging, used for tests and still frames.

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

    /// Height at morph position `open`: the same two-hop staging as
    /// [`Self::width`].
    pub fn height(&self, open: f32) -> f32 {
        let t = open.clamp(-0.15, 1.15);
        if t < 0.5 {
            self.pill_h + (self.glance_h - self.pill_h) * smootherstep(t * 2.0)
        } else {
            self.glance_h + (self.card_h - self.glance_h) * smootherstep((t - 0.5) * 2.0)
        }
    }

    /// Sample the closed outline centered at `cx`, top edge at `top`: a
    /// rounded rect `w` x `h`. Anchored keeps the top edge flat (glued to
    /// the screen) with bottom radius `card_radius`; `floating` rounds all
    /// four corners with a radius capped at half the height so the collapsed
    /// pill is a true capsule. Returns a dense polygon (clockwise from the
    /// top centre): straight runs every ~5px, corner arcs in 24 steps.
    pub fn outline(&self, cx: f32, top: f32, w: f32, h: f32, floating: bool) -> Vec<(f32, f32)> {
        use std::f32::consts::{FRAC_PI_2, PI};
        let h = h.max(2.0);
        // Anchored only curves the bottom corners (the top is pinned flat to
        // the screen edge), so the radius may use the full height, not just
        // half of it.
        let rb = self.card_radius.min(h).min(w / 2.0).max(1.0);
        let (rt, rb) = if floating {
            let r = self.card_radius.min(h / 2.0).min(w / 2.0).max(1.0);
            (r, r)
        } else {
            (0.0, rb)
        };

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
        let arc = |poly: &mut Vec<(f32, f32)>, ccx: f32, ccy: f32, a0: f32, a1: f32, rad: f32| {
            let k = 24;
            for i in 0..k {
                let a = a0 + (a1 - a0) * i as f32 / k as f32;
                poly.push((ccx + rad * a.cos(), ccy + rad * a.sin()));
            }
        };
        straight(&mut poly, cx, top, r - rt, top);
        if floating {
            arc(&mut poly, r - rt, top + rt, -FRAC_PI_2, 0.0, rt);
        }
        straight(&mut poly, r, top + rt, r, b - rb);
        arc(&mut poly, r - rb, b - rb, 0.0, FRAC_PI_2, rb);
        straight(&mut poly, r - rb, b, l + rb, b);
        arc(&mut poly, l + rb, b - rb, FRAC_PI_2, PI, rb);
        straight(&mut poly, l, b - rb, l, top + rt);
        if floating {
            arc(&mut poly, l + rt, top + rt, PI, 3.0 * FRAC_PI_2, rt);
        }
        straight(&mut poly, l + rt, top, cx, top);
        poly
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
    fn sizes_hit_pill_glance_card() {
        let g = DropletGeom::default();
        for (t, w, h) in [(0.0, g.pill_w, g.pill_h), (0.5, g.glance_w, g.glance_h), (1.0, g.card_w, g.card_h)] {
            assert!((g.width(t) - w).abs() < 0.01, "width({t}) = {}", g.width(t));
            assert!((g.height(t) - h).abs() < 0.01, "height({t}) = {}", g.height(t));
        }
    }

    #[test]
    fn outline_is_closed_centered_and_flush() {
        let g = DropletGeom::default();
        for t in [0.0, 0.25, 0.45, 0.7, 1.0] {
            let (w, h) = (g.width(t), g.height(t));
            let pts = g.outline(500.0, 0.0, w, h, false);
            assert!(pts.len() >= 48);
            let mean_x: f32 = pts.iter().map(|p| p.0).sum::<f32>() / pts.len() as f32;
            assert!((mean_x - 500.0).abs() < 2.0, "t={t} mean {mean_x}");
            assert!(pts.iter().all(|p| p.1 >= -0.01));
            assert!(pts.iter().any(|p| p.1 < 0.01), "top edge stays flush");
            // Anchored: the flat top spans the full width, corners included.
            let top: Vec<f32> = pts.iter().filter(|p| p.1 < 0.01).map(|p| p.0).collect();
            let (min, max) = (top.iter().cloned().fold(f32::MAX, f32::min), top.iter().cloned().fold(f32::MIN, f32::max));
            assert!(min <= 500.0 - w / 2.0 + 0.5 && max >= 500.0 + w / 2.0 - 0.5, "t={t} top run {min}..{max} vs w={w}");
        }
    }

    #[test]
    fn floating_outline_rounds_the_top() {
        let g = DropletGeom::default();
        let pts = g.outline(500.0, 0.0, g.pill_w, g.pill_h, true);
        let r = g.card_radius.min(g.pill_h / 2.0).min(g.pill_w / 2.0).max(1.0);
        // No point sits on a top corner: every point on the top edge lies
        // within w/2 - r of the centre (the flat run between the two arcs).
        for p in pts.iter().filter(|p| p.1 < 0.01) {
            assert!((p.0 - 500.0).abs() <= g.pill_w / 2.0 - r + 0.5, "top corner at {:?}", p);
        }
        assert!(pts.iter().all(|p| p.1 >= -0.01));
        let mean_x: f32 = pts.iter().map(|p| p.0).sum::<f32>() / pts.len() as f32;
        assert!((mean_x - 500.0).abs() < 2.0);
    }
}
