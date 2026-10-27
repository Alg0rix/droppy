//! Palette and metrics. One place to tune the look.
//!
//! The active [`Palette`] is chosen by `Settings::theme` ([`IslandTheme`]):
//! `layout::build` calls [`set_theme`] each frame and every draw call reads
//! colours through [`pal`]. `Onyx` keeps the original constants exactly, so
//! the classic droplet renders pixel-for-pixel as before.

use super::scene::Color;
use std::sync::atomic::{AtomicUsize, Ordering};
use teardrop_core::{IslandTheme, Page};

/// Every colour a page can ask for, plus the body/eye tones.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub body_top: Color,
    pub body_bottom: Color,
    pub rim: Color,
    /// Specular top-light drawn over the body (alpha 0 = none). Fades to
    /// clear by ~55% of the silhouette height — the liquid-glass shine.
    pub sheen: Color,
    /// Drop-shadow strength multiplier.
    pub shadow: f32,
    pub text: Color,
    pub text_2: Color,
    pub text_3: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub surface_press: Color,
    pub hairline: Color,
    pub red: Color,
    pub green: Color,
    pub amber: Color,
    /// Eye capsule / pupil tones (inverted on light bodies).
    pub sclera: Color,
    pub pupil: Color,
    /// Light body: [`Palette::ink`] washes go dark and accents deepen.
    pub light: bool,
}

impl Palette {
    /// Neutral overlay at alpha `a`: a white wash on dark bodies, a dark
    /// ink wash on light ones — same "subtle fill" semantics either way.
    pub fn ink(&self, a: f32) -> Color {
        if self.light {
            Color::rgba(18, 22, 34, a)
        } else {
            Color::rgba(255, 255, 255, a)
        }
    }
}

const ONYX: Palette = Palette {
    body_top: Color::rgb(22, 22, 27),
    body_bottom: Color::rgb(9, 9, 12),
    rim: Color::rgba(255, 255, 255, 0.10),
    sheen: Color::rgba(255, 255, 255, 0.0),
    shadow: 1.0,
    text: Color::rgb(246, 246, 248),
    text_2: Color::rgba(255, 255, 255, 0.64),
    text_3: Color::rgba(255, 255, 255, 0.38),
    surface: Color::rgba(255, 255, 255, 0.065),
    surface_hover: Color::rgba(255, 255, 255, 0.115),
    surface_press: Color::rgba(255, 255, 255, 0.17),
    hairline: Color::rgba(255, 255, 255, 0.08),
    red: Color::rgb(255, 99, 99),
    green: Color::rgb(74, 222, 128),
    amber: Color::rgb(251, 191, 36),
    sclera: Color::rgb(242, 242, 246),
    pupil: Color::rgb(14, 14, 18),
    light: false,
};

/// Shared text tones for the light bodies (glass + porcelain).
const INK_2: Color = Color::rgba(18, 22, 36, 0.62);
const INK_3: Color = Color::rgba(18, 22, 36, 0.40);

const LIQUID_GLASS: Palette = Palette {
    body_top: Color::rgba(255, 255, 255, 0.74),
    body_bottom: Color::rgba(226, 233, 248, 0.46),
    rim: Color::rgba(255, 255, 255, 0.95),
    sheen: Color::rgba(255, 255, 255, 0.62),
    shadow: 0.8,
    text: Color::rgb(20, 22, 30),
    text_2: Color::rgba(20, 24, 40, 0.66),
    text_3: Color::rgba(20, 24, 40, 0.42),
    surface: Color::rgba(255, 255, 255, 0.38),
    surface_hover: Color::rgba(255, 255, 255, 0.58),
    surface_press: Color::rgba(255, 255, 255, 0.26),
    hairline: Color::rgba(20, 26, 48, 0.09),
    red: Color::rgb(255, 59, 48),
    green: Color::rgb(40, 180, 80),
    amber: Color::rgb(235, 140, 0),
    sclera: Color::rgb(38, 40, 52),
    pupil: Color::rgb(244, 245, 250),
    light: true,
};

