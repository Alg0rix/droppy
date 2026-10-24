//! Audio visualizer bars.
//!
//! Driven by the real output level of the default playback device (the same
//! peak meter the eyes use). The level is spread across bars with a bass-heavy
//! tilt and per-bar wobble, then smoothed with a fast attack and slow decay so
//! it reads like a spectrum without needing an FFT.

pub const BARS: usize = 24;

#[derive(Debug, Clone)]
pub struct Visualizer {
    bars: [f32; BARS],
    t: f32,
}

impl Default for Visualizer {
    fn default() -> Self {
        Self { bars: [0.0; BARS], t: 0.0 }
    }
}

impl Visualizer {
    /// `peak` = measured output level 0..1 (None = no meter, fall back to a
    /// gentle idle motion while `playing`).
    pub fn tick(&mut self, dt: f32, peak: Option<f32>, playing: bool) {
        self.t += dt;
        let level = match (playing, peak) {
            (false, _) => 0.0,
            (true, Some(p)) => p.clamp(0.0, 1.0).powf(0.55),
            (true, None) => 0.45,
        };
        for (i, b) in self.bars.iter_mut().enumerate() {
            let f = i as f32 / (BARS - 1) as f32;
            let tilt = 1.0 - 0.55 * f;
            let k = i as f32;
            let wob = 0.5 + 0.5 * ((self.t * (4.0 + 0.37 * k) + k * 1.9).sin() * 0.6 + (self.t * (7.3 + 0.21 * k) + k * 0.7).sin() * 0.4);
            let target = (level * tilt * (0.25 + 0.75 * wob)).clamp(0.0, 1.0);
            let rate = if target > *b { 28.0 } else { 7.0 };
            *b += (target - *b) * (1.0 - (-rate * dt).exp());
            if *b < 0.002 {
                *b = 0.0;
            }
        }
    }

    pub fn active(&self) -> bool {
        self.bars.iter().any(|b| *b > 0.0)
    }

    /// `n` bar heights 0..1, sampled evenly across the band.
    pub fn sample(&self, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let pos = if n > 1 { i as f32 / (n - 1) as f32 * (BARS - 1) as f32 } else { 0.0 };
                let (lo, frac) = (pos.floor() as usize, pos.fract());
                let hi = (lo + 1).min(BARS - 1);
                self.bars[lo] * (1.0 - frac) + self.bars[hi] * frac
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loud_is_taller_than_quiet_and_pause_decays_to_zero() {
        let mut quiet = Visualizer::default();
        let mut loud = Visualizer::default();
        for _ in 0..120 {
            quiet.tick(1.0 / 60.0, Some(0.05), true);
            loud.tick(1.0 / 60.0, Some(0.9), true);
        }
        let sum = |v: &Visualizer| v.sample(8).iter().sum::<f32>();
        assert!(sum(&loud) > sum(&quiet) * 1.5);
        for _ in 0..300 {
            loud.tick(1.0 / 60.0, Some(0.9), false);
        }
        assert!(!loud.active());
    }
}
