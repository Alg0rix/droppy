use super::*;
/// Skits the face may put on unprompted while nothing is happening: every
/// one-off that isn't bound to an event or a mood lives here, so a long idle
/// stretch never repeats the same trick twice.
const BITS: &[React] = &[
    React::Laptop,
    React::Read,
    React::Phone,
    React::Coffee,
    React::Snack,
    React::Game,
    React::Guitar,
    React::Sing,
    React::Lift,
    React::Magic,
    React::Photo,
    React::Plant,
    React::Paint,
    React::Umbrella,
    React::Balloon,
    React::Hug,
    React::SideEye,
    React::BrowWiggle,
    React::Squint,
    React::Doze,
    React::Meditate,
    React::Shiver,
    React::Hot,
    React::Nap,
    React::Spy,
    React::Inspect,
    React::Spin,
    React::Moonwalk,
    React::Beep,
    React::Wave,
    React::BubbleGum,
    React::Kiss,
    React::Grumble,
    React::Eavesdrop,
    React::Idea,
    React::Facepalm,
    React::Cringe,
    React::Cheer,
    React::March,
    React::Pogo,
    React::Ninja,
    React::ZeroG,
    React::Splash,
    React::Catch,
    React::Hack,
    React::Juggle,
    React::Proud,
    React::PeekABoo,
    React::Count,
    React::Wait,
    React::Hype,
    React::Sniff,
    React::Breathe,
    React::Rub,
    React::Rage,
    React::Bobble,
    React::Binge,
    React::Stargaze,
    React::Mirror,
    React::Bask,
    React::Toast,
    React::Drum,
    React::Scope,
    React::News,
    React::Pizza,
    React::Dice,
    React::Kite,
    React::Radio,
    React::Lantern,
    React::Tiptoe,
    React::Workout,
    React::Swim,
    React::Pirouette,
    React::Daydream,
    React::Whistle,
    React::Hum,
    React::Measure,
    React::Balance,
    React::Nervous,
    React::Sulk,
    React::Puppy,
    React::Scheme,
    React::TaDa,
    React::Bow,
    React::FistPump,
    React::Tumble,
    React::Flip,
    React::Skate,
    React::Scan,
    React::PowerUp,
    React::Glitch,
    React::Compass,
    React::Radar,
    React::Metronome,
    React::Morse,
    React::Hammock,
    React::Popcorn,
    React::CardTrick,
    React::Breeze,
    React::Trapped,
    React::Origami,
    React::WarmHands,
    React::Fireplace,
    React::Storm,
    React::Aurora,
    React::Dribble,
    React::Jog,
    React::Row,
    React::Yodel,
    React::AhChoo,
    React::Jelly,
    React::Cloudwatch,
    React::Photobomb,
    React::HideSeek,
    React::Vogue,
    React::Flex,
    React::JumpRope,
    React::Skip,
    React::Plie,
    React::Karate,
    React::Sumo,
    React::Fencing,
    React::Archery,
    React::Darts,
    React::Bowling,
    React::Golf,
    React::Fishing,
    React::Surf,
    React::Ski,
    React::Pinwheel,
    React::BubbleWand,
    React::Polish,
    React::Flashlight,
    React::Shush,
    React::SlowClap,
    React::Fidget,
    React::Coin,
    React::Letter,
    React::Selfie,
    React::Marshmallow,
    React::Meteor,
    React::Moth,
    React::Spring,
    React::YoYo,
    React::Jenga,
    React::Dominoes,
    React::Box,
];

