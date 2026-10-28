use super::*;
impl Eyes {
    pub(super) fn compose(&mut self, s: &Senses) -> (Pose, Extras, Live, Option<(f32, f32)>) {
        use Mood::*;
        let t = self.t;
        let mut p = Pose::rest();
        let mut x = Extras::default();
        let mut lv = Live { flash: (self.live.flash - 0.12).max(0.0), ..Live::default() };
        let mut look: Option<(f32, f32)> = None;
        let toward = s.mouse.map(|(mx, my)| ((mx / 80.0).clamp(-1.0, 1.0), (my / 50.0).clamp(-1.0, 1.0)));

        match self.mood {
            Neutral => {}
            Delight => {
                p.size = 1.15;
                p.happy = 0.5;
                p.pupil = 1.2;
                p.brow = 1.5;
            }
            Shy => {
                let side = s.mouse.map(|(mx, _)| mx.signum()).unwrap_or(1.0);
                look = Some((-side * 0.9, 0.5));
                p.blush = 1.0;
                p.happy = 0.35;
                p.dx = -side * 3.0;
            }
            Stare => {
                let k = ((self.stare_s - 1.5) / 5.0).clamp(0.0, 1.0);
                p.size = 1.0 + 0.25 * k;
                p.pupil = 0.85;
                lv.dx = (t * 40.0).sin() * 0.3 * k;
                look = toward;
            }
            Cross => {
                p.cross = 1.0;
                p.open = 0.9;
                p.size = 1.05;
                look = Some((0.0, 0.4));
            }
            Chase => {
                p.size = 1.2;
                p.pupil = 0.8;
                p.brow = 1.0;
            }
            Eager => {
                p.size = 1.2;
                p.happy = 0.3;
                p.brow = 1.0;
                p.pupil = 1.25;
            }
            Purr => {
                p.happy = 1.0;
                p.open = 0.0;
                p.blush = 0.8;
                lv.dx = (t * 45.0).sin() * 0.35;
            }
            Dance | Headbang | Vibe => self.dance_pose(&mut p, &mut x, &mut lv, &mut look),
            Work(c) => self.work_pose(c, &mut p, &mut x, &mut lv, &mut look),
            Determined => {
                p.angry = 0.5;
                p.open = 0.7;
                p.brow = -1.5;
                look = Some((0.0, 0.2));
            }
            Relaxed => {
                p.happy = 0.7;
                p.open = 0.6;
                lv.dy = t.sin() * 0.6;
            }
            Hush => {
                p.open = 0.0;
                p.happy = 0.2;
                x.caption = Some("shh");
            }
            Watchful => {
                p.open = 0.6;
                p.angry = 0.15;
                look = Some(((t * 0.7).sin() * 0.85, 0.0));
                x.rec = Some(if s.env.camera { Color::rgb(74, 222, 128) } else { Color::rgb(251, 146, 60) });
            }
            Drowsy => {
                let f = (t % 4.5) / 4.5;
                if f < 0.7 {
                    let k = f / 0.7;
                    p.open = 0.4 * (1.0 - k);
                    lv.dy = 2.2 * k;
                } else if f < 0.78 {
                    p.open = 0.75;
                    lv.dy = -1.0;
                } else {
                    p.open = 0.45;
                }
                p.sad = 0.25;
            }
            Asleep => {
                p.open = 0.0;
                lv.dy = (t * 0.9).sin() * 0.7;
                lv.squash = (t * 0.9).sin() * 0.03;
                x.dream = t % 9.0 > 7.0;
                if x.dream {
                    look = Some(((t * 9.0).sin() * 0.8, 0.0));
                }
            }
            Lost => {
                p.size_l = 1.2;
                p.sad = 0.3;
                look = Some(((t * 0.9).sin() * 0.8, -0.2));
            }
            Tired => {
                p.open = 0.45;
                p.sad = 0.55;
                lv.dy = 1.0;
            }
            Sweaty => {
                p.size = 1.05;
            }
            Strained => {
                p.open = 0.6;
                p.angry = 0.3;
                lv.squash = 0.18;
                lv.dx = (t * 30.0).sin() * 0.3;
            }
            Pleading => {
                p.size = 1.18;
                p.pupil = 1.45;
                p.sad = 0.35;
                p.brow = 1.5;
                look = Some((0.0, 0.7));
                x.shine = true;
            }
            Gloomy => {
                p.sad = 0.6;
                p.open = 0.7;
                look = Some((0.0, 0.4));
            }
            Sunny => {
                x.shades = true;
                p.happy = 0.4;
                lv.dy = (t * 1.5).sin() * 0.5;
            }
            Foggy => {
                p.open = 0.55;
                look = Some(((t * 0.5).sin() * 0.7, 0.0));
            }
            Spooky => {
                x.glow = Some(Color::rgb(134, 255, 170));
                x.slit = true;
                lv.dx = (t * 0.8).sin() * 1.5;
                lv.rot = (t * 0.8).sin() * 0.05;
            }
            Sleepy => {
                p.open = 0.55;
                p.sad = 0.2;
            }
            Hungry => {
                look = Some((0.0, 0.8));
                p.mouth = 0.6;
                p.happy = 0.25;
            }
            Party => {
                x.shades = true;
                lv.dy = -(t * 4.0).sin().abs() * 1.5;
            }
            Grumpy => {
                p.angry = 0.45;
                p.open = 0.55;
                p.brow = -2.5;
            }
            Bored => {
                p.open = 0.65;
                p.sad = 0.1;
            }
            Typing => {
                p.open = 0.8;
                p.brow = 0.3;
                look = Some((self.key_x, 0.85));
                if self.keys.len() >= 14 {
                    // On fire.
                    p.angry = 0.3;
                    p.brow = -1.0;
                }
            }
            Thinking => {
                p.open = 0.9;
                p.brow = 1.3;
                look = Some((-0.55 + 0.25 * (t * 2.5).cos(), -0.75 + 0.15 * (t * 2.5).sin()));
                lv.rot = -0.05;
                lv.dy = (t * 1.7).sin() * 0.5;
            }
            Listening => {
                let lvl = self.listen_lvl;
                p.open = 1.0;
                p.brow = 0.6 + 0.6 * lvl;
                p.size = 1.0 + 0.2 * lvl;
                // Head tilted toward the mic on the left.
                lv.rot = -0.08;
                lv.dy = (t * 1.4).sin() * 0.4;
                look = Some(if lvl > 0.08 { (-0.7, 0.05) } else { (0.15 * (t * 0.8).sin(), 0.0) });
            }
        }

        // Last ten seconds of a focus/break: eyes tick-tock, growing wider.
        if let Some(left) = s.focus_left.filter(|&l| l <= 10.0 && l > 0.0) {
            let k = (10.0 - left) / 10.0;
            let side = if self.tick_sec % 2 == 0 { 0.8 } else { -0.8 };
            look = Some((side, 0.0));
            p.size = 1.0 + 0.25 * k;
            p.brow = 1.5 * k;
            p.angry = 0.0;
            p.open = 1.0;
        }

        // Mouse wheel: pupils spin like a reel.
        if t - self.scroll_at < 1.2 {
            look = Some((self.roll.cos() * 0.85, -self.roll.sin() * 0.85));
            p.size = 1.08;
            p.happy = 0.0;
        }

        // A window switch steals the gaze for a moment.
        if let Some((g, _)) = self.glance {
            look = Some(g);
        }

        // Sheep, if one is hopping, steals the gaze.
        if let Some(sh) = self.parts.iter().find(|p| p.kind == Kind::Sheep) {
            look = Some(((sh.x / s.area.0.max(1.0)).clamp(-1.0, 1.0), 0.6));
        }

        // Accessories and calendar.
        let (mo, d) = (s.local.month(), s.local.day());
        x.hat = if is_birthday(s) {
            Hat::Party
        } else if mo == 12 {
            Hat::Santa
        } else if matches!(self.mood, Asleep | Drowsy) {
            Hat::Nightcap
        } else {
            Hat::None
        };
        x.pumpkin = (mo == 10 && d >= 24) || (mo == 11 && d == 1);
        if is_birthday(s) && self.mood == Neutral {
            p.happy = 0.5;
        }

        if let Some((r, st)) = self.react {
            self.react_pose(r, t - st, s, &mut p, &mut x, &mut lv, &mut look);
        }
        if let Some(g) = self.live_gauge() {
            self.gauge_pose(g, &mut p, &mut x, &mut lv, &mut look);
        }
        (p, x, lv, look)
    }

