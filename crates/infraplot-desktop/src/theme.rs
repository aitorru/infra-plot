//! Colour themes, the same four as the web editor (`web/src/ui/theme.ts`).

use gpui::{Hsla, rgb};
use infraplot_model::catalog::parse_hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Light,
    Dark,
    Solarized,
    Nier,
}

impl ThemeName {
    pub const ALL: [Self; 4] = [Self::Light, Self::Dark, Self::Solarized, Self::Nier];

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.theme().slug == s)
    }

    pub fn theme(self) -> &'static Theme {
        match self {
            Self::Light => &LIGHT,
            Self::Dark => &DARK,
            Self::Solarized => &SOLARIZED,
            Self::Nier => &NIER,
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&t| t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Colours are `0xrrggbb`.
#[derive(Debug)]
pub struct Theme {
    pub slug: &'static str,
    pub label: &'static str,
    /// Whether the canvas background is dark (drives fill toning).
    pub dark: bool,
    /// Canvas background.
    pub paper: u32,
    /// Panels and label backgrounds.
    pub panel: u32,
    /// Default stroke and text colour.
    pub ink: u32,
    /// Secondary text: zone kind tags, hints.
    pub muted: u32,
    /// Hairlines and panel borders.
    pub line: u32,
    /// Grid dots.
    pub grid: u32,
    /// Selection, active tools, previews.
    pub accent: u32,
    /// Zone borders.
    pub zone_stroke: u32,
    /// Errors and destructive actions.
    pub danger: u32,
}

pub static LIGHT: Theme = Theme {
    slug: "light",
    label: "Light",
    dark: false,
    paper: 0xf8f9fa,
    panel: 0xffffff,
    ink: 0x1f2328,
    muted: 0x6a737d,
    line: 0xe1e4e8,
    grid: 0xd0d5db,
    accent: 0x4f5bd5,
    zone_stroke: 0x6a737d,
    danger: 0xcf222e,
};

pub static DARK: Theme = Theme {
    slug: "dark",
    label: "Dark",
    dark: true,
    paper: 0x15171c,
    panel: 0x1e2127,
    ink: 0xe6e8eb,
    muted: 0x8b949e,
    line: 0x30353d,
    grid: 0x2c3139,
    accent: 0x8c95ff,
    zone_stroke: 0x8b949e,
    danger: 0xff6b6b,
};

pub static SOLARIZED: Theme = Theme {
    slug: "solarized",
    label: "Solarized",
    dark: true,
    paper: 0x002b36,
    panel: 0x073642,
    ink: 0xeee8d5,
    muted: 0x839496,
    line: 0x1c4b56,
    grid: 0x0f4450,
    accent: 0x268bd2,
    zone_stroke: 0x93a1a1,
    danger: 0xdc322f,
};

pub static NIER: Theme = Theme {
    slug: "nier",
    label: "NieR",
    dark: false,
    paper: 0xd1cdb7,
    panel: 0xdad4bb,
    ink: 0x4e4b42,
    muted: 0x7d796b,
    line: 0xb4ae96,
    grid: 0xbdb8a1,
    accent: 0x4e4b42,
    zone_stroke: 0x6b675a,
    danger: 0xcd664d,
};

/// `a` mixed with `b`; `k` = 0 gives `a`, 1 gives `b`.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // masked to a byte
pub fn mix(a: u32, b: u32, k: f32) -> u32 {
    let ch = |c: u32, shift: u32| f32::from(((c >> shift) & 0xff) as u8);
    let mut out = 0;
    for shift in [16, 8, 0] {
        let v = ch(a, shift) + (ch(b, shift) - ch(a, shift)) * k;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=255
        let v = v.round().clamp(0.0, 255.0) as u32;
        out |= v << shift;
    }
    out
}

impl Theme {
    /// A fill colour as it should appear on this theme's paper: pastel defaults glow on dark
    /// backgrounds, so there they are blended towards the paper.
    #[must_use]
    pub fn surface(&self, color: u32) -> u32 {
        if self.dark {
            mix(color, self.paper, 0.55)
        } else {
            color
        }
    }
}

pub fn hsla(color: u32) -> Hsla {
    rgb(color).into()
}

/// An element's own colour, or `fallback` when it has none (or an invalid one).
#[must_use]
pub fn color_or(custom: Option<&str>, fallback: u32) -> u32 {
    custom.and_then(parse_hex).unwrap_or(fallback)
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)] // exact grid values
mod tests {
    use super::*;

    #[test]
    fn mixing() {
        assert_eq!(mix(0x000000, 0xffffff, 0.5), 0x808080);
        assert_eq!(mix(0x123456, 0xabcdef, 0.0), 0x123456);
        assert_eq!(mix(0x123456, 0xabcdef, 1.0), 0xabcdef);
        assert_eq!(ThemeName::parse("nier"), Some(ThemeName::Nier));
        assert_eq!(ThemeName::Nier.next(), ThemeName::Light);
    }
}
