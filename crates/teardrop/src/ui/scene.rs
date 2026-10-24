//! Retained draw list. Pure data: the layout code builds a [`Scene`], the
//! Direct2D painter replays it. Keeping this free of Windows types makes every
//! page layout unit-testable on any OS.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
    }

    /// Same colour with alpha multiplied by `a`.
    pub fn fade(self, a: f32) -> Self {
        Self { a: self.a * a.clamp(0.0, 1.0), ..self }
    }

    pub fn mix(self, o: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (o.r - self.r) * t,
            g: self.g + (o.g - self.g) * t,
            b: self.b + (o.b - self.b) * t,
            a: self.a + (o.a - self.a) * t,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }

    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(self.x + d, self.y + d, (self.w - 2.0 * d).max(0.0), (self.h - 2.0 * d).max(0.0))
    }

    pub fn offset(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    Solid(Color),
    /// Top to bottom.
    VGrad(Color, Color),
    /// Top-left to bottom-right.
    DGrad(Color, Color),
}

impl Fill {
    fn faded(self, a: f32) -> Fill {
        match self {
            Fill::Solid(c) => Fill::Solid(c.fade(a)),
            Fill::VGrad(x, y) => Fill::VGrad(x.fade(a), y.fade(a)),
            Fill::DGrad(x, y) => Fill::DGrad(x.fade(a), y.fade(a)),
        }
    }
}

impl From<Color> for Fill {
    fn from(c: Color) -> Self {
        Fill::Solid(c)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
    Bold,
}

#[derive(Debug, Clone)]
pub enum Cmd {
    /// The island silhouette: soft shadow, body fill, 1px rim light.
    Body { pts: Vec<(f32, f32)>, fill: Fill, rim: Color, shadow: f32 },
    Rect { r: Rect, radius: f32, fill: Fill },
    Stroke { r: Rect, radius: f32, width: f32, color: Color, dashed: bool },
    Circle { cx: f32, cy: f32, r: f32, fill: Fill },
    /// Arc stroke. `from`/`to` are turns (0..1) clockwise from 12 o'clock.
    Ring { cx: f32, cy: f32, r: f32, width: f32, from: f32, to: f32, color: Color },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color },
    Poly { pts: Vec<(f32, f32)>, fill: Fill },
    Polyline { pts: Vec<(f32, f32)>, width: f32, color: Color },
    /// Single-line text, vertically centred in `r`, ellipsised on overflow.
    /// `karaoke` overpaints the first `frac` of the text in a second colour.
    Text {
        s: String,
        r: Rect,
        size: f32,
        weight: Weight,
        color: Color,
        align: Align,
        karaoke: Option<(f32, Color)>,
    },
    Clip(Rect),
    PopClip,
}

/// Draw-list builder. Carries a translate + opacity so pages can slide and
/// fade as a unit without every call site knowing about it.
#[derive(Debug, Default, Clone)]
pub struct Scene {
    pub cmds: Vec<Cmd>,
    dx: f32,
    dy: f32,
    alpha: f32,
}

impl Scene {
    pub fn new() -> Self {
        Self { cmds: Vec::with_capacity(256), dx: 0.0, dy: 0.0, alpha: 1.0 }
    }

    pub fn set_transform(&mut self, dx: f32, dy: f32, alpha: f32) {
        self.dx = dx;
        self.dy = dy;
        self.alpha = alpha.clamp(0.0, 1.0);
    }

    pub fn reset_transform(&mut self) {
        self.set_transform(0.0, 0.0, 1.0);
    }

    fn r(&self, r: Rect) -> Rect {
        r.offset(self.dx, self.dy)
    }

    fn c(&self, c: Color) -> Color {
        c.fade(self.alpha)
    }

    fn f(&self, f: impl Into<Fill>) -> Fill {
        f.into().faded(self.alpha)
    }

    fn pts(&self, pts: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
        pts.into_iter().map(|(x, y)| (x + self.dx, y + self.dy)).collect()
    }

    pub fn body(&mut self, pts: Vec<(f32, f32)>, fill: Fill, rim: Color, shadow: f32) {
        self.cmds.push(Cmd::Body { pts, fill, rim, shadow });
    }