    /// The current dance move, timed to the beat. `pl` is the beat pulse
    /// scaled by how hard the beat hit, `ph` the progress through the beat.
    fn dance_pose(&self, p: &mut Pose, x: &mut Extras, lv: &mut Live, look: &mut Option<(f32, f32)>) {
        let t = self.t;
        let b = &self.beat;
        let pl = b.pulse(t) * (0.55 + 0.6 * b.hit);
        let ph = b.phase(t);
        let ease = ph * ph * (3.0 - 2.0 * ph);
        let arc = (ph * PI).sin();
        let side = if b.count.is_multiple_of(2) { 1.0 } else { -1.0 };
        let sw = b.sway(t);
        let step = b.count.wrapping_sub(self.dance_from);
        match self.dance {
            Move::Sway => {
                p.happy = 0.35;
                p.open = 0.95;
                *look = Some((sw * 0.6, -0.1));
                lv.dy = -pl * 3.2;
                lv.squash = pl * 0.14;
                lv.rot = sw * 0.13;
                lv.dx = sw * 2.5;
            }
            Move::Bounce => {
                p.happy = 0.55;
                p.open = 0.9;
                *look = Some((0.0, -0.2));
                lv.dy = -pl * 4.0;
                lv.squash = pl * 0.16 - 0.03;
            }
            Move::Nod => {
                p.open = 0.7;
                p.happy = 0.2;
                *look = Some((side * 0.2, 0.45));
                lv.dy = pl * 2.6;
                lv.rot = side * 0.03;
            }
            Move::Shuffle => {
                p.happy = 0.4;
                *look = Some((side * 0.8, 0.0));
                lv.dx = side * (2.0 * ease - 1.0) * 4.0;
                lv.rot = side * 0.06 * arc;
                lv.squash = -pl * 0.08;
            }
            Move::Twist => {
                p.happy = 0.5;
                *look = Some((-side * 0.5, 0.0));
                lv.rot = side * 0.2 * arc;
                lv.squash = side * 0.07 * arc;
            }
            Move::LookAround => {
                const DIRS: [(f32, f32); 4] = [(-0.8, -0.3), (0.8, -0.3), (-0.6, 0.5), (0.6, 0.5)];
                *look = Some(DIRS[(b.count % 4) as usize]);
                p.size = 1.05;
                p.brow = 1.2 * pl;
                lv.dy = -pl * 1.5;
            }
            Move::Hop => {
                p.happy = 1.0;
                p.open = 0.9;
                lv.squash = pl * 0.1;
            }
            Move::Groove => {
                let f = ((step % 2) as f32 + ph) / 2.0 * TAU;
                p.open = 0.55;
                p.happy = 0.55;
                *look = Some((f.sin() * 0.5, 0.1));
                lv.dx = f.sin() * 3.2;
                lv.dy = (2.0 * f).sin() * 1.2;
                lv.rot = f.cos() * 0.07;
            }
            Move::Pump => {
                p.size = 1.0 + 0.16 * pl;
                p.pupil = 1.0 + 0.4 * pl;
                p.brow = 1.5 * pl;
                lv.squash = -0.06 * pl;
                *look = Some((0.0, -0.1));
            }
            Move::WinkStep => {
                if ph < 0.55 {
                    if side > 0.0 {
                        p.wink_r = 1.0;
                    } else {
                        p.wink_l = 1.0;
                    }
                }
                p.happy = 0.5;
                lv.dx = sw * 2.0;
                lv.dy = -pl * 2.0;
            }
            Move::Bang => {
                p.angry = 0.55;
                p.open = 0.7;
                p.brow = -2.0;
                *look = Some((0.0, 0.6));
                lv.dy = pl * 4.0 - 1.0;
                lv.rot = side * pl * 0.06;
                lv.squash = -pl * 0.06;
            }
            Move::SideBang => {
                p.angry = 0.5;
                p.open = 0.7;
                p.brow = -2.0;
                *look = Some((side * 0.4, 0.6));
                lv.dy = pl * 2.5;
                lv.dx = side * pl * 3.0;
                lv.rot = side * pl * 0.2;
            }
            Move::Thrash => {
                p.angry = 0.65;
                p.open = 0.55;
                p.brow = -2.5;
                *look = Some((0.0, 0.7));
                lv.dy = pl * 4.5 - 1.0;
                lv.dx = (t * 38.0).sin() * pl;
                lv.squash = -pl * 0.08;
            }
            Move::Drift => {
                p.happy = 0.9;
                p.open = 0.45;
                lv.dx = (t * 1.3).sin() * 3.0;
                lv.rot = (t * 1.3).sin() * 0.11;
            }
            Move::Float => {
                p.open = 0.0;
                p.happy = 0.8;
                lv.dy = (t * 1.1).sin() * 1.6 - 0.5;
                lv.rot = (t * 0.7).sin() * 0.06;
            }
            Move::Lean => {
                let l = if (step / 4).is_multiple_of(2) { 1.0 } else { -1.0 };
                p.open = 0.6;
                p.happy = 0.6;
                *look = Some((l * 0.6, -0.2));
                lv.rot = l * 0.12;
                lv.dx = l * 3.0;
                lv.dy = pl * 1.2;
            }
            Move::Twirl => {
                p.happy = 0.6;
                p.open = 0.9;
                lv.rot = ease * TAU * side;
                lv.dy = -arc * 2.5;
                *look = Some((0.0, -0.3));
            }
            Move::Pendulum => {
                p.open = 0.7;
                p.happy = 0.3;
                *look = Some((sw * 0.9, 0.2));
                lv.dx = sw * 6.0;
                lv.rot = sw * 0.2;
                lv.dy = -pl * 1.2;
            }
            Move::Vibrate => {
                p.open = 0.9;
                p.brow = 1.0;
                lv.dx = (t * 55.0).sin() * 0.9 * (0.4 + pl);
                lv.dy = (t * 47.0).cos() * 0.5;
                *look = Some((side * 0.2, -0.1));
            }
            Move::Kick => {
                p.happy = 0.45;
                p.open = 0.85;
                *look = Some((side * 0.9, 0.0));
                lv.dx = side * pl * 5.0;
                lv.rot = side * pl * 0.14;
                lv.squash = -pl * 0.07;
            }
        }
        match self.mood {
            Mood::Headbang => {
                p.angry = p.angry.max(0.35);
                p.brow = p.brow.min(-1.0);
            }
            Mood::Vibe if self.genre == Genre::Sad => {
                p.sad = 0.5;
                p.happy = p.happy.min(0.15);
                p.open = p.open.min(0.55);
            }
            _ => {}
        }
        x.shades = self.mood == Mood::Dance && self.genre == Genre::Party;
    }

