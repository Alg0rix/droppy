//! Idle eyes: a tiny expression engine for the collapsed island.
//!
//! Three layers, each cheap to extend:
//! - [`Mood`]: a lasting state re-picked every frame from what is going on
//!   (music, idle time, clock, battery, network, the pointer...). Adding one
//!   is an enum arm, a line in `pick_mood` and a pose in `compose`.
//! - [`React`]: a short one-off played over the mood (surprise, wink,
//!   sneeze...). Window events queue them with [`Eyes::react`]; anything
//!   queued while the face is hidden plays once it shows again.
//! - [`Mood::Work`]: during a running Focus block the face acts out the kind
//!   of app in front (`teardrop_core::apps`): one pose and one ambient
//!   effect per [`AppCategory`], in `work_pose` / `work_parts`.
//! - [`Pose`]: plain numbers the painter draws. Moods and reactions only
//!   produce a target pose; the brain eases toward it and layers blinks,
//!   gaze and particles on top.
//!
//! Pure data like the rest of `ui`: no Windows calls, unit-tested anywhere.

use super::scene::{Align, Color, Fill, Rect, Scene, Weight};
use super::theme::{hsl, TEXT, TEXT_3};
use crate::viewmodel::{Ambient, Weather};
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike, Weekday};
use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use teardrop_core::pomodoro::Phase;
use teardrop_core::{AppCategory, AppTheme, EyeStyle, Playback, Settings, SystemSnapshot, Track};

const SCLERA: Color = Color::rgb(242, 242, 246);
const PUPIL: Color = Color::rgb(14, 14, 18);
const PINK: Color = Color::rgb(255, 92, 122);
const GOLD: Color = Color::rgb(251, 191, 36);
const ICE: Color = Color::rgb(125, 211, 252);
const WHITE: Color = Color::rgb(255, 255, 255);
/// Horizontal distance from the face centre to each eye.
const EYE_X: f32 = 15.0;

/// Where on the collapsed island the face sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    /// Nothing else going on: the face is the whole island.
    Idle,
    /// Replaces the equalizer next to the artwork while music plays.
    Music,
    /// Replaces the phase label next to the focus ring.
    Focus,
    /// Rides at the right end of a level HUD (volume, brightness) and
    /// reacts to the level live.
    Hud,
}

/// Face centre + scale for a spot inside the collapsed body rect.
pub fn place(spot: Spot, body: Rect) -> (f32, f32, f32) {
    match spot {
        Spot::Idle => (body.cx(), body.cy() + 1.0, 1.0),
        Spot::Music => (body.right() - 31.0, body.cy(), 0.78),
        Spot::Focus => (body.x + 66.0, body.cy(), 0.72),
        Spot::Hud => (body.right() - 26.0, body.cy(), 0.62),
    }
}

/// Half extents (face units) particles and peeks may use around the face.
pub fn area(spot: Spot, body: Rect) -> (f32, f32) {
    match spot {
        Spot::Idle => ((body.w / 2.0 - 12.0).max(24.0), body.h / 2.0),
        Spot::Hud => (26.0, body.h / 2.0 / 0.62),
        _ => (24.0, body.h / 2.0 / 0.75),
    }
}

/// Everything the brain looks at, rebuilt every frame.
pub struct Senses<'a> {
    pub local: NaiveDateTime,
    /// The face is on screen this frame.
    pub visible: bool,
    /// Pointer relative to the face centre, logical px.
    pub mouse: Option<(f32, f32)>,
    /// Pointer rests on the collapsed island.
    pub hovering: bool,
    pub track: &'a Track,
    pub music: bool,
    /// Some while the face sits next to the focus ring.
    pub focus: Option<Phase>,
    pub sys: &'a SystemSnapshot,
    /// Today's water progress 0..1.
    pub water: f32,
    pub env: &'a Ambient,
    pub set: &'a Settings,
    pub area: (f32, f32),
    /// The AI is writing a reply.
    pub thinking: bool,
    /// Seconds left on a running focus/break timer.
    pub focus_left: Option<f32>,
    /// What the app in front is themed as, while a Focus block runs.
    pub app: Option<AppTheme>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Neutral,
    Delight,
    Shy,
    Stare,
    Cross,
    Chase,
    Eager,
    Purr,
    Dance,
    Headbang,
    Vibe,
    Determined,
    Relaxed,
    Hush,
    Watchful,
    Drowsy,
    Asleep,
    Lost,
    Tired,
    Sweaty,
    Strained,
    Pleading,
    Gloomy,
    Sunny,
    Foggy,
    Spooky,
    Sleepy,
    Hungry,
    Party,
    Grumpy,
    Bored,
    /// Keys are flying: eyes on the keyboard, a bob per keystroke.
    Typing,
    /// Waiting on the AI: eyes up and to the side, dots drifting.
    Thinking,
    /// Focus block running: acting out the kind of app in front.
    Work(AppCategory),
}

/// Symbols that replace the eyes for a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    None,
    Heart,
    Star,
    Cross,
    Spiral,
    /// `> <`
    Squeeze,
    /// `O O`
    Ring,
    Question,
}

/// A level the user is dialling right now (shown in a HUD).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gauge {
    Volume,
    Brightness,
}

/// How long the face keeps reacting to a level after the last change
/// (the HUD itself lives 2.2 s).
const GAUGE_HOLD: f32 = 2.4;

#[derive(Debug, Clone, Copy, PartialEq)]
struct GaugeState {
    kind: Gauge,
    level: f32,
    muted: bool,
    /// When the level last changed.
    at: f32,
    /// When mute last flipped (for the squeeze).
    mute_at: f32,
}

/// One dance step. The dancer switches move every phrase (4 or 8 beats)
/// so a long song never loops the same wiggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Side to side on the beat.
    Sway,
    /// Hop up on every beat.
    Bounce,
    /// Cool head nod, eyes low.
    Nod,
    /// Slide left, slide right.
    Shuffle,
    /// Tilt back and forth with a squash.
    Twist,
    /// A new direction every beat.
    LookAround,
    /// Big springy hops on the downbeat.
    Hop,
    /// Lazy figure-eight, half-lidded.
    Groove,
    /// Eyes pump bigger on the beat.
    Pump,
    /// Winks left, right, left...
    WinkStep,
    /// Straight headbang.
    Bang,
    /// Banging into each side.
    SideBang,
    /// Fast thrash with a shake.
    Thrash,
    /// Slow sway, eyes soft.
    Drift,
    /// Eyes closed, floating, looking up.
    Float,
    /// Lean one way for a bar, then the other.
    Lean,
}

impl Move {
    /// Moves a mood can pick from, with a weight each. High energy favours
    /// the bigger moves.
    fn pool(mood: Mood, energy: f32) -> &'static [(Move, u32)] {
        let hot = energy > 0.45;
        match mood {
            Mood::Headbang => &[(Move::Bang, 4), (Move::SideBang, 3), (Move::Thrash, 2), (Move::Nod, 2), (Move::Hop, 1)],
            Mood::Vibe => &[(Move::Drift, 4), (Move::Float, 3), (Move::Lean, 3), (Move::Nod, 2), (Move::Groove, 2)],
            _ if hot => &[
                (Move::Bounce, 3),
                (Move::Hop, 3),
                (Move::Shuffle, 3),
                (Move::Twist, 2),
                (Move::Pump, 2),
                (Move::LookAround, 2),
                (Move::WinkStep, 1),
                (Move::Sway, 1),
            ],
            _ => &[
                (Move::Sway, 3),
                (Move::Groove, 3),
                (Move::Nod, 2),
                (Move::Shuffle, 2),
                (Move::Twist, 2),
                (Move::LookAround, 2),
                (Move::WinkStep, 1),
                (Move::Bounce, 1),
            ],
        }
    }
}

/// Springy follow-through layered over the pose: hops, squishes and
/// wobbles overshoot and settle instead of easing in flat.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Jiggle {
    y: f32,
    vy: f32,
    sq: f32,
    vsq: f32,
    rot: f32,
    vrot: f32,
}

impl Jiggle {
    fn step(&mut self, dt: f32) {
        let n = (dt * 240.0).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        for _ in 0..n {
            self.vy += (-self.y * 260.0 - self.vy * 13.0) * h;
            self.y += self.vy * h;
            self.vsq += (-self.sq * 420.0 - self.vsq * 15.0) * h;
            self.sq += self.vsq * h;
            self.vrot += (-self.rot * 220.0 - self.vrot * 11.0) * h;
            self.rot += self.vrot * h;
        }
        // Keep the follow-through small whatever piles up.
        self.y = self.y.clamp(-6.0, 6.0);
        self.sq = self.sq.clamp(-0.25, 0.25);
        self.rot = self.rot.clamp(-0.35, 0.35);
    }

    /// Jump: up and stretched, lands with a squish.
    fn hop(&mut self, v: f32) {
        self.vy -= v;
        self.vsq -= v * 0.012;
    }

    /// Get squashed down.
    fn squish(&mut self, v: f32) {
        self.vy += v * 0.4;
        self.vsq += v * 0.02;
    }

