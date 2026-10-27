use super::*;
pub(super) fn shades(sc: &mut Scene, xf: &Xf, k: f32) {
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

pub(super) fn hat(sc: &mut Scene, xf: &Xf, h: Hat, k: f32) {
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

/// Held items for the object skits: chunky silhouettes that read at 40 px.
/// Everything is drawn in face space, so props squash and rock with the face.
pub(super) fn prop(sc: &mut Scene, xf: &Xf, which: Prop, k: f32, s: f32, t: f32) {
    let pp = |pts: &[(f32, f32)]| pts.iter().map(|&(x, y)| xf.p(x, y)).collect::<Vec<_>>();
    let fill = |pts: Vec<(f32, f32)>| pts.into_iter().map(|(x, y)| xf.p(x, y)).collect::<Vec<_>>();
    let dark = Color::rgb(38, 40, 52);
    let line = pal().text_3;
    match which {
        Prop::Laptop => {
            sc.rect(xf.rect(-9.0, 14.0, 18.0, 2.4), 1.0 * s, dark);
            let top = 14.0 - 9.0 * k;
            let w2 = 7.0 * (0.55 + 0.45 * k);
            sc.poly(pp(&[(-7.0, 14.0), (7.0, 14.0), (w2, top), (-w2, top)]), dark);
            if k > 0.4 {
                let w3 = w2 - 1.2;
                sc.poly(pp(&[(-5.8, 13.2), (5.8, 13.2), (w3, top + 1.4), (-w3, top + 1.4)]), ICE.fade(0.55));
            }
        }
        Prop::Book => {
            let w = 2.0 + 7.5 * k;
            for side in [-1.0f32, 1.0] {
                sc.poly(pp(&[(0.0, 9.8), (side * w, 8.6), (side * (w + 0.6), 15.6), (0.0, 16.2)]), Color::rgb(178, 84, 84));
            }
            if k > 0.3 {
                for side in [-1.0f32, 1.0] {
                    sc.poly(pp(&[(0.0, 10.4), (side * (w - 0.7), 9.6), (side * (w - 0.2), 15.0), (0.0, 15.6)]), pal().sclera);
                    for i in 0..2 {
                        let y = 11.6 + 1.7 * i as f32;
                        let (x0, y0) = xf.p(side * 1.4, y);
                        let (x1, y1) = xf.p(side * (w - 1.6), y - 0.5);
                        sc.line(x0, y0, x1, y1, 0.55 * s, pal().pupil.fade(0.55));
                    }
                }
            }
        }
        Prop::Phone => {
            let cy = 13.0 - 2.5 * k;
            sc.rect(xf.rect(8.6, cy - 4.5, 5.0, 9.0), 1.4 * s, dark);
            if k > 0.2 {
                let glow = 0.5 + 0.25 * (t * 3.0).sin();
                sc.rect(xf.rect(9.5, cy - 3.6, 3.2, 7.2), 0.7 * s, Color::rgb(148, 163, 253).fade(glow));
            }
        }
        Prop::Mug => {
            let cx = -9.5 - 1.5 * k;
            let cy = 15.0 - 5.0 * k;
            sc.rect(xf.rect(cx - 3.0, cy - 2.8, 6.0, 5.6), 1.1 * s, Color::rgb(220, 90, 90));
            let pts: Vec<(f32, f32)> = (0..=8)
                .map(|i| {
                    let a = PI / 2.0 + PI * i as f32 / 8.0;
                    xf.p(cx - 3.4 + 1.9 * a.cos(), cy + 1.9 * a.sin())
                })
                .collect();
            sc.polyline(pts, 1.0 * s, Color::rgb(220, 90, 90));
        }
        Prop::Cookie => {
            let r = 3.4 * (1.0 - 0.55 * k);
            if r > 0.9 {
                sc.poly(fill(ellipse(9.0, 14.0, r, r, 14)), Color::rgb(196, 148, 84));
                for (dx, dy) in [(-0.9, -0.8), (0.9, 0.5), (-0.1, 1.0)] {
                    let (cx, cy) = xf.p(9.0 + dx * r * 0.6, 14.0 + dy * r * 0.6);
                    sc.circle(cx, cy, 0.65 * s, Color::rgb(92, 62, 42));
                }
            }
        }
        Prop::Gamepad => {
            sc.rect(xf.rect(-7.5, 12.6, 15.0, 4.8), 2.3 * s, dark);
            sc.poly(fill(ellipse(-5.6, 16.0, 2.0, 2.4, 12)), dark);
            sc.poly(fill(ellipse(5.6, 16.0, 2.0, 2.4, 12)), dark);
            let (dx, dy) = xf.p(-4.2, 14.6);
            sc.circle(dx, dy, 1.0 * s, pal().sclera.fade(0.8));
            let press = (t * 14.0).sin() > 0.0;
            let (bx, by) = xf.p(3.4, 14.6);
            sc.circle(bx, by, 0.95 * s, if press { PINK } else { pal().sclera.fade(0.7) });
            let (bx2, by2) = xf.p(5.6, 14.2);
            sc.circle(bx2, by2, 0.95 * s, if press { pal().sclera.fade(0.7) } else { ICE });
        }
        Prop::Guitar => {
            let wood = Color::rgb(198, 122, 62);
            sc.poly(fill(ellipse(1.0, 14.6, 4.2, 3.4, 14)), wood);
            sc.poly(fill(ellipse(5.4, 12.6, 2.8, 2.4, 12)), wood);
            sc.polyline(pp(&[(6.0, 11.8), (13.5, 7.6)]), 1.6 * s, Color::rgb(140, 90, 50));
            let (hx, hy) = xf.p(1.0, 14.6);
            sc.circle(hx, hy, 1.2 * s, Color::rgb(58, 38, 24));
            let sy = 12.2 + k * 4.4;
            sc.polyline(pp(&[(2.2, sy - 0.8), (2.2, sy + 0.8)]), 1.0 * s, pal().sclera.fade(0.9));
        }
        Prop::Mic => {
            let my = 12.5 - 3.0 * k;
            sc.polyline(pp(&[(9.5, my + 5.0), (9.5, my + 0.6)]), 1.5 * s, dark);
            let (mx, myy) = xf.p(9.5, my - 1.0);
            sc.circle(mx, myy, 2.6 * s, Color::rgb(96, 98, 112));
            let pts: Vec<(f32, f32)> = (0..=6)
                .map(|i| {
                    let a = PI * i as f32 / 6.0;
                    xf.p(9.5 - 2.6 * a.cos(), my - 1.0 - 0.6 * a.sin())
                })
                .collect();
            sc.polyline(pts, 0.6 * s, line.fade(0.7));
        }
        Prop::Dumbbell => {
            let y = 16.0 - 10.0 * k;
            sc.polyline(pp(&[(-8.5, y), (8.5, y)]), 1.3 * s, line);
            for side in [-1.0f32, 1.0] {
                sc.rect(xf.rect(side * 8.5 - 1.1, y - 2.8, 2.2, 5.6), 0.6 * s, dark);
                sc.rect(xf.rect(side * 10.9 - 1.1, y - 2.0, 2.2, 4.0), 0.6 * s, dark);
            }
        }
        Prop::Wand => {
            let tip = (11.0, 4.5 - 3.0 * k);
            sc.polyline(pp(&[(7.5, 11.5), tip]), 1.1 * s, pal().text.fade(0.9));
            sc.poly(fill(sparkle(tip.0, tip.1, 1.4 + 1.4 * k)), GOLD);
        }
        Prop::Camera => {
            let cy = 13.5 - 3.5 * k;
            sc.rect(xf.rect(-5.0, cy - 3.0, 10.0, 6.0), 1.3 * s, dark);
            sc.rect(xf.rect(-1.8, cy - 4.4, 3.6, 1.6), 0.5 * s, dark);
            let (lx, ly) = xf.p(0.0, cy);
            sc.circle(lx, ly, 2.4 * s, Color::rgb(18, 20, 28));
            sc.circle(lx, ly, 1.1 * s, ICE.fade(0.85));
        }
        Prop::Plant => {
            sc.poly(pp(&[(9.0, 13.4), (15.0, 13.4), (13.9, 17.0), (10.1, 17.0)]), Color::rgb(202, 110, 66));
            let h = 6.5 * k;
            if h > 0.5 {
                sc.polyline(pp(&[(12.0, 13.4), (12.0, 13.4 - h)]), 1.0 * s, Color::rgb(74, 160, 80));
                for side in [-1.0f32, 1.0] {
                    sc.poly(fill(ellipse(12.0 + side * 1.7, 13.4 - h * 0.55, 1.6, 0.8, 10)), Color::rgb(74, 160, 80));
                }
                if k > 0.8 {
                    let (fx, fy) = xf.p(12.0, 13.4 - h - 0.8);
                    sc.circle(fx, fy, 1.3 * s, PINK);
                }
            }
        }
        Prop::Brush => {
            let tip = (8.0 + 3.5 * (k * 2.0 - 1.0), 11.5 + 1.2 * (k * TAU).sin());
            sc.polyline(pp(&[(10.5, 7.5), tip]), 1.3 * s, Color::rgb(170, 120, 70));
            sc.poly(pp(&[tip, (tip.0 - 1.4, tip.1 + 1.2), (tip.0 + 0.4, tip.1 + 2.0)]), PINK);
        }
        Prop::Umbrella => {
            let w = 11.5 * k;
            if w > 1.0 {
                let pts: Vec<(f32, f32)> = (0..=16)
                    .map(|i| {
                        let a = PI * i as f32 / 16.0;
                        xf.p(-w * a.cos(), -15.0 - 5.5 * k * a.sin())
                    })
                    .collect();
                sc.polyline(pts, 1.6 * s, PINK);
                for i in [0.25, 0.5, 0.75] {
                    let a = PI * i;
                    sc.polyline(pp(&[(0.0, -15.0), (-w * a.cos(), -15.0 - 5.5 * k * a.sin())]), 0.7 * s, PINK.fade(0.6));
                }
                sc.polyline(pp(&[(0.0, -15.0 - 5.5 * k - 0.5), (0.0, -15.0), (0.0, -5.0), (1.6, -4.0)]), 1.0 * s, line);
            }
        }
        Prop::Balloon => {
            let r = 1.2 + 3.4 * k;
            let by = -11.0 - 6.0 * k;
            sc.polyline(pp(&[(8.0, -7.0), (9.2, -10.0), (8.6, by + r * 0.9)]), 0.7 * s, line);
            sc.poly(fill(ellipse(8.6, by, r * 0.85, r, 16)), PINK.fade(0.9));
            let (kx, ky) = xf.p(8.6, by + r);
            sc.circle(kx, ky, 0.7 * s, PINK);
        }
        Prop::Teddy => {
            let sq = 1.0 - 0.18 * (k * PI).sin();
            let brown = Color::rgb(160, 110, 70);
            for (ox, oy, r) in [(-11.6, 8.6, 0.9), (-7.4, 8.6, 0.9)] {
                sc.poly(fill(ellipse(ox, oy, r, r, 10)), brown);
            }
            sc.poly(fill(ellipse(-9.5, 14.2, 3.2 * sq, 2.9, 14)), brown);
            sc.poly(fill(ellipse(-9.5, 10.0, 2.3, 2.1, 14)), brown);
            let (nx, ny) = xf.p(-9.5, 10.6);
            sc.circle(nx, ny, 0.6 * s, dark);
        }
        Prop::Drum => {
            sc.poly(fill(ellipse(0.0, 13.0, 5.5, 2.2, 14)), pal().sclera);
            sc.rect(xf.rect(-5.5, 13.0, 11.0, 4.5), 1.0 * s, Color::rgb(178, 84, 84));
            sc.polyline(pp(&[(-5.5, 13.0), (-5.5, 17.0), (5.5, 17.0), (5.5, 13.0)]), 1.0 * s, dark);
            for (sx, ph) in [(-6.5, 0.0), (6.5, 0.5)] {
                let tip = 10.0 - 3.5 * ((k + ph).fract() * PI).sin();
                sc.polyline(pp(&[(sx * 1.4, 6.5), (sx, tip)]), 0.9 * s, Color::rgb(190, 140, 84));
            }
        }
        Prop::Telescope => {
            let ext = 4.0 + 3.5 * k;
            sc.polyline(pp(&[(4.0, 12.0), (4.0 + ext, 6.5 - 2.0 * k)]), 3.2 * s, Color::rgb(140, 110, 60));
            sc.polyline(pp(&[(4.0 + ext * 0.6, 9.6 - 1.2 * k), (4.0 + ext, 6.5 - 2.0 * k)]), 2.0 * s, GOLD);
            let (ex, ey) = xf.p(4.0 + ext + 0.8, 6.1 - 2.0 * k);
            sc.circle(ex, ey, 1.4 * s, ICE.fade(0.7));
            sc.polyline(pp(&[(3.0, 12.0), (0.5, 16.5)]), 0.9 * s, line);
            sc.polyline(pp(&[(5.0, 12.0), (7.5, 16.5)]), 0.9 * s, line);
        }
        Prop::Newspaper => {
            sc.rect(xf.rect(-10.5, 8.5, 21.0, 8.5), 0.5 * s, pal().sclera);
            sc.polyline(pp(&[(0.0, 8.5), (0.0, 17.0)]), 0.6 * s, pal().pupil.fade(0.5));
            for side in [-1.0f32, 1.0] {
                for i in 0..3 {
                    let y = 10.5 + 2.0 * i as f32;
                    let (x0, y0) = xf.p(side * 1.6, y);
                    let (x1, y1) = xf.p(side * 9.0, y);
                    sc.line(x0, y0, x1, y1, 0.6 * s, pal().pupil.fade(0.5));
                }
                let (hx0, hy0) = xf.p(side * 1.6, 9.4);
                let (hx1, hy1) = xf.p(side * 9.0, 9.4);
                sc.line(hx0, hy0, hx1, hy1, 1.2 * s, pal().pupil.fade(0.8));
            }
        }
        Prop::Pizza => {
            // A slice being pulled right and up, with a cheese string.
            let ox = 7.0 + 3.0 * k;
            let oy = 15.5 - 4.0 * k;
            sc.poly(pp(&[(ox - 3.4, oy - 2.5), (ox + 3.4, oy - 2.5), (ox, oy + 3.0)]), Color::rgb(244, 180, 70));
            sc.polyline(pp(&[(ox - 3.4, oy - 2.5), (ox + 3.4, oy - 2.5)]), 1.6 * s, Color::rgb(210, 120, 60));
            for (dx, dy) in [(-1.4, -1.4), (1.2, -1.6), (0.0, -0.3)] {
                let (cx, cy) = xf.p(ox + dx, oy + dy);
                sc.circle(cx, cy, 0.7 * s, Color::rgb(200, 60, 60));
            }
            sc.polyline(pp(&[(ox - 0.5, oy + 3.0), (7.5, 15.0)]), 0.6 * s, pal().sclera.fade(0.8));
        }
        Prop::Dice => {
            let a = k * TAU * 2.0;
            let (c, sn) = (a.cos(), a.sin());
            let at = |u: f32, v: f32| xf.p(9.0 + u * c - v * sn, 13.5 + u * sn + v * c);
            sc.poly(vec![at(-2.6, -2.6), at(2.6, -2.6), at(2.6, 2.6), at(-2.6, 2.6)], pal().sclera);
            for (u, v) in [(-1.3, -1.3), (1.3, -1.3), (0.0, 0.0), (-1.3, 1.3), (1.3, 1.3)] {
                let (px, py) = at(u, v);
                sc.circle(px, py, 0.55 * s, pal().pupil.fade(0.85));
            }
        }
        Prop::Kite => {
            let kx = (t * 1.1).sin() * 5.0;
            let ky = -14.0 - (t * 1.7).sin().abs() * 2.5;
            sc.polyline(pp(&[(6.0, 12.0), (2.0 + kx * 0.3, 2.0), (kx, ky + 3.5)]), 0.7 * s, line);
            sc.poly(pp(&[(kx, ky - 3.2), (kx + 3.0, ky), (kx, ky + 4.2), (kx - 3.0, ky)]), Color::rgb(96, 165, 250));
            sc.polyline(pp(&[(kx, ky - 3.2), (kx, ky + 4.2)]), 0.5 * s, pal().sclera.fade(0.7));
            for i in 0..3 {
                let f = i as f32 / 3.0;
                let (bx, by) = xf.p(6.0 + (kx - 6.0) * (0.3 + f * 0.5), 12.0 + (ky + 3.5 - 12.0) * (0.3 + f * 0.5));
                sc.line(bx - 0.9 * s, by - 0.9 * s, bx + 0.9 * s, by + 0.9 * s, 0.6 * s, PINK);
            }
        }
        Prop::Radio => {
            sc.rect(xf.rect(-7.0, 11.0, 13.0, 6.5), 1.2 * s, dark);
            sc.polyline(pp(&[(4.5, 11.0), (8.5, 5.5)]), 0.9 * s, line);
            for i in 0..3 {
                let (x0, y0) = xf.p(-5.5, 12.5 + 1.6 * i as f32);
                let (x1, _) = xf.p(-1.5, 12.5 + 1.6 * i as f32);
                sc.line(x0, y0, x1, y0, 0.6 * s, pal().sclera.fade(0.6));
            }
            let (dx0, dy0) = xf.p(2.5, 14.5);
            sc.circle(dx0, dy0, 1.6 * s, pal().sclera.fade(0.8));
            let (px, py) = xf.p(1.2 + 4.5 * k, 12.0);
            sc.circle(px, py, 0.6 * s, PINK);
        }
        Prop::Lantern => {
            let swing = (t * 2.0).sin() * 1.5 * k;
            let (lx, ly) = (10.0 + swing, 12.5);
            sc.polyline(pp(&[(lx - 1.6, ly - 4.5), (lx, ly - 5.8), (lx + 1.6, ly - 4.5)]), 0.8 * s, line);
            sc.rect(xf.rect(lx - 2.6, ly - 4.0, 5.2, 6.5), 1.0 * s, dark);
            sc.rect(xf.rect(lx - 1.5, ly - 3.0, 3.0, 4.5), 0.5 * s, Color::rgb(255, 200, 90).fade(0.5 + 0.4 * k));
            let (gx, gy) = xf.p(lx, ly - 0.8);
            sc.circle(gx, gy, 4.5 * s * k, GOLD.fade(0.15 * k));
        }
    }
}

pub(super) fn pumpkin(sc: &mut Scene, (x, y): (f32, f32), k: f32) {
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
