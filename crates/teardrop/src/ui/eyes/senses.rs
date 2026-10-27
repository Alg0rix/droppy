use super::*;
impl Eyes {
    // -------------------------------------------------------------- tick

    pub fn tick(&mut self, dt: f32, s: &Senses) {
        let dt = dt.clamp(0.0, 0.1);
        self.t += dt;
        let t = self.t;
        self.sense_keys(s);
        self.sense_hover(dt, s);
        self.sense_pointer(dt, s);
        self.roll += (self.roll_to - self.roll) * (1.0 - (-12.0 * dt).exp());
        if self.glance.is_some_and(|(_, until)| t >= until) {
            self.glance = None;
        }
        self.beat.feed(s.env.audio_peak, s.music, t, dt);
        let lvl = s.recording.unwrap_or(0.0);
        self.listen_lvl += (lvl - self.listen_lvl) * (1.0 - (-12.0 * dt).exp());
        self.edges(s);
        self.mood = self.pick_mood(s);
        // A real activity taking over cuts a long skit short — but not
        // pointer play (purring, delight), which flaps with the pointer.
        let loud = matches!(self.mood, Mood::Dance | Mood::Headbang | Mood::Vibe | Mood::Typing | Mood::Work(_) | Mood::Listening | Mood::Thinking);
        if loud && self.react.is_some_and(|(r, st)| r.duration() > 2.4 && t - st > 0.5) {
            self.react = None;
        }
        let work = match self.mood {
            Mood::Work(_) => s.app,
            _ => None,
        };
        if work.is_some() && work.map(|w| w.cat) != self.work.map(|w| w.cat) && s.visible {
            // Switched apps: a little hop into the new role, its effect right after.
            self.jig.hop(18.0);
            self.timers[T_WORK] = t + 0.3;
        }
        self.work = work;

        self.choreo(s);

        if let Some((r, st)) = self.react {
            if t - st >= r.duration() {
                self.react = None;
                // Some skits leave an aftertaste: a catch makes it proud,
                // a doomscroll leaves it dizzy, a cheer leaves it glowing.
                if let Some(next) = r.follow_up() {
                    self.queue.push_back((next, t));
                }
            }
        }
        let gauging = self.live_gauge().is_some();
        if s.visible {
            if self.react.is_none() && !gauging {
                while let Some((r, q)) = self.queue.pop_front() {
                    if t - q < 25.0 {
                        self.start(r);
                        break;
                    }
                }
            }
            if gauging {
                self.next_bit = self.next_bit.max(t + 4.0);
            } else {
                self.bits(s);
            }
            self.ambient_parts(s);
        } else {
            // Hidden time doesn't age the queue: what happened while the
            // island was open plays once it collapses.
            for q in &mut self.queue {
                q.1 += dt;
            }
        }
        self.reaction_moments();

        let (target, extras, live, own_look) = self.compose(s);
        self.extras = extras;
        self.live = live;
        let k = 1.0 - (-14.0 * dt).exp();
        self.pose.ease_to(&target, k);
        self.update_gaze(dt, s, own_look);
        self.pose.look = self.gaze;
        self.jig.step(dt);
        self.blink = self.blink_amount();
        self.update_parts(dt, s);
        self.ring = s.set.mood_ring.then(|| ring_color(s));
    }

    /// Keyboard activity anywhere: new input since last frame while the
    /// pointer stayed put counts as a keystroke.
    fn sense_keys(&mut self, s: &Senses) {
        let t = self.t;
        let idle = s.env.idle_ms;
        let input = idle < self.last_idle;
        self.last_idle = idle;
        let still = match (s.mouse, self.last_mouse) {
            (Some((x, y)), Some((lx, ly))) => (x - lx).abs() < 0.5 && (y - ly).abs() < 0.5,
            _ => true,
        };
        if input && still && !s.env.lbutton {
            self.keys.push_back(t);
            if s.visible && (self.mood == Mood::Typing || matches!(self.mood, Mood::Work(c) if types(c))) {
                self.jig.hop(if self.mood == Mood::Typing { 5.0 } else { 3.0 });
                self.key_x = self.range(-0.5, 0.5);
            }
        }
        while self.keys.front().is_some_and(|&k| t - k > 2.0) {
            self.keys.pop_front();
        }
        let typing = self.keys.len() >= 5;
        match (typing, self.typing_since) {
            (true, None) => self.typing_since = Some(t),
            (false, Some(since)) => {
                self.typing_since = None;
                // A long writing burst ends: stretch it out.
                if t - since > 30.0 {
                    let r = if self.rand() < 0.5 { React::Stretch } else { React::Sigh };
                    self.react(r);
                }
            }
            _ => {}
        }
    }

    /// Long hovers tickle; leaving after one gets a pout.
    fn sense_hover(&mut self, dt: f32, s: &Senses) {
        if s.hovering {
            if self.hover_s == 0.0 {
                self.hover_side = s.mouse.map(|(x, _)| x.signum()).unwrap_or(1.0);
            }
            self.hover_s += dt;
            if self.hover_s > 2.5 && !self.giggled {
                self.giggled = true;
                self.react(React::Giggle);
            }
        } else {
            if self.hover_s > 1.5 && s.visible {
                self.hover_side = s.mouse.map(|(x, _)| x.signum()).unwrap_or(self.hover_side);
                self.react(React::Pout);
            }
            self.hover_s = 0.0;
            self.giggled = false;
        }
    }

