use super::*;
impl Eyes {
    // ------------------------------------------------------------- paint

    pub fn draw(&self, sc: &mut Scene, pt: &Paint) {
        let p = &self.pose;
        let lv = &self.live;
        let j = &self.jig;
        // Keep the face inside the pill: skits may slide it around, but it
        // should never fully leave the window (that reads as the island
        // blinking off, not as a dance move).
        let lx = (pt.bounds.w / 2.0 / pt.scale - 14.0).max(4.0);
        let ly = (pt.bounds.h / 2.0 / pt.scale - 14.0).max(4.0);
        let xf = Xf::new(pt.cx, pt.cy, pt.scale, (p.dx + lv.dx).clamp(-lx, lx), (p.dy + lv.dy + j.y).clamp(-ly, ly), p.rot + lv.rot + j.rot, p.squash + lv.squash + j.sq);
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
            sc.polyline(pts, 1.3 * pt.scale, pal().text.fade(0.9));
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
        if pt.props {
            if let Some(pr) = x.prop {
                prop(sc, &xf, pr, x.prop_k, pt.scale, self.t);
            }
        }
        let at = |x: f32, y: f32| (pt.cx + x * pt.scale, pt.cy + y * pt.scale);
        if x.pumpkin && pt.props {
            pumpkin(sc, at(34.0, 4.0), pt.scale);
        }
        if let Some(cap) = x.caption {
            let (cx, cy) = at(31.0, 1.0);
            let w = 20.0 * pt.scale;
            sc.text(cap, Rect::new(cx - w / 2.0, cy - 6.0 * pt.scale, w, 12.0 * pt.scale), 8.0 * pt.scale, Weight::Semibold, pal().text_3, Align::Center);
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
            _ => pal().sclera.into(),
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
                let col = if x.glow.is_some() { Color::rgb(6, 30, 14) } else { pal().pupil };
                let w = 1.0 * k * p.pupil.powf(1.5);
                put(sc, clip_ellipse(&lid, px, py, w, ry * 0.92, 10), col.into());
            }
            _ => {
                let pr = 3.1 * k * p.pupil;
                if let Some(c) = tint {
                    put(sc, clip_ellipse(&lid, px, py, pr * 1.7, pr * 1.7, 18), c.into());
                }
                put(sc, clip_ellipse(&lid, px, py, pr, pr, 16), pal().pupil.into());
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
        let pupil = tint.map(|c| c.mix(Color::rgb(0, 0, 0), 0.3)).unwrap_or(pal().pupil);
        let (ppx, ppy) = (snap(px), snap(py));
        let half = (2.0 * self.pose.pupil).max(1.0);
        let mut x = -lid.rx;
        while x < lid.rx - 0.01 {
            let xc = x + cell / 2.0;
            if let Some((t, b)) = lid.span(xc) {
                let (t, b) = (snap(t), snap(b));
                if b > t {
                    sc.rect(xf.rect(ex + x, t, cell, b - t), 0.0, pal().sclera);
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
                sc.polyline(vec![xf.p(ex - d, -d), xf.p(ex + d, d)], w, pal().sclera);
                sc.polyline(vec![xf.p(ex - d, d), xf.p(ex + d, -d)], w, pal().sclera);
            }
            Glyph::Spiral => {
                let pts = (0..=40)
                    .map(|i| {
                        let f = i as f32 / 40.0;
                        let a = f * TAU * 2.3 + t * 7.0 * side;
                        xf.p(ex + a.cos() * rx * 0.95 * f, a.sin() * ry * 0.95 * f)
                    })
                    .collect();
                sc.polyline(pts, 1.6 * xf.s, pal().sclera);
            }
            Glyph::Squeeze => {
                let (dx, dy) = (rx * 0.55, ry * 0.62);
                sc.polyline(vec![xf.p(ex + side * dx, -dy), xf.p(ex - side * dx, 0.0), xf.p(ex + side * dx, dy)], w, pal().sclera);
            }
            Glyph::Ring => {
                let mut pts: Vec<_> = ellipse(ex, 0.0, rx * 0.8, ry * 0.8, 20).into_iter().map(|(x, y)| xf.p(x, y)).collect();
                pts.push(pts[0]);
                sc.polyline(pts, 2.0 * xf.s, pal().sclera);
            }
            Glyph::Question => {
                let (cx, cy) = xf.p(ex, 0.0);
                let r = 9.0 * xf.s;
                sc.text("?", Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r), 13.0 * xf.s, Weight::Bold, pal().sclera, Align::Center);
            }
            Glyph::Minus => {
                sc.polyline(vec![xf.p(ex - rx * 0.5, 0.0), xf.p(ex + rx * 0.5, 0.0)], w, pal().sclera);
            }
            Glyph::Moon => {
                let mut pts: Vec<(f32, f32)> = (0..=14)
                    .map(|i| {
                        let a = -PI * 0.62 + PI * 1.24 * i as f32 / 14.0;
                        xf.p(ex + a.cos() * rx * 0.85, a.sin() * ry * 0.85)
                    })
                    .collect();
                for i in (0..=14).rev() {
                    let a = -PI * 0.45 + PI * 0.9 * i as f32 / 14.0;
                    pts.push(xf.p(ex + rx * 0.35 + a.cos() * rx * 0.62, a.sin() * ry * 0.62));
                }
                sc.poly(pts, GOLD);
            }
            Glyph::Bolt => {
                let pts: Vec<(f32, f32)> =
                    [(rx * 0.15, -ry * 0.9), (-rx * 0.3, ry * 0.1), (rx * 0.05, ry * 0.1), (-rx * 0.15, ry * 0.9), (rx * 0.35, -ry * 0.15), (0.0, -ry * 0.15)]
                        .iter()
                        .map(|&(u, v)| xf.p(ex + u, v))
                        .collect();
                sc.poly(pts, GOLD);
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
            sc.polyline(pts, 1.7 * xf.s, pal().text.fade(show * 0.95));
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
                    sc.text(s, Rect::new(x - sz, y - sz, 2.0 * sz, 2.0 * sz), sz, Weight::Bold, pal().text.fade(a * 0.85), Align::Center);
                }
                Kind::Note => {
                    let sz = p.size * k;
                    let s = if p.hue > 0.5 { "\u{266B}" } else { "\u{266A}" };
                    let col = pt.accent.unwrap_or(pal().text);
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
                    sc.polyline(pts, 1.3 * k, pal().text.fade(0.85 * fade));
                }
                Kind::Dash => {
                    // Trails behind the way it flies.
                    let len = 7.0 * k * (1.0 - p.age / p.life);
                    sc.line(x, y, x - p.vx.signum() * len, y, 1.1 * k, pal().text.fade(0.7 * a));
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
                    sc.poly(vec![at(-w / 2.0, -h / 2.0), at(w / 2.0, -h / 2.0), at(w / 2.0, h / 2.0), at(-w / 2.0, h / 2.0)], pal().sclera.fade(a));
                    for i in 0..3 {
                        let v = -h / 2.0 + h * (0.3 + 0.2 * i as f32);
                        let (x0, y0) = at(-w * 0.32, v);
                        let (x1, y1) = at(w * if i == 2 { 0.05 } else { 0.32 }, v);
                        sc.line(x0, y0, x1, y1, 0.6 * k, pal().pupil.fade(0.7 * a));
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
                    sc.text("?", Rect::new(x - sz, y - sz, 2.0 * sz, 2.0 * sz), sz, Weight::Bold, pal().text.fade(a), Align::Center);
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
                    sc.line(fx, fy - 4.0 * k, fx, fy + 3.0 * k, 0.9 * k, pal().text_3);
                    sc.line(fx - 3.0 * k, fy - 1.5 * k, fx + 3.0 * k, fy - 1.5 * k, 0.9 * k, pal().text_3);
                    let leg = pal().text.fade(0.7);
                    sc.line(x - 1.6 * k, y + 1.4 * k, x - 1.6 * k, y + 3.0 * k, 0.8 * k, leg);
                    sc.line(x + 1.6 * k, y + 1.4 * k, x + 1.6 * k, y + 3.0 * k, 0.8 * k, leg);
                    for (ox, oy, r) in [(-1.8, 0.0, 1.9), (0.4, -0.9, 2.1), (1.8, 0.3, 1.8)] {
                        sc.circle(x + ox * k, y + oy * k, r * k, WHITE);
                    }
                    sc.circle(x + 3.8 * k, y - 0.6 * k, 1.3 * k, Color::rgb(120, 120, 130));
                }
                Kind::Dot => sc.circle(x, y, 0.9 * k, pal().text.fade(a)),
                Kind::Steam => {
                    let pts: Vec<(f32, f32)> = (0..=5)
                        .map(|i| {
                            let yy = y - i as f32 * 2.0 * k;
                            (x + (i as f32 * 1.4 + p.age * 3.5 + p.hue * 6.0).sin() * 1.4 * k, yy)
                        })
                        .collect();
                    sc.polyline(pts, 0.9 * k, pal().text.fade(0.45 * a));
                }
                Kind::Fly => {
                    sc.circle(x, y, 2.4 * k, GOLD.fade(0.22 * a));
                    sc.circle(x, y, 1.1 * k, GOLD.fade(a));
                }
                Kind::Sym(txt) => {
                    let sz = p.size * k;
                    let col = self.work.map(|w| w.tint.map(|[r, g, b]| Color::rgb(r, g, b)).unwrap_or(work_color(w.cat))).unwrap_or(pal().text);
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

/// Colour of a work theme's symbols.
fn work_color(c: AppCategory) -> Color {
    use AppCategory::*;
    let col = match c {
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
    };
    // Pastels wash out on a light body.
    if pal().light {
        col.mix(Color::rgb(10, 12, 20), 0.30)
    } else {
        col
    }
}

pub(super) fn is_birthday(s: &Senses) -> bool {
    s.set.birthday.is_some_and(|(m, d)| s.local.month() == m as u32 && s.local.day() == d as u32)
}

/// Mood ring: hue follows the hour, saturation the water intake, and a
/// busy CPU pulls it toward red.
pub(super) fn ring_color(s: &Senses) -> Color {
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
pub(super) struct Lid {
    pub(super) rx: f32,
    pub(super) ry: f32,
    pub(super) open: f32,
    pub(super) angry: f32,
    pub(super) sad: f32,
    pub(super) happy: f32,
    pub(super) side: f32,
    pub(super) style: EyeStyle,
}

impl Lid {
    pub(super) fn span(&self, x: f32) -> Option<(f32, f32)> {
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

    pub(super) fn gap(&self) -> f32 {
        (0..=16).filter_map(|i| self.span(-self.rx + 2.0 * self.rx * i as f32 / 16.0)).map(|(t, b)| b - t).fold(0.0, f32::max)
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

pub(super) fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, n: usize) -> Vec<(f32, f32)> {
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

pub(super) fn sparkle(x: f32, y: f32, r: f32) -> Vec<(f32, f32)> {
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

/// Face transform: squash, rotate about the face centre, then offset.
pub(super) struct Xf {
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

    pub(super) fn p(&self, x: f32, y: f32) -> (f32, f32) {
        let (x, y) = (x * self.sx, y * self.sy);
        let (x, y) = (x * self.cos - y * self.sin, x * self.sin + y * self.cos);
        (self.cx + (x + self.dx) * self.s, self.cy + (y + self.dy) * self.s)
    }

    /// Axis-aligned cell (pixel style ignores rotation on purpose).
    pub(super) fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        let (px, py) = self.p(x, y);
        Rect::new(px, py, w * self.s * self.sx, h * self.s * self.sy)
    }
}