    /// Act out the level in a volume or brightness HUD. Runs last so it wins
    /// over the mood; the face sits right of the bar and watches it fill.
    /// Acting out the app in front. Each category gets
    /// a gaze pattern that reads as the job (reading code line by line,
    /// hopping spreadsheet cells, scrubbing a timeline...) plus a few lid
    /// and brow settings; `work_parts` adds the matching particles.
    fn work_pose(&self, c: AppCategory, p: &mut Pose, x: &mut Extras, lv: &mut Live, look: &mut Option<(f32, f32)>) {
        use AppCategory::*;
        let t = self.t;
        // How hard the keys are flying right now, 0..1.
        let busy = (self.keys.len() as f32 / 14.0).min(1.0);
        // Progress through a repeating cycle of `period` seconds, and which cycle.
        let cyc = |period: f32| ((t / period).fract(), (t / period) as u32);
        match c {
            Coding => {
                // Read a line left to right, carriage-return to the next.
                let (f, n) = cyc(1.6);
                let row = (n % 4) as f32 / 3.0;
                *look = Some((-0.7 + 1.4 * (f / 0.85).min(1.0), -0.3 + 0.6 * row));
                p.open = 0.82;
                p.brow = -0.6;
                p.angry = 0.12 + 0.25 * busy;
                if busy > 0.3 {
                    // Hammering keys: eyes on the keyboard.
                    *look = Some((self.key_x * 0.6, 0.8));
                }
            }
            Terminal => {
                // Watch the prompt at the bottom left; output scrolls the gaze up.
                let (f, _) = cyc(0.9);
                *look = Some((-0.6 + 0.15 * (t * 0.5).sin(), 0.6 - 0.35 * f));
                p.open = 0.62;
                p.angry = 0.1;
                p.pupil = 0.85;
            }
            Writing => {
                // A pen tracing cursive across the page.
                let (f, _) = cyc(3.0);
                *look = Some((-0.75 + 1.5 * f, 0.35 + 0.15 * (t * 9.0).sin()));
                p.happy = 0.25;
                p.open = 0.88;
                lv.rot = 0.04 * (t * 0.7).sin();
            }
            Spreadsheet => {
                // Hop from cell to cell on a 3x3 grid.
                let (_, n) = cyc(0.55);
                let h = n.wrapping_mul(2_654_435_761) >> 13;
                *look = Some(((h % 3) as f32 * 0.7 - 0.7, ((h / 3) % 3) as f32 * 0.5 - 0.5));
                p.open = 0.85;
                p.size = 0.95;
                p.pupil = 0.85;
            }
            Slides => {
                // Presenting: look at the slide, then turn to the audience and beam.
                let (f, _) = cyc(4.0);
                if f < 0.55 {
                    *look = Some((0.8, -0.15));
                    p.brow = 0.6;
                } else {
                    *look = Some((0.0, 0.0));
                    p.happy = 0.4;
                    p.brow = 1.2;
                    p.size = 1.08;
                }
            }
            Planning => {
                // Scan down a list, one item at a time.
                let (f, n) = cyc(2.4);
                let item = (n % 3) as f32;
                *look = Some((-0.45 + 0.35 * (f * 2.0).min(1.0), -0.6 + 0.6 * item));
                p.open = 0.86;
                p.happy = if f > 0.8 { 0.45 } else { 0.1 };
            }
            Data => {
                // Ride a chart up and to the right, wide-eyed at the peak.
                let (f, _) = cyc(2.8);
                *look = Some((-0.8 + 1.6 * f, 0.6 - 1.2 * f + 0.15 * (f * TAU * 3.0).sin()));
                p.pupil = 1.0 + 0.3 * f;
                p.brow = 1.4 * f;
            }
            Research => {
                // Fast skim: zigzag across the page, brows up.
                let (f, n) = cyc(0.9);
                *look = Some((-0.8 + 1.6 * f, -0.4 + 0.4 * (n % 3) as f32));
                p.brow = 1.2;
                p.size = 1.05;
            }
            Reading => {
                // Slow, calm lines; a gentle breathing bob.
                let (f, n) = cyc(3.2);
                *look = Some((-0.6 + 1.2 * f, 0.1 + 0.5 * (n % 5) as f32 / 4.0));
                p.open = 0.62;
                p.happy = 0.2;
                lv.dy = 0.4 * (t * 0.8).sin();
            }
            Design => {
                // Artist's squint: one eye half shut, measuring, gaze orbiting.
                let a = t * 0.9;
                *look = Some((a.cos() * 0.7, a.sin() * 0.45));
                p.wink_l = 0.45 + 0.15 * (t * 0.5).sin();
                p.brow = 0.6;
                lv.rot = 0.06 * (t * 0.45).sin();
            }
            Chat => {
                // Back and forth between two people, a bounce on each turn.
                let (f, n) = cyc(1.3);
                let side = if n % 2 == 0 { -0.75 } else { 0.75 };
                *look = Some((side, 0.1));
                p.happy = 0.35;
                p.size = 1.05;
                lv.dy = -(1.0 - f).powi(4) * 1.4;
            }
            Email => {
                // Read down a message; the inbox weighs a little.
                let (f, _) = cyc(3.5);
                *look = Some((0.3 * (t * 2.2).sin(), -0.5 + 1.2 * f));
                p.open = 0.75;
                p.sad = 0.12;
            }
            Meeting => {
                // Attentive, nodding along; a green "on air" dot.
                *look = Some((0.15 * (t * 0.4).sin(), -0.05));
                p.size = 1.06;
                p.brow = 0.6;
                lv.dy = (t * TAU / 1.8).sin().max(0.0) * 1.2;
                x.rec = Some(Color::rgb(74, 222, 128));
            }
            Learning => {
                // Eager student: board up there, notes down here.
                let (f, _) = cyc(3.0);
                *look = Some((0.2 * t.sin(), if f < 0.6 { -0.65 } else { 0.7 }));
                p.pupil = 1.3;
                p.size = 1.1;
                p.brow = 1.5;
            }
            Media => {
                // Watching: wide, still, lit by the screen.
                *look = Some((0.05 * (t * 0.3).sin(), 0.05));
                p.size = 1.1;
                p.pupil = 1.2;
                p.happy = 0.2;
                x.screen = Some(hsl((t * 25.0) % 360.0, 0.7, 0.6));
            }
            Studio => {
                // Scrub the timeline: sweep right, snap back; recording light on.
                let (f, _) = cyc(2.2);
                let k = if f < 0.8 { f / 0.8 } else { 1.0 - (f - 0.8) / 0.2 };
                *look = Some((-0.85 + 1.7 * k, 0.55));
                p.open = 0.8;
                p.angry = 0.15;
                x.rec = Some(Color::rgb(248, 113, 113));
            }
        }
    }