    fn wobble(&mut self, v: f32) {
        self.vrot += v;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum React {
    Surprised,
    Huh,
    EyeRoll,
    Sparkle,
    Sigh,
    Wake,
    Yawn,
    Stretch,
    Wink,
    DoubleBlink,
    Sneeze,
    Hiccups,
    ClockGlance,
    Peek,
    Flash,
    GotIt,
    Perk,
    Shocked,
    Happy,
    Gulp,
    Love,
    Dizzy,
    Annoyed,
    Defeated,
    Goodnight,
    WelcomeBack,
    Satisfied,
    BarrelRoll,
    Confused,
    WatchGame,
    LookUp,
    /// A note got written: read it over, then a proud nod.
    Noted,
    /// Something got deleted: squeeze and a little puff.
    Poof,
    /// Whoosh to one side (`true` = right): next track, calendar step.
    Swipe(bool),
    /// Record scratch: the music stopped mid-dance.
    Scratch,
    /// Big win: confetti, stars and bounces.
    Celebrate,
    /// Undid something: awkward look away, blush, sweat.
    Oops,
    /// Clean sweep across with a dust trail.
    Sweep,
    /// Sharp inhale with a `!`.
    Gasp,
    /// Off you go: wink and a tilt.
    Salute,
    /// Speed lines and shades.
    Zoom,
    /// Yes: two quick nods.
    Nod,
    /// No: shake side to side.
    Shake,
    /// CAPS LOCK: shouting.
    Shout,
    /// Battery nearly dead: X eyes, slump.
    Faint,
    /// Tickled by a long hover.
    Giggle,
    /// The pointer left after a long visit.
    Pout,
    Emote(Glyph),
}

impl React {
    pub fn duration(self) -> f32 {
        use React::*;
        match self {
            Surprised => 0.9,
            Huh => 2.4,
            EyeRoll => 1.4,
            Sparkle => 1.6,
            Sigh => 2.2,
            Wake => 1.0,
            Yawn => 2.6,
            Stretch => 2.4,
            Wink => 0.7,
            DoubleBlink => 0.5,
            Sneeze => 2.0,
            Hiccups => 3.4,
            ClockGlance => 1.8,
            Peek => 4.0,
            Flash => 0.8,
            GotIt => 0.7,
            Perk => 1.4,
            Shocked => 1.6,
            Happy => 1.6,
            Gulp => 1.4,
            Love => 1.6,
            Dizzy => 2.6,
            Annoyed => 1.8,
            Defeated => 2.0,
            Goodnight => 2.0,
            WelcomeBack => 1.6,
            Satisfied => 1.2,
            BarrelRoll => 1.5,
            Confused => 2.0,
            WatchGame => 1.5,
            LookUp => 2.5,
            Noted => 2.0,
            Poof => 0.9,
            Swipe(_) => 0.8,
            Scratch => 1.6,
            Celebrate => 2.4,
            Oops => 1.6,
            Sweep => 1.2,
            Gasp => 1.4,
            Salute => 1.4,
            Zoom => 1.6,
            Nod => 0.9,
            Shake => 0.9,
            Shout => 1.4,
            Faint => 2.6,
            Giggle => 1.6,
            Pout => 1.4,
            Emote(_) => 1.6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hat {
    None,
    Santa,
    Party,
    Nightcap,
}

/// The eased part of the face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Lid opening 0..1.
    pub open: f32,
    pub wink_l: f32,
    pub wink_r: f32,
    /// `^ ^` lower lids.
    pub happy: f32,
    /// Inner lids down.
    pub angry: f32,
    /// Outer lids down.
    pub sad: f32,
    pub look: (f32, f32),
    /// Pupils converge.
    pub cross: f32,
    pub pupil: f32,
    pub size: f32,
    /// Extra scale on the left eye only (confused).
    pub size_l: f32,
    pub dx: f32,
    pub dy: f32,
    pub rot: f32,
    pub squash: f32,
    /// Brow raise in px (negative furrows).
    pub brow: f32,
    pub blush: f32,
    /// Little `o` mouth 0..1.3.
    pub mouth: f32,
}

impl Pose {
    pub fn rest() -> Self {
        Self {
            open: 1.0,
            wink_l: 0.0,
            wink_r: 0.0,
            happy: 0.0,
            angry: 0.0,
            sad: 0.0,
            look: (0.0, 0.0),
            cross: 0.0,
            pupil: 1.0,
            size: 1.0,
            size_l: 1.0,
            dx: 0.0,
            dy: 0.0,
            rot: 0.0,
            squash: 0.0,
            brow: 0.0,
            blush: 0.0,
            mouth: 0.0,
        }
    }

    fn ease_to(&mut self, o: &Pose, k: f32) {
        let l = |a: &mut f32, b: f32| *a += (b - *a) * k;
        l(&mut self.open, o.open);
        l(&mut self.wink_l, o.wink_l);
        l(&mut self.wink_r, o.wink_r);
        l(&mut self.happy, o.happy);
        l(&mut self.angry, o.angry);
        l(&mut self.sad, o.sad);
        l(&mut self.cross, o.cross);
        l(&mut self.pupil, o.pupil);
        l(&mut self.size, o.size);
        l(&mut self.size_l, o.size_l);
        l(&mut self.dx, o.dx);
        l(&mut self.dy, o.dy);
        l(&mut self.rot, o.rot);
        l(&mut self.squash, o.squash);
        l(&mut self.brow, o.brow);
        l(&mut self.blush, o.blush);
        l(&mut self.mouth, o.mouth);
    }
}

/// Unsmoothed per-frame motion (beats, shakes, peeks).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Live {
    dx: f32,
    dy: f32,
    rot: f32,
    squash: f32,
    flash: f32,
}

/// Switches that are on or off, never eased.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extras {
    pub glyph: Glyph,
    pub hat: Hat,
    pub shades: bool,
    pub pumpkin: bool,
    pub glow: Option<Color>,
    pub slit: bool,
    pub shine: bool,
    pub dream: bool,
    pub caption: Option<&'static str>,
    pub rec: Option<Color>,
    /// Soft light on the face (a screen being watched).
    pub screen: Option<Color>,
}

impl Default for Extras {
    fn default() -> Self {
        Self { glyph: Glyph::None, hat: Hat::None, shades: false, pumpkin: false, glow: None, slit: false, shine: false, dream: false, caption: None, rec: None, screen: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Z,
    Note,
    Heart,
    Spark,
    Sweat,
    Snow,
    Rain,
    Confetti,
    Firework,
    Bubble,
    Question,
    Anger,
    Drool,
    Sheep,
    Dot,
    /// Sound wave arc, `vx` holds the side it travels to.
    Wave,
    /// A tiny page with lines, floats up when a note is saved.
    Page,
    /// Horizontal speed line.
    Dash,
    /// `!` mark.
    Bang,
    /// A short symbol (`{}`, `>_`, `@`...) in the work theme's colour.
    Sym(&'static str),
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    kind: Kind,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    size: f32,
    hue: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Genre {
    Plain,
    Chill,
    Heavy,
    Love,
    Party,
    Sad,
}

fn genre_of(t: &Track) -> Genre {
    let s = format!("{} {} {}", t.title, t.artist, t.album).to_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| s.contains(w));
    if has(&["sad", "cry", "tears", "alone", "lonely", "goodbye"]) {
        Genre::Sad
    } else if has(&["lofi", "lo-fi", "chill", "sleep", "ambient", "acoustic", "piano", "lullaby", "jazz", "calm"]) {
        Genre::Chill
    } else if has(&["metal", "rock", "punk", "thrash", "hardcore", "grunge", "dubstep"]) {
        Genre::Heavy
    } else if has(&["love", "heart", "kiss", "valentine", "darling"]) {
        Genre::Love
    } else if has(&["party", "dance", "edm", "remix", "club", "disco", "house"]) {
        Genre::Party
    } else {
        Genre::Plain
    }
}

/// Onset detector over the output peak meter. Falls back to a steady
/// 120 bpm when there is no meter (or it reads silence while "playing").
#[derive(Debug, Clone)]
struct Beat {
    slow: f32,
    energy: f32,
    last: f32,
    period: f32,
    count: u32,
    real: bool,
    silent_s: f32,
    /// Strength of the latest beat 0..1 (how far the onset jumped over the
    /// running level); filler beats are soft.
    hit: f32,
}

impl Beat {
    fn new() -> Self {
        Self { slow: 0.0, energy: 0.35, last: 0.0, period: 0.5, count: 0, real: false, silent_s: 0.0, hit: 0.5 }
    }

    fn feed(&mut self, peak: Option<f32>, playing: bool, t: f32, dt: f32) {
        if !playing {
            self.real = false;
            return;
        }
        let k = |r: f32| 1.0 - (-r * dt).exp();
        match peak {
            Some(pk) => {
                self.slow += (pk - self.slow) * k(1.2);
                self.energy += (self.slow.min(1.0) - self.energy) * k(0.8);
                self.silent_s = if pk < 0.002 { self.silent_s + dt } else { 0.0 };
                self.real = self.silent_s < 3.0;
                let gap = t - self.last;
                let edge = self.slow * 1.3 + 0.015;
                if self.real && pk > edge && gap > 0.27 {
                    if gap < 1.4 {
                        self.period += (gap.clamp(0.3, 1.2) - self.period) * 0.3;
                    }
                    self.last = t;
                    self.count += 1;
                    self.hit = (0.45 + (pk / edge - 1.0) * 1.2).clamp(0.45, 1.0);
                }
            }
            None => self.real = false,
        }
        // Keep moving: metronome without a meter, filler beats in quiet bars.
        let wait = if self.real { self.period * 2.2 } else { self.period };
        if t - self.last >= wait {
            self.last = t;
            self.count += 1;
            self.hit = if self.real { 0.35 } else { 0.6 };
        }
    }

    /// Progress through the current beat 0..1.
    fn phase(&self, t: f32) -> f32 {
        ((t - self.last) / self.period).clamp(0.0, 1.0)
    }

    fn pulse(&self, t: f32) -> f32 {
        (-(t - self.last).max(0.0) * 7.0).exp()
    }

    fn sway(&self, t: f32) -> f32 {
        let ph = ((t - self.last) / self.period).clamp(0.0, 1.0);
        let side = if self.count % 2 == 0 { 1.0 } else { -1.0 };
        side * (ph * PI).sin()
    }
}

// Spawn timers (indexes into `Eyes::timers`).
const T_Z: usize = 0;
const T_BUBBLE: usize = 1;
const T_THINK: usize = 2;
const T_HEART: usize = 3;
const T_SWEAT: usize = 4;
const T_QUESTION: usize = 5;
const T_DROOL: usize = 6;
const T_CONFETTI: usize = 7;
const T_RAIN: usize = 8;
const T_SNOW: usize = 9;
const T_FIREWORK: usize = 10;
const T_LIGHTNING: usize = 11;
const T_SEASON_HEART: usize = 12;
const T_WORK: usize = 13;

pub struct Eyes {
    t: f32,
    rng: u64,
    pub mood: Mood,
    pose: Pose,
    live: Live,
    extras: Extras,
    blink: f32,
    blink_at: f32,
    blink_start: Option<f32>,
    blink_double: bool,
    gaze: (f32, f32),
    gaze_target: (f32, f32),
    saccade_at: f32,
    react: Option<(React, f32)>,
    react_mark: bool,
    queue: VecDeque<(React, f32)>,
    parts: Vec<Particle>,
    timers: [f32; 16],
    beat: Beat,
    genre: Genre,
    ring: Option<Color>,
    started: bool,
    track_key: String,
    track_t: f32,
    last_pos: u64,
    last_dur: u64,
    last_play: Playback,
    was_charging: Option<bool>,
    was_offline: bool,
    was_asleep: bool,
    was_locked: bool,
    last_day: Option<NaiveDate>,
    glanced_hour: Option<u32>,
    next_bit: f32,
    pokes: VecDeque<f32>,
    last_mouse: Option<(f32, f32)>,
    speed: f32,
    chase_s: f32,
    stare_s: f32,
    near_s: f32,
    rub_dir: f32,
    rubs: VecDeque<f32>,
    purr_until: f32,
    pleading_until: f32,
    last_water: f32,
    jig: Jiggle,
    gauge: Option<GaugeState>,
    dance: Move,
    dance_from: u32,
    dance_len: u32,
    seen_beat: u32,
    noted_pages: u32,
    /// Look target from a window switch, and until when.
    glance: Option<((f32, f32), f32)>,
    /// Mouse-wheel spin: current pupil angle, target, last notch time.
    roll: f32,
    roll_to: f32,
    scroll_at: f32,
    scrolls: VecDeque<f32>,
    last_idle: u64,
    keys: VecDeque<f32>,
    key_x: f32,
    typing_since: Option<f32>,
    hover_s: f32,
    hover_side: f32,
    giggled: bool,
    last_batt: Option<u8>,
    was_caps: Option<bool>,
    tick_sec: i32,
    /// A skip button was pressed: the track change that follows is ours.
    skip_until: f32,
    /// The work theme playing (Focus block + known app).
    work: Option<AppTheme>,
}

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
        }
    }

    fn rand(&mut self) -> f32 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
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
    fn wave_burst(&mut self, n: u32) {
        for i in 0..n {
            for side in [-1.0f32, 1.0] {
                self.spawn(Kind::Wave, side * 20.0, 0.0, side, 0.0, 0.55 + 0.12 * i as f32, 1.0 + 0.35 * i as f32);
            }
        }
    }

    /// The level HUD the face is acting out, while it is fresh.
    fn live_gauge(&self) -> Option<GaugeState> {
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

    fn start(&mut self, r: React) {
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
            _ => {}
        }
    }

    fn spawn(&mut self, kind: Kind, x: f32, y: f32, vx: f32, vy: f32, life: f32, size: f32) {
        if self.parts.len() >= 48 {
            self.parts.remove(0);
        }
        let hue = self.rand();
        self.parts.push(Particle { kind, x, y, vx, vy, age: 0.0, life, size, hue });
    }

    fn due(&mut self, i: usize, period: f32) -> bool {
        if self.t >= self.timers[i] {
            let j = self.range(0.8, 1.2);
            self.timers[i] = self.t + period * j;
            true
        } else {
            false
        }
    }

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
        self.edges(s);
        self.mood = self.pick_mood(s);
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
        if h >= 22 || h < 5 {
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

    /// Dance bookkeeping, once per beat: switch moves at phrase ends, kick
    /// the spring on the accents, toss notes on every other beat.
    fn choreo(&mut self, s: &Senses) {
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
            _ => {}
        }
        if s.visible && c % 2 == 0 {
            let (ax, ay) = s.area;
            let side = if self.rand() < 0.5 { -1.0 } else { 1.0 };
            let size = 7.5 + 2.0 * amp;
            self.spawn(Kind::Note, side * (ax - 6.0).min(26.0), ay * 0.3, side * 2.0, -7.0 - 3.0 * amp, 1.6, size);
        }
    }

    /// Little unprompted bits of personality while nothing else happens.
    fn bits(&mut self, s: &Senses) {
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
                if r < 0.3 {
                    self.start(React::EyeRoll);
                } else if r < 0.55 {
                    self.start(React::LookUp);
                } else if r < 0.85 {
                    // Counting sheep: one hops the fence, the eyes follow it.
                    let ax = s.area.0;
                    self.spawn(Kind::Sheep, -ax, 0.0, 0.0, 0.0, 4.5, 1.0);
                } else {
                    self.start(React::Yawn);
                }
                self.next_bit = self.t + self.range(5.0, 10.0);
            }
            Drowsy | Sleepy | Tired => {
                if r < 0.6 {
                    self.start(React::Yawn);
                } else {
                    self.start(React::DoubleBlink);
                }
                self.next_bit = self.t + self.range(8.0, 18.0);
            }
            _ => {
                let pick = match (r * 100.0) as u32 {
                    0..=7 => React::Wink,
                    8..=14 => React::Peek,
                    15..=21 => React::ClockGlance,
                    22..=29 => {
                        let g = [Glyph::Heart, Glyph::Star, Glyph::Squeeze, Glyph::Ring, Glyph::Cross, Glyph::Question];
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

    /// Mood- and calendar-driven particles.
    fn ambient_parts(&mut self, s: &Senses) {
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
            Party => {
                if self.due(T_CONFETTI, 0.9) {
                    let x = self.range(-ax, ax);
                    let vy = self.range(8.0, 12.0);
                    self.spawn(Kind::Confetti, x, -ay, 0.0, vy, 2.0 * ay / vy, 1.0);
                }
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
                let side = if (self.t / 1.3) as u32 % 2 == 0 { -1.0 } else { 1.0 };
                self.jig.hop(8.0);
                self.spawn(Kind::Sym("\u{2026}"), side * edge, -6.0, 0.0, -5.0, 1.2, 7.0);
            }
            Email => self.spawn(Kind::Sym(pick(r, &["@", "\u{2709}"])), side * edge, -2.0, 0.0, -6.0, 1.6, 6.5),
            Meeting => {
                // Someone is talking on one side.
                let side = if (self.t / 6.0) as u32 % 2 == 0 { -1.0 } else { 1.0 };
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
    fn reaction_moments(&mut self) {
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
            _ => {}
        }
    }

    fn compose(&mut self, s: &Senses) -> (Pose, Extras, Live, Option<(f32, f32)>) {
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
        let side = if b.count % 2 == 0 { 1.0 } else { -1.0 };
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
                let l = if (step / 4) % 2 == 0 { 1.0 } else { -1.0 };
                p.open = 0.6;
                p.happy = 0.6;
                *look = Some((l * 0.6, -0.2));
                lv.rot = l * 0.12;
                lv.dx = l * 3.0;
                lv.dy = pl * 1.2;
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
    /// Acting out the app in front during a Focus block. Each category gets
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

    #[allow(clippy::too_many_arguments)]
    fn react_pose(&self, r: React, e: f32, s: &Senses, p: &mut Pose, x: &mut Extras, lv: &mut Live, look: &mut Option<(f32, f32)>) {
        let pr = (e / r.duration()).clamp(0.0, 1.0);
        let bell = (pr * PI).sin();
        let calm = |p: &mut Pose| {
            p.happy = 0.0;
            p.angry = 0.0;
            p.sad = 0.0;
        };
        match r {
            React::Surprised => {
                calm(p);
                p.size = 1.28;
                p.open = 1.0;
                p.pupil = 0.65;
                p.brow = 2.5;
                p.mouth = 0.7 * bell;
                lv.dy = -1.5 * bell;
            }
            React::Huh => {
                calm(p);
                p.open = 1.0;
                p.brow = 1.5;
                p.size_l = 1.12;
                *look = Some(((e * 2.6).sin() * 0.9, -0.1));
            }
            React::EyeRoll => {
                let a = PI * (1.0 - pr);
                *look = Some((a.cos(), -a.sin() * 0.9));
                p.open = 0.75;
                p.angry = 0.15;
                p.brow = -1.0;
            }
            React::Sparkle => {
                x.glyph = Glyph::Star;
                p.size = 1.0 + 0.15 * (e * 10.0).sin().abs();
            }
            React::Sigh => {
                p.open = 0.0;
                p.happy = 1.0;
                lv.dy = bell * 1.5;
            }
            React::Wake => {
                calm(p);
                p.brow = 2.0;
                if e < 0.25 {
                    p.size = 1.35;
                    p.open = 1.0;
                    p.pupil = 0.6;
                    lv.dy = -2.5;
                }
            }
            React::Yawn => {
                p.open = 0.12;
                p.happy = 0.5;
                p.mouth = 1.3 * bell;
                p.brow = 1.5 * bell;
                lv.squash = -0.14 * bell;
            }
            React::Stretch => {
                if pr < 0.6 {
                    p.open = 0.0;
                    p.happy = 1.0;
                    lv.squash = -0.2 * (pr / 0.6 * PI).sin();
                    lv.rot = (e * 3.0).sin() * 0.08;
                } else {
                    calm(p);
                    p.open = 1.0;
                    p.size = 1.15;
                }
            }
            React::Wink => {
                if pr < 0.75 {
                    p.wink_r = 1.0;
                    p.happy = p.happy.max(0.45);
                }
            }
            React::DoubleBlink => {}
            React::Sneeze => {
                if e < 1.2 {
                    let k = e / 1.2;
                    p.open = 1.0 - 0.75 * k;
                    p.mouth = 0.4 * k;
                    p.brow = 2.0 * k;
                    lv.dy = -2.0 * k;
                    lv.rot = -0.06 * k;
                } else if e < 1.45 {
                    p.open = 0.0;
                    p.angry = 0.6;
                    lv.dy = 3.0;
                    lv.squash = 0.22;
                    lv.dx = (e * 80.0).sin() * 1.2;
                } else {
                    p.open = 0.6;
                }
            }
            React::Hiccups => {
                let h = [0.2, 1.0, 1.8, 2.7]
                    .iter()
                    .map(|&ti| {
                        let u = e - ti;
                        if (0.0..0.22).contains(&u) {
                            (u / 0.22 * PI).sin()
                        } else {
                            0.0
                        }
                    })
                    .fold(0.0f32, f32::max);
                lv.dy = -3.2 * h;
                p.size = 1.0 + 0.15 * h;
                p.open = 1.0;
                p.mouth = 0.5 * h;
            }
            React::ClockGlance => {
                *look = Some((0.95, 0.9));
                p.open = 0.85;
            }
            React::Peek => {
                let side = if (self.t - e) as i64 % 2 == 0 { 1.0 } else { -1.0 };
                let k = (pr * 4.0).min(1.0).min((1.0 - pr) * 4.0);
                lv.dx = side * k * (s.area.0 - 18.0).max(0.0);
                lv.rot = side * 0.12 * k;
                p.size = 1.08;
                *look = Some((side * 0.9, 0.0));
            }
            React::Flash => {
                lv.flash = (1.0 - pr * 2.5).max(0.0);
                if pr < 0.45 {
                    x.glyph = Glyph::Squeeze;
                }
            }
            React::GotIt => {
                lv.dy = 1.5 * bell;
                p.happy = 0.6 * bell;
                p.open = 1.0 - 0.5 * bell;
            }
            React::Perk => {
                calm(p);
                p.size = 1.15;
                p.brow = 2.0;
                p.open = 1.0;
                *look = Some((0.75, 1.0));
            }
            React::Shocked => {
                x.glyph = Glyph::Ring;
                p.size = 1.15;
                p.mouth = 1.0;
                p.brow = 3.0;
                lv.dx = (e * 55.0).sin() * 0.9 * (1.0 - pr);
            }
            React::Happy => {
                p.happy = 1.0;
                p.open = 0.9;
                lv.dy = -(e * 9.0).sin().abs() * 2.2 * (1.0 - pr);
            }
            React::Gulp => {
                if e < 0.6 {
                    p.open = 0.5;
                    p.mouth = 0.6;
                    lv.dy = (e * 20.0).sin() * 0.8;
                } else {
                    p.happy = 1.0;
                    p.open = 0.8;
                    lv.dy = -bell;
                }
            }
            React::Love => {
                x.glyph = Glyph::Heart;
                p.size = 1.0 + 0.18 * (e * 7.0).sin().abs();
            }
            React::Dizzy => {
                x.glyph = Glyph::Spiral;
                lv.rot = (e * 5.0).sin() * 0.18;
                lv.dx = (e * 3.3).sin() * 2.0;
            }
            React::Annoyed => {
                p.angry = 0.85;
                p.open = 0.45;
                p.brow = -2.5;
                *look = Some((0.0, -0.2));
                lv.dx = (e * 40.0).sin() * 0.3;
            }
            React::Defeated => {
                calm(p);
                p.sad = 0.8;
                p.open = 0.55;
                *look = Some((0.0, 0.9));
                lv.dy = 1.2;
            }
            React::Goodnight => {
                if pr < 0.4 {
                    p.wink_r = 1.0;
                    p.happy = 0.6;
                } else {
                    p.open = 0.0;
                    p.happy = 0.3;
                }
                if x.hat == Hat::None {
                    x.hat = Hat::Nightcap;
                }
            }
            React::WelcomeBack => {
                if pr < 0.6 {
                    x.glyph = Glyph::Star;
                } else {
                    p.happy = 1.0;
                }
                lv.dy = -(e * 8.0).sin().abs() * 2.0 * (1.0 - pr);
            }
            React::Satisfied => {
                p.open = 0.0;
                p.happy = 1.0;
                lv.dy = bell * 1.5;
            }
            React::BarrelRoll => {
                let k = pr * pr * (3.0 - 2.0 * pr);
                lv.rot = k * TAU;
                lv.dy = -bell * 2.0;
                p.size = 1.1;
            }
            React::Confused => {
                p.size_l = 1.3;
                p.sad = 0.25;
                p.brow = 1.0;
                *look = Some(((e * 1.7).sin() * 0.6, -0.3));
            }
            React::WatchGame => {
                p.happy = 0.3;
                p.open = 0.9;
                *look = Some((0.0, 1.0));
                lv.dy = (e * 7.0).sin();
            }
            React::LookUp => {
                p.open = 0.95;
                p.brow = 1.5;
                *look = Some((0.15, -1.0));
            }
            React::Noted => {
                if e < 1.1 {
                    // Read it over: two lines, left to right.
                    let line = (e / 0.55).floor();
                    let u = (e / 0.55).fract();
                    *look = Some((-0.8 + 1.6 * u, 0.35 + 0.3 * line));
                    p.open = 0.85;
                    p.brow = 0.6;
                    p.happy = 0.0;
                } else {
                    // Proud double nod.
                    let k = ((e - 1.1) / 0.9).clamp(0.0, 1.0);
                    p.happy = 0.9;
                    p.open = 0.85;
                    lv.dy = (k * TAU * 2.0).sin().abs() * 2.2 * (1.0 - k);
                }
            }
            React::Poof => {
                if pr < 0.35 {
                    x.glyph = Glyph::Squeeze;
                } else {
                    p.size = 1.1;
                    p.brow = 1.5;
                    *look = Some((0.0, 0.5));
                }
            }
            React::Swipe(right) => {
                let sd = if right { 1.0 } else { -1.0 };
                let k = if pr < 0.35 { (pr / 0.35).powf(0.5) } else { 1.0 - (pr - 0.35) / 0.65 };
                lv.dx = sd * 8.0 * k;
                lv.squash = 0.15 * k;
                lv.rot = sd * 0.08 * k;
                *look = Some((sd, 0.0));
                p.open = 0.9;
            }
            React::Scratch => {
                calm(p);
                if e < 0.35 {
                    p.size = 1.25;
                    p.pupil = 0.6;
                    p.brow = 2.5;
                    lv.dx = (e * 90.0).sin() * 1.2;
                } else {
                    p.sad = 0.5;
                    p.open = 0.6;
                    *look = Some((0.0, 0.6));
                    lv.dy = 1.5;
                }
            }
            React::Celebrate => {
                if (e / 0.4) as i32 % 2 == 0 {
                    x.glyph = Glyph::Star;
                } else {
                    p.happy = 1.0;
                    p.open = 0.9;
                }
                lv.dy = -(e * 8.0).sin().abs() * 3.0 * (1.0 - pr * 0.5);
                lv.rot = (e * 6.0).sin() * 0.12;
            }
            React::Oops => {
                calm(p);
                p.blush = 1.0;
                p.happy = 0.3;
                p.mouth = 0.3;
                p.brow = 1.2;
                *look = Some((0.85, -0.35));
                lv.dx = -1.5 * bell;
                lv.squash = 0.06 * bell;
            }
            React::Sweep => {
                let u = -1.0 + 2.0 * pr;
                *look = Some((u, 0.4));
                lv.dx = u * 6.0;
                lv.rot = u * 0.06;
                p.happy = 0.4;
            }
            React::Gasp => {
                calm(p);
                if e < 0.3 {
                    lv.squash = -0.15 * (e / 0.3);
                    lv.dy = -2.0 * (e / 0.3);
                }
                p.size = 1.3;
                p.pupil = 0.55;
                p.mouth = if e < 0.6 { 1.0 } else { 0.6 * (1.0 - pr) };
                p.brow = 3.0;
            }
            React::Salute => {
                if pr < 0.7 {
                    p.wink_r = 1.0;
                }
                p.happy = 0.4;
                p.brow = 1.5;
                lv.rot = -0.12 * bell;
                lv.dx = 1.5 * bell;
            }
            React::Zoom => {
                x.shades = true;
                lv.dx = (e * 14.0).sin() * 6.0 * (1.0 - pr);
                lv.squash = 0.12 * (1.0 - pr);
                p.happy = 0.3;
            }
            React::Nod => {
                lv.dy = (pr * 2.0 * TAU).sin().max(0.0) * 2.4;
                p.happy = 0.5;
                p.open = 0.9;
                *look = Some((0.0, 0.3));
            }
            React::Shake => {
                let sh = (pr * 3.0 * TAU).sin() * (1.0 - pr * 0.5);
                lv.dx = sh * 3.0;
                *look = Some((-sh, 0.1));
                p.angry = 0.15;
                p.open = 0.85;
            }
            React::Shout => {
                calm(p);
                p.angry = 0.5;
                p.size = 1.25;
                p.pupil = 0.7;
                p.brow = -2.0;
                p.mouth = 1.2 * bell;
                lv.dx = (e * 60.0).sin() * 0.7;
            }
            React::Faint => {
                if pr < 0.6 {
                    x.glyph = Glyph::Cross;
                    lv.dy = 3.0 * pr / 0.6;
                    lv.rot = 0.25 * pr / 0.6;
                } else {
                    calm(p);
                    p.sad = 0.6;
                    p.open = 0.3;
                    lv.dy = 2.0;
                }
            }
            React::Giggle => {
                p.happy = 1.0;
                p.open = 0.0;
                p.blush = 1.0;
                lv.dy = (e * 28.0).sin() * 0.8 * (1.0 - pr);
                lv.rot = (e * 14.0).sin() * 0.05;
            }
            React::Pout => {
                calm(p);
                p.sad = 0.55;
                p.open = 0.8;
                p.brow = 1.0;
                *look = Some((self.hover_side * 0.9, 0.2));
                lv.dy = 0.8;
            }
            React::Emote(g) => {
                x.glyph = g;
                p.size = 1.0 + 0.1 * bell;
                if g == Glyph::Ring {
                    p.size_l = 0.7;
                }
            }
        }
    }

    fn update_gaze(&mut self, dt: f32, s: &Senses, own: Option<(f32, f32)>) {
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

    fn blink_amount(&mut self) -> f32 {
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

    fn update_parts(&mut self, dt: f32, s: &Senses) {
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

    // ------------------------------------------------------------- paint

    pub fn draw(&self, sc: &mut Scene, pt: &Paint) {
        let p = &self.pose;
        let lv = &self.live;
        let j = &self.jig;
        let xf = Xf::new(pt.cx, pt.cy, pt.scale, p.dx + lv.dx, p.dy + lv.dy + j.y, p.rot + lv.rot + j.rot, p.squash + lv.squash + j.sq);
        let tint = pt.tint.or(self.work.and_then(|w| w.tint).map(|[r, g, b]| Color::rgb(r, g, b))).or(self.ring);
        for side in [-1.0f32, 1.0] {
            self.eye(sc, &xf, side, pt.style, tint, pt.accent);
        }
        self.brows(sc, &xf, pt.style);
        if p.mouth > 0.05 {
            let ry = radii(pt.style).1;
            let pts = ellipse(0.0, ry + 4.0, 2.0 * p.mouth, 2.3 * p.mouth, 16);
            let mut pts: Vec<_> = pts.iter().map(|&(x, y)| xf.p(x, y)).collect();
            pts.push(pts[0]);
            sc.polyline(pts, 1.3 * pt.scale, TEXT.fade(0.9));
        }
        if p.blush > 0.02 {
            for side in [-1.0f32, 1.0] {
                let pts = ellipse(side * 22.5, 5.0, 3.6, 1.9, 14).into_iter().map(|(x, y)| xf.p(x, y)).collect();
                sc.poly(pts, Color::rgba(255, 120, 150, 0.42 * p.blush.min(1.0)));
            }
        }
        let x = &self.extras;
        if x.shades {
            shades(sc, &xf, pt.scale);
        }
        hat(sc, &xf, x.hat, pt.scale);
        let at = |x: f32, y: f32| (pt.cx + x * pt.scale, pt.cy + y * pt.scale);
        if x.pumpkin && pt.props {
            pumpkin(sc, at(34.0, 4.0), pt.scale);
        }
        if let Some(cap) = x.caption {
            let (cx, cy) = at(31.0, 1.0);
            let w = 20.0 * pt.scale;
            sc.text(cap, Rect::new(cx - w / 2.0, cy - 6.0 * pt.scale, w, 12.0 * pt.scale), 8.0 * pt.scale, Weight::Semibold, TEXT_3, Align::Center);
        }
        if let Some(c) = x.rec {
            let (cx, cy) = at(31.0, -9.0);
            let pulse = 0.75 + 0.25 * (self.t * 4.0).sin();
            sc.circle(cx, cy, 2.4 * pt.scale, c.fade(pulse));
        }
        self.particles(sc, pt);
        if lv.flash > 0.01 {
            sc.rect(pt.bounds, pt.bounds.h / 2.0, WHITE.fade(lv.flash * 0.45));
        }
    }

    fn eye(&self, sc: &mut Scene, xf: &Xf, side: f32, style: EyeStyle, tint: Option<Color>, accent: Option<Color>) {
        let p = &self.pose;
        let x = &self.extras;
        let k = p.size * if side < 0.0 { p.size_l } else { 1.0 };
        let ex = side * EYE_X;
        let (rx0, ry0) = radii(style);
        let (rx, ry) = (rx0 * k, ry0 * k);
        if x.glyph != Glyph::None {
            self.glyph(sc, xf, side, ex, rx, ry);
            return;
        }
        let wink = if side < 0.0 { p.wink_l } else { p.wink_r };
        let open = (p.open * (1.0 - self.blink) * (1.0 - wink.clamp(0.0, 1.0))).clamp(0.0, 1.0);
        let lid = Lid { rx, ry, open, angry: p.angry, sad: p.sad, happy: p.happy.clamp(0.0, 1.0), side, style };

        let glow = x.glow.or(if style == EyeStyle::Robot { Some(tint.unwrap_or(Color::rgb(61, 230, 255))) } else { None }).or(x.screen).or(accent);
        let sclera: Fill = match (x.glow, style) {
            (Some(g), _) => g.into(),
            (_, EyeStyle::Cat) => {
                let c = tint.unwrap_or(Color::rgb(250, 204, 21));
                Fill::VGrad(c.mix(WHITE, 0.25), c.mix(Color::rgb(0, 0, 0), 0.2))
            }
            (_, EyeStyle::Robot) => tint.unwrap_or(Color::rgb(61, 230, 255)).fade(0.92).into(),
            _ => SCLERA.into(),
        };
        let line_col = match sclera {
            Fill::Solid(c) | Fill::VGrad(c, _) | Fill::DGrad(c, _) => c,
        };

        if lid.gap() < 0.9 {
            // Closed: a soft curve, `^` when happy, with a dreaming bump.
            let n = 14;
            let pts: Vec<(f32, f32)> = (0..=n)
                .map(|i| {
                    let u = -1.0 + 2.0 * i as f32 / n as f32;
                    let mut y = (0.3 - lid.happy * 0.95) * ry * (1.0 - u * u);
                    if x.dream {
                        let c = p.look.0 * 0.6;
                        y -= 1.4 * (-((u - c).powi(2)) / 0.05).exp();
                    }
                    xf.p(ex + u * rx * 0.9, y)
                })
                .collect();
            sc.polyline(pts, 1.9 * xf.s, line_col);
            return;
        }

        if let Some(g) = glow {
            let (cx, cy) = xf.p(ex, 0.0);
            sc.circle(cx, cy, rx * 1.7 * xf.s, g.fade(0.06));
            sc.circle(cx, cy, rx * 1.3 * xf.s, g.fade(0.09));
        }

        let maxx = match style {
            EyeStyle::Anime => rx * 0.25,
            EyeStyle::Cat => rx * 0.45,
            _ => rx * 0.40,
        };
        let px = p.look.0.clamp(-1.0, 1.0) * maxx - side * rx * 0.42 * p.cross;
        let py = p.look.1.clamp(-1.0, 1.0) * ry * 0.36;

        if style == EyeStyle::Pixel {
            self.pixel_eye(sc, xf, &lid, ex, px, py, tint);
            return;
        }

        let outline = lid.outline(22);
        if outline.len() >= 3 {
            sc.poly(outline.iter().map(|&(u, v)| xf.p(ex + u, v)).collect(), sclera);
        }

        let put = |sc: &mut Scene, pts: Vec<(f32, f32)>, fill: Fill| {
            if pts.len() >= 3 {
                sc.poly(pts.into_iter().map(|(u, v)| xf.p(ex + u, v)).collect(), fill);
            }
        };
        let slit = x.slit || style == EyeStyle::Cat;
        match style {
            EyeStyle::Robot => {
                // A bright scan bar instead of a pupil.
                put(sc, clip_ellipse(&lid, px, 0.0, 1.3 * k, ry * 2.0, 6), WHITE.fade(0.95).into());
            }
            EyeStyle::Anime if !slit => {
                let ir = rx * 0.66 * p.pupil.powf(0.3);
                let c = tint.unwrap_or(Color::rgb(96, 165, 250));
                put(sc, clip_ellipse(&lid, px, py, ir, ir * 1.12, 18), Fill::VGrad(c.mix(Color::rgb(0, 0, 0), 0.35), c.mix(WHITE, 0.3)));
                let pr = ir * 0.45 * p.pupil;
                put(sc, clip_ellipse(&lid, px, py, pr, pr * 1.1, 14), Color::rgb(20, 20, 40).into());
                self.highlights(sc, xf, &lid, ex, px - ir * 0.35, py - ir * 0.4, 1.5 * k, true);
            }
            _ if slit => {
                let col = if x.glow.is_some() { Color::rgb(6, 30, 14) } else { PUPIL };
                let w = 1.0 * k * p.pupil.powf(1.5);
                put(sc, clip_ellipse(&lid, px, py, w, ry * 0.92, 10), col.into());
            }
            _ => {
                let pr = 3.1 * k * p.pupil;
                if let Some(c) = tint {
                    put(sc, clip_ellipse(&lid, px, py, pr * 1.7, pr * 1.7, 18), c.into());
                }
                put(sc, clip_ellipse(&lid, px, py, pr, pr, 16), PUPIL.into());
                let big = x.shine;
                self.highlights(sc, xf, &lid, ex, px - pr * 0.4, py - pr * 0.45, if big { 1.3 * k } else { 0.8 * k }, big);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn highlights(&self, sc: &mut Scene, xf: &Xf, lid: &Lid, ex: f32, hx: f32, hy: f32, r: f32, second: bool) {
        let inside = |x: f32, y: f32| lid.span(x).is_some_and(|(t, b)| y > t + 0.3 && y < b - 0.3);
        if inside(hx, hy) {
            let (cx, cy) = xf.p(ex + hx, hy);
            sc.circle(cx, cy, r * xf.s, WHITE.fade(0.95));
        }
        if second {
            let (sx, sy) = (hx + r * 2.6, hy + r * 2.4);
            if inside(sx, sy) {
                let (cx, cy) = xf.p(ex + sx, sy);
                sc.circle(cx, cy, r * 0.5 * xf.s, WHITE.fade(0.8));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn pixel_eye(&self, sc: &mut Scene, xf: &Xf, lid: &Lid, ex: f32, px: f32, py: f32, tint: Option<Color>) {
        let cell = 2.0;
        let snap = |v: f32| (v / cell).round() * cell;
        let pupil = tint.map(|c| c.mix(Color::rgb(0, 0, 0), 0.3)).unwrap_or(PUPIL);
        let (ppx, ppy) = (snap(px), snap(py));
        let half = (2.0 * self.pose.pupil).max(1.0);
        let mut x = -lid.rx;
        while x < lid.rx - 0.01 {
            let xc = x + cell / 2.0;
            if let Some((t, b)) = lid.span(xc) {
                let (t, b) = (snap(t), snap(b));
                if b > t {
                    sc.rect(xf.rect(ex + x, t, cell, b - t), 0.0, SCLERA);
                    if xc > ppx - half && xc < ppx + half {
                        let (pt, pb) = ((ppy - half).max(t), (ppy + half).min(b));
                        if pb > pt {
                            sc.rect(xf.rect(ex + x, pt, cell, pb - pt), 0.0, pupil);
                        }
                    }
                }
            }
            x += cell;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn glyph(&self, sc: &mut Scene, xf: &Xf, side: f32, ex: f32, rx: f32, ry: f32) {
        let t = self.t;
        let w = 2.2 * xf.s;
        match self.extras.glyph {
            Glyph::None => {}
            Glyph::Heart => {
                let pts = (0..28)
                    .map(|i| {
                        let a = i as f32 / 28.0 * TAU;
                        let hx = 16.0 * a.sin().powi(3);
                        let hy = -(13.0 * a.cos() - 5.0 * (2.0 * a).cos() - 2.0 * (3.0 * a).cos() - (4.0 * a).cos());
                        xf.p(ex + hx * rx / 15.0, hy * ry / 15.0 - 0.5)
                    })
                    .collect();
                sc.poly(pts, PINK);
            }
            Glyph::Star => {
                let rot = -PI / 2.0 + t * 1.5 * side;
                let pts = (0..10)
                    .map(|i| {
                        let a = rot + i as f32 / 10.0 * TAU;
                        let r = if i % 2 == 0 { rx * 1.1 } else { rx * 0.48 };
                        xf.p(ex + a.cos() * r, a.sin() * r)
                    })
                    .collect();
                sc.poly(pts, GOLD);
            }
            Glyph::Cross => {
                let d = rx * 0.7;
                sc.polyline(vec![xf.p(ex - d, -d), xf.p(ex + d, d)], w, SCLERA);
                sc.polyline(vec![xf.p(ex - d, d), xf.p(ex + d, -d)], w, SCLERA);
            }
            Glyph::Spiral => {
                let pts = (0..=40)
                    .map(|i| {
                        let f = i as f32 / 40.0;
                        let a = f * TAU * 2.3 + t * 7.0 * side;
                        xf.p(ex + a.cos() * rx * 0.95 * f, a.sin() * ry * 0.95 * f)
                    })
                    .collect();
                sc.polyline(pts, 1.6 * xf.s, SCLERA);
            }
            Glyph::Squeeze => {
                let (dx, dy) = (rx * 0.55, ry * 0.62);
                sc.polyline(vec![xf.p(ex + side * dx, -dy), xf.p(ex - side * dx, 0.0), xf.p(ex + side * dx, dy)], w, SCLERA);
            }
            Glyph::Ring => {
                let mut pts: Vec<_> = ellipse(ex, 0.0, rx * 0.8, ry * 0.8, 20).into_iter().map(|(x, y)| xf.p(x, y)).collect();
                pts.push(pts[0]);
                sc.polyline(pts, 2.0 * xf.s, SCLERA);
            }
            Glyph::Question => {
                let (cx, cy) = xf.p(ex, 0.0);
                let r = 9.0 * xf.s;
                sc.text("?", Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r), 13.0 * xf.s, Weight::Bold, SCLERA, Align::Center);
            }
        }
    }

    fn brows(&self, sc: &mut Scene, xf: &Xf, style: EyeStyle) {
        let p = &self.pose;
        let show = (p.angry.max(p.sad) * 1.8 + p.brow.abs() / 2.0).clamp(0.0, 1.0);
        if show < 0.04 {
            return;
        }
        let ry0 = radii(style).1;
        for side in [-1.0f32, 1.0] {
            let k = p.size * if side < 0.0 { p.size_l } else { 1.0 };
            let ex = side * EYE_X;
            let base = -ry0 * k - 3.2 - p.brow;
            let inner = base + p.angry * 3.0 - p.sad * 2.2;
            let outer = base - p.angry * 0.6 + p.sad * 1.4;
            let pts = vec![xf.p(ex + side * 5.5, outer), xf.p(ex - side * 5.5, inner)];
            sc.polyline(pts, 1.7 * xf.s, TEXT.fade(show * 0.95));
        }
    }

    fn particles(&self, sc: &mut Scene, pt: &Paint) {
        let k = pt.scale;
        let at = |x: f32, y: f32| (pt.cx + x * k, pt.cy + y * k);
        for p in &self.parts {
            let a = (p.age / 0.25).min(1.0) * (1.0 - (p.age / p.life).powi(2)).max(0.0);
            let (x, y) = at(p.x, p.y);
            match p.kind {
                Kind::Z => {
                    let sz = p.size * (1.0 + p.age * 0.25) * k;
                    let s = if p.age > p.life * 0.4 { "Z" } else { "z" };
                    sc.text(s, Rect::new(x - sz, y - sz, 2.0 * sz, 2.0 * sz), sz, Weight::Bold, TEXT.fade(a * 0.85), Align::Center);
                }
                Kind::Note => {
                    let sz = p.size * k;
                    let s = if p.hue > 0.5 { "\u{266B}" } else { "\u{266A}" };
                    let col = pt.accent.unwrap_or(TEXT);
                    sc.text(s, Rect::new(x - sz, y - sz, 2.0 * sz, 2.0 * sz), sz, Weight::Bold, col.fade(a * 0.9), Align::Center);
                }
                Kind::Heart => sc.poly(heart(x, y, p.size * k), PINK.fade(a)),
                Kind::Spark => {
                    let tw = (p.age / p.life * PI).sin();
                    sc.poly(sparkle(x, y, p.size * k * tw.max(0.1)), if p.hue > 0.5 { GOLD } else { WHITE });
                }
                Kind::Sweat | Kind::Drool => {
                    let stretch = if p.kind == Kind::Drool { 1.0 + p.age * 0.6 } else { 1.0 };
                    sc.poly(drop(x, y, p.size * k, stretch), ICE.fade(a));
                }
                Kind::Snow => sc.circle(x, y, 1.1 * k, WHITE.fade(0.85 * a)),
                Kind::Wave => {
                    // `)` on the right, `(` on the left, growing as it travels.
                    let side = p.vx.signum();
                    let r = (3.0 + p.age / p.life * 7.0) * p.size * k;
                    let pts = (0..=8)
                        .map(|i| {
                            let ang = (-0.75 + 1.5 * i as f32 / 8.0) * 0.9;
                            (x + side * ang.cos() * r - side * r * 0.6, y + ang.sin() * r)
                        })
                        .collect();
                    let fade = 1.0 - p.age / p.life;
                    sc.polyline(pts, 1.3 * k, TEXT.fade(0.85 * fade));
                }
                Kind::Dash => {
                    // Trails behind the way it flies.
                    let len = 7.0 * k * (1.0 - p.age / p.life);
                    sc.line(x, y, x - p.vx.signum() * len, y, 1.1 * k, TEXT.fade(0.7 * a));
                }
                Kind::Bang => {
                    let sz = 9.0 * k;
                    sc.text("!", Rect::new(x - sz, y - sz, 2.0 * sz, 2.0 * sz), sz, Weight::Bold, GOLD.fade(a), Align::Center);
                }
                Kind::Page => {
                    let (w, h) = (5.0 * k, 6.4 * k);
                    let tilt = (p.age * 3.0 + p.hue * 6.0).sin() * 0.25;
                    let (c, sn) = (tilt.cos(), tilt.sin());
                    let at = |u: f32, v: f32| (x + u * c - v * sn, y + u * sn + v * c);
                    sc.poly(vec![at(-w / 2.0, -h / 2.0), at(w / 2.0, -h / 2.0), at(w / 2.0, h / 2.0), at(-w / 2.0, h / 2.0)], SCLERA.fade(a));
                    for i in 0..3 {
                        let v = -h / 2.0 + h * (0.3 + 0.2 * i as f32);
                        let (x0, y0) = at(-w * 0.32, v);
                        let (x1, y1) = at(w * if i == 2 { 0.05 } else { 0.32 }, v);
                        sc.line(x0, y0, x1, y1, 0.6 * k, PUPIL.fade(0.7 * a));
                    }
                }
                Kind::Rain => sc.line(x, y, x - 0.6 * k, y + 3.0 * k, 0.9 * k, Color::rgb(147, 197, 253).fade(a)),
                Kind::Confetti => {
                    let r = p.age * 6.0 + p.hue * 6.0;
                    let (c, s) = (r.cos(), r.sin());
                    let pts = [(-1.0, -0.6), (1.0, -0.6), (1.0, 0.6), (-1.0, 0.6)].iter().map(|&(u, v)| (x + (u * c - v * s) * k, y + (u * s + v * c) * k)).collect();
                    sc.poly(pts, hsl(p.hue * 360.0, 0.8, 0.62).fade(a));
                }
                Kind::Firework => sc.circle(x, y, 1.0 * k, hsl(p.hue * 360.0, 0.9, 0.65).fade(a)),
                Kind::Bubble => {
                    let f = p.age / p.life;
                    let col = Color::rgba(186, 230, 253, 0.75);
                    if f < 0.94 {
                        let r = (1.0 + 4.5 * f) * k;
                        let mut pts: Vec<_> = ellipse(x, y, r, r, 18);
                        pts.push(pts[0]);
                        sc.polyline(pts, 0.9 * k, col);
                        sc.circle(x - r * 0.35, y - r * 0.35, 0.7 * k, WHITE.fade(0.8));
                    } else {
                        for i in 0..4 {
                            let ang = i as f32 / 4.0 * TAU + 0.4;
                            sc.circle(x + ang.cos() * 6.5 * k, y + ang.sin() * 6.5 * k, 0.6 * k, col);
                        }
                    }
                }
                Kind::Question => {
                    let sz = 9.0 * k;
                    sc.text("?", Rect::new(x - sz, y - sz, 2.0 * sz, 2.0 * sz), sz, Weight::Bold, TEXT.fade(a), Align::Center);
                }
                Kind::Anger => {
                    let c = Color::rgb(255, 99, 99).fade(a);
                    let s = 1.0 + 0.15 * (p.age * 12.0).sin();
                    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        let pts = vec![(x + sx * 2.6 * k * s, y + sy * 0.8 * k * s), (x + sx * 0.8 * k * s, y + sy * 0.8 * k * s), (x + sx * 0.8 * k * s, y + sy * 2.6 * k * s)];
                        sc.polyline(pts, 1.1 * k, c);
                    }
                }
                Kind::Sheep => {
                    let (fx, fy) = at(0.0, pt.bounds.h / 2.0 / k * 0.45);
                    sc.line(fx, fy - 4.0 * k, fx, fy + 3.0 * k, 0.9 * k, TEXT_3);
                    sc.line(fx - 3.0 * k, fy - 1.5 * k, fx + 3.0 * k, fy - 1.5 * k, 0.9 * k, TEXT_3);
                    let leg = TEXT.fade(0.7);
                    sc.line(x - 1.6 * k, y + 1.4 * k, x - 1.6 * k, y + 3.0 * k, 0.8 * k, leg);
                    sc.line(x + 1.6 * k, y + 1.4 * k, x + 1.6 * k, y + 3.0 * k, 0.8 * k, leg);
                    for (ox, oy, r) in [(-1.8, 0.0, 1.9), (0.4, -0.9, 2.1), (1.8, 0.3, 1.8)] {
                        sc.circle(x + ox * k, y + oy * k, r * k, WHITE);
                    }
                    sc.circle(x + 3.8 * k, y - 0.6 * k, 1.3 * k, Color::rgb(120, 120, 130));
                }
                Kind::Dot => sc.circle(x, y, 0.9 * k, TEXT.fade(a)),
                Kind::Sym(txt) => {
                    let sz = p.size * k;
                    let col = self.work.map(|w| w.tint.map(|[r, g, b]| Color::rgb(r, g, b)).unwrap_or(work_color(w.cat))).unwrap_or(TEXT);
                    sc.text(txt, Rect::new(x - 2.0 * sz, y - sz, 4.0 * sz, 2.0 * sz), sz, Weight::Bold, col.fade(a * 0.9), Align::Center);
                }
            }
        }
    }
}

/// What the painter needs from layout.
pub struct Paint {
    pub cx: f32,
    pub cy: f32,
    pub scale: f32,
    /// The pill: flash fills it, particles stay inside it.
    pub bounds: Rect,
    pub style: EyeStyle,
    /// Iris colour; the mood ring fills in when this is None.
    pub tint: Option<Color>,
    /// Soft glow behind the eyes and colour of the notes (album art).
    pub accent: Option<Color>,
    /// Room for side props (the pumpkin).
    pub props: bool,
}

/// Keyboard-heavy categories: keystrokes bob the face while they play.
fn types(c: AppCategory) -> bool {
    use AppCategory::*;
    matches!(c, Coding | Terminal | Writing | Spreadsheet | Data | Chat | Email)
}

/// Colour of a work theme's symbols.
fn work_color(c: AppCategory) -> Color {
    use AppCategory::*;
    match c {
        Coding => Color::rgb(125, 211, 252),
        Terminal => Color::rgb(74, 222, 128),
        Writing => Color::rgb(251, 191, 36),
        Spreadsheet => Color::rgb(34, 197, 94),
        Slides => Color::rgb(251, 146, 60),
        Planning => Color::rgb(45, 212, 191),
        Data => Color::rgb(167, 139, 250),
        Research => Color::rgb(56, 189, 248),
        Reading => Color::rgb(253, 230, 138),
        Design => Color::rgb(244, 114, 182),
        Chat => Color::rgb(96, 165, 250),
        Email => Color::rgb(147, 197, 253),
        Meeting => Color::rgb(74, 222, 128),
        Learning => Color::rgb(250, 204, 21),
        Media => Color::rgb(248, 113, 113),
        Studio => Color::rgb(251, 113, 133),
    }
}

fn is_birthday(s: &Senses) -> bool {
    s.set.birthday.is_some_and(|(m, d)| s.local.month() == m as u32 && s.local.day() == d as u32)
}

/// Mood ring: hue follows the hour, saturation the water intake, and a
/// busy CPU pulls it toward red.
fn ring_color(s: &Senses) -> Color {
    let hour = s.local.hour() as f32 + s.local.minute() as f32 / 60.0;
    let hue = (hour / 24.0 * 360.0 + 200.0) % 360.0;
    let sat = 0.45 + 0.45 * s.water.clamp(0.0, 1.0);
    let hot = ((s.sys.cpu_pct - 50.0) / 50.0).clamp(0.0, 1.0);
    hsl(hue, sat, 0.6).mix(Color::rgb(255, 80, 80), hot * 0.8)
}

fn radii(style: EyeStyle) -> (f32, f32) {
    match style {
        EyeStyle::Round => (7.0, 7.0),
        EyeStyle::Cat => (8.0, 6.5),
        EyeStyle::Pixel => (6.0, 6.0),
        EyeStyle::Anime => (7.5, 8.5),
        EyeStyle::Robot => (7.5, 5.5),
    }
}

/// Half height of the open eye at normalised column `u` (-1..1).
fn half(style: EyeStyle, u: f32) -> f32 {
    let a = (1.0 - u * u).max(0.0);
    match style {
        EyeStyle::Round | EyeStyle::Anime => a.sqrt(),
        EyeStyle::Cat => a.powf(0.85),
        EyeStyle::Pixel | EyeStyle::Robot => (1.0 - u.abs().powi(6)).max(0.0).powf(1.0 / 6.0),
    }
}

/// Eye opening as vertical spans per column, after the lids cut in.
struct Lid {
    rx: f32,
    ry: f32,
    open: f32,
    angry: f32,
    sad: f32,
    happy: f32,
    side: f32,
    style: EyeStyle,
}

impl Lid {
    fn span(&self, x: f32) -> Option<(f32, f32)> {
        if x.abs() > self.rx {
            return None;
        }
        let u = x / self.rx;
        let h = half(self.style, u) * self.ry;
        let inner = (u * -self.side) * 0.5 + 0.5;
        let outer = 1.0 - inner;
        let c = 1.0 - self.open;
        let lt = -self.ry + c * self.ry + self.angry * self.ry * 0.95 * inner + self.sad * self.ry * 0.85 * outer;
        let lb = self.ry - c * self.ry - self.happy * self.ry * 1.55 * (1.0 - u * u);
        let (t, b) = ((-h).max(lt), h.min(lb));
        (b > t).then_some((t, b))
    }

    fn outline(&self, n: usize) -> Vec<(f32, f32)> {
        let mut tops = Vec::with_capacity(n + 1);
        let mut bots = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let x = -self.rx + 2.0 * self.rx * i as f32 / n as f32;
            if let Some((t, b)) = self.span(x) {
                tops.push((x, t));
                bots.push((x, b));
            }
        }
        bots.reverse();
        tops.extend(bots);
        tops
    }

    fn gap(&self) -> f32 {
        (0..=16)
            .filter_map(|i| self.span(-self.rx + 2.0 * self.rx * i as f32 / 16.0))
            .map(|(t, b)| b - t)
            .fold(0.0, f32::max)
    }
}

/// An ellipse intersected with the lid opening, as a polygon.
fn clip_ellipse(lid: &Lid, cx: f32, cy: f32, rx: f32, ry: f32, n: usize) -> Vec<(f32, f32)> {
    let mut tops = Vec::new();
    let mut bots = Vec::new();
    for i in 0..=n {
        let x = cx - rx + 2.0 * rx * i as f32 / n as f32;
        let du = (x - cx) / rx;
        let hh = ry * (1.0 - du * du).max(0.0).sqrt();
        if let Some((t, b)) = lid.span(x) {
            let (a, c) = ((cy - hh).max(t), (cy + hh).min(b));
            if c > a {
                tops.push((x, a));
                bots.push((x, c));
            }
        }
    }
    bots.reverse();
    tops.extend(bots);
    tops
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * TAU;
            (cx + a.cos() * rx, cy + a.sin() * ry)
        })
        .collect()
}

fn heart(x: f32, y: f32, r: f32) -> Vec<(f32, f32)> {
    (0..20)
        .map(|i| {
            let a = i as f32 / 20.0 * TAU;
            let hx = 16.0 * a.sin().powi(3);
            let hy = -(13.0 * a.cos() - 5.0 * (2.0 * a).cos() - 2.0 * (3.0 * a).cos() - (4.0 * a).cos());
            (x + hx * r / 16.0, y + hy * r / 16.0)
        })
        .collect()
}

fn sparkle(x: f32, y: f32, r: f32) -> Vec<(f32, f32)> {
    (0..8)
        .map(|i| {
            let a = i as f32 / 8.0 * TAU;
            let rr = if i % 2 == 0 { r } else { r * 0.3 };
            (x + a.cos() * rr, y + a.sin() * rr)
        })
        .collect()
}

/// Teardrop pointing up.
fn drop(x: f32, y: f32, r: f32, stretch: f32) -> Vec<(f32, f32)> {
    let mut pts = vec![(x, y - r * 1.8 * stretch)];
    for i in 0..=12 {
        let a = -PI / 6.0 + (PI + PI / 3.0) * i as f32 / 12.0;
        pts.push((x + a.cos() * r, y + a.sin() * r));
    }
    pts
}

fn shades(sc: &mut Scene, xf: &Xf, k: f32) {
    for side in [-1.0f32, 1.0] {
        let ex = side * EYE_X;
        let pts = [(-9.0, -6.0), (9.0, -6.0), (8.0, 3.0), (3.0, 6.5), (-3.0, 6.5), (-8.0, 3.0)];
        let poly: Vec<_> = pts.iter().map(|&(u, v)| xf.p(ex + u, v)).collect();
        let mut rim = poly.clone();
        rim.push(rim[0]);
        sc.poly(poly, Fill::VGrad(Color::rgb(64, 64, 76), Color::rgb(18, 18, 24)));
        sc.polyline(rim, 1.0 * k, WHITE.fade(0.35));
        sc.polyline(vec![xf.p(ex - 6.0, -3.5), xf.p(ex - 2.0, -1.0)], 1.2 * k, WHITE.fade(0.55));
    }
    sc.polyline(vec![xf.p(-6.0, -4.0), xf.p(6.0, -4.0)], 1.6 * k, Color::rgb(140, 140, 150));
}

fn hat(sc: &mut Scene, xf: &Xf, h: Hat, k: f32) {
    let pp = |pts: &[(f32, f32)]| pts.iter().map(|&(x, y)| xf.p(x, y)).collect::<Vec<_>>();
    match h {
        Hat::None => {}
        Hat::Santa => {
            sc.poly(pp(&[(7.0, -10.0), (13.0, -18.0), (21.0, -21.0), (28.0, -19.0), (23.0, -10.0)]), Color::rgb(220, 38, 38));
            sc.polyline(pp(&[(6.0, -9.5), (24.0, -9.5)]), 3.6 * k, WHITE);
            let (cx, cy) = xf.p(28.5, -19.0);
            sc.circle(cx, cy, 2.6 * k, WHITE);
        }
        Hat::Party => {
            sc.poly(pp(&[(-24.0, -10.0), (-7.0, -10.0), (-18.0, -20.0)]), Fill::DGrad(PINK, GOLD));
            sc.polyline(pp(&[(-20.5, -14.0), (-12.5, -14.0)]), 1.2 * k, WHITE.fade(0.8));
            let (cx, cy) = xf.p(-18.0, -20.0);
            sc.circle(cx, cy, 2.0 * k, GOLD);
        }
        Hat::Nightcap => {
            sc.poly(pp(&[(-25.0, -9.0), (-5.0, -9.0), (-9.0, -16.0), (-18.0, -19.0), (-28.0, -15.0), (-33.0, -8.0), (-28.0, -11.0)]), Color::rgb(96, 165, 250));
            sc.polyline(pp(&[(-26.0, -9.0), (-4.0, -9.0)]), 3.0 * k, WHITE);
            let (cx, cy) = xf.p(-33.0, -7.0);
            sc.circle(cx, cy, 2.4 * k, WHITE);
        }
    }
}

fn pumpkin(sc: &mut Scene, (x, y): (f32, f32), k: f32) {
    let orange = Color::rgb(251, 146, 60);
    sc.poly(ellipse(x - 2.2 * k, y, 3.2 * k, 3.8 * k, 16), orange.mix(Color::rgb(0, 0, 0), 0.15));
    sc.poly(ellipse(x + 2.2 * k, y, 3.2 * k, 3.8 * k, 16), orange.mix(Color::rgb(0, 0, 0), 0.15));
    sc.poly(ellipse(x, y, 3.0 * k, 4.0 * k, 16), orange);
    sc.rect(Rect::new(x - 0.7 * k, y - 5.6 * k, 1.4 * k, 2.2 * k), 0.5 * k, Color::rgb(74, 160, 80));
    let dark = Color::rgb(60, 24, 0);
    sc.poly(vec![(x - 2.6 * k, y - 0.4 * k), (x - 1.0 * k, y - 0.4 * k), (x - 1.8 * k, y - 1.8 * k)], dark);
    sc.poly(vec![(x + 1.0 * k, y - 0.4 * k), (x + 2.6 * k, y - 0.4 * k), (x + 1.8 * k, y - 1.8 * k)], dark);
    sc.polyline(vec![(x - 2.2 * k, y + 1.3 * k), (x, y + 2.3 * k), (x + 2.2 * k, y + 1.3 * k)], 0.8 * k, dark);
}

/// Face transform: squash, rotate about the face centre, then offset.
struct Xf {
    cx: f32,
    cy: f32,
    s: f32,
    dx: f32,
    dy: f32,
    cos: f32,
    sin: f32,
    sx: f32,
    sy: f32,
}

impl Xf {
    fn new(cx: f32, cy: f32, s: f32, dx: f32, dy: f32, rot: f32, squash: f32) -> Self {
        Self { cx, cy, s, dx, dy, cos: rot.cos(), sin: rot.sin(), sx: 1.0 + squash, sy: 1.0 - squash }
    }

    fn p(&self, x: f32, y: f32) -> (f32, f32) {
        let (x, y) = (x * self.sx, y * self.sy);
        let (x, y) = (x * self.cos - y * self.sin, x * self.sin + y * self.cos);
        (self.cx + (x + self.dx) * self.s, self.cy + (y + self.dy) * self.s)
    }

    /// Axis-aligned cell (pixel style ignores rotation on purpose).
    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        let (px, py) = self.p(x, y);
        Rect::new(px, py, w * self.s * self.sx, h * self.s * self.sy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    struct World {
        track: Track,
        sys: SystemSnapshot,
        env: Ambient,
        set: Settings,
        local: NaiveDateTime,
        mouse: Option<(f32, f32)>,
        hovering: bool,
        music: bool,
        water: f32,
    }

    impl World {
        fn new() -> Self {
            // A plain Wednesday afternoon: no calendar moods.
            let local = NaiveDate::from_ymd_opt(2026, 6, 10).unwrap().and_hms_opt(15, 30, 0).unwrap();
            Self {
                track: Track::default(),
                sys: SystemSnapshot { cpu_pct: 10.0, mem_used_gb: 4.0, mem_total_gb: 16.0, disk_used_gb: 100.0, disk_total_gb: 500.0, battery_pct: None, charging: false },
                env: Ambient::default(),
                set: Settings::default(),
                local,
                mouse: None,
                hovering: false,
                music: false,
                water: 0.0,
            }
        }

        fn senses(&self) -> Senses<'_> {
            Senses {
                local: self.local,
                visible: true,
                mouse: self.mouse,
                hovering: self.hovering,
                track: &self.track,
                music: self.music,
                focus: None,
                sys: &self.sys,
                water: self.water,
                env: &self.env,
                set: &self.set,
                area: (52.0, 20.0),
                thinking: false,
                focus_left: None,
                app: None,
            }
        }
    }

    fn run(e: &mut Eyes, w: &World, secs: f32) {
        for _ in 0..(secs * 60.0) as usize {
            e.tick(1.0 / 60.0, &w.senses());
        }
    }

    fn settled() -> (Eyes, World) {
        let mut e = Eyes::new(7);
        let w = World::new();
        run(&mut e, &w, 3.0); // play the startup stretch
        (e, w)
    }

    #[test]
    fn starts_with_a_stretch_then_rests() {
        let mut e = Eyes::new(1);
        let w = World::new();
        e.tick(1.0 / 60.0, &w.senses());
        assert_eq!(e.current(), Some(React::Stretch));
        run(&mut e, &w, 3.0);
        assert_eq!(e.mood, Mood::Neutral);
    }

    #[test]
    fn music_makes_it_dance_and_genre_changes_the_move() {
        let (mut e, mut w) = settled();
        w.music = true;
        w.track = Track { title: "Song".into(), artist: "Band".into(), playback: Playback::Playing, duration_ms: 200_000, ..Track::default() };
        run(&mut e, &w, 1.0);
        assert_eq!(e.mood, Mood::Dance);
        w.track.title = "Lofi beats to study to".into();
        run(&mut e, &w, 0.5);
        assert_eq!(e.mood, Mood::Vibe);
        w.track.title = "Heavy Metal Thunder".into();
        run(&mut e, &w, 0.5);
        assert_eq!(e.mood, Mood::Headbang);
    }

    #[test]
    fn real_peaks_drive_beats() {
        let mut b = Beat::new();
        let mut t = 0.0;
        let dt = 1.0 / 60.0;
        let start = b.count;
        for i in 0..240 {
            t += dt;
            // A kick every 30 frames (0.5 s).
            let pk = if i % 30 == 0 { 0.9 } else { 0.1 };
            b.feed(Some(pk), true, t, dt);
        }
        assert!(b.real);
        assert!(b.count - start >= 7, "beats {}", b.count - start);
        assert!((b.period - 0.5).abs() < 0.12, "period {}", b.period);
    }

    #[test]
    fn idling_falls_asleep_and_input_wakes_it() {
        let (mut e, mut w) = settled();
        w.env.idle_ms = 200_000;
        run(&mut e, &w, 0.2);
        assert_eq!(e.mood, Mood::Drowsy);
        w.env.idle_ms = 400_000;
        run(&mut e, &w, 3.0);
        assert_eq!(e.mood, Mood::Asleep);
        assert!(e.parts.iter().any(|p| p.kind == Kind::Z), "snoring z's");
        assert_eq!(e.extras().hat, Hat::Nightcap);
        w.env.idle_ms = 100;
        run(&mut e, &w, 0.1);
        assert_eq!(e.current(), Some(React::Wake));
    }

    #[test]
    fn hover_delights_and_pokes_escalate() {
        let (mut e, mut w) = settled();
        w.hovering = true;
        w.mouse = Some((0.0, 0.0));
        run(&mut e, &w, 0.2);
        assert_eq!(e.mood, Mood::Delight);
        w.hovering = false;
        w.mouse = None;
        for _ in 0..5 {
            e.poke();
        }
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Annoyed));
        run(&mut e, &w, 2.0);
        assert_eq!(e.current(), Some(React::Dizzy));
    }

    #[test]
    fn rubbing_back_and_forth_is_petting() {
        let (mut e, mut w) = settled();
        for i in 0..60 {
            let x = if (i / 6) % 2 == 0 { -30.0 + (i % 6) as f32 * 10.0 } else { 30.0 - (i % 6) as f32 * 10.0 };
            w.mouse = Some((x, 25.0));
            e.tick(1.0 / 60.0, &w.senses());
        }
        assert!(e.petting());
        assert_eq!(e.mood, Mood::Purr);
    }

    #[test]
    fn staring_contest_is_lost_eventually() {
        let (mut e, mut w) = settled();
        w.mouse = Some((30.0, 40.0));
        run(&mut e, &w, 3.0);
        assert_eq!(e.mood, Mood::Stare);
        assert_eq!(e.blink, 0.0, "no blinking during the contest");
        run(&mut e, &w, 5.0);
        assert!(e.current() == Some(React::Defeated) || e.queue.iter().any(|(r, _)| *r == React::Defeated));
    }

    #[test]
    fn pointer_right_under_the_nose_crosses_the_eyes() {
        let (mut e, mut w) = settled();
        w.mouse = Some((0.0, 20.0));
        run(&mut e, &w, 0.5);
        assert_eq!(e.mood, Mood::Cross);
        assert!(e.pose().cross > 0.8);
    }

    #[test]
    fn track_changes_react() {
        let (mut e, mut w) = settled();
        w.music = true;
        w.track = Track { title: "A".into(), artist: "X".into(), playback: Playback::Playing, duration_ms: 200_000, position_ms: 1_000, ..Track::default() };
        // The first song is a surprise; let it play out.
        run(&mut e, &w, 1.0);
        // Skipped after a few seconds: eye roll.
        w.track.title = "B".into();
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::EyeRoll));
        run(&mut e, &w, 2.0);
        // Paused mid-dance: record scratch.
        w.track.playback = Playback::Paused;
        w.music = false;
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Scratch));
        // Paused while not dancing (resumed but face busy elsewhere): huh?
        run(&mut e, &w, 2.0);
        w.track.playback = Playback::Playing;
        w.hovering = true;
        run(&mut e, &w, 0.5);
        w.track.playback = Playback::Paused;
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Huh));
    }

    #[test]
    fn system_signals_pick_moods() {
        let (mut e, mut w) = settled();
        w.env.offline = true;
        run(&mut e, &w, 0.1);
        assert_eq!(e.mood, Mood::Lost);
        w.env.offline = false;
        w.env.camera = true;
        run(&mut e, &w, 0.1);
        assert_eq!(e.mood, Mood::Watchful);
        assert!(e.extras().rec.is_some());
        w.env.camera = false;
        w.sys.cpu_pct = 97.0;
        run(&mut e, &w, 0.1);
        assert_eq!(e.mood, Mood::Sweaty);
        w.sys.cpu_pct = 10.0;
        w.sys.battery_pct = Some(12);
        run(&mut e, &w, 0.1);
        assert_eq!(e.mood, Mood::Tired);
        w.env.quiet = true;
        run(&mut e, &w, 0.1);
        assert_eq!(e.mood, Mood::Hush);
    }

    #[test]
    fn calendar_moods_and_props() {
        let (mut e, mut w) = settled();
        w.local = NaiveDate::from_ymd_opt(2026, 12, 18).unwrap().and_hms_opt(12, 15, 0).unwrap();
        run(&mut e, &w, 0.1);
        assert_eq!(e.mood, Mood::Hungry);
        assert_eq!(e.extras().hat, Hat::Santa);
        w.local = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap().and_hms_opt(0, 20, 0).unwrap();
        run(&mut e, &w, 0.1);
        assert_eq!(e.mood, Mood::Spooky);
        assert!(e.extras().pumpkin);
        w.set.birthday = Some((10, 30));
        run(&mut e, &w, 0.1);
        assert_eq!(e.extras().hat, Hat::Party);
    }

    #[test]
    fn weather_is_opt_in() {
        let (mut e, mut w) = settled();
        w.env.weather = Weather::Rain;
        run(&mut e, &w, 0.1);
        assert_eq!(e.mood, Mood::Neutral);
        w.set.weather_mood = true;
        run(&mut e, &w, 0.5);
        assert_eq!(e.mood, Mood::Gloomy);
        assert!(e.parts.iter().any(|p| p.kind == Kind::Rain));
    }

    #[test]
    fn hidden_face_keeps_reactions_for_later() {
        let (mut e, w) = settled();
        e.react(React::Gulp);
        let mut s = w.senses();
        s.visible = false;
        for _ in 0..30 {
            e.tick(1.0 / 60.0, &s);
        }
        assert_eq!(e.current(), None);
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Gulp));
    }

    #[test]
    fn every_style_and_glyph_draws_inside_bounds() {
        let (mut e, w) = settled();
        let bounds = Rect::new(20.0, 0.0, 128.0, 40.0);
        let reacts = [None, Some(React::Love), Some(React::Sparkle), Some(React::Dizzy), Some(React::Shocked), Some(React::Flash), Some(React::Emote(Glyph::Question)), Some(React::Emote(Glyph::Cross))];
        for style in EyeStyle::ALL {
            for r in reacts {
                if let Some(r) = r {
                    e.start(r);
                }
                run(&mut e, &w, 0.3);
                let mut sc = Scene::new();
                e.draw(&mut sc, &Paint { cx: bounds.cx(), cy: bounds.cy(), scale: 1.0, bounds, style, tint: None, accent: None, props: true });
                assert!(!sc.cmds.is_empty(), "{style:?} {r:?} drew nothing");
            }
        }
    }

    #[test]
    fn lids_close_into_a_line_and_happy_lids_curve_up() {
        let open = Lid { rx: 7.0, ry: 7.0, open: 1.0, angry: 0.0, sad: 0.0, happy: 0.0, side: 1.0, style: EyeStyle::Round };
        assert!((open.gap() - 14.0).abs() < 0.2);
        let shut = Lid { open: 0.0, ..open };
        assert!(shut.gap() < 0.01);
        let happy = Lid { happy: 1.0, ..open };
        let (_, b) = happy.span(0.0).unwrap();
        assert!(b < 0.0, "happy lower lid rises above the centre");
        let angry = Lid { angry: 1.0, ..open };
        let (inner_top, _) = angry.span(-6.0).unwrap();
        let (outer_top, _) = angry.span(6.0).unwrap();
        assert!(inner_top > outer_top, "inner corner (toward the face centre) droops");
    }

    #[test]
    fn particles_stay_bounded() {
        let (mut e, mut w) = settled();
        w.local = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap().and_hms_opt(15, 0, 0).unwrap();
        w.set.birthday = Some((1, 1));
        run(&mut e, &w, 20.0);
        assert!(e.parts.len() <= 48);
        assert!(e.parts.iter().any(|p| p.kind == Kind::Firework) || e.parts.iter().any(|p| p.kind == Kind::Confetti));
    }

    #[test]
    fn dancing_cycles_through_moves() {
        let (mut e, mut w) = settled();
        w.music = true;
        w.track = Track { title: "Song".into(), artist: "Band".into(), playback: Playback::Playing, duration_ms: 200_000, ..Track::default() };
        let mut seen = Vec::new();
        for _ in 0..90 * 4 {
            run(&mut e, &w, 0.25);
            if !seen.contains(&e.dance_move()) {
                seen.push(e.dance_move());
            }
        }
        assert!(seen.len() >= 4, "only {seen:?}");
        // Headbang only picks from its own moves.
        w.track.title = "Heavy Metal Thunder".into();
        run(&mut e, &w, 3.0);
        assert!(Move::pool(Mood::Headbang, 0.5).iter().any(|&(m, _)| m == e.dance_move()));
    }

    #[test]
    fn volume_gauge_tracks_the_level_and_lets_go() {
        let (mut e, w) = settled();
        e.gauge(Gauge::Volume, 0.2, false);
        run(&mut e, &w, 0.6);
        let quiet = e.pose().size;
        e.gauge(Gauge::Volume, 0.7, false);
        assert!(e.parts.iter().any(|p| p.kind == Kind::Wave), "turning it up sends sound waves");
        run(&mut e, &w, 0.6);
        assert!(e.pose().size > quiet + 0.1, "eyes grow with the volume");
        assert!(e.pose().look.0 < 0.0, "eyes watch the bar on the left");
        e.gauge(Gauge::Volume, 0.7, true);
        run(&mut e, &w, 0.8);
        assert!(e.pose().open < 0.1, "muted clamps the eyes shut");
        run(&mut e, &w, GAUGE_HOLD + 0.5);
        assert!(e.live_gauge().is_none());
        assert!(e.pose().open > 0.8, "back to the mood once the HUD is gone");
    }

    #[test]
    fn brightness_moves_the_pupils() {
        let (mut e, w) = settled();
        e.gauge(Gauge::Brightness, 0.05, false);
        run(&mut e, &w, 0.6);
        let dark = e.pose().pupil;
        e.gauge(Gauge::Brightness, 1.0, false);
        run(&mut e, &w, 0.6);
        assert!(dark > e.pose().pupil + 0.5, "pupils dilate in the dark");
        assert!(e.extras().shades);
    }

    #[test]
    fn reactions_wait_out_a_gauge() {
        let (mut e, w) = settled();
        e.gauge(Gauge::Volume, 0.5, false);
        e.react(React::Gulp);
        run(&mut e, &w, 1.0);
        assert_eq!(e.current(), None);
        run(&mut e, &w, GAUGE_HOLD);
        assert_eq!(e.current(), Some(React::Gulp));
    }

    #[test]
    fn notes_written_while_open_play_after_collapse() {
        let (mut e, w) = settled();
        e.noted(400);
        let mut s = w.senses();
        s.visible = false;
        // A long writing session: well past the usual 25 s staleness.
        for _ in 0..60 * 40 {
            e.tick(1.0 / 60.0, &s);
        }
        e.island_closed();
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Noted));
        assert_eq!(e.parts.iter().filter(|p| p.kind == Kind::Page).count(), 3);
    }

    #[test]
    fn jiggle_overshoots_then_settles() {
        let mut j = Jiggle::default();
        j.hop(60.0);
        let mut low = 0.0f32;
        let mut high = 0.0f32;
        for _ in 0..120 {
            j.step(1.0 / 60.0);
            low = low.min(j.y);
            high = high.max(j.y);
        }
        assert!(low < -0.5 && high > 0.0, "goes up and overshoots back down");
        assert!(j.y.abs() < 0.05 && j.sq.abs() < 0.01);
    }

    #[test]
    fn hud_face_draws_with_new_particles() {
        let (mut e, w) = settled();
        let body = Rect::new(20.0, 0.0, 310.0, 38.0);
        let (cx, cy, scale) = place(Spot::Hud, body);
        assert!(cx < body.right() && cx > body.x);
        e.gauge(Gauge::Volume, 0.4, false);
        e.gauge(Gauge::Volume, 0.95, false);
        e.noted(10);
        run(&mut e, &w, 0.2);
        e.start(React::Noted);
        run(&mut e, &w, 0.2);
        let mut sc = Scene::new();
        e.draw(&mut sc, &Paint { cx, cy, scale, bounds: body, style: EyeStyle::Round, tint: None, accent: None, props: false });
        assert!(!sc.cmds.is_empty());
    }

    #[test]
    fn keystrokes_make_it_watch_the_keyboard() {
        let (mut e, mut w) = settled();
        w.mouse = Some((200.0, 300.0));
        // A key every 3 frames: input since last frame, pointer still.
        for i in 0..120 {
            w.env.idle_ms = if i % 3 == 0 { 5 } else { 5 + (i % 3) as u64 * 16 };
            e.tick(1.0 / 60.0, &w.senses());
        }
        assert_eq!(e.mood, Mood::Typing);
        assert!(e.pose().look.1 > 0.5, "eyes on the keyboard");
        w.env.idle_ms = 4_000;
        run(&mut e, &w, 2.5);
        assert_ne!(e.mood, Mood::Typing);
    }

    #[test]
    fn skip_button_swipes_once() {
        let (mut e, mut w) = settled();
        w.music = true;
        w.track = Track { title: "A".into(), artist: "X".into(), playback: Playback::Playing, duration_ms: 200_000, ..Track::default() };
        run(&mut e, &w, 30.0);
        e.skip(false);
        w.track.title = "B".into();
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Swipe(false)));
        assert!(e.queue.is_empty(), "the track change it caused stays quiet");
    }

    #[test]
    fn wheel_spins_the_pupils_and_too_much_gets_dizzy() {
        let (mut e, w) = settled();
        e.scrolled(1);
        run(&mut e, &w, 0.3);
        let a = e.pose().look;
        e.scrolled(1);
        e.scrolled(1);
        run(&mut e, &w, 0.3);
        assert!((e.pose().look.0 - a.0).abs() + (e.pose().look.1 - a.1).abs() > 0.3, "pupils moved round");
        for _ in 0..12 {
            e.scrolled(-1);
        }
        assert!(e.queue.iter().any(|(r, _)| *r == React::Dizzy));
    }

    #[test]
    fn long_hover_giggles_and_leaving_pouts() {
        let (mut e, mut w) = settled();
        w.hovering = true;
        w.mouse = Some((10.0, 0.0));
        run(&mut e, &w, 2.7);
        assert_eq!(e.current(), Some(React::Giggle));
        run(&mut e, &w, 2.0);
        w.hovering = false;
        w.mouse = Some((300.0, 0.0));
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Pout));
    }

    #[test]
    fn caps_lock_and_battery_edges() {
        let (mut e, mut w) = settled();
        w.env.caps = true;
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Shout));
        run(&mut e, &w, 2.0);
        w.env.caps = false;
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Sigh));
        run(&mut e, &w, 3.0);
        w.sys.battery_pct = Some(11);
        run(&mut e, &w, 0.1);
        w.sys.battery_pct = Some(10);
        run(&mut e, &w, 0.05);
        assert_eq!(e.current(), Some(React::Faint));
        run(&mut e, &w, 3.0);
        // A tired face may be mid-yawn: the reaction plays or waits its turn.
        let fired = |e: &Eyes, r: React| e.current() == Some(r) || e.queue.iter().any(|(q, _)| *q == r);
        w.sys.charging = true;
        run(&mut e, &w, 0.05);
        assert!(fired(&e, React::Love));
        run(&mut e, &w, 4.0);
        w.sys.charging = false;
        run(&mut e, &w, 0.05);
        assert!(fired(&e, React::Gasp));
    }

    #[test]
    fn glance_follows_a_window_switch() {
        let (mut e, w) = settled();
        e.glance(-1.0, 0.8);
        run(&mut e, &w, 0.3);
        assert!(e.pose().look.0 < -0.7);
        run(&mut e, &w, 1.0);
        assert!(e.glance.is_none());
    }

    #[test]
    fn focus_countdown_and_thinking() {
        let (mut e, mut w) = settled();
        let mut s = w.senses();
        s.focus = Some(Phase::Focus);
        s.focus_left = Some(5.0);
        for _ in 0..20 {
            e.tick(1.0 / 60.0, &s);
        }
        assert!(e.pose().size > 1.05 && e.pose().look.0.abs() > 0.6, "wide eyes ticking side to side");
        w.mouse = None;
        let mut s = w.senses();
        s.thinking = true;
        for _ in 0..90 {
            e.tick(1.0 / 60.0, &s);
        }
        assert_eq!(e.mood, Mood::Thinking);
        assert!(e.pose().look.1 < -0.4, "looking up while it thinks");
        assert!(e.parts.iter().any(|p| p.kind == Kind::Dot));
    }

    fn work_senses(w: &World, cat: AppCategory) -> Senses<'_> {
        let mut s = w.senses();
        s.focus = Some(Phase::Focus);
        s.app = Some(AppTheme { cat, tint: None });
        s
    }

    #[test]
    fn focus_acts_out_the_app_in_front() {
        let (mut e, w) = settled();
        let bounds = Rect::new(20.0, 0.0, 128.0, 40.0);
        let mut looks = Vec::new();
        for cat in AppCategory::ALL {
            let s = work_senses(&w, cat);
            e.parts.clear();
            let mut effect = false;
            for _ in 0..(6.0 * 60.0) as usize {
                e.tick(1.0 / 60.0, &s);
                // Every theme puts something on screen besides the eyes.
                effect |= !e.parts.is_empty() || e.extras().rec.is_some() || e.extras().screen.is_some();
            }
            assert_eq!(e.mood, Mood::Work(cat));
            assert!(effect, "{cat:?} has no effect");
            assert!(e.parts.len() <= 48);
            looks.push((cat, e.pose().look));
            for style in EyeStyle::ALL {
                let mut sc = Scene::new();
                e.draw(&mut sc, &Paint { cx: bounds.cx(), cy: bounds.cy(), scale: 1.0, bounds, style, tint: None, accent: None, props: true });
                assert!(!sc.cmds.is_empty(), "{cat:?} {style:?} drew nothing");
            }
        }
        // A pause or a break drops the theme; no app keeps the old look.
        let mut s = work_senses(&w, AppCategory::Coding);
        s.focus = Some(Phase::ShortBreak);
        e.tick(1.0 / 60.0, &s);
        assert_eq!(e.mood, Mood::Relaxed);
        s.focus = Some(Phase::Focus);
        s.app = None;
        e.tick(1.0 / 60.0, &s);
        assert_eq!(e.mood, Mood::Determined);
    }

    #[test]
    fn work_themes_move_the_gaze_differently() {
        // Sample each theme's gaze over a few seconds: the paths must differ.
        let w = World::new();
        let trace = |cat: AppCategory| {
            let mut e = Eyes::new(3);
            let s = work_senses(&w, cat);
            let mut pts = Vec::new();
            for i in 0..(8.0 * 60.0) as usize {
                e.tick(1.0 / 60.0, &s);
                if i % 30 == 0 {
                    pts.push(e.pose().look);
                }
            }
            pts
        };
        let a = trace(AppCategory::Coding);
        let b = trace(AppCategory::Media);
        let moved = |p: &[(f32, f32)]| p.windows(2).map(|w| (w[1].0 - w[0].0).abs() + (w[1].1 - w[0].1).abs()).sum::<f32>();
        assert!(moved(&a) > moved(&b) * 3.0, "reading code roams more than watching a video");
        let d = trace(AppCategory::Data);
        assert_ne!(a, d);
    }

    #[test]
    fn switching_apps_hops_and_tints() {
        let (mut e, w) = settled();
        let s = work_senses(&w, AppCategory::Writing);
        for _ in 0..60 {
            e.tick(1.0 / 60.0, &s);
        }
        let mut s = work_senses(&w, AppCategory::Planning);
        s.app = Some(AppTheme { cat: AppCategory::Planning, tint: Some([34, 197, 94]) });
        e.tick(1.0 / 60.0, &s);
        assert!(e.jig.vy < 0.0, "hops into the new role");
        assert_eq!(e.work.and_then(|w| w.tint), Some([34, 197, 94]));
        for _ in 0..(3.0 * 60.0) as usize {
            e.tick(1.0 / 60.0, &s);
        }
        assert!(e.parts.iter().any(|p| p.kind == Kind::Sym("\u{2713}")), "ticks items off");
        // Typing in a keyboard-heavy theme bobs the face.
        let mut w2 = World::new();
        w2.mouse = Some((300.0, 0.0));
        let mut e = Eyes::new(9);
        for i in 0..120u64 {
            w2.env.idle_ms = if i % 6 == 0 { 0 } else { 50 + i };
            e.tick(1.0 / 60.0, &work_senses(&w2, AppCategory::Coding));
        }
        assert!(e.keys.len() >= 5);
    }

    #[test]
    fn every_new_reaction_plays_and_draws() {
        let all = [
            React::Swipe(true),
            React::Swipe(false),
            React::Scratch,
            React::Celebrate,
            React::Oops,
            React::Sweep,
            React::Gasp,
            React::Salute,
            React::Zoom,
            React::Nod,
            React::Shake,
            React::Shout,
            React::Faint,
            React::Giggle,
            React::Pout,
            React::Noted,
            React::Poof,
        ];
        let (mut e, w) = settled();
        let bounds = Rect::new(20.0, 0.0, 128.0, 40.0);
        for r in all {
            e.start(r);
            let rest = *e.pose();
            for step in 0..((r.duration() * 60.0) as usize) {
                e.tick(1.0 / 60.0, &w.senses());
                if step % 15 == 0 {
                    let mut sc = Scene::new();
                    e.draw(&mut sc, &Paint { cx: bounds.cx(), cy: bounds.cy(), scale: 1.0, bounds, style: EyeStyle::Round, tint: None, accent: None, props: true });
                    assert!(!sc.cmds.is_empty(), "{r:?}");
                }
            }
            assert!(e.pose() != &rest || e.jig != Jiggle::default(), "{r:?} did nothing");
            run(&mut e, &w, 0.5);
        }
    }
}
