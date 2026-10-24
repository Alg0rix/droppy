//! Palette and metrics. One place to tune the look.

use super::scene::Color;
use teardrop_core::Page;

pub const BODY_TOP: Color = Color::rgb(22, 22, 27);
pub const BODY_BOTTOM: Color = Color::rgb(9, 9, 12);
pub const RIM: Color = Color::rgba(255, 255, 255, 0.10);

pub const TEXT: Color = Color::rgb(246, 246, 248);
pub const TEXT_2: Color = Color::rgba(255, 255, 255, 0.64);
pub const TEXT_3: Color = Color::rgba(255, 255, 255, 0.38);

pub const SURFACE: Color = Color::rgba(255, 255, 255, 0.065);
pub const SURFACE_HOVER: Color = Color::rgba(255, 255, 255, 0.115);
pub const SURFACE_PRESS: Color = Color::rgba(255, 255, 255, 0.17);
pub const HAIRLINE: Color = Color::rgba(255, 255, 255, 0.08);

pub const RED: Color = Color::rgb(255, 99, 99);
pub const GREEN: Color = Color::rgb(74, 222, 128);
pub const AMBER: Color = Color::rgb(251, 191, 36);

/// Space reserved around the island for its shadow.
pub const PAD: f32 = 20.0;

pub fn accent(page: Page) -> Color {
    match page {
        Page::Music => Color::rgb(255, 92, 122),
        Page::Focus => Color::rgb(255, 159, 67),
        Page::Water => Color::rgb(61, 200, 245),
        Page::Shelf => Color::rgb(167, 139, 250),
        Page::Tasks => Color::rgb(74, 222, 128),
        Page::Notes => Color::rgb(129, 140, 248),
        Page::Clipboard => Color::rgb(251, 191, 36),
        Page::Today => Color::rgb(96, 165, 250),
        Page::Speed => Color::rgb(45, 212, 191),
        Page::Usage => Color::rgb(251, 146, 60),
        Page::Voice => Color::rgb(248, 113, 113),
        Page::Ai => Color::rgb(192, 132, 252),
        Page::Settings => Color::rgb(156, 163, 175),
    }
}

/// Deterministic two-tone gradient for artwork placeholders.
pub fn art_colors(seed: &str) -> (Color, Color) {
    let mut h: u32 = 2166136261;
    for b in seed.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let hue = (h % 360) as f32;
    (hsl(hue, 0.78, 0.60), hsl((hue + 48.0) % 360.0, 0.72, 0.38))
}

pub fn hsl(h: f32, s: f32, l: f32) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Color { r: r + m, g: g + m, b: b + m, a: 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_is_deterministic_and_varied() {
        assert_eq!(art_colors("a"), art_colors("a"));
        assert_ne!(art_colors("Midnight"), art_colors("Daylight"));
    }

    #[test]
    fn hsl_primaries() {
        let red = hsl(0.0, 1.0, 0.5);
        assert!(red.r > 0.99 && red.g < 0.01 && red.b < 0.01);
    }
}