    fn gauge_pose(&self, g: GaugeState, p: &mut Pose, x: &mut Extras, lv: &mut Live, look: &mut Option<(f32, f32)>) {
        let t = self.t;
        p.happy = 0.0;
        p.sad = 0.0;
        p.angry = 0.0;
        p.mouth = 0.0;
        p.brow = 0.0;
        p.size = 1.0;
        p.size_l = 1.0;
        p.open = 1.0;
        x.glyph = Glyph::None;
        x.shades = false;
        match g.kind {
            Gauge::Volume if g.muted => {
                // Clamp the eyes shut: `> <` for a beat, then a tight line.
                if t - g.mute_at < 0.45 {
                    x.glyph = Glyph::Squeeze;
                } else {
                    p.open = 0.0;
                    p.happy = 0.15;
                    p.angry = 0.25;
                }
                p.brow = -1.0;
                lv.dy = 0.8;
            }
            Gauge::Volume => {
                let v = g.level;
                p.size = 0.86 + 0.34 * v;
                p.pupil = 1.3 - 0.55 * v;
                p.open = 0.78 + 0.22 * v;
                *look = Some((-1.0 + 0.6 * v, -0.15 * v));
                if v < 0.12 {
                    // Straining to hear: lean in toward the bar, little `o`.
                    p.brow = 1.8;
                    p.mouth = 0.35;
                    p.size = 0.92;
                    *look = Some((-0.9, 0.15));
                    lv.rot = -0.08;
                    lv.dx = -1.5;
                } else if v > 0.85 {
                    // Too loud: wince and rattle.
                    let k = (v - 0.85) / 0.15;
                    p.angry = 0.35 * k;
                    p.open = 1.0 - 0.35 * k;
                    p.brow = -1.5 * k;
                    p.mouth = 0.5 * k;
                    lv.dx = (t * 48.0).sin() * 0.5 * k;
                } else {
                    p.happy = 0.25;
                    p.brow = 0.8 * v;
                }
            }
            Gauge::Brightness => {
                let b = g.level;
                // Pupils dilate in the dark and pinch in bright light.
                p.pupil = 1.6 - 1.0 * b;
                *look = Some((-0.7 + 0.4 * b, -0.5 * b));
                if b > 0.75 {
                    let k = (b - 0.75) / 0.25;
                    p.open = 1.0 - 0.45 * k;
                    p.happy = 0.45 * k;
                    p.brow = -0.8 * k;
                    x.shades = b >= 0.97;
                } else if b < 0.25 {
                    let k = (0.25 - b) / 0.25;
                    p.size = 1.0 + 0.15 * k;
                    p.brow = 1.2 * k;
                    x.shine = true;
                }
            }
        }
    }

