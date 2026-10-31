use gpui::{Hsla, Rgba, WindowAppearance, rgba};

use crate::settings::{Appearance, Glass, Settings};

pub fn with_alpha(c: Rgba, a: f32) -> Rgba {
    Rgba { a, ..c }
}

/// Liquid Glass palette: translucent layers over a blurred window, hairline
/// highlights on top edges, and the system accent blue.
#[derive(Clone, Copy)]
pub struct Theme {
    pub window_tint: Rgba,
    pub glass: Rgba,
    pub glass_strong: Rgba,
    pub glass_hover: Rgba,
    pub stroke: Rgba,
    pub highlight: Rgba,
    pub text: Rgba,
    pub text_dim: Rgba,
    pub text_faint: Rgba,
    pub accent: Rgba,
    /// Fill behind white text (selected rows, active pills, menu hover): dark enough for 4.5:1.
    pub selection: Rgba,
    pub accent_soft: Rgba,
    pub row_alt: Rgba,
    pub good: Rgba,
    pub warn: Rgba,
    pub hot: Rgba,
}

impl Theme {
    pub fn resolve(appearance: WindowAppearance, settings: &Settings) -> Self {
        let mut t = match settings.appearance {
            Appearance::Light => Self::light(),
            Appearance::Dark => Self::dark(),
            Appearance::System => Self::for_appearance(appearance),
        };
        if settings.glass == Glass::Tinted {
            t.window_tint = with_alpha(t.window_tint, 0.82);
            t.glass = with_alpha(t.glass, 0.86);
            t.glass_strong = with_alpha(t.glass_strong, 0.94);
        }
        if settings.high_contrast {
            t.window_tint = with_alpha(t.window_tint, 0.97);
            t.glass = with_alpha(t.glass, 1.0);
            t.glass_strong = with_alpha(t.glass_strong, 1.0);
            t.text_dim = with_alpha(t.text, 0.9);
            t.text_faint = with_alpha(t.text, 0.7);
            t.stroke = with_alpha(t.text, 0.45);
            t.highlight = with_alpha(t.text, 0.55);
        }
        t
    }

    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            _ => Self::light(),
        }
    }

    fn light() -> Self {
        Self {
            window_tint: rgba(0xf4f6fa66),
            glass: rgba(0xffffff8c),
            glass_strong: rgba(0xffffffc4),
            glass_hover: rgba(0xffffffe0),
            stroke: rgba(0x00000024),
            highlight: rgba(0xffffffd9),
            text: rgba(0x1d1d1fff),
            text_dim: rgba(0x3c3c43c2),
            text_faint: rgba(0x3c3c43a6),
            // Darker than the dark-mode blue/red/orange/green so they read as text on white.
            accent: rgba(0x0066ccff),
            selection: rgba(0x0060d0ff),
            accent_soft: rgba(0x0a84ff26),
            row_alt: rgba(0xffffff4d),
            good: rgba(0x1e7b34ff),
            warn: rgba(0xb25000ff),
            hot: rgba(0xd70015ff),
        }
    }

    fn dark() -> Self {
        Self {
            window_tint: rgba(0x1c1c1e73),
            glass: rgba(0x2c2c2e8c),
            glass_strong: rgba(0x3a3a3cc4),
            glass_hover: rgba(0x48484ae0),
            stroke: rgba(0xffffff2b),
            highlight: rgba(0xffffff2e),
            text: rgba(0xf5f5f7ff),
            text_dim: rgba(0xebebf5b8),
            text_faint: rgba(0xebebf58c),
            accent: rgba(0x0a84ffff),
            selection: rgba(0x0060d0ff),
            accent_soft: rgba(0x0a84ff38),
            row_alt: rgba(0xffffff0a),
            good: rgba(0x30d158ff),
            warn: rgba(0xff9f0aff),
            hot: rgba(0xff453aff),
        }
    }

    pub fn load_color(&self, pct: f32) -> Hsla {
        if pct >= 80. {
            self.hot.into()
        } else if pct >= 50. {
            self.warn.into()
        } else {
            self.accent.into()
        }
    }
}
