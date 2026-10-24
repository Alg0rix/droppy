//! Vector icons composed from scene primitives (no font or bitmap needed).

use super::scene::{Color, Rect, Scene};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Next,
    Prev,
    Note,
    Timer,
    Drop,
    Tray,
    Check,
    List,
    Clipboard,
    Calendar,
    Gauge,
    Gear,
    Plus,
    Minus,
    Close,
    Folder,
    Speaker,
    Muted,
    Bolt,
    Headphones,
    Undo,
    Reset,
    File,
}

/// Draw `icon` centred at (cx, cy) inside a `size`px box.
pub fn draw(s: &mut Scene, icon: Icon, cx: f32, cy: f32, size: f32, c: Color) {
    let u = size / 16.0; // design grid is 16x16
    let sw = (1.5 * u).max(1.2);
    let p = |x: f32, y: f32| (cx + (x - 8.0) * u, cy + (y - 8.0) * u);
    let rect = |x: f32, y: f32, w: f32, h: f32| Rect::new(cx + (x - 8.0) * u, cy + (y - 8.0) * u, w * u, h * u);
    match icon {
        Icon::Play => s.poly(vec![p(4.5, 2.5), p(13.5, 8.0), p(4.5, 13.5)], c),
        Icon::Pause => {
            s.rect(rect(3.5, 2.5, 3.2, 11.0), 1.2 * u, c);
            s.rect(rect(9.3, 2.5, 3.2, 11.0), 1.2 * u, c);
        }
        Icon::Next => {
            s.poly(vec![p(2.5, 3.0), p(11.0, 8.0), p(2.5, 13.0)], c);
            s.rect(rect(11.6, 3.0, 2.4, 10.0), 1.0 * u, c);
        }
        Icon::Prev => {
            s.poly(vec![p(13.5, 3.0), p(5.0, 8.0), p(13.5, 13.0)], c);
            s.rect(rect(2.0, 3.0, 2.4, 10.0), 1.0 * u, c);
        }
        Icon::Note => {
            s.circle(p(5.2, 11.6).0, p(5.2, 11.6).1, 2.4 * u, c);
            s.line(p(7.4, 11.2).0, p(7.4, 11.2).1, p(7.4, 2.6).0, p(7.4, 2.6).1, sw, c);
            s.polyline(vec![p(7.4, 2.6), p(11.6, 3.6), p(12.4, 6.2)], sw, c);
        }
        Icon::Timer => {
            s.ring(cx, cy + 0.6 * u, 5.6 * u, sw, 0.0, 1.0, c);
            s.line(p(8.0, 8.6).0, p(8.0, 8.6).1, p(8.0, 5.0).0, p(8.0, 5.0).1, sw, c);
            s.line(p(6.4, 1.6).0, p(6.4, 1.6).1, p(9.6, 1.6).0, p(9.6, 1.6).1, sw, c);
        }
        Icon::Drop => s.poly(drop_pts(cx, cy, size * 0.9), c),
        Icon::Tray => {
            s.polyline(vec![p(2.0, 9.0), p(2.0, 13.0), p(14.0, 13.0), p(14.0, 9.0)], sw, c);
            s.polyline(vec![p(5.2, 6.2), p(8.0, 9.2), p(10.8, 6.2)], sw, c);
            s.line(p(8.0, 2.4).0, p(8.0, 2.4).1, p(8.0, 9.0).0, p(8.0, 9.0).1, sw, c);
        }
        Icon::Check => s.polyline(vec![p(3.2, 8.4), p(6.6, 11.6), p(12.8, 4.6)], sw * 1.15, c),
        Icon::List => {
            for (i, y) in [4.0, 8.0, 12.0].iter().enumerate() {
                let _ = i;
                s.circle(p(3.2, *y).0, p(3.2, *y).1, 1.0 * u, c);
                s.line(p(6.2, *y).0, p(6.2, *y).1, p(13.4, *y).0, p(13.4, *y).1, sw, c);
            }
        }
        Icon::Clipboard => {
            s.stroke(rect(3.0, 3.0, 10.0, 11.5), 2.0 * u, sw, c);
            s.rect(rect(5.6, 1.4, 4.8, 3.2), 1.2 * u, c);
        }
        Icon::Calendar => {
            s.stroke(rect(2.4, 3.2, 11.2, 10.6), 2.2 * u, sw, c);
            s.line(p(2.6, 6.6).0, p(2.6, 6.6).1, p(13.4, 6.6).0, p(13.4, 6.6).1, sw, c);
            s.line(p(5.4, 1.8).0, p(5.4, 1.8).1, p(5.4, 4.4).0, p(5.4, 4.4).1, sw, c);
            s.line(p(10.6, 1.8).0, p(10.6, 1.8).1, p(10.6, 4.4).0, p(10.6, 4.4).1, sw, c);
        }
        Icon::Gauge => {
            s.ring(cx, cy + 0.8 * u, 6.2 * u, sw, 0.62, 1.38, c);
            s.line(p(8.0, 9.2).0, p(8.0, 9.2).1, p(11.0, 5.0).0, p(11.0, 5.0).1, sw, c);
        }
        Icon::Gear => {
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let (sn, cs) = a.sin_cos();
                s.line(cx + cs * 5.0 * u, cy + sn * 5.0 * u, cx + cs * 7.0 * u, cy + sn * 7.0 * u, sw * 1.5, c);
            }
            s.ring(cx, cy, 4.6 * u, sw * 1.2, 0.0, 1.0, c);
            s.circle(cx, cy, 1.5 * u, c);
        }
        Icon::Plus => {
            s.line(p(8.0, 3.0).0, p(8.0, 3.0).1, p(8.0, 13.0).0, p(8.0, 13.0).1, sw * 1.1, c);
            s.line(p(3.0, 8.0).0, p(3.0, 8.0).1, p(13.0, 8.0).0, p(13.0, 8.0).1, sw * 1.1, c);
        }
        Icon::Minus => s.line(p(3.0, 8.0).0, p(3.0, 8.0).1, p(13.0, 8.0).0, p(13.0, 8.0).1, sw * 1.1, c),
        Icon::Close => {
            s.line(p(4.0, 4.0).0, p(4.0, 4.0).1, p(12.0, 12.0).0, p(12.0, 12.0).1, sw, c);
            s.line(p(12.0, 4.0).0, p(12.0, 4.0).1, p(4.0, 12.0).0, p(4.0, 12.0).1, sw, c);
        }
        Icon::Folder => {
            s.polyline(
                vec![p(2.0, 12.6), p(2.0, 3.8), p(6.2, 3.8), p(7.6, 5.4), p(14.0, 5.4), p(14.0, 12.6), p(2.0, 12.6)],
                sw,
                c,
            );
        }
        Icon::Speaker => {
            s.poly(vec![p(2.0, 6.0), p(5.0, 6.0), p(8.6, 2.8), p(8.6, 13.2), p(5.0, 10.0), p(2.0, 10.0)], c);
            s.ring(cx + 0.4 * u, cy, 3.6 * u, sw, 0.16, 0.34, c);
            s.ring(cx + 0.4 * u, cy, 6.4 * u, sw, 0.17, 0.33, c.fade(0.8));
        }
        Icon::Muted => {
            s.poly(vec![p(2.0, 6.0), p(5.0, 6.0), p(8.6, 2.8), p(8.6, 13.2), p(5.0, 10.0), p(2.0, 10.0)], c);
            s.line(p(10.6, 5.6).0, p(10.6, 5.6).1, p(14.4, 10.4).0, p(14.4, 10.4).1, sw, c);
            s.line(p(14.4, 5.6).0, p(14.4, 5.6).1, p(10.6, 10.4).0, p(10.6, 10.4).1, sw, c);
        }
        Icon::Bolt => s.poly(vec![p(9.4, 1.4), p(3.6, 9.0), p(7.6, 9.0), p(6.6, 14.6), p(12.4, 6.6), p(8.4, 6.6)], c),
        Icon::Headphones => {
            s.ring(cx, cy + 0.6 * u, 5.8 * u, sw, 0.75, 1.25, c);
            s.rect(rect(1.8, 8.2, 3.2, 5.4), 1.4 * u, c);
            s.rect(rect(11.0, 8.2, 3.2, 5.4), 1.4 * u, c);
        }
        Icon::Undo => {
            s.ring(cx, cy + 0.4 * u, 4.8 * u, sw, 0.20, 0.82, c);
            s.polyline(vec![p(1.8, 2.8), p(3.4, 6.8), p(7.6, 5.4)], sw, c);
        }
        Icon::Reset => {
            s.ring(cx, cy, 5.2 * u, sw, 0.08, 0.92, c);
            s.polyline(vec![p(8.0, 0.8), p(11.0, 2.8), p(8.2, 4.8)], sw, c);
        }
        Icon::File => {
            s.polyline(vec![p(3.6, 1.8), p(9.4, 1.8), p(12.6, 5.0), p(12.6, 14.2), p(3.6, 14.2), p(3.6, 1.8)], sw, c);
            s.polyline(vec![p(9.2, 1.8), p(9.2, 5.2), p(12.6, 5.2)], sw, c);
        }
    }
}

