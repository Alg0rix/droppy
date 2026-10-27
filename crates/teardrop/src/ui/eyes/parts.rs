use super::*;
impl Eyes {
    /// Mood- and calendar-driven particles.
    pub(super) fn ambient_parts(&mut self, s: &Senses) {
        use Mood::*;
        let (ax, ay) = s.area;
        let mood = self.mood;
        match mood {
            Asleep => {
                if self.due(T_Z, 1.5) {
                    let size = self.range(6.0, 7.5);
                    self.spawn(Kind::Z, 12.0, -5.0, 3.0, -4.0, 2.2, size);
                }
                let bubbling = self.parts.iter().any(|p| p.kind == Kind::Bubble);
                if !bubbling && self.due(T_BUBBLE, 3.6) {
                    self.spawn(Kind::Bubble, 9.0, 7.0, 0.0, 0.0, 3.0, 1.0);
                }
            }
            Dance | Headbang | Vibe => {
                // Notes ride the beat (see `choreo`).
                if self.genre == Genre::Love && self.due(T_HEART, 1.0) {
                    let x = self.range(-16.0, 16.0);
                    self.spawn(Kind::Heart, x, 4.0, 0.0, -7.0, 1.4, 2.6);
                }
            }
            Work(c) => self.work_parts(c, s),
            Thinking => {
                if self.due(T_THINK, 0.45) {
                    let i = (self.t * 2.2) as i32 % 3;
                    self.spawn(Kind::Dot, -22.0 - 5.0 * i as f32, -11.0, 0.0, -2.0, 1.3, 1.0);
                }
            }
            Listening => {
                // Sound leaving the mouth: little dots drift up-left while you talk.
                let lvl = self.listen_lvl;
                if lvl > 0.3 && self.due(T_THINK, 0.5 - 0.3 * lvl) {
                    self.spawn(Kind::Dot, -20.0, 2.0, -4.0, -3.0, 0.9, 1.0);
                }
            }
            Typing if self.keys.len() >= 14 => {
                if self.due(T_SWEAT, 1.6) {
                    self.spawn(Kind::Sweat, 22.0, -8.0, 0.0, 2.0, 1.2, 2.0);
                }
            }
            Purr => {
                if self.due(T_HEART, 0.6) {
                    let x = self.range(-20.0, 20.0);
                    self.spawn(Kind::Heart, x, 2.0, 0.0, -8.0, 1.2, 2.8);
                }
            }
            Sweaty => {
                if self.due(T_SWEAT, 2.0) {
                    self.spawn(Kind::Sweat, 22.0, -8.0, 0.0, 2.0, 1.4, 2.2);
                }
            }
            Lost => {
                if self.due(T_QUESTION, 2.6) {
                    let side = if self.rand() < 0.5 { -1.0 } else { 1.0 };
                    self.spawn(Kind::Question, side * 25.0, -4.0, 0.0, -2.5, 1.4, 1.0);
                }
            }
            Hungry => {
                if self.due(T_DROOL, 4.5) {
                    self.spawn(Kind::Drool, 3.0, 10.0, 0.0, 1.2, 2.2, 1.0);
                }
            }
            Party if self.due(T_CONFETTI, 0.9) => {
                let x = self.range(-ax, ax);
                let vy = self.range(8.0, 12.0);
                self.spawn(Kind::Confetti, x, -ay, 0.0, vy, 2.0 * ay / vy, 1.0);
            }
            _ => {}
        }

        let weather = if s.set.weather_mood { s.env.weather } else { Weather::Unknown };
        if matches!(weather, Weather::Rain | Weather::Storm) && self.due(T_RAIN, 0.12) {
            let x = self.range(-ax, ax);
            self.spawn(Kind::Rain, x, -ay, -4.0, 42.0, 2.0 * ay / 42.0, 1.0);
        }
        if weather == Weather::Storm && self.due(T_LIGHTNING, 9.0) && self.rand() < 0.5 {
            self.live.flash = 1.0;
        }
        let (mo, d) = (s.local.month(), s.local.day());
        let wintry = matches!(mo, 12 | 1 | 2);
        let snowing = weather == Weather::Snow;
        if (snowing || wintry) && self.due(T_SNOW, if snowing { 0.35 } else { 1.1 }) {
            let x = self.range(-ax, ax);
            let vy = self.range(5.0, 8.0);
            self.spawn(Kind::Snow, x, -ay, 0.0, vy, 2.0 * ay / vy, 1.0);
        }
        if mo == 1 && d == 1 && self.due(T_FIREWORK, 2.2) {
            let side = if self.rand() < 0.5 { -1.0 } else { 1.0 };
            let (cx, cy) = (side * self.range(ax * 0.45, ax * 0.8), self.range(-ay * 0.5, 0.0));
            let hue = self.rand();
            for i in 0..10 {
                let a = i as f32 / 10.0 * TAU;
                let v = self.range(14.0, 20.0);
                self.spawn(Kind::Firework, cx, cy, a.cos() * v, a.sin() * v, 1.0, 1.0);
                if let Some(p) = self.parts.last_mut() {
                    p.hue = hue;
                }
            }
        }
        if mo == 2 && d == 14 && self.due(T_SEASON_HEART, 1.4) {
            let x = self.range(-ax * 0.8, ax * 0.8);
            self.spawn(Kind::Heart, x, ay * 0.4, 0.0, -6.0, 1.6, 2.4);
        }
        if is_birthday(s) && self.due(T_CONFETTI, 0.5) {
            let x = self.range(-ax, ax);
            let vy = self.range(8.0, 12.0);
            self.spawn(Kind::Confetti, x, -ay, 0.0, vy, 2.0 * ay / vy, 1.0);
        }
    }

