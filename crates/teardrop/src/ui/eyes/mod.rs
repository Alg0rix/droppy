//! Idle eyes: a tiny expression engine for the collapsed island.
//!
//! Three layers, each cheap to extend:
//! - [`Mood`]: a lasting state re-picked every frame from what is going on
//!   (music, idle time, clock, battery, network, the pointer...). Adding one
//!   is an enum arm, a line in `pick_mood` and a pose in `compose`.
//! - [`React`]: a short one-off played over the mood (surprise, wink,
//!   sneeze...). Window events queue them with [`Eyes::react`]; anything
//!   queued while the face is hidden plays once it shows again.
//! - [`Mood::Work`]: the face acts out the kind of app in front
//!   (`teardrop_core::apps`): one pose and one ambient effect per
//!   [`AppCategory`], in `work_pose` / `work_parts`. Ambient pick below the
//!   real states, promoted above them while a Focus block runs.
//! - [`Pose`]: plain numbers the painter draws. Moods and reactions only
//!   produce a target pose; the brain eases toward it and layers blinks,
//!   gaze and particles on top.
//! - [`Prop`]: a held item for the object skits (a laptop, a book, a mug,
//!   a guitar...), drawn under the face and animated by `Extras::prop_k`.
//!
//! Pure data like the rest of `ui`: no Windows calls, unit-tested anywhere.

use super::scene::{Align, Color, Fill, Rect, Scene, Weight};
use super::theme::{hsl, pal};
use crate::viewmodel::{Ambient, Weather};
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike, Weekday};
use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use teardrop_core::pomodoro::Phase;
use teardrop_core::{AppCategory, AppTheme, EyeStyle, Playback, Settings, SystemSnapshot, Track};

mod api;
mod bits;
mod paint;
mod parts;
mod pose;
mod props;
mod senses;
mod skits;

use bits::*;
pub use paint::*;
use props::*;

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
    /// Focus ring shares the pill with the music capsule: the face sits
    /// between the visualizer and the countdown.
    FocusMusic,
    /// Rides at the right end of a level HUD (volume, brightness) and
    /// reacts to the level live.
    Hud,
    /// Sits between the REC label and the timer while recording; listens to the mic.
    Rec,
}

/// Face centre + scale for a spot inside the collapsed body rect.
pub fn place(spot: Spot, body: Rect) -> (f32, f32, f32) {
    match spot {
        Spot::Idle => (body.cx(), body.cy() + 1.0, 1.12),
        Spot::Music => (body.right() - 31.0, body.cy(), 0.88),
        Spot::Focus => (body.x + 66.0, body.cy(), 0.84),
        Spot::FocusMusic => (body.right() - 95.0, body.cy(), 0.84),
        Spot::Hud => (body.right() - 26.0, body.cy(), 0.72),
        Spot::Rec => (body.x + 104.0, body.cy(), 0.76),
    }
}