impl Eyes {
    /// Dance bookkeeping, once per beat: switch moves at phrase ends, kick
    /// the spring on the accents, toss notes on every other beat.
    pub(super) fn choreo(&mut self, s: &Senses) {
        let c = self.beat.count;
        let dancing = matches!(self.mood, Mood::Dance | Mood::Headbang | Mood::Vibe);
        if !dancing || c == self.seen_beat {
            self.seen_beat = c;
            return;
        }
        self.seen_beat = c;
        let pool = Move::pool(self.mood, self.beat.energy);
        let fits = pool.iter().any(|&(m, _)| m == self.dance);
        if !fits || c.wrapping_sub(self.dance_from) >= self.dance_len {
            let cur = self.dance;
            let total: u32 = pool.iter().filter(|&&(m, _)| m != cur).map(|&(_, w)| w).sum();
            let mut r = (self.rand() * total as f32) as u32;
            let mut pick = pool.iter().map(|&(m, _)| m).find(|&m| m != cur).unwrap_or(cur);
            for &(m, w) in pool.iter().filter(|&&(m, _)| m != cur) {
                if r < w {
                    pick = m;
                    break;
                }
                r -= w;
            }
            self.dance = pick;
            self.dance_from = c;
            self.dance_len = if self.rand() < 0.6 { 8 } else { 4 };
            let w = if self.rand() < 0.5 { -2.5 } else { 2.5 };
            self.jig.wobble(w);
        }
        let amp = self.beat.hit;
        let side = if c % 2 == 0 { 1.0 } else { -1.0 };
        let step = c.wrapping_sub(self.dance_from);
        match self.dance {
            Move::Hop if step % 2 == 0 => self.jig.hop(20.0 + 40.0 * amp),
            Move::Bounce => self.jig.hop(10.0 + 18.0 * amp),
            Move::Bang | Move::Thrash => self.jig.squish(15.0 + 25.0 * amp),
            Move::SideBang => self.jig.wobble(side * 2.5 * amp),
            Move::Pump => self.jig.squish(10.0 * amp),
            Move::Twirl => self.jig.wobble(side * 4.0 * amp),
            Move::Kick | Move::Vibrate => self.jig.hop(6.0 + 14.0 * amp),
            _ => {}
        }
        if s.visible && c % 2 == 0 {
            let (ax, ay) = s.area;
            let side = if self.rand() < 0.5 { -1.0 } else { 1.0 };
            let size = 7.5 + 2.0 * amp;
            self.spawn(Kind::Note, side * (ax - 6.0).min(26.0), ay * 0.3, side * 2.0, -7.0 - 3.0 * amp, 1.6, size);
        }
    }

    /// A random idle skit, weighted by the world and with memory: mornings
    /// get coffee, nights get stargazing, rain gets the umbrella, and
    /// whatever just played is off the table for a while.
    pub(super) fn bit(&mut self, s: &Senses) -> React {
        let watching = (self.hover_s > 0.5 || self.stare_s > 0.5 || self.near_s > 1.0) && !self.petting();
        let mut total = 0u32;
        for &r in BITS {
            if !self.recent.contains(&r) {
                total += bit_weight(r, s, watching);
            }
        }
        let mut pick = BITS[BITS.len() - 1];
        if total > 0 {
            let mut roll = (self.rand() * total as f32) as u32;
            for &r in BITS {
                if self.recent.contains(&r) {
                    continue;
                }
                let w = bit_weight(r, s, watching);
                if roll < w {
                    pick = r;
                    break;
                }
                roll = roll.saturating_sub(w);
            }
        }
        self.recent.push_back(pick);
        while self.recent.len() > 10 {
            self.recent.pop_front();
        }
        pick
    }