    fn sense_pointer(&mut self, dt: f32, s: &Senses) {
        let t = self.t;
        let Some((x, y)) = s.mouse else {
            self.last_mouse = None;
            self.speed = 0.0;
            self.chase_s = 0.0;
            self.stare_s = self.stare_s.min(0.0);
            self.near_s = 0.0;
            return;
        };
        if let Some((lx, ly)) = self.last_mouse {
            let d = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
            let inst = d / dt.max(1e-3);
            self.speed += (inst - self.speed) * (1.0 - (-12.0 * dt).exp());
            let dx = x - lx;
            let over = x.abs() < s.area.0 + 8.0 && y > -s.area.1 - 4.0 && y < s.area.1 + 40.0;
            if over && dx.abs() > 1.5 {
                let dir = dx.signum();
                if self.rub_dir != 0.0 && dir != self.rub_dir {
                    self.rubs.push_back(t);
                }
                self.rub_dir = dir;
            }
        }
        while self.rubs.front().is_some_and(|&r| t - r > 1.6) {
            self.rubs.pop_front();
        }
        if self.rubs.len() >= 4 {
            self.purr_until = t + 1.2;
        }
        self.last_mouse = Some((x, y));
        let dist = (x * x + y * y).sqrt();

        if self.speed > 1600.0 && dist < 520.0 {
            self.chase_s += dt;
        } else if self.speed < 400.0 {
            if self.chase_s > 1.0 && !self.petting() {
                self.react(React::Dizzy);
            }
            self.chase_s = 0.0;
        }

        let still = self.speed < 18.0;
        if still && dist < 95.0 && !s.hovering {
            if self.stare_s >= 0.0 {
                self.stare_s += dt;
            } else {
                self.stare_s = (self.stare_s + dt).min(0.0);
            }
        } else if self.stare_s > 0.0 {
            self.stare_s = 0.0;
        } else {
            self.stare_s = (self.stare_s + dt).min(0.0);
        }
        if self.stare_s > 7.0 {
            // Lost the staring contest.
            self.blink_start = Some(t);
            self.blink_double = false;
            self.react(React::Defeated);
            self.stare_s = -45.0;
        }

        if dist < 150.0 && !s.hovering && !still {
            self.near_s += dt;
        } else if dist >= 150.0 {
            self.near_s = (self.near_s - dt * 2.0).max(0.0);
        }
    }

    /// Things that happened since last frame, turned into reactions.
    fn edges(&mut self, s: &Senses) {
        let t = self.t;
        if !self.started {
            self.started = true;
            self.react(React::Stretch);
            self.last_day = Some(s.local.date());
        }

        // Music transitions.
        let tr = s.track;
        let key = format!("{}\0{}", tr.title, tr.artist);
        if key != self.track_key {
            if !self.track_key.is_empty() && !tr.title.is_empty() && t >= self.skip_until {
                let ended = self.last_dur > 0 && self.last_pos + 10_000 >= self.last_dur;
                if ended {
                    self.react(React::Sigh);
                } else if t - self.track_t < 20.0 && self.last_play == Playback::Playing {
                    self.react(React::EyeRoll);
                } else {
                    self.react(React::Swipe(true));
                }
            }
            self.track_key = key;
            self.track_t = t;
            self.genre = genre_of(tr);
        } else if !tr.title.is_empty() {
            if tr.playback == Playback::Playing && self.last_pos > 15_000 && tr.position_ms < 4_000 {
                self.react(React::Sparkle);
            }
            if self.last_play == Playback::Playing && tr.playback == Playback::Paused {
                let dancing = matches!(self.mood, Mood::Dance | Mood::Headbang | Mood::Vibe);
                self.react(if dancing { React::Scratch } else { React::Huh });
            }
            if self.last_play == Playback::Playing && tr.playback == Playback::Stopped && self.last_dur > 0 && self.last_pos + 10_000 >= self.last_dur {
                self.react(React::Sigh);
            }
        }
        self.last_pos = tr.position_ms;
        self.last_dur = tr.duration_ms;
        self.last_play = tr.playback;

        // Power.
        if let Some(b) = s.sys.battery_pct {
            if self.was_charging == Some(false) && s.sys.charging {
                self.react(React::Love);
            } else if self.was_charging == Some(true) && !s.sys.charging {
                self.react(React::Gasp);
            }
            self.was_charging = Some(s.sys.charging);
            if let Some(prev) = self.last_batt {
                if s.sys.charging && b >= 100 && prev < 100 {
                    self.react(React::Satisfied);
                } else if !s.sys.charging && b <= 10 && prev > 10 {
                    self.react(React::Faint);
                }
            }
            self.last_batt = Some(b);
        }

        // CAPS LOCK.
        if self.was_caps == Some(!s.env.caps) {
            self.react(if s.env.caps { React::Shout } else { React::Sigh });
        }
        self.was_caps = Some(s.env.caps);

        // Focus countdown: a tick per second over the last ten.
        if let Some(left) = s.focus_left.filter(|&l| l <= 10.0 && l > 0.0) {
            let sec = left.ceil() as i32;
            if sec != self.tick_sec {
                self.tick_sec = sec;
                self.jig.hop(10.0 + (10 - sec) as f32 * 2.0);
            }
        } else {
            self.tick_sec = -1;
        }

        // Network.
        if s.env.offline && !self.was_offline {
            self.react(React::Confused);
        } else if !s.env.offline && self.was_offline {
            self.react(React::GotIt);
        }
        self.was_offline = s.env.offline;

        // Waking up after idling off (unlock plays its own welcome).
        let asleep = s.env.idle_ms > 300_000 && !s.env.locked;
        if self.was_asleep && !asleep && s.env.idle_ms < 5_000 && !self.was_locked {
            self.react(React::Wake);
        }
        self.was_asleep = asleep;
        self.was_locked = s.env.locked;

        // First input of a new day: big stretch.
        let today = s.local.date();
        if self.last_day.is_some_and(|d| d != today) && s.env.idle_ms < 3_000 && s.local.hour() >= 5 {
            self.react(React::Stretch);
            self.last_day = Some(today);
        }

        // Logged water: stop pleading.
        if self.last_water >= 0.0 && s.water > self.last_water + 0.001 {
            self.pleading_until = -1.0;
        }
        self.last_water = s.water;
    }

