//! Island-level effects: one-shot flourishes that move the whole island,
//! not just the face — a droplet dripping off the pill, a breathing pulse
//! around the rim, a ripple where the pointer slipped by, plus the little
//! context helpers (sun-angle sheen, meeting anticipation, peek rules).
//!
//! Every timeline is pure seconds-in data-out: `UiState` owns one [`Fx`],
//! `layout::build` reads it each frame, and the selftest + unit tests can
//! drive the same math deterministically on any OS.

use super::scene::Color;
use std::f32::consts::PI;

// ------------------------------------------------------------- timelines

/// The droplet pinches off the island's bottom tip this many seconds in.
pub const DRIP_DETACH: f32 = 0.34;
/// Total drip timeline (pinch + fall + fade).
pub const DRIP_DUR: f32 = 1.15;
/// Three breathing rings chase each other off the rim over this span.
pub const PULSE_DUR: f32 = 1.5;
/// Pointer-neighbourhood ripple length (two staggered rings).
pub const RIPPLE_DUR: f32 = 0.7;

fn sm(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Transient island-level effects, all in seconds since their trigger.
#[derive(Debug, Clone, Copy, Default)]
pub struct Fx {
    /// Detached-droplet timeline, or None.
    pub drip: Option<f32>,
    /// Rim pulse: (elapsed, colour), or None.
    pub pulse: Option<(f32, Color)>,
    /// Ripple at the pointer: (elapsed, pointer in window px), or None.
    pub ripple: Option<(f32, (f32, f32))>,
    /// Raised for one tick when the droplet pinches off; the shell consumes
    /// it to kick the island springs (UiState cannot reach them itself).
    pub detach: bool,
}

impl Fx {
    /// Something landed that a droplet can answer: a water reminder, a
    /// finished download, a completed AI reply, an update found.
    pub fn drip(&mut self) {
        self.drip = Some(0.0);
    }

    /// A meaningful external event breathes once off the rim.
    pub fn pulse(&mut self, c: Color) {
        self.pulse = Some((0.0, c));
    }

    /// The pointer crossed into the near band around the collapsed pill.
    pub fn ripple(&mut self, at: (f32, f32)) {
        self.ripple = Some((0.0, at));
    }

    /// Any effect still running this frame.
    pub fn alive(&self) -> bool {
        self.drip.is_some() || self.pulse.is_some() || self.ripple.is_some()
    }

    /// Advance every running timeline by `dt` seconds. Returns `alive()`.
    pub fn tick(&mut self, dt: f32) -> bool {
        if let Some(t) = &mut self.drip {
            *t += dt;
            if *t - dt < DRIP_DETACH && *t >= DRIP_DETACH {
                self.detach = true;
            }
            if *t > DRIP_DUR {
                self.drip = None;
            }
        }
        if let Some((t, _)) = &mut self.pulse {
            *t += dt;
            if *t > PULSE_DUR {
                self.pulse = None;
            }
        }
        if let Some((t, _)) = &mut self.ripple {
            *t += dt;
            if *t > RIPPLE_DUR {
                self.ripple = None;
            }
        }
        self.alive()
    }
}

/// Where the drip stands at `t` seconds in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DripState {
    /// Nub growing under the island's bottom tip before detaching (px).
    pub grow: f32,
    /// The detached droplet: (x drift, y under the bottom edge, y-stretch).
    pub drop: Option<(f32, f32, f32)>,
    /// End-of-flight fade for the droplet (1 while falling).
    pub alpha: f32,
}

/// The drip at `t`: a nub swells under the island's bottom centre, then a
/// droplet lets go and accelerates away, stretching as it picks up speed.
pub fn drip_state(t: f32) -> DripState {
    if t < DRIP_DETACH {
        DripState { grow: sm(t / DRIP_DETACH) * 5.5, drop: None, alpha: 1.0 }
    } else {
        let f = t - DRIP_DETACH;
        let y = 2.0 + 110.0 * f * f;
        let stretch = 1.0 + (f / 0.45).min(1.0) * 0.7;
        let alpha = 1.0 - sm((t - (DRIP_DUR - 0.18)) / 0.18);
        DripState { grow: 0.0, drop: Some((f * 7.0, y, stretch)), alpha }
    }
}

/// Right after the droplet lets go the freed bottom edge wobbles sideways
/// for a moment — the little shudder of letting go, decaying in ~0.5 s.
pub fn drip_shudder(t: f32) -> f32 {
    if t < DRIP_DETACH {
        0.0
    } else {
        let f = t - DRIP_DETACH;
        (f * 38.0).sin() * (-f * 8.5).exp() * 1.6
    }
}

/// Phase and alpha of the i-th breathing ring at `t` seconds in (rings are
/// staggered a third of a beat apart). None once the pulse is over.
pub fn pulse_ring(t: f32, i: u32) -> Option<(f32, f32)> {
    if t > PULSE_DUR {
        return None;
    }
    let ph = (t * 1.9 + i as f32 * 0.37).fract();
    Some((ph, (1.0 - ph) * (1.0 - t / PULSE_DUR)))
}