    pub fn rect(&mut self, r: Rect, radius: f32, fill: impl Into<Fill>) {
        if self.alpha <= 0.003 {
            return;
        }
        let (r, fill) = (self.r(r), self.f(fill));
        self.cmds.push(Cmd::Rect { r, radius, fill });
    }

    pub fn stroke(&mut self, r: Rect, radius: f32, width: f32, color: Color) {
        let (r, color) = (self.r(r), self.c(color));
        self.cmds.push(Cmd::Stroke { r, radius, width, color, dashed: false });
    }

    pub fn dashed(&mut self, r: Rect, radius: f32, width: f32, color: Color) {
        let (r, color) = (self.r(r), self.c(color));
        self.cmds.push(Cmd::Stroke { r, radius, width, color, dashed: true });
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, fill: impl Into<Fill>) {
        if self.alpha <= 0.003 {
            return;
        }
        let fill = self.f(fill);
        self.cmds.push(Cmd::Circle { cx: cx + self.dx, cy: cy + self.dy, r, fill });
    }

    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, width: f32, from: f32, to: f32, color: Color) {
        if to - from <= 0.0001 {
            return;
        }
        let color = self.c(color);
        self.cmds.push(Cmd::Ring { cx: cx + self.dx, cy: cy + self.dy, r, width, from, to, color });
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color) {
        let color = self.c(color);
        self.cmds.push(Cmd::Line { x1: x1 + self.dx, y1: y1 + self.dy, x2: x2 + self.dx, y2: y2 + self.dy, width, color });
    }

    pub fn poly(&mut self, pts: Vec<(f32, f32)>, fill: impl Into<Fill>) {
        if self.alpha <= 0.003 {
            return;
        }
        let (pts, fill) = (self.pts(pts), self.f(fill));
        self.cmds.push(Cmd::Poly { pts, fill });
    }

    pub fn polyline(&mut self, pts: Vec<(f32, f32)>, width: f32, color: Color) {
        let (pts, color) = (self.pts(pts), self.c(color));
        self.cmds.push(Cmd::Polyline { pts, width, color });
    }

    pub fn text(&mut self, s: impl Into<String>, r: Rect, size: f32, weight: Weight, color: Color, align: Align) {
        if self.alpha <= 0.003 {
            return;
        }
        let (r, color) = (self.r(r), self.c(color));
        self.cmds.push(Cmd::Text { s: s.into(), r, size, weight, color, align, karaoke: None });
    }

    pub fn text_karaoke(&mut self, s: impl Into<String>, r: Rect, size: f32, weight: Weight, base: Color, lit: Color, frac: f32) {
        if self.alpha <= 0.003 {
            return;
        }
        let (r, base, lit) = (self.r(r), self.c(base), self.c(lit));
        self.cmds.push(Cmd::Text { s: s.into(), r, size, weight, color: base, align: Align::Left, karaoke: Some((frac.clamp(0.0, 1.0), lit)) });
    }

    pub fn clip(&mut self, r: Rect) {
        let r = self.r(r);
        self.cmds.push(Cmd::Clip(r));
    }

    pub fn pop_clip(&mut self) {
        self.cmds.push(Cmd::PopClip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_offsets_and_fades() {
        let mut s = Scene::new();
        s.set_transform(10.0, 5.0, 0.5);
        s.rect(Rect::new(0.0, 0.0, 4.0, 4.0), 1.0, Color::rgb(255, 255, 255));
        match &s.cmds[0] {
            Cmd::Rect { r, fill: Fill::Solid(c), .. } => {
                assert_eq!((r.x, r.y), (10.0, 5.0));
                assert!((c.a - 0.5).abs() < 1e-4);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn invisible_scene_emits_nothing() {
        let mut s = Scene::new();
        s.set_transform(0.0, 0.0, 0.0);
        s.rect(Rect::new(0.0, 0.0, 4.0, 4.0), 1.0, Color::rgb(1, 2, 3));
        s.text("x", Rect::new(0.0, 0.0, 4.0, 4.0), 12.0, Weight::Regular, Color::rgb(1, 2, 3), Align::Left);
        assert!(s.cmds.is_empty());
    }
}