    pub(super) fn update_gaze(&mut self, dt: f32, s: &Senses, own: Option<(f32, f32)>) {
        let t = self.t;
        let dist = s.mouse.map(|(x, y)| (x * x + y * y).sqrt()).unwrap_or(f32::MAX);
        let target = if let Some(l) = own {
            l
        } else if dist < 280.0 {
            let (x, y) = s.mouse.unwrap_or_default();
            ((x / 80.0).clamp(-1.0, 1.0), (y / 50.0).clamp(-1.0, 1.0))
        } else {
            if t >= self.saccade_at {
                self.gaze_target = if self.rand() < 0.25 { (0.0, 0.0) } else { (self.range(-0.8, 0.8), self.range(-0.5, 0.5)) };
                self.saccade_at = t + if self.mood == Mood::Sweaty { self.range(0.15, 0.5) } else { self.range(0.6, 2.8) };
            }
            self.gaze_target
        };
        // Pupils dilate as the pointer comes close.
        if dist < 260.0 {
            self.pose.pupil *= 1.0 + 0.3 * (1.0 - dist / 260.0);
        }
        let rate = if self.mood == Mood::Chase { 3.0 } else { 18.0 };
        let k = 1.0 - (-rate * dt).exp();
        self.gaze.0 += (target.0 - self.gaze.0) * k;
        self.gaze.1 += (target.1 - self.gaze.1) * k;
    }

    pub(super) fn blink_amount(&mut self) -> f32 {
        let t = self.t;
        let suppressed = matches!(self.mood, Mood::Stare | Mood::Asleep | Mood::Purr | Mood::Hush);
        if self.blink_start.is_none() && t >= self.blink_at && !suppressed {
            self.blink_start = Some(t);
            self.blink_double = self.rand() < 0.15;
        }
        let Some(st) = self.blink_start else { return 0.0 };
        let e = t - st;
        const DUR: f32 = 0.15;
        const GAP: f32 = 0.08;
        if e < DUR {
            (e / DUR * PI).sin()
        } else if self.blink_double && e < DUR * 2.0 + GAP {
            if e > DUR + GAP {
                ((e - DUR - GAP) / DUR * PI).sin()
            } else {
                0.0
            }
        } else {
            self.blink_start = None;
            let slow = if matches!(self.mood, Mood::Sleepy | Mood::Tired) { 1.6 } else { 1.0 };
            self.blink_at = t + self.range(2.0, 6.5) * slow;
            0.0
        }
    }
}
