use super::*;
impl Eyes {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn react_pose(&self, r: React, e: f32, s: &Senses, p: &mut Pose, x: &mut Extras, lv: &mut Live, look: &mut Option<(f32, f32)>) {
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
            // ------------------------------------------------------ skits
            React::Laptop => {
                let lid = (e / 0.5).min(1.0).min(((r.duration() - e) / 0.45).clamp(0.0, 1.0));
                x.prop = Some(Prop::Laptop);
                x.prop_k = lid * lid * (3.0 - 2.0 * lid);
                if x.prop_k > 0.6 && e < 2.7 {
                    // Eyes on the deck; a small bob per keystroke.
                    *look = Some(((e * 6.0).sin() * 0.45, 0.85));
                    p.open = 0.85;
                    p.brow = -0.4;
                    p.angry = 0.1;
                    lv.dy = (e * 9.0).sin().max(0.0) * 0.4;
                    x.screen = Some(ICE);
                } else {
                    p.happy = 0.6;
                    *look = Some((0.0, 0.6));
                }
            }
            React::Read => {
                let k = (e / 0.4).min(1.0).min(((r.duration() - e) / 0.4).clamp(0.0, 1.0));
                x.prop = Some(Prop::Book);
                x.prop_k = k;
                if k >= 1.0 && e < r.duration() - 0.5 {
                    // Scanning lines, slowing on a good bit.
                    let u = (e * 1.1).fract();
                    let line = ((e * 1.1) as u32 % 3) as f32;
                    *look = Some((-0.75 + 1.5 * u, 0.3 + 0.22 * line));
                    p.open = 0.68;
                    p.brow = 0.5;
                    if line == 1.0 && u > 0.75 {
                        p.happy = 0.55;
                    }
                } else {
                    p.happy = 0.8;
                    p.open = 0.85;
                }
            }
            React::Phone => {
                x.prop = Some(Prop::Phone);
                x.prop_k = (e / 0.4).min(1.0);
                x.screen = Some(Color::rgb(148, 163, 253));
                if e < 0.5 {
                    p.happy = 0.4;
                } else if e < 2.3 {
                    // Flick, flick, flick: the eyes scroll with the thumb.
                    let flick = ((e - 0.5) / 0.35).floor();
                    *look = Some((0.45, 0.5 + 0.1 * (flick % 3.0)));
                    p.open = 0.7;
                    p.pupil = 0.8;
                    lv.dy = ((e - 0.5) * 2.8).fract().min(0.15) * 3.0;
                } else {
                    x.glyph = Glyph::Spiral;
                    lv.dx = (e * 4.0).sin() * 1.5;
                }
            }
            React::Coffee => {
                let k = ((e - 0.3) / 0.4).clamp(0.0, 1.0).min(((r.duration() - e) / 0.5).clamp(0.0, 1.0));
                x.prop = Some(Prop::Mug);
                x.prop_k = k;
                if k > 0.8 && e < 1.9 {
                    // The long sip.
                    p.open = 0.0;
                    p.happy = 0.9;
                    p.mouth = 0.6;
                    lv.rot = -0.08;
                    lv.dy = -0.8 * k;
                } else {
                    p.happy = 0.7;
                    p.open = 0.9;
                    p.blush = 0.3 * bell;
                }
            }
            React::Snack => {
                x.prop = Some(Prop::Cookie);
                x.prop_k = pr;
                // Three bites, then a happy chew.
                let bite = (e * 4.2).sin().abs();
                p.mouth = 0.3 + 0.7 * bite * (1.0 - pr * 0.5);
                p.happy = 0.7;
                p.open = 0.75;
                *look = Some((0.5, 0.7));
                lv.squash = -0.06 * bite;
            }
            React::Game => {
                x.prop = Some(Prop::Gamepad);
                x.prop_k = 1.0;
                if e < 2.1 {
                    // Locked in: the eyes dart with the action.
                    *look = Some(((e * 9.0).sin().signum() * 0.7, 0.5));
                    p.open = 0.9;
                    p.angry = 0.25;
                    p.brow = -1.5;
                    lv.dx = (e * 26.0).sin() * 0.6;
                } else {
                    p.happy = 1.0;
                    p.open = 1.0;
                    p.brow = 2.0;
                    *look = Some((0.0, -0.3));
                }
            }
            React::Guitar => {
                x.prop = Some(Prop::Guitar);
                x.prop_k = (e * 3.0).fract();
                // Strum on the beat, rocking side to side.
                *look = Some((0.2, 0.7));
                p.happy = 0.6;
                p.open = 0.7;
                lv.rot = (e * 3.2).sin() * 0.1;
                lv.dy = -((e * 3.0).fract() * PI).sin() * 1.4;
            }
            React::Sing => {
                x.prop = Some(Prop::Mic);
                x.prop_k = (e / 0.4).min(1.0);
                // La-a-a: the mouth opens on the note, head tipped back.
                let note = (e * 4.0).sin().abs();
                p.mouth = 0.3 + 0.9 * note;
                p.open = 0.0;
                p.happy = 1.0;
                lv.rot = -0.1;
                lv.dy = -1.0 - 0.8 * note;
            }
            React::Lift => {
                x.prop = Some(Prop::Dumbbell);
                if e < 0.8 {
                    // Squatting to grab it.
                    let k = (e / 0.8 * PI).sin();
                    lv.squash = 0.2 * k;
                    lv.dy = 2.5 * k;
                    *look = Some((0.0, 0.8));
                    p.open = 0.6;
                    x.prop_k = 0.0;
                } else if e < 1.9 {
                    // The pull: shaking, straining.
                    let k = ((e - 0.8) / 1.1).min(1.0);
                    x.prop_k = k * k;
                    p.angry = 0.5;
                    p.open = 0.5;
                    p.brow = -2.0;
                    lv.dx = (e * 40.0).sin() * 0.6;
                    lv.dy = -3.0 * k;
                } else {
                    // Held high, then a proud little flex.
                    x.prop_k = 1.0;
                    p.happy = 0.8;
                    p.open = 0.9;
                    p.brow = 1.5;
                    lv.dy = -3.0 + bell * 0.5;
                }
            }
            React::Magic => {
                x.prop = Some(Prop::Wand);
                x.prop_k = bell;
                if e < 1.1 {
                    // The wind-up: drawing a circle with the wand.
                    *look = Some(((e * 5.0).cos() * 0.7, -0.3 + (e * 5.0).sin() * 0.4));
                    p.open = 0.85;
                    p.brow = 1.0;
                    lv.rot = (e * 5.0).sin() * 0.08;
                } else {
                    x.glyph = Glyph::Star;
                    p.size = 1.1;
                    lv.dy = -1.5 * bell;
                }
            }
            React::Photo => {
                x.prop = Some(Prop::Camera);
                x.prop_k = (e / 0.5).min(1.0).min(((r.duration() - e) / 0.4).clamp(0.0, 1.0));
                if e < 0.55 {
                    // Levelling the shot.
                    lv.rot = -0.06;
                    lv.dy = -0.8;
                    *look = Some((0.0, -0.1));
                    p.open = 0.8;
                    p.size_l = 1.1;
                } else if e < 0.9 {
                    p.happy = 0.9;
                    p.open = 0.0;
                } else {
                    // Checking the shot: a pleased tilt.
                    p.happy = 0.7;
                    *look = Some((0.5, 0.4));
                    lv.rot = 0.08;
                }
            }
            React::Plant => {
                x.prop = Some(Prop::Plant);
                x.prop_k = (e / 2.4).clamp(0.0, 1.0);
                // Eyes follow the sprout up.
                *look = Some((0.5, 0.8 - 0.9 * x.prop_k));
                p.open = 0.9;
                p.brow = 1.2 * x.prop_k;
                if e > 2.0 {
                    p.happy = 0.9;
                    p.blush = 0.4;
                }
            }
            React::Paint => {
                x.prop = Some(Prop::Brush);
                x.prop_k = (e * 0.8).fract();
                // The artist's squint, gaze orbiting the canvas.
                let a = e * 3.4;
                *look = Some((a.cos() * 0.75, 0.15 + a.sin() * 0.4));
                p.wink_l = 0.5;
                p.brow = 0.7;
                lv.rot = (e * 1.8).sin() * 0.07;
                lv.dx = a.cos() * 1.5;
            }
            React::Umbrella => {
                x.prop = Some(Prop::Umbrella);
                x.prop_k = (e / 0.35).min(1.0).min(((r.duration() - e) / 0.4).clamp(0.0, 1.0));
                // Dry and smug under it.
                p.happy = 0.7;
                p.open = 0.8;
                *look = Some(((e * 1.1).sin() * 0.4, -0.35));
                lv.dy = (e * 2.2).sin() * 0.5;
            }
            React::Balloon => {
                let k = (e / 0.6).min(1.0);
                if e < 2.05 {
                    x.prop = Some(Prop::Balloon);
                    x.prop_k = k;
                    // It pulls and the face stretches upward.
                    lv.squash = -0.16 * k;
                    lv.dy = -3.5 * k + (e * 2.0).sin() * 0.5;
                    *look = Some((0.4, -0.9));
                    p.open = 0.9;
                    p.brow = 1.0;
                } else {
                    // The pop: wide, small-pupiled shock, then a blink.
                    calm(p);
                    p.size = 1.3;
                    p.pupil = 0.55;
                    p.brow = 3.0;
                    p.mouth = 0.8 * (1.0 - (pr - 0.73) * 3.0).max(0.0);
                    lv.dx = (e * 60.0).sin() * 0.8;
                }
            }
            React::Hug => {
                x.prop = Some(Prop::Teddy);
                x.prop_k = bell;
                p.open = 0.0;
                p.happy = 1.0;
                p.blush = 0.8;
                // Squeeze, release, squeeze.
                lv.squash = 0.1 * (e * 5.0).sin().max(0.0);
                lv.rot = (e * 2.2).sin() * 0.06;
            }
            React::SideEye => {
                let side = if e < r.duration() / 2.0 { -1.0 } else { 1.0 };
                *look = Some((side * 1.0, 0.15));
                p.open = 0.6;
                p.angry = 0.25;
                p.brow = -0.8;
                lv.rot = side * 0.05;
                lv.dx = side * 1.5;
            }
            React::BrowWiggle => {
                p.brow = if (e * 6.0) as i32 % 2 == 0 { 2.2 } else { -0.8 };
                p.happy = 0.5;
                p.open = 0.95;
                lv.dy = -((e * 6.0).fract() * PI).sin() * 1.2;
                lv.rot = (e * 3.0).sin() * 0.05;
            }
            React::Squint => {
                if pr < 0.7 {
                    p.open = 0.3;
                    p.size = 0.92;
                    p.brow = -0.8;
                    lv.dx = -2.0 * (pr / 0.7);
                    *look = Some((0.6, 0.1));
                } else {
                    p.open = 1.0;
                    p.brow = 1.5;
                    p.happy = 0.3;
                }
            }
            React::Doze => {
                // Two nod-offs with a snap back up.
                let phase = if e < 1.4 { e / 1.4 } else { (e - 1.4) / 1.4 };
                let dip = (phase * PI).min(PI * 0.9).sin();
                p.open = 1.0 - 0.9 * dip;
                lv.dy = 3.0 * dip;
                lv.rot = 0.06 * dip;
                if phase > 0.92 {
                    p.open = 1.0;
                    p.brow = 2.0;
                    lv.dy = -1.0;
                }
                p.sad = 0.2;
            }
            React::Meditate => {
                p.open = 0.0;
                p.happy = 0.6;
                let breath = (e * 1.2).sin();
                p.size = 1.0 + 0.06 * breath;
                lv.dy = -2.0 - 0.8 * breath;
                lv.rot = (e * 0.6).sin() * 0.03;
                x.glow = Some(GOLD.fade(0.4 + 0.2 * breath));
            }
            React::Shiver => {
                p.open = 0.65;
                p.sad = 0.3;
                lv.dx = (e * 62.0).sin() * 0.9;
                lv.dy = (e * 55.0).cos() * 0.4;
                lv.squash = 0.06;
                x.glow = Some(ICE.fade(0.5));
                *look = Some((0.0, 0.4));
            }
            React::Hot => {
                p.mouth = 0.5 + 0.4 * (e * 8.0).sin().abs();
                p.open = 0.7;
                p.sad = 0.15;
                lv.dy = (e * 8.0).sin().abs() * 0.8;
                lv.rot = (e * 1.4).sin() * 0.05;
                *look = Some((0.0, -0.4));
            }
            React::Nap => {
                if e < 1.8 {
                    let k = (e / 1.8).min(1.0);
                    p.open = (1.0 - k).max(0.0);
                    lv.dy = 2.2 * k;
                    lv.rot = 0.08 * k;
                    p.happy = 0.3;
                } else {
                    // Jolt: wide, looking around, unsure where it is.
                    calm(p);
                    p.open = 1.0;
                    p.size = 1.15;
                    p.brow = 2.5;
                    lv.dy = -1.0;
                    *look = Some(((e * 8.0).sin() * 0.6, -0.2));
                }
            }
            React::Spy => {
                p.open = 0.5;
                p.size = 0.95;
                p.pupil = 0.85;
                p.brow = -0.6;
                let sweep = ((e * 0.9).fract() * 2.0 - 1.0).abs() * 2.0 - 1.0;
                *look = Some((sweep, -0.35));
                lv.rot = sweep * 0.04;
            }
            React::Inspect => {
                p.size_l = 1.5;
                p.wink_r = 0.45;
                p.brow = 0.8;
                *look = Some(((e * 1.9).sin() * 0.7, 0.55 + 0.25 * (e * 0.9).cos()));
                lv.rot = 0.1;
                lv.dx = 1.0;
            }
            React::Spin => {
                lv.rot = pr * TAU;
                p.happy = 0.6;
                lv.dy = -bell * 3.0;
                lv.squash = -0.1 * bell;
                if pr > 0.85 {
                    lv.dx = (e * 30.0).sin() * (1.0 - pr) * 6.0;
                }
            }
            React::Moonwalk => {
                // Feet go left, eyes stay right.
                lv.dx = -8.0 * pr;
                *look = Some((0.8, 0.0));
                p.happy = 0.4;
                p.open = 0.8;
                lv.squash = 0.05 * (e * 10.0).sin();
                x.shades = e > 0.5;
            }
            React::Beep => {
                // Stepped gaze snaps and a hard square blink.
                const SNAPS: [(f32, f32); 5] = [(-0.8, -0.2), (0.0, -0.5), (0.8, -0.2), (0.4, 0.4), (-0.4, 0.4)];
                *look = Some(SNAPS[(e * 5.0) as usize % 5]);
                p.open = if (e * 5.0).fract() < 0.12 { 0.0 } else { 0.9 };
                p.pupil = 0.9;
                x.glow = Some(ICE.fade(0.6));
                lv.rot = 0.0;
            }
            React::Wave => {
                p.happy = 1.0;
                p.open = 1.0;
                p.brow = 1.5;
                lv.dy = -(e * 10.0).sin().abs() * 3.0;
                lv.rot = (e * 10.0).sin() * 0.08;
                lv.dx = (e * 5.0).sin() * 1.5;
            }
            React::BubbleGum => {
                if e < 1.85 {
                    // Watching it grow.
                    *look = Some((0.0, 0.65));
                    p.mouth = 0.4;
                    p.brow = 0.8;
                    p.open = 0.9;
                    lv.dy = -0.5 * bell;
                } else {
                    // Popped on its nose.
                    p.open = 0.0;
                    p.angry = 0.3;
                    lv.squash = 0.15 * (1.0 - pr);
                    *look = Some((0.0, 0.8));
                }
            }
            React::Kiss => {
                if pr < 0.4 {
                    p.wink_l = 1.0;
                    p.happy = 0.8;
                    p.blush = 0.6;
                } else {
                    p.mouth = 0.55;
                    p.happy = 0.7;
                    *look = Some((0.9, -0.2));
                    lv.dy = -bell;
                    p.blush = 0.5;
                }
            }
            React::Grumble => {
                p.angry = 0.7;
                p.open = 0.4;
                p.brow = -2.5;
                *look = Some((-0.3, 0.5));
                lv.dy = 1.0;
                lv.squash = 0.08 + 0.06 * (e * 6.0).sin().abs();
                p.mouth = 0.2 * (e * 6.0).sin().abs();
            }
            React::Eavesdrop => {
                if e < 1.7 {
                    // Very still, ear toward the sound on the left.
                    lv.rot = -0.16;
                    lv.dx = -4.0;
                    *look = Some((-0.95, 0.0));
                    p.open = 0.9;
                    p.pupil = 0.8;
                    p.brow = 0.6;
                } else {
                    calm(p);
                    p.size = 1.25;
                    p.brow = 3.0;
                    p.mouth = 0.5;
                    lv.dx = 0.0;
                    lv.rot = 0.0;
                    *look = Some((-0.6, -0.3));
                }
            }
            React::Idea => {
                if e < 0.7 {
                    p.open = 0.7;
                    p.sad = 0.2;
                    *look = Some((0.0, 0.4));
                    lv.dy = 0.8;
                } else {
                    x.glyph = Glyph::Bolt;
                    p.size = 1.2;
                    p.brow = 2.0;
                }
            }
            React::Facepalm => {
                lv.squash = 0.18;
                *look = Some((0.0, 0.9));
                p.open = 0.4;
                p.sad = 0.5;
                p.brow = -1.0;
                lv.dy = 1.8;
                lv.dx = (e * 2.4).sin() * 1.6 * (1.0 - pr * 0.5);
                lv.rot = (e * 2.4).sin() * 0.05;
            }
            React::Cringe => {
                if pr < 0.6 {
                    x.glyph = Glyph::Squeeze;
                    lv.dx = (e * 45.0).sin() * 0.7;
                    lv.squash = 0.12;
                } else {
                    // One eye opens first, unsure it's over.
                    p.open = 0.5;
                    p.size_l = 0.85;
                    p.sad = 0.3;
                    *look = Some((0.4, 0.3));
                }
            }
            React::Cheer => {
                // Alternating winks and high hops.
                if (e / 0.4) as i32 % 2 == 0 {
                    p.wink_l = 1.0;
                } else {
                    p.wink_r = 1.0;
                }
                p.happy = 0.9;
                lv.dy = -((e * 8.0).sin().abs()) * 4.0;
                lv.rot = (e * 8.0).cos() * 0.1;
                p.brow = 1.5;
            }
            React::March => {
                let step = (e * 5.0).sin();
                lv.dy = step.abs() * -1.6;
                lv.rot = step.signum() * 0.09;
                lv.dx = step * 1.5;
                p.open = 0.9;
                p.happy = 0.35;
                p.brow = 0.8;
                *look = Some((step.signum() * 0.4, -0.1));
            }
            React::Pogo => {
                let hop_h = 1.5 + 3.5 * pr;
                lv.dy = -((e * 6.0).sin().abs()) * hop_h;
                lv.squash = 0.12 * ((e * 6.0).cos().max(0.0));
                p.happy = 0.7;
                p.open = 1.0;
                p.brow = 1.2;
            }
            React::Ninja => {
                if e < 0.7 {
                    // Sucking in to vanish.
                    let k = e / 0.7;
                    p.open = 0.3;
                    p.size = 1.0 - 0.4 * k;
                    lv.squash = 0.2 * k;
                    *look = Some((-0.6, 0.0));
                } else {
                    // Reappeared on the other side, smug.
                    p.happy = 0.6;
                    p.open = 0.9;
                    lv.dx = 6.0 * (1.0 - (e - 0.7) / 1.3);
                    *look = Some((-0.4, 0.0));
                    p.brow = 0.8;
                }
            }
            React::ZeroG => {
                lv.dy = -1.5 + (e * 1.4).sin() * 1.6;
                lv.rot = (e * 0.8).sin() * 0.16;
                lv.dx = (e * 0.5).cos() * 2.5;
                p.open = 0.95;
                p.size = 1.08;
                p.brow = 1.0;
                *look = Some(((e * 0.9).sin() * 0.5, -0.3));
                p.mouth = 0.25;
            }
            React::Splash => {
                if e < 0.7 {
                    // The leap up and over.
                    let k = e / 0.7;
                    lv.dy = -(k * PI).sin() * 4.5;
                    p.happy = 0.6;
                    p.open = 1.0;
                    p.brow = 1.5;
                    *look = Some((0.0, -0.4));
                } else {
                    p.open = 0.5;
                    p.happy = 0.4;
                    p.mouth = 0.3;
                    lv.dy = 1.2 * (1.0 - pr);
                    *look = Some((0.0, 0.7));
                }
            }
            React::Catch => {
                if e < 2.1 {
                    // Tracking the firefly wherever it loops — or your
                    // pointer, if you wave it while it's playing.
                    let t = s.mouse.map_or_else(
                        || self.parts.iter().find(|p| p.kind == Kind::Fly).map(|f| (f.x / s.area.0.max(1.0), f.y / s.area.1.max(1.0))).unwrap_or((0.0, 0.0)),
                        |(mx, my)| (mx / s.area.0.max(1.0), my / s.area.1.max(1.0)),
                    );
                    let (lx, ly) = (t.0.clamp(-1.0, 1.0), t.1.clamp(-1.0, 1.0));
                    *look = Some((lx, ly * 0.8));
                    lv.dx = lx * 1.5;
                    p.open = 1.0;
                    p.pupil = 0.8;
                    p.brow = 1.2;
                } else {
                    p.happy = 1.0;
                    p.open = 0.85;
                    p.blush = 0.4;
                    // Caught it: face lit by the glow.
                    x.glow = Some(Color::rgb(150, 255, 120).fade(0.25));
                    *look = Some((0.5, 0.5));
                }
            }
            React::Hack => {
                x.shades = e > 0.25;
                *look = Some(((e * 11.0).sin() * 0.5, 0.85));
                p.open = 0.8;
                p.brow = -1.2;
                p.angry = 0.2;
                lv.dy = (e * 14.0).sin().max(0.0) * 0.6;
                lv.dx = (e * 3.0).sin() * 1.0;
            }
            React::Juggle => {
                // Eyes chase the peak of each throw: up, across, up.
                let u = (e * 1.8).fract();
                let side = if (e * 1.8) as i32 % 2 == 0 { -1.0 } else { 1.0 };
                *look = Some((side * (0.9 - 1.8 * u), -(u * PI).sin() * 0.9));
                p.open = 1.0;
                p.brow = 1.2;
                p.size = 1.05;
                lv.dy = -0.6 * (u * PI).sin();
                lv.rot = -side * 0.05;
            }
            React::Proud => {
                p.size = 1.15;
                p.brow = 2.0;
                p.happy = 0.55;
                p.open = 0.75;
                lv.squash = -0.1;
                lv.rot = -0.05;
                x.shine = true;
                *look = Some((0.2, -0.3));
            }
            React::PeekABoo => {
                if e < 0.9 {
                    p.open = 0.0;
                    lv.dy = 0.5;
                } else {
                    calm(p);
                    p.open = 1.0;
                    p.size = 1.3;
                    p.brow = 3.0;
                    p.mouth = 0.6 * bell;
                    p.happy = 0.8;
                    lv.dy = -2.0 * bell;
                }
            }
            React::Count => {
                if e < 2.0 {
                    // Ticking through them, gaze stepping down-right.
                    let n = (e * 2.2).floor();
                    *look = Some((-0.6 + 0.3 * n.min(4.0), 0.8));
                    p.open = 0.8;
                    p.brow = 0.6;
                    p.mouth = 0.25 * (e * 9.0).sin().abs();
                    lv.dy = ((e * 2.2).fract() * PI).sin() * 0.6;
                } else {
                    p.happy = 0.9;
                    p.brow = 2.0;
                    *look = Some((0.0, -0.3));
                }
            }
            React::Wait => {
                // Foot tapping, checking the time twice.
                lv.dy = (e * 10.0).sin().abs() * 0.7;
                if (e > 0.9 && e < 1.3) || (e > 2.0 && e < 2.4) {
                    *look = Some((0.95, 0.9));
                } else {
                    *look = Some(((e * 0.8).sin() * 0.4, 0.1));
                }
                p.open = 0.7;
                p.sad = 0.2;
                p.brow = -0.5;
            }
            React::Hype => {
                let pump = (e * 12.0).sin().abs();
                p.size = 1.0 + 0.15 * pump;
                p.brow = 2.0 * pump;
                p.angry = 0.15;
                p.open = 1.0;
                lv.dx = (e * 24.0).sin() * 0.8;
                lv.dy = -pump * 1.5;
                *look = Some((0.0, -0.2));
            }
            React::Sniff => {
                // Two quick forward dips, then a suspicious narrow.
                if e < 0.9 {
                    lv.dy = -((e * 7.0).fract() * PI).sin() * 1.4;
                    p.mouth = 0.2;
                    *look = Some((0.0, 0.4));
                    p.open = 0.85;
                } else {
                    p.open = 0.6;
                    p.brow = -0.6;
                    *look = Some(((e * 1.5).sin() * 0.6, 0.0));
                }
            }
            React::Breathe => {
                if pr < 0.45 {
                    let k = pr / 0.45;
                    p.size = 1.0 + 0.18 * k;
                    p.brow = 1.5 * k;
                    lv.dy = -1.5 * k;
                    p.open = 0.7;
                } else {
                    let k = (pr - 0.45) / 0.55;
                    p.size = 1.18 - 0.18 * k;
                    p.happy = 0.7 * k;
                    p.open = 0.5;
                    lv.dy = -1.5 + 2.0 * k;
                    lv.squash = 0.06 * k;
                }
            }
            React::Rub => {
                // Eyes shut, rocking in a tired little circle.
                p.open = 0.15;
                lv.rot = (e * 4.0).sin() * 0.1;
                lv.dx = (e * 4.0).cos() * 1.2;
                lv.dy = 0.8 + (e * 4.0).sin() * 0.5;
                p.sad = 0.25;
            }
            React::Rage => {
                if e < 1.4 {
                    p.angry = 0.9;
                    p.open = 0.35;
                    p.brow = -3.0;
                    p.mouth = 0.5;
                    lv.dx = (e * 50.0).sin() * 1.0;
                    lv.squash = 0.1;
                    x.glow = Some(Color::rgb(255, 80, 80).fade(0.55));
                } else {
                    calm(p);
                    p.sad = 0.5;
                    p.open = 0.4;
                    lv.dy = 2.0;
                    lv.squash = 0.12;
                }
            }
            React::Bobble => {
                lv.rot = (e * 7.0).sin() * 0.2 * (1.0 - pr * 0.5);
                lv.dx = (e * 7.0).sin() * 1.8 * (1.0 - pr * 0.5);
                p.happy = 0.3;
                p.open = 0.85;
            }
            React::Binge => {
                // Slack-jawed, unblinking, lit by the flicker.
                x.screen = Some(hsl((e * 90.0) % 360.0, 0.5, 0.55));
                *look = Some((0.0, 0.05));
                p.open = 1.0;
                p.size = 1.1;
                p.pupil = 1.25;
                p.mouth = 0.3;
                p.brow = -0.5;
            }
            React::Stargaze => {
                *look = Some(((e * 0.7).sin() * 0.4, -0.95));
                p.open = 1.0;
                p.size = 1.1;
                p.pupil = 1.2;
                p.brow = 1.2;
                p.mouth = 0.2;
                lv.dy = -0.5 + (e * 0.9).sin() * 0.4;
                x.shine = true;
            }
            React::Mirror => {
                if e < 0.6 {
                    *look = Some((0.3, -0.2));
                    p.brow = 1.5;
                    p.open = 0.9;
                } else if e < 1.2 {
                    p.wink_l = 1.0;
                    p.happy = 0.8;
                    lv.rot = 0.1;
                } else {
                    p.happy = 0.9;
                    p.brow = 1.0;
                    lv.dy = -bell;
                    p.blush = 0.3;
                }
            }
            React::Bask => {
                x.shades = true;
                p.happy = 0.7;
                p.open = 0.5;
                lv.dx = (e * 1.2).sin() * 2.0;
                lv.rot = (e * 1.2).sin() * 0.08;
                x.glow = Some(GOLD.fade(0.3));
            }
            React::Toast => {
                x.prop = Some(Prop::Mug);
                x.prop_k = (e / 0.4).min(1.0);
                *look = Some((-0.2, -0.5));
                p.happy = 0.8;
                p.brow = 1.8;
                lv.dy = -2.0 * (e / 0.5).min(1.0) + 2.0 * pr;
                lv.rot = -0.08;
            }
            // -------------------------------------------------- skits two
            React::Drum => {
                x.prop = Some(Prop::Drum);
                x.prop_k = (e * 14.0).fract();
                // Alternating hits, faster near the end.
                let alt = (e * 14.0) as i32 % 2 == 0;
                lv.dx = if alt { -1.5 } else { 1.5 };
                lv.dy = -((e * 14.0).fract() * PI).sin() * 1.6;
                p.open = 0.85;
                p.happy = 0.4;
                p.brow = 0.8;
                *look = Some((if alt { -0.5 } else { 0.5 }, 0.7));
            }
            React::Scope => {
                x.prop = Some(Prop::Telescope);
                x.prop_k = (e / 0.6).min(1.0);
                if e < 1.9 {
                    // Long slow scan across the sky.
                    *look = Some((-0.9 + e * 0.9, -0.55));
                    p.open = 0.6;
                    p.size_l = 1.25;
                    p.brow = 0.6;
                    lv.rot = 0.08;
                } else {
                    calm(p);
                    p.size = 1.2;
                    p.brow = 2.5;
                    p.happy = 0.7;
                    *look = Some((0.85, -0.5));
                }
            }
            React::News => {
                x.prop = Some(Prop::Newspaper);
                x.prop_k = (e / 0.4).min(1.0).min(((r.duration() - e) / 0.5).clamp(0.0, 1.0));
                // Down the columns, brow falling at the headlines.
                let col = ((e * 0.8) as u32 % 2) as f32;
                let u = (e * 0.8).fract();
                *look = Some((-0.5 + col * 1.0, -0.2 + 0.9 * u));
                p.open = 0.75;
                p.brow = -0.8 * u;
                p.sad = 0.15 * u;
            }
            React::Pizza => {
                x.prop = Some(Prop::Pizza);
                x.prop_k = (e / 0.5).min(1.0);
                if e < 1.0 {
                    // Pulling the slice: eyes track the cheese stretch up.
                    *look = Some((0.5, 0.6 - 0.8 * x.prop_k));
                    p.open = 0.9;
                    p.brow = 1.2;
                    lv.dy = -x.prop_k;
                } else {
                    p.happy = 0.9;
                    p.open = 0.7;
                    p.mouth = 0.4 + 0.5 * (e * 6.0).sin().abs();
                    *look = Some((0.0, 0.6));
                }
            }
            React::Dice => {
                x.prop = Some(Prop::Dice);
                x.prop_k = if e < 1.4 { (e * 9.0).fract() } else { 0.0 };
                if e < 1.4 {
                    // Shaking the cup.
                    lv.rot = (e * 9.0).sin() * 0.15;
                    lv.dx = (e * 9.0).cos() * 2.0;
                    *look = Some((0.0, 0.7));
                    p.open = 0.7;
                    p.angry = 0.2;
                } else {
                    p.open = 1.0;
                    p.brow = 1.8;
                    *look = Some((0.0, 0.5));
                    lv.dy = -bell;
                }
            }
            React::Kite => {
                x.prop = Some(Prop::Kite);
                x.prop_k = 1.0;
                // The kite pulls, the face leans back into the run.
                *look = Some(((e * 1.1).sin() * 0.5, -0.85));
                p.open = 0.95;
                p.happy = 0.6;
                p.brow = 1.0;
                lv.rot = -0.08;
                lv.dx = (e * 1.1).sin() * 2.0;
                lv.dy = -1.5 + (e * 5.0).sin().abs() * -0.6;
            }
            React::Radio => {
                x.prop = Some(Prop::Radio);
                x.prop_k = if e < 1.8 { (e * 1.4).fract() } else { 1.0 };
                if e < 1.8 {
                    // Tuning through static: squinting down the dial.
                    *look = Some((-0.7 + 1.4 * x.prop_k, 0.55));
                    p.open = 0.55;
                    p.angry = 0.15;
                    lv.dx = (e * 30.0).sin() * 0.3;
                } else {
                    p.happy = 0.8;
                    p.open = 0.9;
                    lv.rot = (e * 4.0).sin() * 0.06;
                }
            }
            React::Lantern => {
                x.prop = Some(Prop::Lantern);
                x.prop_k = bell;
                x.glow = Some(Color::rgb(251, 191, 36).fade(0.35 * bell));
                // Wandering look into the dark corners.
                *look = Some(((e * 0.8).sin() * 0.9, 0.1 + (e * 0.4).cos() * 0.3));
                p.open = 0.9;
                p.size = 1.05;
                p.pupil = 1.3;
            }
            React::Tiptoe => {
                // Slow sneaky steps, glancing back mid-way.
                let step = (e * 3.0).sin();
                lv.dx = -6.0 * pr;
                lv.dy = -step.abs() * 1.2;
                lv.rot = 0.06;
                p.open = 0.6;
                p.pupil = 0.8;
                *look = Some(if (0.9..1.4).contains(&e) { (0.9, 0.2) } else { (-0.8, 0.3) });
            }
            React::Workout => {
                // Jumping jacks: stretch tall, squash wide.
                let f = (e * 4.0).fract();
                let k = (f * TAU).sin().max(0.0);
                lv.squash = -0.18 * k;
                lv.dy = -4.0 * k;
                p.open = 0.9;
                p.happy = 0.4;
                p.brow = 1.0 * k;
                *look = Some((0.0, -0.3));
            }
            React::Swim => {
                // Freestyle: face comes up for air every stroke.
                let f = (e * 1.4).fract();
                let breathe = (f * TAU).sin().max(0.0);
                p.open = 0.4 + 0.6 * breathe;
                *look = Some(((f * TAU).cos() * 0.6, -0.2 * breathe + 0.3));
                lv.dy = -1.5 * breathe;
                lv.rot = (f * TAU).cos() * 0.12;
                p.mouth = 0.3 * breathe;
            }
            React::Pirouette => {
                lv.rot = pr * TAU * 2.0;
                lv.dy = -bell * 2.5;
                p.open = 0.9;
                p.happy = 0.7;
                if pr > 0.9 {
                    lv.dx = (e * 35.0).sin() * 2.0 * (1.0 - pr) * 4.0;
                }
            }
            React::Daydream => {
                p.open = 0.0;
                p.happy = 0.9;
                p.blush = 0.3;
                lv.dy = -1.0 + (e * 1.1).sin() * 0.8;
                lv.rot = (e * 0.9).sin() * 0.07;
                lv.dx = (e * 0.5).sin() * 1.5;
            }
            React::Whistle => {
                // Cheeks out, a tune going up.
                p.mouth = 0.45;
                p.open = 0.8;
                p.happy = 0.4;
                *look = Some(((e * 1.6).sin() * 0.5, -0.4));
                lv.rot = (e * 2.4).sin() * 0.05;
                lv.dy = (e * 3.0).sin() * 0.5;
            }
            React::Hum => {
                p.open = 0.0;
                p.happy = 0.85;
                lv.dx = (e * 2.0).sin() * 2.5;
                lv.rot = (e * 2.0).cos() * 0.09;
                lv.dy = (e * 1.0).sin() * 0.4;
            }
            React::Measure => {
                // Eyeballing a length: left end, right end, double-check.
                let phase = (e * 1.2).fract();
                let (lx, squint) = if phase < 0.4 {
                    (-0.85, 0.5)
                } else if phase < 0.8 {
                    (0.85, 0.5)
                } else {
                    (0.0, 0.3)
                };
                *look = Some((lx, 0.5));
                p.open = squint;
                p.size = 0.92;
                p.brow = 0.4;
                lv.rot = if phase < 0.4 { -0.05 } else { 0.05 };
            }
            React::Balance => {
                // Tightrope: overcorrects left and right.
                let wob = (e * 3.2).sin() + (e * 7.7).sin() * 0.4;
                lv.rot = wob * 0.18;
                lv.dx = wob * 1.8;
                lv.dy = -0.5;
                p.open = 0.9;
                p.pupil = 0.8;
                p.brow = 1.2;
                *look = Some((0.0, -0.1));
            }
            React::Nervous => {
                *look = Some(((e * 6.0).sin() * 0.7, 0.2 + (e * 3.0).cos() * 0.3));
                p.open = 0.95;
                p.pupil = 0.75;
                p.size = 1.05;
                lv.dx = (e * 40.0).sin() * 0.4;
                if e > 1.4 && e < 1.7 {
                    p.mouth = 0.5;
                    lv.dy = 0.8;
                }
            }
            React::Sulk => {
                *look = Some((-0.9, 0.3));
                p.open = 0.55;
                p.sad = 0.5;
                p.brow = -1.2;
                lv.rot = -0.14;
                lv.dx = -2.0;
                lv.dy = 1.0;
                if e > 1.5 && e < 1.9 {
                    // One glance back.
                    *look = Some((0.5, 0.1));
                }
            }
            React::Puppy => {
                p.size = 1.3;
                p.pupil = 1.5;
                p.open = 1.0;
                p.sad = 0.3;
                p.brow = 2.0;
                x.shine = true;
                lv.rot = (e * 1.5).sin() * 0.08;
                *look = Some(((e * 1.5).sin() * 0.2, -0.4));
            }
            React::Scheme => {
                p.angry = 0.4;
                p.open = 0.55;
                p.brow = -1.8;
                p.happy = 0.15;
                *look = Some(((e * 2.2).sin() * 0.7, -0.1));
                lv.dx = (e * 25.0).sin() * 0.4;
                lv.squash = 0.05;
                p.mouth = 0.15;
            }
            React::TaDa => {
                if e < 0.7 {
                    // Wind the arms back.
                    lv.dx = -3.0 * bell;
                    lv.rot = -0.12 * bell;
                    p.open = 0.7;
                    p.brow = 0.8;
                } else {
                    x.glyph = Glyph::Star;
                    p.size = 1.15;
                    lv.dx = 3.0 * (1.0 - pr);
                    lv.rot = 0.1;
                }
            }
            React::Bow => {
                let dip = (pr * PI).sin();
                lv.dy = 4.5 * dip;
                lv.squash = 0.16 * dip;
                p.open = 0.4 + 0.5 * (1.0 - dip);
                p.happy = 0.5;
                *look = Some((0.0, 0.6 * dip));
                lv.rot = 0.05 * dip;
            }
            React::FistPump => {
                // Yes! Yes! Two sharp pumps.
                let pump = ((e * 8.0).fract() * PI).sin();
                lv.dy = -3.5 * pump;
                lv.rot = -0.1 * pump;
                p.happy = 0.9;
                p.open = 0.9;
                p.brow = 1.8;
                p.angry = 0.15;
            }
            React::Tumble => {
                // Forward roll with real travel.
                lv.rot = pr * TAU;
                lv.dx = (pr * 2.0 - 1.0) * -10.0;
                lv.dy = -bell * 2.0;
                p.happy = 0.5;
                if pr > 0.9 {
                    lv.squash = 0.15;
                }
            }
            React::Flip => {
                // Backflip: rises, rotates backward, sticks it.
                lv.dy = -bell * 5.0;
                lv.rot = -pr * TAU;
                p.open = 0.9;
                p.brow = 1.5;
                if pr > 0.88 {
                    lv.squash = 0.18;
                    p.happy = 0.8;
                }
            }
            React::Skate => {
                // Long glides, pushing off each side.
                let stride = (e * 1.6).sin();
                lv.dx = stride * 8.0;
                lv.rot = stride * 0.12;
                lv.dy = -stride.abs() * 0.8;
                p.happy = 0.5;
                p.open = 0.85;
                *look = Some((stride.signum() * 0.6, -0.1));
            }
            React::Scan => {
                // Photocopier: a light bar crawls down, carriage returns.
                let (f, n) = ((e / 0.4).fract(), (e / 0.4) as u32 % 4);
                *look = Some((-0.8 + 1.6 * f, -0.5 + 0.35 * n as f32));
                p.open = 0.75;
                p.pupil = 0.8;
                x.screen = Some(ICE.fade(0.5));
                lv.rot = 0.02;
            }
            React::PowerUp => {
                if e < 1.5 {
                    // Charging: trembling as the glow builds.
                    let k = e / 1.5;
                    lv.dx = (e * 55.0).sin() * (0.3 + 0.8 * k);
                    lv.squash = -0.1 * k;
                    p.angry = 0.3 + 0.3 * k;
                    p.open = 0.6;
                    p.brow = -2.0;
                    *look = Some((0.0, -0.3));
                    x.glow = Some(GOLD.fade(0.2 + 0.4 * k));
                } else {
                    calm(p);
                    x.glyph = Glyph::Bolt;
                    p.size = 1.2;
                    p.brow = 2.0;
                }
            }
            React::Glitch => {
                // Frames flicker: pose snaps every ~90 ms.
                let snap = (e * 11.0) as u32;
                let h = snap.wrapping_mul(0x9E37_79B9) >> 20;
                p.open = [0.2, 1.0, 0.6, 0.0][(h % 4) as usize];
                p.size = [0.85, 1.15, 1.0, 0.9][((h >> 2) % 4) as usize];
                *look = Some(([-0.7, 0.7, 0.0, -0.4][((h >> 4) % 4) as usize], [-0.5, 0.3, -0.8, 0.5][((h >> 6) % 4) as usize]));
                lv.dx = (e * 90.0).sin() * 1.0;
                if snap % 3 == 0 {
                    x.glyph = Glyph::Cross;
                }
            }
            React::Compass => {
                // The needle spins, slows, settles north.
                let wind = (1.0 - pr).powi(2) * TAU * 2.0;
                let a = e * 8.0 * (1.0 - pr) - wind;
                *look = Some((a.cos() * (1.0 - pr * 0.6), a.sin() * (1.0 - pr * 0.6) - pr * 0.4));
                p.open = 0.85;
                p.brow = 0.8;
                if pr > 0.85 {
                    *look = Some((0.0, -0.9));
                    p.brow = 1.8;
                }
            }
            React::Radar => {
                let a = pr * TAU;
                *look = Some((a.cos(), a.sin() * 0.7));
                p.open = 0.9;
                p.size = 1.05;
                lv.rot = 0.02;
                if e > 1.8 && e < 2.1 {
                    // The blip.
                    p.size = 1.2;
                    p.brow = 1.5;
                }
            }
            React::Metronome => {
                // Dead-even ticks; the face is the pendulum.
                let tick = (e * 2.4).sin();
                *look = Some((tick.signum() * 0.8, 0.0));
                lv.rot = tick * 0.16;
                lv.dx = tick * 3.0;
                p.open = 0.8;
                p.angry = 0.15;
            }
            React::Morse => {
                // dot dot dash: lids blink the pattern.
                const DOTS: [f32; 8] = [0.0, 0.4, 0.9, 1.3, 1.8, 2.0, 2.1, 2.2];
                let blinking = DOTS.iter().any(|&d| (e - d).abs() < 0.09 || (d >= 1.8 && (e - d) < 0.35));
                p.open = if blinking { 0.0 } else { 0.9 };
                p.brow = 0.8;
                *look = Some((0.0, -0.2));
            }
            React::Hammock => {
                p.open = 0.0;
                p.happy = 0.8;
                lv.dx = (e * 1.3).sin() * 4.0;
                lv.rot = (e * 1.3).cos() * 0.12;
                lv.dy = (e * 2.6).sin() * 0.7;
            }
            React::Popcorn => {
                if e < 1.2 {
                    // Eyes on the kernel as it arcs.
                    let u = e / 1.2;
                    *look = Some((0.5 - u * 0.5, -(u * PI).sin() * 0.9));
                    p.open = 0.95;
                    p.brow = 1.0;
                } else {
                    p.mouth = 0.3 + 0.5 * (e * 7.0).sin().abs();
                    p.happy = 0.8;
                    p.open = 0.75;
                    *look = Some((0.0, 0.1));
                }
            }
            React::CardTrick => {
                if e < 0.5 {
                    *look = Some((-0.8, 0.4));
                } else if e < 1.0 {
                    *look = Some((0.8, 0.4));
                } else if e < 1.4 {
                    // Gone. Both eyes dart between empty hands.
                    *look = Some(((e * 20.0).sin().signum() * 0.6, 0.4));
                    p.size = 1.1;
                    p.brow = 2.0;
                } else {
                    p.happy = 0.9;
                    p.brow = 1.5;
                    *look = Some((0.0, -0.2));
                }
                p.open = 0.9;
            }
            React::Breeze => {
                // Pushed by a gust, squinting into it.
                lv.dx = 4.0 * bell;
                lv.rot = 0.12 * bell;
                lv.squash = 0.08 * bell;
                p.open = 0.5;
                p.angry = 0.15;
                *look = Some((-0.6, -0.1));
            }
            React::Trapped => {
                // Mime in a box: palms on each wall in turn.
                let wall = (e * 1.6) as i32 % 4;
                let (dx, rot, lx) = match wall {
                    0 => (-4.0, -0.1, -0.8),
                    1 => (4.0, 0.1, 0.8),
                    2 => (0.0, 0.0, 0.0),
                    _ => (0.0, 0.0, 0.5),
                };
                lv.dx = dx;
                lv.rot = rot;
                lv.squash = if wall == 3 { -0.12 } else { 0.0 };
                *look = Some((lx, -0.2));
                p.open = 0.85;
                p.brow = 1.0;
            }
            React::Origami => {
                // Crease, fold, crease: head pressing down each fold.
                let fold = ((e * 2.6).fract() * PI).sin();
                lv.dy = 1.8 * fold;
                lv.squash = 0.1 * fold;
                *look = Some((0.3, 0.8));
                p.open = 0.7;
                p.angry = 0.15;
                if e > 1.9 {
                    p.happy = 0.85;
                    p.brow = 1.5;
                    *look = Some((0.0, -0.3));
                }
            }
            React::WarmHands => {
                // Rub, breathe, rub.
                let rub = (e * 14.0).sin();
                lv.dx = rub * 1.4;
                lv.dy = 0.8;
                p.open = 0.5;
                p.happy = 0.35;
                p.mouth = if e < 0.6 { 0.4 } else { 0.1 };
                *look = Some((0.0, 0.8));
                lv.rot = rub * 0.04;
            }
            React::Fireplace => {
                x.glow = Some(Color::rgb(255, 140, 60).fade(0.25 + 0.15 * (e * 7.0).sin().abs()));
                x.screen = Some(Color::rgb(255, 120, 50).fade(0.4));
                *look = Some(((e * 0.6).sin() * 0.3, 0.3));
                p.open = 0.55;
                p.happy = 0.5;
                lv.dy = (e * 0.8).sin() * 0.5;
            }
            React::Storm => {
                if e < 1.0 {
                    *look = Some(((e * 1.5).sin() * 0.5, -0.4));
                    p.open = 0.9;
                    p.size = 1.05;
                } else if e < 1.35 {
                    // The flash: clamped shut, head back.
                    p.open = 0.0;
                    lv.dy = -1.5;
                    lv.squash = 0.1;
                } else {
                    calm(p);
                    p.open = 0.8;
                    p.brow = 2.0;
                    p.mouth = 0.4;
                    *look = Some((0.0, -0.6));
                }
            }
            React::Aurora => {
                x.glow = Some(hsl((e * 60.0) % 360.0, 0.7, 0.6).fade(0.45));
                *look = Some(((e * 0.5).sin() * 0.6, -0.85));
                p.open = 1.0;
                p.size = 1.1;
                p.pupil = 1.3;
                p.happy = 0.4;
                lv.dy = (e * 0.8).sin() * 0.6;
            }
            React::Dribble => {
                // Eyes down on the ball: a bounce per dribble.
                let f = (e * 5.0).fract();
                let k = (f * TAU).sin().max(0.0);
                *look = Some(((e * 5.0).sin() * 0.5, 0.75 + 0.2 * k));
                lv.dy = -2.5 * k;
                p.open = 0.85;
                p.brow = 0.6;
                lv.rot = (e * 5.0).cos() * 0.06;
            }
            React::Jog => {
                // Steady pace, arms swinging (well, bouncing).
                let f = e * 6.0;
                lv.dy = -f.sin().abs() * 2.2;
                lv.rot = f.sin() * 0.05;
                p.open = 0.8;
                p.happy = 0.3;
                p.mouth = 0.25;
                *look = Some((0.0, -0.15));
            }
            React::Row => {
                // Catch, pull, recover: a long sweep each stroke.
                let f = (e * 1.8).fract();
                let pull = if f < 0.35 { f / 0.35 } else { 1.0 - (f - 0.35) / 0.65 };
                lv.dx = -3.0 + 6.0 * pull;
                lv.rot = -0.08 + 0.16 * pull;
                lv.dy = pull * 1.0;
                p.open = 0.8;
                p.angry = 0.2 * pull;
                *look = Some((0.0, -0.3));
            }
            // -------------------------------------------------- skits three
            React::Yodel => {
                // Yodel-ay-ee-oo: head back, wide open.
                let note = (e * 5.0).sin().abs();
                p.mouth = 0.5 + 0.8 * note;
                p.open = 0.0;
                p.happy = 1.0;
                lv.rot = -0.12;
                lv.dy = -1.5 - note;
            }
            React::AhChoo => {
                if e < 1.2 {
                    // The build: twitching, pupils shrinking.
                    let k = e / 1.2;
                    p.open = 1.0 - 0.7 * k;
                    p.mouth = 0.3 * k;
                    p.brow = 2.5 * k;
                    lv.dy = -1.8 * k;
                    lv.dx = (e * 40.0).sin() * 0.5 * k;
                } else {
                    // Stifled: clamped, deflating slowly.
                    x.glyph = Glyph::Squeeze;
                    lv.squash = 0.14 * (1.0 - pr);
                    lv.dy = 1.0 * (1.0 - pr);
                }
            }
            React::Jelly => {
                let w = (e * 16.0).sin() * (1.0 - pr);
                lv.rot = w * 0.14;
                lv.squash = (e * 16.0).cos() * 0.1 * (1.0 - pr);
                lv.dx = w * 1.2;
                p.happy = 0.4;
            }
            React::Cloudwatch => {
                p.open = 0.7;
                p.happy = 0.45;
                *look = Some((-0.9 + 1.8 * ((e * 0.4).fract()), -0.6));
                lv.dy = (e * 0.7).sin() * 0.6;
                lv.rot = 0.05;
            }
            React::Photobomb => {
                // Slides in from the edge, enormous grin.
                lv.dx = (s.area.0 - 10.0) * (1.0 - (e / 0.35).min(1.0));
                p.happy = 1.0;
                p.open = 1.0;
                p.size = 1.15;
                p.brow = 2.0;
                lv.rot = 0.08;
            }
            React::HideSeek => {
                if e < 0.7 {
                    // Ducking out of sight.
                    lv.dy = 6.0 * (e / 0.7);
                    p.open = (1.0 - e / 0.7).max(0.2);
                } else if e < 1.3 {
                    lv.dy = 7.0;
                    p.open = 0.0;
                } else {
                    // BOO — up again, delighted.
                    lv.dy = 7.0 - 8.0 * ((e - 1.3) / 0.3).min(1.0);
                    p.happy = 1.0;
                    p.open = 1.0;
                    p.brow = 2.5;
                }
            }
            React::Vogue => {
                // Strike, hold, strike.
                let ph = (e * 2.0) as i32 % 4;
                let (dx, rot, lx) = match ph {
                    0 => (-4.0, -0.12, -0.6),
                    1 => (4.0, 0.12, 0.6),
                    2 => (-3.0, 0.1, -0.3),
                    _ => (3.0, -0.1, 0.3),
                };
                lv.dx = dx;
                lv.rot = rot;
                *look = Some((lx, -0.3));
                p.open = 0.8;
                p.brow = 1.5;
                lv.dy = -((e * 2.0).fract() * PI).sin() * 1.5;
            }
            React::Flex => {
                let side = if (e / 0.8) as i32 % 2 == 0 { -1.0 } else { 1.0 };
                lv.rot = side * 0.14;
                lv.dx = side * 2.5;
                lv.squash = -0.12;
                p.happy = 0.8;
                p.brow = 1.5;
                p.open = 0.8;
                *look = Some((side * 0.5, 0.2));
            }
            React::JumpRope => {
                // Rope round, hop on the pass.
                let f = (e * 6.5).fract();
                lv.dy = -(f * TAU).sin().max(0.0) * 3.0;
                lv.rot = (f * TAU).cos() * 0.05;
                p.happy = 0.6;
                p.open = 0.85;
                p.brow = 0.8;
                *look = Some((0.0, 0.4));
            }
            React::Skip => {
                // Light double-hops drifting forward.
                let f = (e * 5.0).fract();
                lv.dy = -(f * TAU).sin().abs() * 2.6;
                lv.dx = (e * 1.2).sin() * 2.0;
                lv.rot = (e * 2.4).sin() * 0.07;
                p.happy = 0.85;
                p.open = 0.9;
                *look = Some(((e * 1.2).sin() * 0.4, -0.2));
            }
            React::Plie => {
                let dip = (pr * PI).sin();
                lv.dy = 3.5 * dip;
                lv.squash = 0.1 * dip;
                p.open = 0.6;
                p.happy = 0.5;
                p.brow = 0.8;
                lv.rot = 0.0;
                *look = Some((0.0, -0.3));
            }
            React::Karate => {
                // Jab left, jab right, scowl.
                let jab = (e * 7.0).fract();
                let side = (e * 7.0) as i32 % 2;
                lv.dx = (if side == 0 { -1.0 } else { 1.0 }) * (jab * PI).sin() * 4.0;
                lv.rot = (if side == 0 { -1.0 } else { 1.0 }) * 0.1 * (jab * PI).sin();
                p.angry = 0.5;
                p.open = 0.7;
                p.brow = -2.0;
                *look = Some(((if side == 0 { -0.8 } else { 0.8 }), 0.2));
            }
            React::Sumo => {
                // Heavy alternating stomps, wide and low.
                let st = (e * 3.4).sin();
                lv.squash = 0.16;
                lv.dy = st.abs() * 1.4;
                lv.rot = st.signum() * 0.1;
                lv.dx = st * 1.5;
                p.angry = 0.4;
                p.open = 0.6;
                *look = Some((0.0, 0.3));
            }
            React::Fencing => {
                // Lunge, recover, lunge.
                let f = (e * 3.0).fract();
                let lunge = if f < 0.3 { (f / 0.3 * PI).sin() } else { 0.0 };
                lv.dx = lunge * 5.0;
                lv.rot = lunge * 0.1;
                *look = Some((0.85, 0.1));
                p.open = 0.75;
                p.angry = 0.3;
                p.brow = -1.0;
            }
            React::Archery => {
                if e < 1.1 {
                    // Drawing: squint down the shaft.
                    *look = Some((0.7, 0.0));
                    p.wink_l = 0.8;
                    p.open = 0.6;
                    p.brow = -1.0;
                    lv.rot = -0.1;
                    lv.dx = -1.5 * (e / 1.1);
                } else {
                    // Loose: watch it fly.
                    *look = Some((0.9, -0.3));
                    p.open = 0.9;
                    p.brow = 1.5;
                    lv.rot = 0.02;
                }
            }
            React::Darts => {
                if e < 1.4 {
                    // Aiming: one eye shut, micro-adjusting.
                    p.wink_r = 0.85;
                    *look = Some((0.8 + (e * 9.0).sin() * 0.05, -0.1));
                    p.open = 0.6;
                    p.brow = -0.8;
                    lv.dy = -0.5 * bell;
                } else {
                    // Watching the dart land.
                    p.open = 1.0;
                    p.brow = 1.8;
                    p.happy = 0.6;
                    *look = Some((0.9, 0.0));
                }
            }
            React::Bowling => {
                if e < 0.9 {
                    // The approach and swing.
                    lv.dx = -3.0 + 4.0 * (e / 0.9);
                    lv.rot = -0.1 + 0.16 * (e / 0.9);
                    *look = Some((0.8, 0.2));
                    p.open = 0.8;
                    p.angry = 0.2;
                } else {
                    // Watching it roll, willing it over.
                    *look = Some((0.9 - 0.3 * (e - 0.9), 0.3));
                    p.open = 0.95;
                    p.brow = 1.4;
                    lv.dx = (e * 6.0).sin() * 0.8;
                }
            }
            React::Golf => {
                if e < 1.4 {
                    // Reading the green, crouched low.
                    *look = Some((-0.6 + 0.8 * (e / 1.4), 0.7));
                    p.open = 0.5;
                    p.size_l = 1.15;
                    lv.dy = 1.5;
                    lv.rot = 0.08;
                } else {
                    // The putt: track the ball, then a fist of joy.
                    let u = ((e - 1.4) / 0.8).min(1.0);
                    *look = Some((0.5 + 0.4 * u, 0.5 - 0.5 * u));
                    p.open = 0.9;
                    p.happy = u * 0.9;
                    p.brow = u * 2.0;
                    lv.dy = -bell;
                }
            }
            React::Fishing => {
                if e < 2.1 {
                    // Watching the bobber, dead still, half-lidded.
                    *look = Some((0.5, 0.65));
                    p.open = 0.5;
                    p.happy = 0.2;
                    lv.dy = (e * 1.2).sin() * 0.4;
                    lv.rot = 0.05;
                } else {
                    // A bite! Reeling, wide-eyed.
                    p.open = 1.0;
                    p.brow = 2.5;
                    p.size = 1.15;
                    *look = Some((0.5, 0.3));
                    lv.dx = (e * 30.0).sin() * 0.6;
                    lv.dy = -bell * 2.0;
                }
            }
            React::Surf => {
                // Riding: knees bent, reading the wave.
                let carve = (e * 1.8).sin();
                lv.rot = carve * 0.15;
                lv.dx = carve * 3.0;
                lv.dy = 1.0 + carve.abs() * 0.5;
                lv.squash = 0.08;
                p.open = 0.85;
                p.happy = 0.5;
                *look = Some((carve * 0.6, -0.2));
            }
            React::Ski => {
                // Tucked low, bumping moguls, quick glances ahead.
                lv.dy = 1.5 + (e * 9.0).sin().abs() * 1.2;
                lv.squash = 0.12;
                lv.rot = (e * 9.0).cos() * 0.06;
                p.open = 0.7;
                p.brow = 0.8;
                *look = Some(((e * 2.0).sin() * 0.5, 0.5));
            }
            React::Pinwheel => {
                // Blowing steady, eyes on the wheel.
                *look = Some((0.4, 0.3));
                p.mouth = 0.5;
                p.open = 0.8;
                p.happy = 0.4;
                lv.dy = -0.4;
                lv.rot = 0.05;
            }
            React::BubbleWand => {
                // Blow, watch them rise.
                p.mouth = 0.4;
                p.open = 0.85;
                p.happy = 0.6;
                *look = Some(((e * 0.9).sin() * 0.4, -0.4 - 0.2 * (e * 0.5).sin()));
                lv.dy = -0.5;
            }
            React::Polish => {
                // Circular rubbing, admiring the gleam.
                let a = e * 7.0;
                lv.dx = a.cos() * 1.6;
                lv.dy = a.sin() * 1.0;
                p.happy = 0.7;
                p.open = 0.8;
                x.shine = e > 1.4;
                *look = Some((0.0, 0.4));
            }
            React::Flashlight => {
                // A beam sweeping the dark in slow pans.
                let pan = (e * 1.0).sin();
                *look = Some((pan * 0.95, -0.1));
                p.open = 1.0;
                p.size = 1.05;
                p.pupil = 1.3;
                x.screen = Some(GOLD.fade(0.35));
                lv.rot = pan * 0.05;
            }
            React::Shush => {
                p.open = 0.0;
                p.happy = 0.2;
                p.angry = 0.35;
                p.brow = -1.5;
                x.caption = Some("shh");
                lv.rot = -0.04;
            }
            React::SlowClap => {
                // Half-lidded, nodding on each slow beat.
                p.open = 0.45;
                p.brow = 1.0;
                p.happy = 0.2;
                lv.dy = -((e * 2.4).fract() * PI).sin() * 1.6;
                *look = Some((0.0, -0.1));
            }
            React::Fidget => {
                // Pupils pinned to the spinner, going round fast.
                let a = e * 12.0;
                *look = Some((a.cos() * 0.8, a.sin() * 0.8));
                p.open = 0.9;
                p.pupil = 0.8;
                lv.rot = a.sin() * 0.03;
            }
            React::Coin => {
                if e < 0.7 {
                    // Up it goes: eyes follow the arc.
                    *look = Some((0.3, -0.9));
                    p.brow = 1.5;
                    p.open = 1.0;
                } else if e < 1.3 {
                    // Snatched out of the air.
                    *look = Some((0.4, 0.5));
                    p.open = 0.8;
                    lv.dy = bell;
                } else {
                    // Peeking at the palm.
                    *look = Some((0.4, 0.85));
                    p.open = 0.7;
                    p.brow = 1.5;
                    p.happy = 0.5;
                }
            }
            React::Letter => {
                // Fold, seal, post off the edge.
                *look = Some((0.3, 0.7));
                p.open = 0.8;
                p.happy = 0.5;
                if e > 1.8 {
                    *look = Some((0.9, -0.3));
                    lv.dy = -bell;
                }
            }
            React::Selfie => {
                x.prop = Some(Prop::Phone);
                x.prop_k = (e / 0.4).min(1.0);
                // Phone held high, grinning up at it.
                *look = Some((0.55, -0.7));
                p.happy = 0.9;
                p.open = 1.0;
                p.brow = 1.5;
                lv.rot = 0.1;
                if e > 1.1 && e < 1.35 {
                    lv.flash = 0.8;
                    p.open = 0.0;
                }
            }
            React::Marshmallow => {
                x.glow = Some(Color::rgb(255, 140, 60).fade(0.3 + 0.15 * (e * 7.0).sin().abs()));
                *look = Some((0.45, 0.55));
                p.open = 0.7;
                p.happy = 0.4;
                lv.rot = (e * 1.4).sin() * 0.06;
                if e > 2.4 {
                    // A bite: toasty.
                    p.mouth = 0.5;
                    p.happy = 0.9;
                    p.blush = 0.4;
                }
            }
            React::Meteor => {
                // A streak: eyes whip across the sky, then awe.
                if e < 0.9 {
                    *look = Some((-0.9 + 2.0 * e, -0.7));
                    p.open = 1.0;
                    p.size = 1.1;
                    p.brow = 1.5;
                } else {
                    p.open = 1.0;
                    p.size = 1.2;
                    p.pupil = 1.3;
                    p.brow = 2.0;
                    p.mouth = 0.4;
                    *look = Some((0.9, -0.8));
                    x.shine = true;
                }
            }
            React::Moth => {
                // Erratic flutter-tracking, getting bumped.
                let a = e * 5.0;
                *look = Some(((a * 1.7).sin() * 0.85, -0.4 + (a * 2.3).cos() * 0.35));
                lv.dx = (e * 11.0).sin() * 1.2;
                p.open = 1.0;
                p.brow = 1.2;
                p.cross = if e > 2.0 { 0.5 } else { 0.0 };
            }
            React::Spring => {
                if e < 0.7 {
                    // Coiling down.
                    let k = e / 0.7;
                    lv.dy = 3.5 * k;
                    lv.squash = 0.25 * k;
                    p.open = 0.7;
                    p.angry = 0.2;
                } else {
                    // Sproing!
                    let k = ((e - 0.7) / 0.6).min(1.0);
                    lv.dy = 3.5 - 8.0 * (k * PI / 2.0).sin();
                    lv.squash = -0.2 * (1.0 - k);
                    p.open = 1.0;
                    p.brow = 2.5;
                    p.happy = 0.7;
                    *look = Some((0.0, -0.5));
                }
            }
            React::YoYo => {
                // Down, sleep, snap back up.
                let f = (e * 1.6).fract();
                let dy = if f < 0.4 { f / 0.4 } else { 1.0 - (f - 0.4) / 0.6 };
                *look = Some((0.4, -0.4 + 1.2 * dy));
                p.open = 0.8;
                p.happy = 0.4;
                lv.dy = dy * 1.5;
                lv.rot = 0.04 * dy;
            }
            React::Jenga => {
                if e < 2.0 {
                    // Easing a block out, hardly breathing.
                    *look = Some((0.0, 0.6));
                    p.open = 0.6;
                    p.pupil = 0.8;
                    p.brow = 1.2;
                    lv.dx = (e * 3.0).sin() * 0.3;
                    lv.dy = 0.5;
                } else {
                    // It held: huge relieved smile.
                    p.happy = 1.0;
                    p.open = 0.9;
                    p.brow = 0.5;
                    lv.dy = -bell;
                }
            }
            React::Dominoes => {
                // Eyes chase the clicks across, flinch at the last.
                let u = (e / 1.6).min(1.0);
                *look = Some((-0.85 + 1.7 * u, 0.5));
                p.open = 0.9;
                p.brow = 1.2 * u;
                if u >= 1.0 {
                    p.happy = 0.7;
                    lv.dy = -bell;
                }
            }
            React::Box => {
                // Duck, weave, jab-jab.
                let f = (e * 4.5).fract();
                let move_ = (e * 4.5) as i32 % 3;
                let k = (f * PI).sin();
                match move_ {
                    0 => {
                        lv.dx = -2.5 * k;
                        lv.rot = -0.1 * k;
                    }
                    1 => {
                        lv.dx = 2.5 * k;
                        lv.rot = 0.1 * k;
                    }
                    _ => {
                        lv.dx = 4.0 * k;
                        lv.squash = -0.08 * k;
                    }
                }
                p.angry = 0.35;
                p.open = 0.75;
                p.brow = -1.0;
                *look = Some((0.0, 0.1));
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
}
