//! Damped springs for buttery island motion.
//!
//! The island never lerps linearly. Every animated scalar (window width and
//! height, hover glow, HUD offset) is a spring so expand/collapse has
//! weight and settle.

#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub stiffness: f32,
    pub damping: f32,
}

impl Spring {
    pub fn new(value: f32, stiffness: f32, damping: f32) -> Self {
        Self { value, velocity: 0.0, stiffness, damping }
    }

    pub fn set_target_soft(&mut self, target: f32) {
        // Nudge velocity toward target so interruptions feel continuous.
        self.velocity += (target - self.value) * 2.0;
    }

    /// Advance by `dt` seconds toward `target`. Returns true while settling.
    pub fn step(&mut self, target: f32, dt: f32) -> bool {
        let dt = dt.clamp(1.0 / 240.0, 1.0 / 15.0);
        let accel = (target - self.value) * self.stiffness - self.velocity * self.damping;
        self.velocity += accel * dt;
        self.value += self.velocity * dt;
        (self.value - target).abs() > 0.0005 || self.velocity.abs() > 0.0005
    }

    pub fn at_rest(&self, target: f32) -> bool {
        (self.value - target).abs() < 0.0005 && self.velocity.abs() < 0.005
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settles() {
        let mut s = Spring::new(0.0, 430.0, 37.0);
        let mut moving = true;
        for _ in 0..600 {
            moving = s.step(1.0, 1.0 / 120.0);
            if !moving {
                break;
            }
        }
        assert!(!moving);
        assert!((s.value - 1.0).abs() < 0.01);
    }

    #[test]
    fn overshoots_when_underdamped() {
        let mut s = Spring::new(0.0, 90.0, 7.5);
        let mut max = 0.0f32;
        for _ in 0..300 {
            s.step(1.0, 1.0 / 120.0);
            max = max.max(s.value);
        }
        assert!(max > 1.0, "loose spring should overshoot, got {max}");
    }
}