/// Teardrop silhouette: tip at the top, round belly below.
pub fn drop_pts(cx: f32, cy: f32, size: f32) -> Vec<(f32, f32)> {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let r = size * 0.34;
    let bcy = cy + size * 0.14;
    let tip = (cx, cy - size * 0.5);
    // Angle (at the belly centre) between "straight up" and each tangent point.
    let beta = (r / (bcy - tip.1)).acos();
    let start = -FRAC_PI_2 + beta; // right tangent point
    let sweep = TAU - 2.0 * beta; // clockwise round the bottom to the left one
    let n = 26;
    let mut pts = Vec::with_capacity(n + 2);
    pts.push(tip);
    for i in 0..=n {
        let a = start + sweep * i as f32 / n as f32;
        pts.push((cx + r * a.cos(), bcy + r * a.sin()));
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_is_symmetric_and_pointed() {
        let pts = drop_pts(50.0, 50.0, 20.0);
        assert!(pts.len() > 10);
        let top = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        assert!((pts[0].1 - top).abs() < 0.01, "tip is highest");
        let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        assert!(((lo + hi) / 2.0 - 50.0).abs() < 0.3, "centred: {lo}..{hi}");
    }

    #[test]
    fn every_icon_draws() {
        use Icon::*;
        let mut s = Scene::new();
        for i in [Play, Pause, Next, Prev, Note, Timer, Drop, Tray, Check, List, Clipboard, Calendar, Gauge, Gear, Plus, Minus, Close, Folder, Speaker, Muted, Bolt, Headphones, Undo, Reset, File] {
            let before = s.cmds.len();
            draw(&mut s, i, 10.0, 10.0, 16.0, Color::rgb(255, 255, 255));
            assert!(s.cmds.len() > before, "{i:?} drew nothing");
        }
    }
}