    fn pick_mood(&self, s: &Senses) -> Mood {
        let e = s.env;
        let t = self.t;
        if e.locked {
            return Mood::Asleep;
        }
        if t < self.purr_until {
            return Mood::Purr;
        }
        let near = |r: f32| s.mouse.is_some_and(|(x, y)| (x * x + y * y).sqrt() < r);
        if e.lbutton && near(320.0) && self.speed > 40.0 && !s.hovering {
            return Mood::Eager;
        }
        if self.chase_s > 0.35 {
            return Mood::Chase;
        }
        if s.hovering {
            return Mood::Delight;
        }
        if let Some((x, y)) = s.mouse {
            if x.abs() < 12.0 && y > 8.0 && y < 48.0 {
                return Mood::Cross;
            }
        }
        if self.stare_s > 1.5 {
            return Mood::Stare;
        }
        if self.near_s > 4.0 {
            return Mood::Shy;
        }
        if s.recording.is_some() {
            return Mood::Listening;
        }
        if s.thinking {
            return Mood::Thinking;
        }
        if let Some(p) = s.focus {
            return match (p, s.app) {
                (Phase::Focus, Some(a)) => Mood::Work(a.cat),
                (Phase::Focus, None) => Mood::Determined,
                _ => Mood::Relaxed,
            };
        }
        if s.music {
            return match self.genre {
                Genre::Chill | Genre::Sad => Mood::Vibe,
                Genre::Heavy => Mood::Headbang,
                _ if self.beat.real && self.beat.energy > 0.62 => Mood::Headbang,
                _ if self.beat.real && self.beat.energy < 0.1 => Mood::Vibe,
                _ => Mood::Dance,
            };
        }
        if self.typing_since.is_some() {
            return Mood::Typing;
        }
        if e.quiet {
            return Mood::Hush;
        }
        if e.mic || e.camera {
            return Mood::Watchful;
        }
        if e.idle_ms > 300_000 {
            return Mood::Asleep;
        }
        if e.idle_ms > 150_000 {
            return Mood::Drowsy;
        }
        // Nothing more pressing: act out whatever app is in front.
        if let Some(a) = s.app {
            return Mood::Work(a.cat);
        }
        if e.offline {
            return Mood::Lost;
        }
        if s.sys.battery_pct.is_some_and(|b| b <= 20) && !s.sys.charging {
            return Mood::Tired;
        }
        if s.sys.cpu_pct > 85.0 {
            return Mood::Sweaty;
        }
        if s.sys.disk_pct() > 92.0 {
            return Mood::Strained;
        }
        if t < self.pleading_until {
            return Mood::Pleading;
        }
        let h = s.local.hour();
        if s.set.weather_mood {
            match e.weather {
                Weather::Rain | Weather::Storm => return Mood::Gloomy,
                Weather::Clear if (7..19).contains(&h) => return Mood::Sunny,
                Weather::Fog => return Mood::Foggy,
                _ => {}
            }
        }
        let (mo, d, wd) = (s.local.month(), s.local.day(), s.local.weekday());
        if h == 0 || (mo == 10 && d == 31 && h >= 18) {
            return Mood::Spooky;
        }
        if !(5..22).contains(&h) {
            return Mood::Sleepy;
        }
        if h == 12 {
            return Mood::Hungry;
        }
        if wd == Weekday::Fri && h >= 17 {
            return Mood::Party;
        }
        if wd == Weekday::Mon && h < 12 {
            return Mood::Grumpy;
        }
        if e.idle_ms > 60_000 {
            return Mood::Bored;
        }
        Mood::Neutral
    }
}