/// Half extents (face units) particles and peeks may use around the face.
pub fn area(spot: Spot, body: Rect) -> (f32, f32) {
    match spot {
        Spot::Idle => ((body.w / 2.0 - 12.0).max(24.0), body.h / 2.0),
        Spot::Hud => (26.0, body.h / 2.0 / 0.62),
        Spot::Rec => (26.0, body.h / 2.0 / 0.66),
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
    /// What the app in front is themed as.
    pub app: Option<AppTheme>,
    /// Mic level 0..1 while voice recording runs.
    pub recording: Option<f32>,
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
    /// Acting out the kind of app in front.
    Work(AppCategory),
    /// Recording: ear cocked toward the mic, eyes pulse with your voice.
    Listening,
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
    /// `- -` flat and unimpressed.
    Minus,
    /// Crescent moons.
    Moon,
    /// Lightning bolts.
    Bolt,
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
    /// A full spin every beat, alternating direction.
    Twirl,
    /// Big slow swings left to right.
    Pendulum,
    /// A full-body buzz on the beat.
    Vibrate,
    /// Sharp sideways kicks on the beat.
    Kick,
}

impl Move {
    /// Moves a mood can pick from, with a weight each. High energy favours
    /// the bigger moves.
    fn pool(mood: Mood, energy: f32) -> &'static [(Move, u32)] {
        let hot = energy > 0.45;
        match mood {
            Mood::Headbang => &[(Move::Bang, 4), (Move::SideBang, 3), (Move::Thrash, 2), (Move::Nod, 2), (Move::Kick, 2), (Move::Hop, 1)],
            Mood::Vibe => &[(Move::Drift, 4), (Move::Float, 3), (Move::Lean, 3), (Move::Pendulum, 3), (Move::Nod, 2), (Move::Groove, 2)],
            _ if hot => &[
                (Move::Bounce, 3),
                (Move::Hop, 3),
                (Move::Shuffle, 3),
                (Move::Twist, 2),
                (Move::Pump, 2),
                (Move::LookAround, 2),
                (Move::Twirl, 2),
                (Move::Kick, 2),
                (Move::Vibrate, 2),
                (Move::WinkStep, 1),
                (Move::Sway, 1),
            ],
            _ => &[
                (Move::Sway, 3),
                (Move::Groove, 3),
                (Move::Pendulum, 2),
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
    // Object skits: a held prop, a little routine, an ending.
    /// Flips the laptop open, types a burst, closes it proud.
    Laptop,
    /// Opens a book, reads a few lines, flips the page.
    Read,
    /// Doomscrolls the phone and ends a little dizzy.
    Phone,
    /// A hot sip: steam, a long drink, an ahh.
    Coffee,
    /// Munches a cookie, crumbs everywhere.
    Snack,
    /// Button-mashes a game and wins.
    Game,
    /// Strums a chord; notes float up.
    Guitar,
    /// Belts a note into the mic.
    Sing,
    /// Deadlifts a dumbbell: squat, strain, hold, flex.
    Lift,
    /// Waves a wand: sparks, star eyes, smoke.
    Magic,
    /// Raises a camera, FLASH, checks the shot.
    Photo,
    /// Watches a sprout grow and loves it.
    Plant,
    /// Paints strokes in the air; flecks fly.
    Paint,
    /// Umbrella pops open; rain taps off it.
    Umbrella,
    /// A balloon lifts it, then pops.
    Balloon,
    /// Squeezes the teddy; hearts float.
    Hug,
    // Stage bits: no prop, just the face and a gimmick.
    /// Slow suspicious side-eye, left then right.
    SideEye,
    /// Eyebrows wiggle one at a time.
    BrowWiggle,
    /// Narrows to slits, leans in, decides it's fine.
    Squint,
    /// Nods off twice, snaps awake.
    Doze,
    /// Eyes closed, levitates, glows faintly.
    Meditate,
    /// Cold: rattles in a frosty glow.
    Shiver,
    /// Overheated: panting mouth, sweat.
    Hot,
    /// A micro-nap and a jolt awake.
    Nap,
    /// Scans the horizon twice through narrowed eyes.
    Spy,
    /// Monocle inspect: one huge eye roaming low.
    Inspect,
    /// A full spin, lands wobbly.
    Spin,
    /// Glides one way while looking the other.
    Moonwalk,
    /// Robot mode: stepped snaps and a cyan glow.
    Beep,
    /// Enthusiastic hello bounces.
    Wave,
    /// Blows a bubble until it pops.
    BubbleGum,
    /// Blows a kiss; the heart drifts off.
    Kiss,
    /// A huffy grumble with an anger vein.
    Grumble,
    /// Ear tilted to listen, then a `!`.
    Eavesdrop,
    /// Ding: a lightbulb moment.
    Idea,
    /// Flat slump and a slow head-shake.
    Facepalm,
    /// Full-body cringe, then one eye opens.
    Cringe,
    /// Cheering: alternating winks and hops.
    Cheer,
    /// Marches in place, left right left.
    March,
    /// Pogo hops that keep getting bigger.
    Pogo,
    /// Poofs small, darts across, reappears smug.
    Ninja,
    /// Weightless drift with a slow roll.
    ZeroG,
    /// Hops into a puddle; drops fly.
    Splash,
    /// Tracks a firefly and catches it.
    Catch,
    /// Shades on, furious typing, code flies.
    Hack,
    /// Juggles three throws with its eyes.
    Juggle,
    /// Chest out, chin up: very proud.
    Proud,
    /// Eyes covered... BOO!
    PeekABoo,
    /// Counts on imaginary fingers, then has it.
    Count,
    /// Waiting: taps a foot, checks the time.
    Wait,
    /// Psyched up: pumping and shaking it off.
    Hype,
    /// Two little sniffs, then suspicious.
    Sniff,
    /// One slow deep breath, in and out.
    Breathe,
    /// Rubbing tired eyes.
    Rub,
    /// Overheats with anger, then deflates.
    Rage,
    /// Bobblehead nodding.
    Bobble,
    /// Glued to a flickering screen.
    Binge,
    /// Gazes up at the stars in awe.
    Stargaze,
    /// Checks itself out and approves.
    Mirror,
    /// Sunbathes behind shades.
    Bask,
    /// Raises a glass with a little clink.
    Toast,
    // Second batch of object skits.
    /// Drumroll, then a crash.
    Drum,
    /// Telescope out, scans the sky, spots something.
    Scope,
    /// Broadsheet open, down the columns, folded away.
    News,
    /// Pulls a slice off the pizza, cheese and all.
    Pizza,
    /// Shakes the dice, rolls, sweats the result.
    Dice,
    /// Kite up high, tugging the string.
    Kite,
    /// Tunes the radio through static to a station.
    Radio,
    /// Lantern lit, searching the dark.
    Lantern,
    // Second batch of stage bits.
    /// Sneaks across on tiptoe.
    Tiptoe,
    /// Jumping jacks.
    Workout,
    /// A few strokes of freestyle, bubbles trailing.
    Swim,
    /// Two quick spins, lands wobbly.
    Pirouette,
    /// Drifts off into a daydream bubble.
    Daydream,
    /// Whistles a little tune.
    Whistle,
    /// Hums contentedly to itself.
    Hum,
    /// Squints and measures something twice.
    Measure,
    /// Tightrope walk: wobble, correct, wobble.
    Balance,
    /// Nervous darts and a gulp.
    Nervous,
    /// Turned away, sulking, glances back once.
    Sulk,
    /// Puppy eyes, pleading hard.
    Puppy,
    /// Scheming: brows down, darting looks, delighted shiver.
    Scheme,
    /// Ta-da! A flourish and a bow.
    TaDa,
    /// Takes a deep theatrical bow.
    Bow,
    /// Two quick fist pumps.
    FistPump,
    /// A forward roll across the island.
    Tumble,
    /// A backflip: up, over, stuck landing.
    Flip,
    /// Skating strides, gliding side to side.
    Skate,
    /// Scans a document like a photocopier, line by line.
    Scan,
    /// Charging up: trembling, glow rising, zap.
    PowerUp,
    /// Signal glitch: pose flickers and jitters.
    Glitch,
    /// A compass needle spinning then settling.
    Compass,
    /// Radar sweep: one smooth circle, a blip.
    Radar,
    /// Metronome: perfectly even ticks left and right.
    Metronome,
    /// Morse code: dot-dot-dash blinks.
    Morse,
    /// Rocking in a hammock, eyes shut.
    Hammock,
    /// Tosses popcorn and catches it.
    Popcorn,
    /// A card trick: here, here... there!
    CardTrick,
    /// A gust of wind pushes it sideways.
    Breeze,
    /// Mime in a box: bonks each wall.
    Trapped,
    /// Folds a page into shape, proud crane.
    Origami,
    /// Breathes on its hands and rubs them warm.
    WarmHands,
    /// Stares into a fire, warm flicker on its face.
    Fireplace,
    /// Watches a storm, flinches at the flash.
    Storm,
    /// Northern lights shimmer over its face.
    Aurora,
    /// Dribbles a ball, eyes on every bounce.
    Dribble,
    /// Jogs in place, working up a sweat.
    Jog,
    /// Rows steady strokes, eyes on the horizon.
    Row,
    // Third batch of stage bits.
    /// Yodels: head back, mouth wide, notes everywhere.
    Yodel,
    /// A sneeze builds... and gets stifled.
    AhChoo,
    /// Wobbles like jelly.
    Jelly,
    /// Watching clouds drift by.
    Cloudwatch,
    /// Pops into frame from the edge, grinning.
    Photobomb,
    /// Ducks out of sight, pops back up.
    HideSeek,
    /// Strikes poses, left and right.
    Vogue,
    /// Flexes, both sides.
    Flex,
    /// Skip rope: steady hops, rope whipping round.
    JumpRope,
    /// Skipping down the street.
    Skip,
    /// A slow, graceful plié.
    Plie,
    /// Hi-ya: two sharp jabs and a scowl.
    Karate,
    /// Sumo stomps, wide and heavy.
    Sumo,
    /// En garde: quick fencing lunges.
    Fencing,
    /// Draws a bow, aims, looses.
    Archery,
    /// Aims a dart, throws, watches it land.
    Darts,
    /// Swings and releases; watches the ball roll in.
    Bowling,
    /// Reads the green, putts, watches it drop.
    Golf,
    /// Casts a line, waits, gets a bite.
    Fishing,
    /// Rides a wave, knees bent, arms out.
    Surf,
    /// Downhill ski: tucked low, bumping over moguls.
    Ski,
    /// Blows a pinwheel spinning.
    Pinwheel,
    /// Blows a stream of bubbles.
    BubbleWand,
    /// Polishes itself to a shine.
    Polish,
    /// A flashlight beam sweeping the dark.
    Flashlight,
    /// Shushes the room, sternly.
    Shush,
    /// The slow sarcastic clap.
    SlowClap,
    /// A fidget spinner gaze, round and round.
    Fidget,
    /// Flips a coin, catches it, peeks at the result.
    Coin,
    /// Seals and posts a letter.
    Letter,
    /// Holds the phone up for a duck-face selfie.
    Selfie,
    /// Toasts a marshmallow over the fire.
    Marshmallow,
    /// A shooting star streaks past; it tracks it, awed.
    Meteor,
    /// A moth flutters by and it can't look away.
    Moth,
    /// Coils down and sproings way up.
    Spring,
    /// Walks the dog on a yoyo.
    YoYo,
    /// Pulls a Jenga block, holds its breath.
    Jenga,
    /// Watches the dominoes topple in a chain.
    Dominoes,
    /// Shadow boxes: duck, weave, jab.
    Box,
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
            Laptop => 3.2,
            Read => 3.4,
            Phone => 2.8,
            Coffee => 2.6,
            Snack => 2.4,
            Game => 2.6,
            Guitar => 2.8,
            Sing => 2.6,
            Lift => 2.6,
            Magic => 2.4,
            Photo => 1.8,
            Plant => 2.8,
            Paint => 2.8,
            Umbrella => 2.6,
            Balloon => 2.8,
            Hug => 2.2,
            SideEye => 1.6,
            BrowWiggle => 1.6,
            Squint => 1.8,
            Doze => 3.0,
            Meditate => 3.2,
            Shiver => 1.8,
            Hot => 2.0,
            Nap => 2.4,
            Spy => 2.8,
            Inspect => 2.2,
            Spin => 1.6,
            Moonwalk => 1.8,
            Beep => 1.8,
            Wave => 1.4,
            BubbleGum => 2.6,
            Kiss => 1.4,
            Grumble => 1.8,
            Eavesdrop => 2.4,
            Idea => 1.8,
            Facepalm => 2.0,
            Cringe => 1.6,
            Cheer => 2.0,
            March => 1.8,
            Pogo => 2.0,
            Ninja => 2.0,
            ZeroG => 3.0,
            Splash => 1.6,
            Catch => 2.8,
            Hack => 2.4,
            Juggle => 2.6,
            Proud => 1.6,
            PeekABoo => 1.8,
            Count => 2.6,
            Wait => 2.8,
            Hype => 1.8,
            Sniff => 1.8,
            Breathe => 3.0,
            Rub => 2.0,
            Rage => 2.2,
            Bobble => 1.8,
            Binge => 3.0,
            Stargaze => 3.0,
            Mirror => 2.0,
            Bask => 2.4,
            Toast => 1.6,
            Drum => 2.6,
            Scope => 2.8,
            News => 3.2,
            Pizza => 2.4,
            Dice => 2.4,
            Kite => 3.0,
            Radio => 2.8,
            Lantern => 2.6,
            Tiptoe => 2.4,
            Workout => 2.4,
            Swim => 2.8,
            Pirouette => 2.0,
            Daydream => 3.0,
            Whistle => 2.4,
            Hum => 2.2,
            Measure => 2.4,
            Balance => 2.6,
            Nervous => 2.0,
            Sulk => 2.4,
            Puppy => 2.2,
            Scheme => 2.6,
            TaDa => 1.8,
            Bow => 1.8,
            FistPump => 1.4,
            Tumble => 1.6,
            Flip => 1.6,
            Skate => 2.4,
            Scan => 2.6,
            PowerUp => 2.4,
            Glitch => 1.8,
            Compass => 2.4,
            Radar => 2.4,
            Metronome => 2.4,
            Morse => 2.4,
            Hammock => 3.0,
            Popcorn => 2.4,
            CardTrick => 2.2,
            Breeze => 1.8,
            Trapped => 2.4,
            Origami => 2.4,
            WarmHands => 2.4,
            Fireplace => 3.0,
            Storm => 2.4,
            Aurora => 3.0,
            Dribble => 2.4,
            Jog => 2.4,
            Row => 2.6,
            Yodel => 2.4,
            AhChoo => 2.0,
            Jelly => 1.6,
            Cloudwatch => 3.0,
            Photobomb => 1.4,
            HideSeek => 2.0,
            Vogue => 2.4,
            Flex => 2.0,
            JumpRope => 2.6,
            Skip => 2.0,
            Plie => 2.4,
            Karate => 1.8,
            Sumo => 2.2,
            Fencing => 1.8,
            Archery => 2.4,
            Darts => 2.2,
            Bowling => 2.8,
            Golf => 2.8,
            Fishing => 3.2,
            Surf => 2.4,
            Ski => 2.2,
            Pinwheel => 2.2,
            BubbleWand => 2.6,
            Polish => 2.4,
            Flashlight => 2.6,
            Shush => 1.8,
            SlowClap => 2.4,
            Fidget => 2.4,
            Coin => 2.4,
            Letter => 2.6,
            Selfie => 2.2,
            Marshmallow => 3.0,
            Meteor => 2.2,
            Moth => 2.6,
            Spring => 1.8,
            YoYo => 2.4,
            Jenga => 2.8,
            Dominoes => 2.4,
            Box => 2.0,
            Emote(_) => 1.6,
        }
    }

    /// A natural continuation some skits lead into, queued when they end.
    fn follow_up(self) -> Option<React> {
        Some(match self {
            React::Fishing | React::Bowling | React::Darts | React::Archery | React::Golf => React::Proud,
            React::Phone | React::Binge | React::Fidget => React::Dizzy,
            React::Lift | React::Workout | React::Jog | React::JumpRope | React::Box | React::Sumo | React::Surf | React::Ski | React::Dribble => React::Hot,
            React::Karate | React::Fencing | React::TaDa | React::CardTrick | React::Magic => React::Bow,
            React::Coffee => React::Perk,
            React::Snack | React::Pizza | React::Popcorn | React::Marshmallow | React::BubbleGum => React::Satisfied,
            React::Doze => React::Nap,
            React::Celebrate | React::Cheer | React::Kiss | React::Hug => React::Emote(Glyph::Heart),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hat {
    None,
    Santa,
    Party,
    Nightcap,
}

/// A held item for the object skits, drawn under the face and animated by
/// `Extras::prop_k` (a lid opening, a sprout growing, a glass raising...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prop {
    Laptop,
    Book,
    Phone,
    Mug,
    Cookie,
    Gamepad,
    Guitar,
    Mic,
    Dumbbell,
    Wand,
    Camera,
    Plant,
    Brush,
    Umbrella,
    Balloon,
    Teddy,
    Drum,
    Telescope,
    Newspaper,
    Pizza,
    Dice,
    Kite,
    Radio,
    Lantern,
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
    /// A held item for the object skits.
    pub prop: Option<Prop>,
    /// How far along the prop's own animation is, 0..1.
    pub prop_k: f32,
}

impl Default for Extras {
    fn default() -> Self {
        Self {
            glyph: Glyph::None,
            hat: Hat::None,
            shades: false,
            pumpkin: false,
            glow: None,
            slit: false,
            shine: false,
            dream: false,
            caption: None,
            rec: None,
            screen: None,
            prop: None,
            prop_k: 0.0,
        }
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
    /// A wisp of steam curling up.
    Steam,
    /// A firefly looping around the face; `vx`/`vy` hold the orbit centre.
    Fly,
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
pub(crate) struct Beat {
    slow: f32,
    energy: f32,
    last: f32,
    period: f32,
    count: u32,
    real: bool,
    silent_s: f32,
    /// Strength of the latest beat 0..1 (how far the onset jumped over the
    /// running level); filler beats are soft.
    pub(crate) hit: f32,
}

impl Beat {
    pub(crate) fn new() -> Self {
        Self { slow: 0.0, energy: 0.35, last: 0.0, period: 0.5, count: 0, real: false, silent_s: 0.0, hit: 0.5 }
    }

    pub(crate) fn feed(&mut self, peak: Option<f32>, playing: bool, t: f32, dt: f32) {
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

    pub(crate) fn pulse(&self, t: f32) -> f32 {
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
    /// Recently played idle skits, so `bits` never repeats itself.
    recent: VecDeque<React>,
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
    /// The work theme playing (themed app in front).
    work: Option<AppTheme>,
    /// Eased mic level while recording.
    listen_lvl: f32,
}
#[cfg(test)]
mod tests;