    /// Little unprompted bits of personality while nothing else happens.
    pub(super) fn bits(&mut self, s: &Senses) {
        if self.react.is_some() || !self.queue.is_empty() {
            return;
        }
        let (h, m) = (s.local.hour(), s.local.minute());
        if m == 0 && self.glanced_hour.is_some_and(|g| g != h) {
            self.glanced_hour = Some(h);
            self.start(React::ClockGlance);
            return;
        }
        if self.glanced_hour.is_none() {
            self.glanced_hour = Some(h);
        }
        if self.t < self.next_bit {
            return;
        }
        use Mood::*;
        let mood = self.mood;
        let calm = matches!(mood, Neutral | Bored | Sleepy | Tired | Drowsy | Hungry | Grumpy | Foggy | Gloomy | Sunny | Party | Spooky | Delight | Dance | Headbang | Vibe);
        if !calm {
            self.next_bit = self.t + 5.0;
            return;
        }
        let r = self.rand();
        match mood {
            Delight => {
                let pick = match (r * 100.0) as u32 {
                    0..=22 => React::Wink,
                    23..=42 => React::Happy,
                    43..=60 => React::Perk,
                    61..=75 => React::Love,
                    76..=88 => React::Sparkle,
                    _ => React::Huh,
                };
                self.start(pick);
                self.next_bit = self.t + self.range(3.5, 7.5);
            }
            Dance | Headbang | Vibe => {
                let pick = match (r * 100.0) as u32 {
                    0..=30 => React::Happy,
                    31..=55 => React::Love,
                    56..=75 => React::BarrelRoll,
                    76..=90 => React::Sparkle,
                    _ => React::Wink,
                };
                self.start(pick);
                self.next_bit = self.t + self.range(8.0, 16.0);
            }
            Bored => {
                if r < 0.14 {
                    let b = self.bit(s);
                    self.start(b);
                } else if r < 0.38 {
                    self.start(React::EyeRoll);
                } else if r < 0.58 {
                    self.start(React::LookUp);
                } else if r < 0.8 {
                    // Counting sheep: one hops the fence, the eyes follow it.
                    let ax = s.area.0;
                    self.spawn(Kind::Sheep, -ax, 0.0, 0.0, 0.0, 4.5, 1.0);
                } else {
                    self.start(React::Yawn);
                }
                self.next_bit = self.t + self.range(5.0, 10.0);
            }
            Drowsy | Sleepy | Tired => {
                let pick = if r < 0.18 {
                    React::Doze
                } else if r < 0.33 {
                    React::Nap
                } else if r < 0.75 {
                    React::Yawn
                } else {
                    React::DoubleBlink
                };
                self.start(pick);
                self.next_bit = self.t + self.range(8.0, 18.0);
            }
            Hungry => {
                let pick = if r < 0.35 {
                    React::Snack
                } else if r < 0.6 {
                    React::Coffee
                } else {
                    self.bit(s)
                };
                self.start(pick);
                self.next_bit = self.t + self.range(6.0, 12.0);
            }
            Sunny => {
                let pick = if r < 0.35 { React::Bask } else { self.bit(s) };
                self.start(pick);
                self.next_bit = self.t + self.range(7.0, 13.0);
            }
            _ => {
                // Skits play more when someone is watching, less mid-typing;
                // petting is play, not watching — the face shouldn't start a
                // routine while you're rubbing it.
                let watching = (self.hover_s > 0.5 || self.stare_s > 0.5 || self.near_s > 1.0) && !self.petting();
                let gate = if watching {
                    0.3
                } else if self.keys.len() >= 8 || self.petting() {
                    0.04
                } else {
                    0.15
                };
                let r = if r < gate {
                    let b = self.bit(s);
                    self.start(b);
                    self.next_bit = self.t + self.range(9.0, 16.0);
                    return;
                } else {
                    (r - gate) / (1.0 - gate)
                };
                let pick = match (r * 100.0) as u32 {
                    0..=7 => React::Wink,
                    8..=14 => React::Peek,
                    15..=21 => React::ClockGlance,
                    22..=29 => {
                        let g = [Glyph::Heart, Glyph::Star, Glyph::Squeeze, Glyph::Ring, Glyph::Cross, Glyph::Question, Glyph::Minus, Glyph::Moon, Glyph::Bolt];
                        React::Emote(g[(self.rand() * g.len() as f32) as usize % g.len()])
                    }
                    30..=36 => React::DoubleBlink,
                    37..=42 => React::Sneeze,
                    43..=48 => React::Hiccups,
                    49..=55 => React::LookUp,
                    56..=61 => React::EyeRoll,
                    62..=67 => React::Sparkle,
                    68..=73 => React::Happy,
                    74..=79 => React::Perk,
                    80..=85 => React::Sigh,
                    86..=89 => React::Huh,
                    90..=93 => React::Surprised,
                    94..=96 => React::Stretch,
                    97..=98 => React::Satisfied,
                    _ => React::BarrelRoll,
                };
                self.start(pick);
                self.next_bit = self.t + self.range(6.0, 15.0);
            }
        }
    }
}
/// Keyboard-heavy categories: keystrokes bob the face while they play.
pub(super) fn types(c: AppCategory) -> bool {
    use AppCategory::*;
    matches!(c, Coding | Terminal | Writing | Spreadsheet | Data | Chat | Email)
}