/// Phase and alpha of the i-th ripple ring at `t` seconds in (two rings,
/// the second trailing half a cycle). None once the ripple is done.
pub fn ripple_ring(t: f32, i: u32) -> Option<(f32, f32)> {
    if t > RIPPLE_DUR {
        return None;
    }
    let ph = t / RIPPLE_DUR * 1.35 - i as f32 * 0.5;
    if !(0.0..1.0).contains(&ph) {
        return None;
    }
    Some((ph, (1.0 - ph) * (1.0 - ph)))
}

/// Sun-angle sheen: a soft band of light slides across the island with the
/// local hour — dawn kisses the left edge, noon sits dead centre, dusk the
/// right. At night there is no sheen. Returns (0..1 across the body, alpha).
pub fn sheen_band(hour: f32) -> Option<(f32, f32)> {
    if !(6.0..20.0).contains(&hour) {
        return None;
    }
    let x = (hour - 6.0) / 14.0;
    let noon = 1.0 - (x - 0.5).abs() * 2.0; // brightest around midday
    Some((x, 0.05 + 0.09 * noon * noon))
}

/// Anticipation level about an upcoming meeting: how far out it is sets
/// the face's itch level. The last ten minutes belong to the countdown
/// pill, and the start itself to the `React::Gasp` alert — this is only
/// the waiting, so it never competes with either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingBand {
    /// 25..40 min out: an occasional clock-check.
    Horizon,
    /// 15..25 min: glances come more often, a little fidgeting.
    Soon,
    /// 10..15 min: hard to look at anything else.
    Close,
}

/// Which anticipation band `secs`-until-start lands in (None = owned by
/// the countdown pill or the Gasp, or simply too far out to matter).
pub fn meeting_band(secs: i64) -> Option<MeetingBand> {
    match secs {
        600..=899 => Some(MeetingBand::Close),
        900..=1499 => Some(MeetingBand::Soon),
        1500..=2400 => Some(MeetingBand::Horizon),
        _ => None,
    }
}

/// 0..1 anticipation inside the bands, rising as the meeting nears.
pub fn meeting_tension(secs: i64) -> f32 {
    match meeting_band(secs) {
        Some(MeetingBand::Close) => 0.70 + 0.30 * (900 - secs) as f32 / 300.0,
        Some(MeetingBand::Soon) => 0.40 + 0.30 * (1500 - secs) as f32 / 600.0,
        Some(MeetingBand::Horizon) => 0.15 + 0.25 * (2400 - secs) as f32 / 900.0,
        None => 0.0,
    }
}

/// May the face peek over the rim? Calm idle only: no act running or
/// queued, the pointer circling the near band (`dist` in face units) but
/// never resting on the island itself.
pub fn peek_eligible(calm: bool, busy: bool, hovering: bool, dist: f32) -> bool {
    calm && !busy && !hovering && (30.0..280.0).contains(&dist)
}

// ------------------------------------------------------------- geometry

/// Closed outline of an ellipse (`n` segments, last point repeats first).
pub fn ellipse_pts(cx: f32, cy: f32, rx: f32, ry: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|i| {
            let a = i as f32 / n as f32 * 2.0 * PI;
            (cx + a.cos() * rx, cy + a.sin() * ry)
        })
        .collect()
}

