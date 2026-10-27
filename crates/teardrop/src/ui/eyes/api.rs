use super::*;
impl Eyes {
    pub fn new(seed: u64) -> Self {
        Self {
            t: 0.0,
            rng: seed | 1,
            mood: Mood::Neutral,
            pose: Pose::rest(),
            live: Live::default(),
            extras: Extras::default(),
            blink: 0.0,
            blink_at: 2.5,
            blink_start: None,
            blink_double: false,
            gaze: (0.0, 0.0),
            gaze_target: (0.0, 0.0),
            saccade_at: 1.0,
            react: None,
            react_mark: false,
            queue: VecDeque::new(),
            recent: VecDeque::new(),
            parts: Vec::new(),
            timers: [0.0; 16],
            beat: Beat::new(),
            genre: Genre::Plain,
            ring: None,
            started: false,
            track_key: String::new(),
            track_t: 0.0,
            last_pos: 0,
            last_dur: 0,
            last_play: Playback::Stopped,
            was_charging: None,
            was_offline: false,
            was_asleep: false,
            was_locked: false,
            last_day: None,
            glanced_hour: None,
            next_bit: 10.0,
            pokes: VecDeque::new(),
            last_mouse: None,
            speed: 0.0,
            chase_s: 0.0,
            stare_s: 0.0,
            near_s: 0.0,
            rub_dir: 0.0,
            rubs: VecDeque::new(),
            purr_until: -1.0,
            pleading_until: -1.0,
            last_water: -1.0,
            jig: Jiggle::default(),
            gauge: None,
            dance: Move::Sway,
            dance_from: 0,
            dance_len: 8,
            seen_beat: 0,
            noted_pages: 1,
            glance: None,
            roll: 0.0,
            roll_to: 0.0,
            scroll_at: -10.0,
            scrolls: VecDeque::new(),
            last_idle: 0,
            keys: VecDeque::new(),
            key_x: 0.0,
            typing_since: None,
            hover_s: 0.0,
            hover_side: 1.0,
            giggled: false,
            last_batt: None,
            was_caps: None,
            tick_sec: -1,
            skip_until: -1.0,
            work: None,
            listen_lvl: 0.0,
        }
    }

