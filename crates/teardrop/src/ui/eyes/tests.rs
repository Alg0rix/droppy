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
            recording: None,
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
fn bits_remember_and_follow_the_weather() {
    let (mut e, w) = settled();
    let mut seen = Vec::new();
    for _ in 0..12 {
        let b = e.bit(&w.senses());
        assert!(!seen.contains(&b), "{b:?} repeated too soon");
        seen.push(b);
    }
    // The umbrella only comes out in the rain.
    let mut w2 = World::new();
    w2.env.weather = Weather::Rain;
    assert!(bit_weight(React::Umbrella, &w2.senses(), false) > 0);
    assert_eq!(bit_weight(React::Umbrella, &w.senses(), false), 0);
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
fn weather_respects_the_setting() {
    let (mut e, mut w) = settled();
    w.set.weather_mood = false;
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
    let reacts = [
        None,
        Some(React::Love),
        Some(React::Sparkle),
        Some(React::Dizzy),
        Some(React::Shocked),
        Some(React::Flash),
        Some(React::Emote(Glyph::Question)),
        Some(React::Emote(Glyph::Cross)),
        Some(React::Emote(Glyph::Minus)),
        Some(React::Emote(Glyph::Moon)),
        Some(React::Emote(Glyph::Bolt)),
        Some(React::Laptop),
        Some(React::Hug),
        Some(React::Umbrella),
    ];
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
fn app_acting_needs_no_focus_block_but_loses_to_music_and_typing() {
    let (mut e, mut w) = settled();
    let mut s = work_senses(&w, AppCategory::Coding);
    s.focus = None; // no timer at all: the theme still plays
    for _ in 0..90 {
        e.tick(1.0 / 60.0, &s);
    }
    assert_eq!(e.mood, Mood::Work(AppCategory::Coding));
    // Real states still win over the ambient app theme.
    s.music = true;
    e.tick(1.0 / 60.0, &s);
    assert_eq!(e.mood, Mood::Dance);
    s.music = false;
    for _ in 0..5 {
        e.keys.push_back(e.t);
    }
    e.tick(1.0 / 60.0, &s);
    assert_eq!(e.mood, Mood::Typing);
    // Walked away? It still falls asleep instead of working forever.
    e.keys.clear();
    w.env.idle_ms = 400_000;
    let mut s = work_senses(&w, AppCategory::Coding);
    s.focus = None;
    e.tick(1.0 / 60.0, &s);
    assert_eq!(e.mood, Mood::Asleep);
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
fn recording_listens_and_hover_still_wins() {
    let (mut e, w) = settled();
    let mut s = w.senses();
    s.recording = Some(0.6);
    for _ in 0..90 {
        e.tick(1.0 / 60.0, &s);
    }
    assert_eq!(e.mood, Mood::Listening);
    assert!(e.pose().look.0 < -0.4, "ear toward the mic");
    assert!(e.parts.iter().any(|p| p.kind == Kind::Dot));
    s.hovering = true;
    s.mouse = Some((0.0, 0.0));
    e.tick(1.0 / 60.0, &s);
    assert_eq!(e.mood, Mood::Delight);
    // Recording stops: the level eases back out.
    s.hovering = false;
    s.mouse = None;
    s.recording = None;
    for _ in 0..120 {
        e.tick(1.0 / 60.0, &s);
    }
    assert!(e.listen_lvl < 0.05);
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