/// Droplet silhouette centred on `x, y`: a tip above `y - r*1.7*stretch`
/// melting into a round bottom of radius `r`. `stretch` > 1 draws it
/// faster (longer).
pub fn drop_pts(x: f32, y: f32, r: f32, stretch: f32) -> Vec<(f32, f32)> {
    let mut pts = vec![(x, y - r * 1.7 * stretch)];
    for i in 0..=14 {
        let a = -PI / 5.0 + (PI + PI / 5.0) * i as f32 / 14.0;
        pts.push((x + a.cos() * r, y + a.sin() * r));
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheen_follows_the_sun() {
        // Night: no band at all.
        for h in [0.0, 3.5, 5.9, 20.5, 23.0] {
            assert!(sheen_band(h).is_none(), "hour {h}");
        }
        // Dawn left, noon centre, dusk right — monotonic sweep.
        let (dawn, _) = sheen_band(7.0).unwrap();
        let (mid, mid_a) = sheen_band(13.0).unwrap();
        let (dusk, _) = sheen_band(19.0).unwrap();
        assert!(dawn < mid && mid < dusk, "{dawn} {mid} {dusk}");
        assert!((mid - 0.5).abs() < 0.01, "noon sits centre: {mid}");
        // Brightest near noon, dimmer at the edges, always subtle.
        let (_, edge_a) = sheen_band(6.5).unwrap();
        assert!(mid_a > edge_a);
        assert!(mid_a <= 0.15);
    }

    #[test]
    fn meeting_bands_classify_by_seconds() {
        assert_eq!(meeting_band(30), None, "the Gasp owns the start");
        assert_eq!(meeting_band(599), None, "the countdown pill owns <10m");
        assert_eq!(meeting_band(600), Some(MeetingBand::Close));
        assert_eq!(meeting_band(750), Some(MeetingBand::Close));
        assert_eq!(meeting_band(899), Some(MeetingBand::Close));
        assert_eq!(meeting_band(900), Some(MeetingBand::Soon));
        assert_eq!(meeting_band(1499), Some(MeetingBand::Soon));
        assert_eq!(meeting_band(1500), Some(MeetingBand::Horizon));
        assert_eq!(meeting_band(2400), Some(MeetingBand::Horizon));
        assert_eq!(meeting_band(2401), None, "beyond 40m it doesn't care");
        assert!(meeting_tension(720) > meeting_tension(1200));
        assert!(meeting_tension(1200) > meeting_tension(2000));
        assert_eq!(meeting_tension(300), 0.0);
        assert!((0.0..=1.0).contains(&meeting_tension(601)));
    }

    #[test]
    fn drip_pinches_then_falls() {
        // Growing nub, no droplet yet.
        let a = drip_state(0.10);
        assert!(a.grow > 0.0 && a.grow < 5.5);
        assert!(a.drop.is_none());
        let b = drip_state(DRIP_DETACH - 0.01);
        assert!(b.grow > a.grow, "the nub swells toward detach");
        // Just past detach: droplet released close under the island.
        let c = drip_state(DRIP_DETACH + 0.05);
        let (_, y0, _) = c.drop.unwrap();
        assert!(y0 > 0.0 && y0 < 4.0, "detaches at the rim: {y0}");
        // It accelerates (gravity): later samples fall faster.
        let (_, y1, _) = drip_state(DRIP_DETACH + 0.30).drop.unwrap();
        let (_, y2, _) = drip_state(DRIP_DETACH + 0.50).drop.unwrap();
        assert!(y2 - y1 > y1 - y0, "fall accelerates");
        // It stretches as it speeds up and fades at the very end.
        let (_, _, s1) = drip_state(DRIP_DETACH + 0.20).drop.unwrap();
        let (_, _, s2) = drip_state(DRIP_DETACH + 0.45).drop.unwrap();
        assert!(s2 > s1, "stretches with speed");
        assert!(drip_state(DRIP_DUR - 0.02).alpha < 0.5);
        // The shudder only lives after detach and decays away.
        assert_eq!(drip_shudder(DRIP_DETACH - 0.01), 0.0);
        assert!(drip_shudder(DRIP_DETACH + 0.1).abs() > 0.1);
        assert!(drip_shudder(DRIP_DETACH + 0.7).abs() < 0.05);
    }

    #[test]
    fn ripple_and_pulse_rings_breathe() {
        // Ripple: first ring starts immediately, the second trails.
        assert!(ripple_ring(0.02, 0).is_some());
        assert!(ripple_ring(0.02, 1).is_none(), "trailing ring waits");
        assert!(ripple_ring(0.4, 1).is_some());
        assert!(ripple_ring(RIPPLE_DUR + 0.01, 0).is_none());
        // Alpha fades as the phase completes.
        let (_, a0) = ripple_ring(0.1, 0).unwrap();
        let (_, a1) = ripple_ring(0.5, 0).unwrap();
        assert!(a0 > a1);
        // Pulse: three staggered rings, gone after the duration.
        let early: Vec<_> = (0..3).filter(|&i| pulse_ring(0.05, i).is_some()).collect();
        assert!(!early.is_empty());
        for i in 0..3 {
            assert!(pulse_ring(PULSE_DUR + 0.01, i).is_none());
        }
        let (_, pa) = pulse_ring(0.3, 0).unwrap();
        assert!(pa > 0.0 && pa <= 1.0);
    }

    #[test]
    fn peek_only_when_calm_and_near() {
        // Happy case: calm face, idle, pointer circling nearby, not on it.
        assert!(peek_eligible(true, false, false, 80.0));
        // Hovering the island itself: it should delight, not peek.
        assert!(!peek_eligible(true, false, true, 80.0));
        // An act or a queue running: no peeking.
        assert!(!peek_eligible(true, true, false, 80.0));
        // Too close reads as hover territory; too far is out of reach.
        assert!(!peek_eligible(true, false, false, 20.0));
        assert!(!peek_eligible(true, false, false, 300.0));
        // Not calm (dancing, working, listening): stays put.
        assert!(!peek_eligible(false, false, false, 80.0));
    }

    #[test]
    fn fx_timelines_expire() {
        let mut fx = Fx::default();
        assert!(!fx.alive());
        fx.drip();
        fx.pulse(Color::rgb(1, 2, 3));
        fx.ripple((10.0, -5.0));
        assert!(fx.alive());
        // Crossing the detach point raises the flag exactly once.
        let mut saw_detach = false;
        for _ in 0..30 {
            fx.tick(1.0 / 60.0);
            saw_detach |= fx.detach;
            fx.detach = false;
        }
        assert!(saw_detach, "detach flag fired on the crossing tick");
        // Everything finishes inside its duration.
        for _ in 0..90 {
            fx.tick(1.0 / 60.0);
        }
        assert!(!fx.alive(), "{:?}", fx);
    }
}