    /// Particles for the work theme, on one shared timer.
    fn work_parts(&mut self, c: AppCategory, s: &Senses) {
        use AppCategory::*;
        let (ax, ay) = s.area;
        let edge = (ax - 6.0).min(24.0);
        let side = if self.rand() < 0.5 { -1.0 } else { 1.0 };
        let pick = |r: f32, xs: &[&'static str]| xs[((r * xs.len() as f32) as usize).min(xs.len() - 1)];
        let period = match c {
            Terminal => 0.35,
            Design => 0.9,
            Meeting => 1.1,
            Studio => 0.8,
            Chat => 1.3,
            Spreadsheet => 1.4,
            Data => 1.6,
            Writing => 1.8,
            Coding => 2.2,
            Planning => 2.4,
            Learning => 2.5,
            Research => 2.6,
            Email => 2.8,
            Reading => 3.2,
            Slides => 4.0,
            Media => 2.0,
        };
        if !self.due(T_WORK, period) {
            return;
        }
        let r = self.rand();
        match c {
            Coding => self.spawn(Kind::Sym(pick(r, &["{ }", "</>", ";", "( )", "=>"])), side * edge, -4.0, side * 1.5, -6.0, 1.6, 6.0),
            Terminal => {
                // Output lines scrolling up the right edge; now and then a prompt.
                self.spawn(Kind::Dot, edge, ay * 0.5, 0.0, -16.0, 1.3, 1.0);
                if r < 0.12 {
                    self.spawn(Kind::Sym(">_"), -edge, ay * 0.35, 0.0, -4.0, 1.4, 6.0);
                }
            }
            Writing => self.spawn(Kind::Sym(pick(r, &["Aa", "\u{00B6}", "\u{201C}\u{201D}", "abc"])), side * edge, -2.0, 0.0, -6.0, 1.6, 5.5),
            Spreadsheet => {
                let y = self.range(-8.0, 4.0);
                self.spawn(Kind::Sym(pick(r, &["#", "\u{03A3}", "%", "=", "+"])), side * edge, y, 0.0, -5.0, 1.3, 6.0);
            }
            Slides => {
                // Next slide: whoosh and a wobble.
                self.jig.wobble(3.0);
                for i in 0..3 {
                    let y = -6.0 + 6.0 * i as f32;
                    self.spawn(Kind::Dash, -18.0, y, -30.0, 0.0, 0.35 + 0.05 * i as f32, 1.0);
                }
            }
            Planning => {
                // Tick an item off with a nod.
                self.jig.hop(14.0);
                self.spawn(Kind::Sym("\u{2713}"), edge, -2.0, 0.0, -8.0, 1.4, 7.0);
            }
            Data => {
                let y = self.range(-4.0, 6.0);
                self.spawn(Kind::Sym(pick(r, &["\u{2197}", "%", "\u{03C0}", "\u{03BC}", "\u{0078}\u{0304}"])), side * edge, y, 1.0, -7.0, 1.5, 6.0);
            }
            Research => {
                if r < 0.6 {
                    self.spawn(Kind::Question, side * 24.0, -5.0, 0.0, -2.5, 1.3, 1.0);
                } else {
                    // Found it.
                    self.jig.hop(16.0);
                    self.spawn(Kind::Spark, side * 22.0, -7.0, 0.0, 0.0, 0.7, 2.6);
                }
            }
            Reading => {
                // Page turn.
                self.spawn(Kind::Page, edge * 0.8, 6.0, -10.0, -6.0, 1.8, 1.0);
            }
            Design => {
                // Paint flecks.
                let (x, vx, vy) = (self.range(-ax, ax), self.range(-3.0, 3.0), self.range(-10.0, -4.0));
                self.spawn(Kind::Firework, x, ay * 0.4, vx, vy, 1.6, 1.0);
            }
            Chat => {
                let side = if ((self.t / 1.3) as u32).is_multiple_of(2) { -1.0 } else { 1.0 };
                self.jig.hop(8.0);
                self.spawn(Kind::Sym("\u{2026}"), side * edge, -6.0, 0.0, -5.0, 1.2, 7.0);
            }
            Email => self.spawn(Kind::Sym(pick(r, &["@", "\u{2709}"])), side * edge, -2.0, 0.0, -6.0, 1.6, 6.5),
            Meeting => {
                // Someone is talking on one side.
                let side = if ((self.t / 6.0) as u32).is_multiple_of(2) { -1.0 } else { 1.0 };
                self.spawn(Kind::Wave, side * 20.0, 0.0, side, 0.0, 0.6, 1.0);
            }
            Learning => {
                if r < 0.5 {
                    // A lightbulb moment.
                    self.jig.hop(14.0);
                    self.spawn(Kind::Spark, 0.0, -ay * 0.7, 0.0, 0.0, 0.8, 2.8);
                } else {
                    self.spawn(Kind::Sym(pick(r * 2.0 - 1.0, &["A+", "?", "\u{2605}", "1+1"])), side * edge, -3.0, 0.0, -6.0, 1.5, 6.0);
                }
            }
            Media => {
                if r < 0.5 {
                    self.spawn(Kind::Note, side * edge, ay * 0.3, side * 2.0, -7.0, 1.6, 7.5);
                }
            }
            Studio => self.wave_burst(1),
        }
    }

    /// One-shot effects at a specific moment inside a reaction.
    pub(super) fn reaction_moments(&mut self) {
        let Some((r, st)) = self.react else { return };
        let e = self.t - st;
        match r {
            React::Sneeze if e >= 1.2 && !self.react_mark => {
                self.react_mark = true;
                for _ in 0..8 {
                    let a = self.range(-0.6, 0.6) + PI / 2.0;
                    let v = self.range(14.0, 24.0);
                    self.spawn(Kind::Dot, 0.0, 6.0, a.cos() * v * 1.6, a.sin() * v * 0.5, 0.5, 1.0);
                }
            }
            React::Celebrate if e >= 1.1 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(50.0);
                let hue = self.rand();
                for i in 0..10 {
                    let a = i as f32 / 10.0 * TAU;
                    let v = self.range(14.0, 20.0);
                    self.spawn(Kind::Firework, 0.0, -6.0, a.cos() * v, a.sin() * v, 0.9, 1.0);
                    if let Some(p) = self.parts.last_mut() {
                        p.hue = hue;
                    }
                }
            }
            React::Zoom if e >= 0.6 && !self.react_mark => {
                self.react_mark = true;
                for i in 0..4 {
                    let y = -7.0 + 4.5 * i as f32;
                    self.spawn(Kind::Dash, 20.0, y, -40.0, 0.0, 0.5, 1.0);
                }
            }
            React::Faint if e >= 1.5 && !self.react_mark => {
                self.react_mark = true;
                self.spawn(Kind::Sweat, 20.0, -8.0, 0.0, 2.0, 1.0, 2.0);
            }
            React::Noted if e >= 1.1 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(30.0);
                self.spawn(Kind::Spark, 22.0, -7.0, 0.0, 0.0, 0.6, 2.4);
            }
            React::Wake if e >= 0.25 && !self.react_mark => {
                self.react_mark = true;
                self.blink_start = Some(self.t);
                self.blink_double = true;
            }
            React::Laptop if e >= 2.7 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(24.0);
                self.spawn(Kind::Sym("</>"), 0.0, -10.0, 0.0, -6.0, 1.1, 6.0);
            }
            React::Read if e >= 2.0 && !self.react_mark => {
                // The page turns with a soft whoosh.
                self.react_mark = true;
                self.jig.wobble(2.0);
                self.spawn(Kind::Page, 8.0, 7.0, -11.0, -7.0, 1.2, 1.0);
            }
            React::Game if e >= 2.1 && !self.react_mark => {
                // The run pays off.
                self.react_mark = true;
                self.jig.hop(40.0);
                for _ in 0..5 {
                    let (x, y, life) = (self.range(-20.0, 20.0), self.range(-10.0, 4.0), self.range(0.5, 0.9));
                    self.spawn(Kind::Spark, x, y, 0.0, 0.0, life, 2.6);
                }
            }
            React::Lift if e >= 1.9 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(30.0);
                self.spawn(Kind::Sweat, 21.0, -8.0, 0.0, 2.0, 1.2, 2.0);
            }
            React::Magic if e >= 1.1 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(36.0);
                for i in 0..8 {
                    let a = i as f32 / 8.0 * TAU;
                    self.spawn(Kind::Spark, a.cos() * 14.0, a.sin() * 8.0 - 4.0, a.cos() * 8.0, a.sin() * 5.0, 0.8, 2.4);
                }
            }
            React::Photo if e >= 0.55 && !self.react_mark => {
                // The shutter fires.
                self.react_mark = true;
                self.live.flash = 1.0;
                self.blink_start = Some(self.t);
                self.blink_double = false;
            }
            React::Plant if e >= 2.0 && !self.react_mark => {
                self.react_mark = true;
                self.spawn(Kind::Heart, 12.0, 2.0, 0.0, -7.0, 1.3, 2.6);
            }
            React::Balloon if e >= 2.05 && !self.react_mark => {
                // Pop.
                self.react_mark = true;
                self.jig.squish(55.0);
                for _ in 0..8 {
                    let a = self.range(0.0, TAU);
                    let v = self.range(12.0, 20.0);
                    self.spawn(Kind::Dot, 8.6, -16.0, a.cos() * v, a.sin() * v * 0.7, 0.5, 1.0);
                }
            }
            React::BubbleGum if e >= 1.85 && !self.react_mark => {
                // The bubble pops on its nose.
                self.react_mark = true;
                self.jig.squish(30.0);
                for _ in 0..5 {
                    let (vx, vy) = (self.range(-14.0, 14.0), self.range(-10.0, 2.0));
                    self.spawn(Kind::Dot, 0.0, 8.0, vx, vy, 0.5, 0.9);
                }
            }
            React::Catch if e >= 2.1 && !self.react_mark => {
                // Got it.
                self.react_mark = true;
                self.jig.hop(34.0);
                self.spawn(Kind::Spark, 14.0, 2.0, 0.0, 0.0, 0.7, 3.0);
            }
            React::Ninja if e >= 0.75 && !self.react_mark => {
                self.react_mark = true;
                for i in 0..4 {
                    let y = -7.0 + 4.5 * i as f32;
                    self.spawn(Kind::Dash, -16.0, y, 34.0, 0.0, 0.4 + 0.08 * i as f32, 1.0);
                }
            }
            React::Eavesdrop if e >= 1.7 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(30.0);
                self.spawn(Kind::Bang, -25.0, -9.0, 0.0, -3.0, 1.0, 1.0);
            }
            React::Idea if e >= 0.7 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(42.0);
                for _ in 0..4 {
                    let (x, life) = (self.range(-14.0, 14.0), self.range(0.5, 0.9));
                    self.spawn(Kind::Spark, x, -12.0, 0.0, 0.0, life, 2.6);
                }
            }
            React::Count if e >= 2.0 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(26.0);
                self.spawn(Kind::Spark, 0.0, -11.0, 0.0, 0.0, 0.7, 2.8);
            }
            React::Toast if e >= 0.5 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(18.0);
                self.spawn(Kind::Spark, -9.0, 4.0, 0.0, 0.0, 0.6, 2.4);
            }
            React::Splash if e >= 0.7 && !self.react_mark => {
                // Landed in the puddle.
                self.react_mark = true;
                self.jig.squish(50.0);
                for i in 0..6 {
                    let a = -PI / 2.0 + (i as f32 - 2.5) * 0.35;
                    self.spawn(Kind::Sweat, a.cos() * 4.0, 8.0, a.cos() * 12.0, -14.0, 1.0, 1.6);
                }
            }
            React::Sing if e >= 1.5 && !self.react_mark => {
                self.react_mark = true;
                self.wave_burst(2);
            }
            React::Guitar if e >= 1.5 && !self.react_mark => {
                self.react_mark = true;
                self.jig.wobble(4.0);
                for i in 0..3 {
                    let (x, vx) = (-8.0 + 8.0 * i as f32, self.range(-2.0, 2.0));
                    self.spawn(Kind::Note, x, 2.0, vx, -8.0, 1.6, 8.5);
                }
            }
            React::Coffee if e >= 1.9 && !self.react_mark => {
                // One last curl of steam with the satisfied sigh.
                self.react_mark = true;
                self.spawn(Kind::Steam, -11.0, 8.0, -0.5, -5.0, 1.6, 1.0);
            }
            React::Drum if e >= 1.9 && !self.react_mark => {
                // The crash on the last hit.
                self.react_mark = true;
                self.jig.hop(50.0);
                self.spawn(Kind::Bang, 0.0, -12.0, 0.0, -4.0, 1.0, 1.0);
                self.wave_burst(1);
            }
            React::Scope if e >= 1.9 && !self.react_mark => {
                // Spotted something: a little hop.
                self.react_mark = true;
                self.jig.hop(36.0);
                self.spawn(Kind::Spark, 16.0, -10.0, 0.0, 0.0, 0.7, 2.8);
            }
            React::Dice if e >= 1.5 && !self.react_mark => {
                // The roll lands.
                self.react_mark = true;
                self.jig.squish(35.0);
                let win = self.rand() < 0.5;
                self.spawn(if win { Kind::Spark } else { Kind::Question }, 18.0, -8.0, 0.0, -2.0, 1.0, if win { 2.6 } else { 1.0 });
            }
            React::Radio if e >= 1.8 && !self.react_mark => {
                // A station locks in.
                self.react_mark = true;
                self.jig.hop(30.0);
                self.spawn(Kind::Note, 14.0, 0.0, 2.0, -6.0, 1.4, 7.5);
            }
            React::PowerUp if e >= 1.5 && !self.react_mark => {
                // The zap.
                self.react_mark = true;
                self.live.flash = 0.8;
                self.jig.hop(48.0);
                for i in 0..4 {
                    let a = -PI / 2.0 + (i as f32 - 1.5) * 0.4;
                    self.spawn(Kind::Spark, a.cos() * 10.0, -4.0 + a.sin() * 6.0, a.cos() * 10.0, a.sin() * 6.0, 0.6, 2.2);
                }
            }
            React::Storm if e >= 1.0 && !self.react_mark => {
                self.react_mark = true;
                self.live.flash = 1.0;
                self.jig.squish(40.0);
            }
            React::CardTrick if e >= 1.4 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(30.0);
                self.spawn(Kind::Spark, 12.0, -6.0, 0.0, 0.0, 0.8, 3.0);
            }
            React::Popcorn if e >= 1.2 && !self.react_mark => {
                // The kernel lands in its mouth.
                self.react_mark = true;
                self.jig.squish(20.0);
                self.spawn(Kind::Dot, 2.0, 6.0, 0.0, 4.0, 0.5, 1.4);
            }
            React::AhChoo if e >= 1.2 && !self.react_mark => {
                // The sneeze gets swallowed at the last second.
                self.react_mark = true;
                self.jig.squish(35.0);
            }
            React::Coin if e >= 1.3 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(24.0);
                self.spawn(Kind::Spark, 8.0, 4.0, 0.0, 0.0, 0.5, 2.2);
            }
            React::Fishing if e >= 2.1 && !self.react_mark => {
                // A bite: the rod jerks.
                self.react_mark = true;
                self.jig.squish(45.0);
                self.jig.wobble(5.0);
            }
            React::Jenga if e >= 2.0 && !self.react_mark => {
                // The tower wobbles but holds.
                self.react_mark = true;
                self.jig.wobble(4.0);
                self.spawn(Kind::Sweat, 20.0, -8.0, 0.0, 2.0, 1.0, 1.8);
            }
            React::Darts | React::Bowling if e >= 1.4 && !self.react_mark => {
                self.react_mark = true;
                self.jig.hop(30.0);
                self.spawn(Kind::Bang, 18.0, -6.0, 0.0, -3.0, 0.9, 1.0);
            }
            React::Archery if e >= 1.5 && !self.react_mark => {
                // The arrow away.
                self.react_mark = true;
                self.jig.wobble(4.0);
                self.spawn(Kind::Dash, 10.0, -4.0, 40.0, -4.0, 0.5, 1.0);
            }
            _ => {}
        }
    }

    pub(super) fn update_parts(&mut self, dt: f32, s: &Senses) {
        let (ax, ay) = s.area;
        for p in &mut self.parts {
            p.age += dt;
            match p.kind {
                Kind::Snow => p.vx = (p.age * 2.0 + p.hue * 6.0).sin() * 3.0,
                Kind::Confetti => p.vx = (p.age * 3.0 + p.hue * 6.0).sin() * 4.0,
                Kind::Note => p.vx = (p.age * 4.0).sin() * 4.0,
                Kind::Firework => {
                    let d = (0.92f32).powf(dt * 60.0);
                    p.vx *= d;
                    p.vy = p.vy * d + 12.0 * dt;
                }
                Kind::Sweat => p.vy += 10.0 * dt,
                Kind::Drool => p.vy += 2.0 * dt,
                Kind::Dot => {
                    let d = (0.85f32).powf(dt * 60.0);
                    p.vx *= d;
                    p.vy *= d;
                }
                Kind::Wave => {
                    p.x += p.vx * 14.0 * dt;
                    continue;
                }
                Kind::Page => p.vx = (p.age * 3.0 + p.hue * 6.0).sin() * 3.0,
                Kind::Dash => p.vx *= (0.9f32).powf(dt * 60.0),
                Kind::Steam => p.vx = (p.age * 3.0 + p.hue * 6.0).sin() * 1.5,
                Kind::Fly => {
                    // Loops around its spawn point, drifting wider and wider.
                    let rr = 4.0 + 15.0 * (p.age / p.life);
                    let a = p.age * 3.6 + p.hue * TAU;
                    p.x = p.vx + a.cos() * rr;
                    p.y = p.vy + a.sin() * rr * 0.55;
                    continue;
                }
                Kind::Sheep => {
                    let k = p.age / p.life;
                    p.x = -ax + 8.0 + (2.0 * ax - 16.0) * k;
                    let u = p.x / 8.0;
                    p.y = ay * 0.45 - if u.abs() < 1.0 { (1.0 - u * u) * 7.0 } else { 0.0 };
                    continue;
                }
                _ => {}
            }
            p.x += p.vx * dt;
            p.y += p.vy * dt;
        }
        self.parts.retain(|p| p.age < p.life && p.x.abs() < ax + 30.0 && p.y.abs() < ay + 30.0);
    }
}