/// How fitting an idle skit is right now: time of day, weather and the
/// week shift the odds, and long routines rarely play to an empty room.
pub(super) fn bit_weight(r: React, s: &Senses, watching: bool) -> u32 {
    let h = s.local.hour();
    let night = !(6..20).contains(&h);
    let morning = (5..11).contains(&h);
    let noon = (11..14).contains(&h);
    let late = h >= 22 || h < 5;
    let rain = matches!(s.env.weather, Weather::Rain | Weather::Storm);
    let snow = s.env.weather == Weather::Snow;
    let friday_eve = s.local.weekday() == Weekday::Fri && h >= 17;
    let w: u32 = match r {
        // Skits that hide the face only play to a watching audience —
        // unprompted they just look like the island blinking.
        React::Ninja | React::HideSeek | React::Photobomb => {
            if watching {
                2
            } else {
                0
            }
        }
        React::Coffee | React::News | React::Letter => {
            if morning {
                6
            } else {
                1
            }
        }
        React::Jog | React::Workout | React::JumpRope => {
            if morning {
                4
            } else {
                2
            }
        }
        React::Stargaze | React::Aurora | React::Meteor | React::Lantern => {
            if night {
                7
            } else {
                0
            }
        }
        React::Fireplace | React::Hammock | React::Daydream => {
            if late {
                5
            } else {
                2
            }
        }
        React::Nap | React::Doze => {
            if late {
                5
            } else {
                1
            }
        }
        React::Umbrella => {
            if rain {
                7
            } else {
                0
            }
        }
        React::Shiver | React::WarmHands => {
            if snow {
                6
            } else if rain {
                3
            } else {
                0
            }
        }
        React::Kite
        | React::Surf
        | React::Skate
        | React::Ski
        | React::Fishing
        | React::Cloudwatch
        | React::Juggle
        | React::Dribble
        | React::Row
        | React::Darts
        | React::Archery
        | React::Golf
        | React::Bowling => {
            if night {
                1
            } else {
                4
            }
        }
        React::Pizza | React::Snack | React::Popcorn => {
            if noon {
                5
            } else {
                2
            }
        }
        React::Sing | React::Guitar | React::Cheer | React::Drum | React::Kiss => {
            if friday_eve {
                5
            } else {
                2
            }
        }
        _ => 3,
    };
    // Seasonal nudges: spooky season, deep winter, high summer.
    let w = match s.local.month() {
        10 if matches!(r, React::Ninja | React::Glitch | React::Storm | React::Lantern | React::Magic | React::Scheme) => w.max(4),
        12 if matches!(r, React::WarmHands | React::Fireplace | React::Shiver | React::Sing | React::Kiss) => w.max(4),
        6..=8 if matches!(r, React::Surf | React::Swim | React::Kite | React::Bask) => w.max(4),
        _ => w,
    };
    if !watching && r.duration() > 2.6 {
        w.min(1)
    } else {
        w
    }
}