const PORCELAIN: Palette = Palette {
    body_top: Color::rgb(252, 251, 249),
    body_bottom: Color::rgb(236, 234, 230),
    rim: Color::rgba(40, 34, 24, 0.12),
    sheen: Color::rgba(255, 255, 255, 0.70),
    shadow: 0.55,
    text: Color::rgb(28, 27, 25),
    text_2: INK_2,
    text_3: INK_3,
    surface: Color::rgba(15, 20, 40, 0.055),
    surface_hover: Color::rgba(15, 20, 40, 0.10),
    surface_press: Color::rgba(15, 20, 40, 0.15),
    hairline: Color::rgba(20, 26, 48, 0.10),
    red: Color::rgb(255, 59, 48),
    green: Color::rgb(40, 180, 80),
    amber: Color::rgb(235, 140, 0),
    sclera: Color::rgb(38, 40, 52),
    pupil: Color::rgb(244, 245, 250),
    light: true,
};

const OCEAN: Palette = Palette {
    body_top: Color::rgb(20, 46, 82),
    body_bottom: Color::rgb(6, 16, 34),
    rim: Color::rgba(150, 205, 255, 0.24),
    sheen: Color::rgba(170, 215, 255, 0.16),
    shadow: 1.0,
    text: Color::rgb(232, 242, 255),
    text_2: Color::rgba(198, 222, 252, 0.64),
    text_3: Color::rgba(168, 200, 240, 0.40),
    surface: Color::rgba(140, 190, 255, 0.07),
    surface_hover: Color::rgba(140, 190, 255, 0.12),
    surface_press: Color::rgba(140, 190, 255, 0.18),
    hairline: Color::rgba(140, 190, 255, 0.09),
    red: Color::rgb(255, 105, 97),
    green: Color::rgb(48, 219, 91),
    amber: Color::rgb(255, 204, 0),
    sclera: Color::rgb(242, 246, 252),
    pupil: Color::rgb(12, 18, 30),
    light: false,
};

const FOREST: Palette = Palette {
    body_top: Color::rgb(16, 46, 32),
    body_bottom: Color::rgb(5, 16, 10),
    rim: Color::rgba(140, 230, 175, 0.22),
    sheen: Color::rgba(160, 235, 190, 0.14),
    shadow: 1.0,
    text: Color::rgb(230, 246, 236),
    text_2: Color::rgba(195, 235, 210, 0.62),
    text_3: Color::rgba(160, 212, 185, 0.38),
    surface: Color::rgba(140, 230, 175, 0.07),
    surface_hover: Color::rgba(140, 230, 175, 0.12),
    surface_press: Color::rgba(140, 230, 175, 0.18),
    hairline: Color::rgba(140, 230, 175, 0.09),
    red: Color::rgb(255, 105, 97),
    green: Color::rgb(48, 219, 91),
    amber: Color::rgb(255, 204, 0),
    sclera: Color::rgb(240, 248, 242),
    pupil: Color::rgb(10, 18, 12),
    light: false,
};

const EMBER: Palette = Palette {
    body_top: Color::rgb(66, 34, 18),
    body_bottom: Color::rgb(20, 9, 5),
    rim: Color::rgba(255, 180, 125, 0.22),
    sheen: Color::rgba(255, 190, 140, 0.14),
    shadow: 1.0,
    text: Color::rgb(252, 238, 226),
    text_2: Color::rgba(252, 216, 188, 0.62),
    text_3: Color::rgba(240, 185, 150, 0.38),
    surface: Color::rgba(255, 175, 120, 0.07),
    surface_hover: Color::rgba(255, 175, 120, 0.12),
    surface_press: Color::rgba(255, 175, 120, 0.18),
    hairline: Color::rgba(255, 175, 120, 0.09),
    red: Color::rgb(255, 105, 97),
    green: Color::rgb(48, 219, 91),
    amber: Color::rgb(255, 204, 0),
    sclera: Color::rgb(250, 240, 230),
    pupil: Color::rgb(22, 12, 8),
    light: false,
};

const AMETHYST: Palette = Palette {
    body_top: Color::rgb(52, 30, 88),
    body_bottom: Color::rgb(14, 8, 28),
    rim: Color::rgba(205, 165, 255, 0.24),
    sheen: Color::rgba(215, 175, 255, 0.16),
    shadow: 1.0,
    text: Color::rgb(241, 234, 255),
    text_2: Color::rgba(215, 195, 252, 0.62),
    text_3: Color::rgba(190, 165, 240, 0.38),
    surface: Color::rgba(190, 150, 255, 0.07),
    surface_hover: Color::rgba(190, 150, 255, 0.12),
    surface_press: Color::rgba(190, 150, 255, 0.18),
    hairline: Color::rgba(190, 150, 255, 0.09),
    red: Color::rgb(255, 105, 97),
    green: Color::rgb(48, 219, 91),
    amber: Color::rgb(255, 204, 0),
    sclera: Color::rgb(246, 240, 252),
    pupil: Color::rgb(18, 10, 30),
    light: false,
};