    pub(super) fn rand(&mut self) -> f32 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    pub(super) fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rand()
    }

    // ------------------------------------------------------------ events

    /// Queue a one-off. Plays as soon as the face is visible and free.
    pub fn react(&mut self, r: React) {
        if self.queue.len() >= 4 {
            self.queue.pop_front();
        }
        if !self.queue.iter().any(|(q, _)| *q == r) {
            self.queue.push_back((r, self.t));
        }
    }

    /// Clicked or opened: a few in a row get annoying, more get dizzy.
    pub fn poke(&mut self) {
        let t = self.t;
        self.jig.squish(40.0);
        self.pokes.push_back(t);
        while self.pokes.front().is_some_and(|&p| t - p > 5.0) {
            self.pokes.pop_front();
        }
        match self.pokes.len() {
            3 => self.react(React::Annoyed),
            n if n >= 5 => {
                self.react(React::Dizzy);
                self.pokes.clear();
            }
            _ => {}
        }
    }

    /// The island just collapsed again: sometimes a cheeky wink. Anything
    /// done while it was open (a note, a task) already queued its own reaction
    /// and gets the stage instead.
    pub fn island_closed(&mut self) {
        if self.queue.is_empty() && self.rand() < 0.65 {
            let pick = match (self.rand() * 4.0) as u32 {
                0 => React::Wink,
                1 => React::Peek,
                2 => React::Satisfied,
                _ => React::Goodnight,
            };
            self.react(pick);
        }
    }

    /// A level HUD changed (volume keys, mute, brightness keys). The face sits
    /// in the HUD and acts the level out live: it watches the bar fill, hops
    /// when it goes up, squishes when it goes down.
    pub fn gauge(&mut self, kind: Gauge, level: f32, muted: bool) {
        let t = self.t;
        let level = level.clamp(0.0, 1.0);
        let prev = self.gauge.filter(|g| g.kind == kind && t - g.at < GAUGE_HOLD);
        let mute_at = match prev {
            Some(g) if g.muted == muted => g.mute_at,
            Some(_) => t,
            None if muted => t,
            None => -10.0,
        };
        self.gauge = Some(GaugeState { kind, level, muted, at: t, mute_at });
        let Some(prev) = prev else {
            // Popping into the HUD; whatever it was doing can wait.
            self.react = None;
            self.jig.hop(28.0);
            return;
        };
        if prev.muted != muted {
            if muted {
                self.jig.squish(60.0);
            } else {
                self.jig.hop(55.0);
                self.wave_burst(3);
            }
            return;
        }
        let d = level - prev.level;
        if d.abs() < 0.001 {
            return;
        }
        let kick = (d.abs() * 600.0).clamp(18.0, 60.0);
        if d > 0.0 {
            self.jig.hop(kick);
            match kind {
                Gauge::Volume => self.wave_burst(if level > 0.75 { 2 } else { 1 }),
                Gauge::Brightness => {
                    let (x, y) = (self.range(-18.0, 18.0), self.range(-12.0, -6.0));
                    self.spawn(Kind::Spark, x, y, 0.0, -4.0, 0.5, 1.6 + 1.6 * level);
                }
            }
            if kind == Gauge::Volume && level >= 0.9 && prev.level < 0.9 {
                self.spawn(Kind::Sweat, 21.0, -8.0, 0.0, 2.0, 1.2, 2.0);
                self.jig.wobble(3.0);
            }
        } else {
            self.jig.squish(kick);
        }
    }

    /// Sound waves rolling off both sides of the face.
    pub(super) fn wave_burst(&mut self, n: u32) {
        for i in 0..n {
            for side in [-1.0f32, 1.0] {
                self.spawn(Kind::Wave, side * 20.0, 0.0, side, 0.0, 0.55 + 0.12 * i as f32, 1.0 + 0.35 * i as f32);
            }
        }
    }

    /// The level HUD the face is acting out, while it is fresh.
    pub(super) fn live_gauge(&self) -> Option<GaugeState> {
        self.gauge.filter(|g| self.t - g.at < GAUGE_HOLD)
    }

    /// A note was saved: read it over, then nod. Longer notes get more pages.
    pub fn noted(&mut self, chars: usize) {
        self.noted_pages = (1 + chars / 160).min(3) as u32;
        self.react(React::Noted);
    }

    /// A window was switched to: glance toward it for a moment. `x`/`y`
    /// are its centre relative to the top-centre of the screen, -1..1.
    pub fn glance(&mut self, x: f32, y: f32) {
        if self.glance.is_some_and(|(_, until)| self.t < until - 0.6) {
            return;
        }
        self.glance = Some(((x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0)), self.t + 0.9));
    }

    /// Next/previous pressed in the app: whoosh that way, and don't also
    /// react to the track change it causes.
    pub fn skip(&mut self, forward: bool) {
        self.skip_until = self.t + 4.0;
        self.react(React::Swipe(forward));
    }

    /// Mouse wheel over the collapsed island: the pupils spin with it like
    /// a reel; a frantic spin gets dizzy.
    pub fn scrolled(&mut self, dir: i32) {
        let t = self.t;
        if t - self.scroll_at > 1.2 {
            self.roll = PI / 2.0;
            self.roll_to = self.roll;
        }
        self.roll_to += dir.signum() as f32 * 0.9;
        self.scroll_at = t;
        self.jig.wobble(dir.signum() as f32 * 1.2);
        self.scrolls.push_back(t);
        while self.scrolls.front().is_some_and(|&p| t - p > 2.0) {
            self.scrolls.pop_front();
        }
        if self.scrolls.len() >= 12 {
            self.scrolls.clear();
            self.react(React::Dizzy);
        }
    }

    /// The dance move playing right now (while music plays).
    #[cfg(test)]
    pub fn dance_move(&self) -> Move {
        self.dance
    }

    /// A water reminder fired: look pleading until something gets logged.
    pub fn water_due(&mut self) {
        self.pleading_until = self.t + 120.0;
    }

    /// Being petted (pointer rubbing back and forth over the island).
    pub fn petting(&self) -> bool {
        self.t < self.purr_until
    }

    pub fn current(&self) -> Option<React> {
        self.react.map(|(r, _)| r)
    }

    #[cfg(test)]
    pub fn pose(&self) -> &Pose {
        &self.pose
    }

    #[cfg(test)]
    pub fn extras(&self) -> &Extras {
        &self.extras
    }

    pub(super) fn start(&mut self, r: React) {
        self.react = Some((r, self.t));
        self.react_mark = false;
        // Anticipation for every reaction: a hop, a squish or a wobble.
        match r {
            React::Surprised | React::Shocked | React::Wake => self.jig.hop(55.0),
            React::Happy | React::Perk | React::GotIt | React::Love | React::WelcomeBack | React::Sparkle => self.jig.hop(38.0),
            React::Satisfied | React::Sigh | React::Defeated | React::Annoyed | React::Gulp | React::Poof => self.jig.squish(45.0),
            React::Celebrate | React::Zoom | React::Gasp | React::Shout => self.jig.hop(60.0),
            React::Nod | React::Salute | React::Giggle => self.jig.hop(20.0),
            React::Faint | React::Pout | React::Oops | React::Scratch => self.jig.squish(50.0),
            React::Swipe(right) => self.jig.wobble(if right { 4.0 } else { -4.0 }),
            React::Huh | React::Confused | React::Dizzy | React::EyeRoll => {
                let w = if self.rand() < 0.5 { -3.5 } else { 3.5 };
                self.jig.wobble(w);
            }
            // Object skits get a small hop; routines with a lift get a big one.
            React::Laptop
            | React::Read
            | React::Phone
            | React::Game
            | React::Guitar
            | React::Sing
            | React::Photo
            | React::Paint
            | React::Toast
            | React::Wave
            | React::Idea
            | React::Catch
            | React::Juggle
            | React::Cheer
            | React::Proud
            | React::PeekABoo
            | React::Eavesdrop
            | React::Count
            | React::Wait
            | React::Kiss
            | React::Spy
            | React::Inspect
            | React::BrowWiggle
            | React::Sniff
            | React::Binge
            | React::Stargaze
            | React::Mirror
            | React::Hack => self.jig.hop(26.0),
            React::Coffee | React::Snack | React::Hug | React::Bask | React::Meditate | React::Breathe | React::Nap | React::BubbleGum => self.jig.hop(14.0),
            React::Lift
            | React::Magic
            | React::Balloon
            | React::Ninja
            | React::Splash
            | React::Pogo
            | React::Hype
            | React::Spin
            | React::March
            | React::Moonwalk
            | React::Rage
            | React::Beep
            | React::ZeroG => self.jig.hop(45.0),
            React::Umbrella | React::Grumble | React::Facepalm | React::Rub | React::Doze | React::SideEye | React::Squint => self.jig.squish(35.0),
            React::Shiver | React::Cringe | React::Bobble | React::Hot | React::Plant => self.jig.wobble(3.0),
            // Batch two: same anticipation grammar, grouped by kick size.
            React::Drum | React::Dice | React::Kite | React::Pirouette | React::TaDa | React::Flip | React::PowerUp | React::Storm | React::Tumble => self.jig.hop(45.0),
            React::Scope
            | React::News
            | React::Pizza
            | React::Radio
            | React::Lantern
            | React::Whistle
            | React::Measure
            | React::CardTrick
            | React::Origami
            | React::Scan
            | React::Metronome
            | React::Morse
            | React::Popcorn
            | React::FistPump
            | React::Bow
            | React::Skate
            | React::Dribble
            | React::Row
            | React::Trapped
            | React::Tiptoe
            | React::Glitch
            | React::Radar
            | React::Compass => self.jig.hop(26.0),
            React::Workout | React::Swim | React::Jog | React::Scheme => self.jig.hop(36.0),
            React::Daydream
            | React::Hum
            | React::Hammock
            | React::Fireplace
            | React::Aurora
            | React::WarmHands
            | React::Puppy
            | React::Sulk
            | React::Nervous
            | React::Balance
            | React::Breeze => self.jig.hop(14.0),
            React::Yodel
            | React::AhChoo
            | React::Photobomb
            | React::HideSeek
            | React::Karate
            | React::Fencing
            | React::Archery
            | React::Darts
            | React::Spring
            | React::Box
            | React::Meteor
            | React::Sumo => self.jig.hop(40.0),
            React::Jelly
            | React::Cloudwatch
            | React::Vogue
            | React::Flex
            | React::Plie
            | React::Pinwheel
            | React::BubbleWand
            | React::Polish
            | React::Flashlight
            | React::Shush
            | React::SlowClap
            | React::Coin
            | React::Letter
            | React::Selfie
            | React::Marshmallow
            | React::Moth
            | React::YoYo
            | React::Jenga => self.jig.hop(24.0),
            React::JumpRope | React::Skip | React::Bowling | React::Golf | React::Fishing | React::Surf | React::Ski | React::Dominoes | React::Fidget => self.jig.hop(30.0),
            _ => {}
        }
        match r {
            React::Sparkle | React::WelcomeBack => {
                for _ in 0..5 {
                    let (x, y, life) = (self.range(-24.0, 24.0), self.range(-9.0, 6.0), self.range(0.5, 0.9));
                    self.spawn(Kind::Spark, x, y, 0.0, 0.0, life, 2.4);
                }
            }
            React::Huh | React::Confused => self.spawn(Kind::Question, 25.0, -5.0, 0.0, -2.0, 1.4, 1.0),
            React::Annoyed => self.spawn(Kind::Anger, -25.0, -7.0, 0.0, 0.0, 1.5, 1.0),
            React::Happy | React::Love => {
                for i in 0..3 {
                    let x = -12.0 + 12.0 * i as f32;
                    self.spawn(Kind::Heart, x, 3.0, 0.0, -8.0, 1.2 + 0.2 * i as f32, 3.0);
                }
                if r == React::Happy {
                    for _ in 0..3 {
                        let x = self.range(-20.0, 20.0);
                        let vx = self.range(-2.0, 2.0);
                        let vy = self.range(6.0, 10.0);
                        self.spawn(Kind::Confetti, x, -10.0, vx, vy, 1.2, 1.0);
                    }
                }
            }
            React::Satisfied => {
                for i in 0..3 {
                    let x = -10.0 + 10.0 * i as f32;
                    self.spawn(Kind::Heart, x, 2.0, 0.0, -6.0, 1.1 + 0.2 * i as f32, 2.6);
                }
            }
            React::BarrelRoll => {
                for _ in 0..4 {
                    let (x, y, life) = (self.range(-22.0, 22.0), self.range(-8.0, 6.0), self.range(0.5, 0.9));
                    self.spawn(Kind::Spark, x, y, 0.0, 0.0, life, 2.2);
                }
            }
            React::GotIt => self.spawn(Kind::Spark, 22.0, -6.0, 0.0, 0.0, 0.6, 2.6),
            React::Noted => {
                for i in 0..self.noted_pages {
                    let x = -12.0 + 12.0 * i as f32;
                    self.spawn(Kind::Page, x, 9.0, 1.5 - i as f32, -14.0, 1.6 + 0.25 * i as f32, 1.0);
                }
            }
            React::Swipe(right) => {
                let sd = if right { -1.0 } else { 1.0 };
                for i in 0..3 {
                    let y = -6.0 + 6.0 * i as f32;
                    self.spawn(Kind::Dash, sd * 18.0, y, sd * 30.0, 0.0, 0.35 + 0.05 * i as f32, 1.0);
                }
            }
            React::Celebrate => {
                for _ in 0..14 {
                    let x = self.range(-30.0, 30.0);
                    let vx = self.range(-6.0, 6.0);
                    let vy = self.range(-14.0, -6.0);
                    let life = self.range(1.2, 2.0);
                    self.spawn(Kind::Confetti, x, 6.0, vx, vy, life, 1.0);
                }
                for _ in 0..4 {
                    let (x, y, life) = (self.range(-26.0, 26.0), self.range(-10.0, 6.0), self.range(0.5, 0.9));
                    self.spawn(Kind::Spark, x, y, 0.0, 0.0, life, 2.6);
                }
            }
            React::Oops => self.spawn(Kind::Sweat, 21.0, -8.0, 0.0, 2.0, 1.3, 2.2),
            React::Sweep => {
                for i in 0..7 {
                    let x = -24.0 + 8.0 * i as f32;
                    let vy = self.range(-6.0, -2.0);
                    self.spawn(Kind::Dot, x, 9.0, 4.0, vy, 0.4 + 0.1 * i as f32, 1.0);
                }
            }
            React::Gasp => self.spawn(Kind::Bang, 24.0, -9.0, 0.0, -3.0, 1.1, 1.0),
            React::Shout => {
                self.spawn(Kind::Bang, -25.0, -8.0, 0.0, -3.0, 1.2, 1.0);
                self.spawn(Kind::Bang, 25.0, -8.0, 0.0, -3.0, 1.2, 1.0);
            }
            React::Salute => self.spawn(Kind::Spark, 24.0, -8.0, 0.0, 0.0, 0.6, 2.4),
            React::Zoom => {
                for i in 0..6 {
                    let y = -9.0 + 3.6 * i as f32;
                    let x = self.range(-28.0, -12.0);
                    self.spawn(Kind::Dash, x, y, -40.0, 0.0, 0.5 + 0.1 * (i % 3) as f32, 1.0);
                }
            }
            React::Giggle => self.spawn(Kind::Heart, 20.0, -2.0, 0.0, -7.0, 1.2, 2.4),
            React::Poof => {
                for i in 0..9 {
                    let a = i as f32 / 9.0 * TAU;
                    let v = self.range(16.0, 24.0);
                    self.spawn(Kind::Dot, 0.0, 2.0, a.cos() * v * 1.4, a.sin() * v * 0.7, 0.55, 1.0);
                }
            }
            React::DoubleBlink => {
                self.blink_start = Some(self.t);
                self.blink_double = true;
            }
            React::Coffee => {
                for i in 0..2 {
                    self.spawn(Kind::Steam, -11.0 - 1.5 * i as f32, 9.0 - 2.0 * i as f32, -0.5, -5.0, 1.4 + 0.6 * i as f32, 1.0);
                }
            }
            React::Snack => {
                for i in 0..4 {
                    let (x, vx, vy) = (self.range(6.0, 12.0), self.range(-2.0, 2.0), self.range(3.0, 7.0));
                    self.spawn(Kind::Dot, x, 12.0, vx, -vy, 0.8 + 0.25 * i as f32, 0.9);
                }
            }
            React::Guitar | React::Sing => {
                for i in 0..4 {
                    let (x, vx, life) = (self.range(-18.0, 14.0), self.range(-2.0, 2.0), 1.2 + 0.4 * i as f32);
                    self.spawn(Kind::Note, x, 2.0, vx, -8.0, life, 7.5);
                }
            }
            React::Paint => {
                for _ in 0..5 {
                    let (x, vx, vy) = (self.range(-18.0, 18.0), self.range(-3.0, 3.0), self.range(-10.0, -4.0));
                    self.spawn(Kind::Firework, x, 8.0, vx, vy, 1.6, 1.0);
                }
            }
            React::Umbrella => {
                for i in 0..7 {
                    let x = -21.0 + 7.0 * i as f32;
                    self.spawn(Kind::Rain, x, -14.0, -2.0, 30.0, 0.9, 1.0);
                }
            }
            React::Hug => {
                for i in 0..2 {
                    self.spawn(Kind::Heart, -6.0 + 10.0 * i as f32, 0.0, 0.0, -7.0, 1.3 + 0.3 * i as f32, 2.6);
                }
            }
            React::Kiss => self.spawn(Kind::Heart, 16.0, -1.0, 9.0, -5.0, 1.3, 3.0),
            React::BubbleGum => self.spawn(Kind::Bubble, 0.0, 8.0, 0.0, 0.0, 1.9, 2.2),
            React::Cheer => {
                for _ in 0..8 {
                    let (x, vx, vy, life) = (self.range(-24.0, 24.0), self.range(-4.0, 4.0), self.range(-12.0, -6.0), self.range(1.0, 1.8));
                    self.spawn(Kind::Confetti, x, 6.0, vx, vy, life, 1.0);
                }
            }
            React::Catch => self.spawn(Kind::Fly, 0.0, 0.0, 0.0, -2.0, 2.9, 1.0),
            React::Hack => {
                for (i, s) in ["</>", "{ }", "=>", ";"].iter().enumerate() {
                    self.spawn(Kind::Sym(s), -18.0 + 12.0 * i as f32, -6.0, 0.0, -7.0, 1.5, 5.5);
                }
            }
            React::Juggle => {
                for i in 0..3 {
                    let vx = -7.0 + 7.0 * i as f32;
                    self.spawn(Kind::Dot, -vx * 0.8, 8.0, vx, -16.0, 1.6 + 0.3 * i as f32, 1.4);
                }
            }
            React::Stargaze => {
                for _ in 0..4 {
                    let (x, y, life) = (self.range(-22.0, 22.0), self.range(-14.0, -8.0), self.range(1.6, 2.6));
                    self.spawn(Kind::Spark, x, y, 0.0, 0.0, life, 2.4);
                }
            }
            React::ZeroG => {
                for _ in 0..5 {
                    let (x, y, life) = (self.range(-24.0, 24.0), self.range(-10.0, 8.0), self.range(1.4, 2.4));
                    self.spawn(Kind::Spark, x, y, 0.0, 0.0, life, 2.2);
                }
            }
            React::Shiver => {
                for _ in 0..4 {
                    let (x, vy) = (self.range(-20.0, 20.0), self.range(5.0, 8.0));
                    self.spawn(Kind::Snow, x, -14.0, 0.0, vy, 1.6, 1.0);
                }
            }
            React::Hot => {
                for i in 0..3 {
                    self.spawn(Kind::Sweat, 16.0 + 5.0 * i as f32, -8.0 + 4.0 * i as f32, 0.0, 2.0, 1.2, 2.0);
                }
            }
            React::Rage | React::Grumble => {
                self.spawn(Kind::Anger, -25.0, -7.0, 0.0, 0.0, 1.4, 1.0);
                if r == React::Rage {
                    self.spawn(Kind::Anger, 25.0, -7.0, 0.0, 0.0, 1.4, 1.0);
                }
            }
            React::Ninja | React::Moonwalk => {
                for i in 0..4 {
                    let y = -7.0 + 4.5 * i as f32;
                    self.spawn(Kind::Dash, 16.0, y, -34.0, 0.0, 0.4 + 0.08 * i as f32, 1.0);
                }
            }
            React::Magic => {
                for _ in 0..3 {
                    let (x, y, life) = (self.range(-14.0, 14.0), self.range(-10.0, 0.0), self.range(0.5, 0.9));
                    self.spawn(Kind::Spark, x, y, 0.0, 0.0, life, 2.2);
                }
            }
            React::Nap => self.spawn(Kind::Z, 12.0, -5.0, 3.0, -4.0, 2.0, 7.0),
            React::Daydream => self.spawn(Kind::Bubble, 14.0, -8.0, 0.0, -1.5, 2.6, 1.4),
            React::Swim => {
                for i in 0..4 {
                    self.spawn(Kind::Bubble, -14.0 + 8.0 * i as f32, 10.0, -1.0, -4.0, 1.2 + 0.3 * i as f32, 0.8);
                }
            }
            React::Whistle | React::Hum => {
                for i in 0..3 {
                    let (x, vx, life) = (self.range(-14.0, 14.0), self.range(-1.5, 1.5), 1.4 + 0.3 * i as f32);
                    self.spawn(Kind::Note, x, 4.0, vx, -6.0, life, 6.5);
                }
            }
            React::Radio => {
                // Static hiss while it tunes.
                for i in 0..4 {
                    self.spawn(Kind::Dash, 18.0, -6.0 + 4.0 * i as f32, -26.0, 0.0, 0.4 + 0.08 * i as f32, 1.0);
                }
            }
            React::Dice => {
                for i in 0..3 {
                    self.spawn(Kind::Dot, -6.0 + 6.0 * i as f32, 10.0, 0.0, -10.0, 0.9, 1.2);
                }
            }
            React::PowerUp => {
                for _ in 0..4 {
                    let (x, y, life) = (self.range(-16.0, 16.0), self.range(-8.0, 8.0), self.range(0.8, 1.4));
                    self.spawn(Kind::Spark, x, y, 0.0, 0.0, life, 2.0);
                }
            }
            React::Jog | React::Workout => {
                for i in 0..2 {
                    self.spawn(Kind::Sweat, 18.0 + 4.0 * i as f32, -7.0, 0.0, 2.0, 1.4, 1.8);
                }
            }
            React::WarmHands => {
                for i in 0..2 {
                    self.spawn(Kind::Steam, -3.0 + 6.0 * i as f32, 9.0, 0.0, -4.0, 1.2, 1.0);
                }
            }
            React::Origami => self.spawn(Kind::Page, 10.0, 8.0, -1.0, -4.0, 1.8, 1.0),
            React::TaDa | React::CardTrick => {
                for _ in 0..5 {
                    let (x, y, life) = (self.range(-20.0, 20.0), self.range(-10.0, 2.0), self.range(0.5, 0.9));
                    self.spawn(Kind::Spark, x, y, 0.0, 0.0, life, 2.4);
                }
            }
            React::Scheme => self.spawn(Kind::Anger, 24.0, -8.0, 0.0, 0.0, 1.2, 0.9),
            React::Nervous => self.spawn(Kind::Sweat, 20.0, -8.0, 0.0, 2.0, 1.4, 1.8),
            React::Yodel => {
                for i in 0..4 {
                    let x = self.range(-18.0, 10.0);
                    self.spawn(Kind::Note, x, 2.0, 2.5, -7.0, 1.4 + 0.3 * i as f32, 7.0);
                }
            }
            React::BubbleWand => {
                for i in 0..5 {
                    let (x, vx) = (self.range(-16.0, 12.0), self.range(-1.5, 1.5));
                    self.spawn(Kind::Bubble, x, 6.0, vx, -5.0, 1.4 + 0.3 * i as f32, 0.9);
                }
            }
            React::Pinwheel => {
                for i in 0..4 {
                    let y = -6.0 + 4.0 * i as f32;
                    self.spawn(Kind::Dash, -18.0, y, 30.0, 0.0, 0.4 + 0.08 * i as f32, 1.0);
                }
            }
            React::Meteor => self.spawn(Kind::Dash, -24.0, -12.0, 55.0, 10.0, 0.9, 1.4),
            React::Letter => self.spawn(Kind::Page, 4.0, 8.0, 2.0, -6.0, 1.6, 1.0),
            React::Marshmallow => self.spawn(Kind::Spark, 10.0, 14.0, 0.0, -4.0, 2.2, 2.2),
            React::Dominoes => {
                for i in 0..5 {
                    self.spawn(Kind::Dot, -20.0 + 8.0 * i as f32, 8.0, 3.0, 0.0, 2.0, 1.1);
                }
            }
            React::Ski => {
                for i in 0..5 {
                    self.spawn(Kind::Dash, 20.0, -8.0 + 4.0 * i as f32, -40.0, 0.0, 0.5 + 0.1 * i as f32, 1.0);
                }
            }
            _ => {}
        }
    }

    pub(super) fn spawn(&mut self, kind: Kind, x: f32, y: f32, vx: f32, vy: f32, life: f32, size: f32) {
        if self.parts.len() >= 48 {
            self.parts.remove(0);
        }
        let hue = self.rand();
        self.parts.push(Particle { kind, x, y, vx, vy, age: 0.0, life, size, hue });
    }

    pub(super) fn due(&mut self, i: usize, period: f32) -> bool {
        if self.t >= self.timers[i] {
            let j = self.range(0.8, 1.2);
            self.timers[i] = self.t + period * j;
            true
        } else {
            false
        }
    }
}