const ROSE: Palette = Palette {
    body_top: Color::rgb(70, 24, 46),
    body_bottom: Color::rgb(20, 7, 14),
    rim: Color::rgba(255, 160, 198, 0.22),
    sheen: Color::rgba(255, 170, 205, 0.14),
    shadow: 1.0,
    text: Color::rgb(253, 236, 244),
    text_2: Color::rgba(252, 205, 224, 0.62),
    text_3: Color::rgba(240, 170, 200, 0.38),
    surface: Color::rgba(255, 155, 195, 0.07),
    surface_hover: Color::rgba(255, 155, 195, 0.12),
    surface_press: Color::rgba(255, 155, 195, 0.18),
    hairline: Color::rgba(255, 155, 195, 0.09),
    red: Color::rgb(255, 105, 97),
    green: Color::rgb(48, 219, 91),
    amber: Color::rgb(255, 204, 0),
    sclera: Color::rgb(250, 240, 245),
    pupil: Color::rgb(24, 10, 16),
    light: false,
};

const STEALTH: Palette =
    Palette { body_top: Color::rgb(7, 7, 9), body_bottom: Color::rgb(0, 0, 0), rim: Color::rgba(255, 255, 255, 0.13), sheen: Color::rgba(255, 255, 255, 0.0), shadow: 0.9, ..ONYX };

const PALETTES: [Palette; 9] = [ONYX, LIQUID_GLASS, PORCELAIN, OCEAN, FOREST, EMBER, AMETHYST, ROSE, STEALTH];

static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// Called once per frame from `layout::build` with `Settings::theme`.
pub fn set_theme(t: IslandTheme) {
    CURRENT.store(t as usize, Ordering::Relaxed);
}

/// The theme currently being drawn (for cache keys that bake in colours).
pub fn theme_index() -> usize {
    CURRENT.load(Ordering::Relaxed)
}

/// Palette of the active theme.
pub fn pal() -> &'static Palette {
    &PALETTES[CURRENT.load(Ordering::Relaxed).min(PALETTES.len() - 1)]
}

/// Palette of any theme (settings swatches draw every theme at once).
pub fn palette_for(t: IslandTheme) -> &'static Palette {
    &PALETTES[(t as usize).min(PALETTES.len() - 1)]
}

/// Space reserved around the island for its shadow.
pub const PAD: f32 = 20.0;

pub fn accent(page: Page) -> Color {
    let c = match page {
        Page::Music => Color::rgb(255, 92, 122),
        Page::Focus => Color::rgb(255, 159, 67),
        Page::Water => Color::rgb(61, 200, 245),
        Page::Shelf => Color::rgb(167, 139, 250),
        Page::Tasks => Color::rgb(74, 222, 128),
        Page::Notes => Color::rgb(129, 140, 248),
        Page::Clipboard => Color::rgb(251, 191, 36),
        Page::Today => Color::rgb(96, 165, 250),
        Page::System => Color::rgb(56, 189, 248),
        Page::Quick => Color::rgb(244, 114, 182),
        Page::Speed => Color::rgb(45, 212, 191),
        Page::Usage => Color::rgb(251, 146, 60),
        Page::Voice => Color::rgb(248, 113, 113),
        Page::Ai => Color::rgb(192, 132, 252),
        Page::Chat => Color::rgb(52, 211, 153),
        Page::Windows => Color::rgb(190, 242, 100),
        Page::Settings => Color::rgb(156, 163, 175),
    };
    // Vivid accents wash out on a light body; deepen them for contrast.
    if pal().light {
        c.mix(Color::rgb(10, 12, 20), 0.22)
    } else {
        c
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

    #[test]
    fn every_theme_has_a_palette() {
        for t in IslandTheme::ALL {
            let p = palette_for(t);
            assert!(p.text.a > 0.9 && p.body_top.a > 0.5);
        }
        assert_eq!(IslandTheme::ALL.len(), PALETTES.len());
    }
}
