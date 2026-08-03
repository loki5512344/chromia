//! Ready-made theme presets: a catalog of tasteful light and dark palettes
//! selectable from the Settings page.
//!
//! Seven presets ship with the app — four light (white-ish backgrounds with
//! dark text) and three dark (deep backgrounds with light text). Every preset
//! has a distinct accent, and text colors are chosen to contrast with the
//! background.

use crate::theme::Palette;

/// A named, ready-made theme preset.
///
/// Hex colors are stored as `&'static str` so the catalog can live as a
/// `const`; [`ThemePreset::palette`] materializes a [`Palette`] on demand.
pub struct ThemePreset {
    /// Canonical name, used in config (`theme.preset`) and the settings combo.
    pub name: &'static str,
    /// Window / page background.
    pub background: &'static str,
    /// Raised surface background.
    pub surface: &'static str,
    /// Overlay / elevated elements.
    pub overlay: &'static str,
    /// Primary accent color.
    pub accent: &'static str,
    /// Primary text color.
    pub text: &'static str,
    /// Secondary (muted) text color.
    pub subtext: &'static str,
}

impl ThemePreset {
    /// Builds the [`Palette`] for this preset.
    pub fn palette(&self) -> Palette {
        Palette {
            background: self.background.into(),
            surface: self.surface.into(),
            overlay: self.overlay.into(),
            accent: self.accent.into(),
            text: self.text.into(),
            subtext: self.subtext.into(),
        }
    }
}

/// Catalog of every built-in preset, ordered as shown in the Settings combo.
pub const ALL: &[ThemePreset] = &[
    // ── Light ────────────────────────────────────────────────────────────────
    ThemePreset {
        name: "Snow",
        background: "#ffffff",
        surface: "#f4f4f5",
        overlay: "#e4e4e7",
        accent: "#e11d48",
        text: "#18181b",
        subtext: "#71717a",
    },
    ThemePreset {
        name: "Pearl",
        background: "#fdfbf7",
        surface: "#f6f0e7",
        overlay: "#ece3d3",
        accent: "#b45309",
        text: "#292524",
        subtext: "#78716c",
    },
    ThemePreset {
        name: "Cloud",
        background: "#f4f6fb",
        surface: "#e8edf6",
        overlay: "#d9e1ef",
        accent: "#2563eb",
        text: "#0f172a",
        subtext: "#64748b",
    },
    ThemePreset {
        name: "Porcelain",
        background: "#f7f8f7",
        surface: "#eceded",
        overlay: "#dfe2e0",
        accent: "#0d9488",
        text: "#1c1917",
        subtext: "#6b7280",
    },
    // ── Dark ─────────────────────────────────────────────────────────────────
    ThemePreset {
        name: "Midnight",
        background: "#0f1115",
        surface: "#191d24",
        overlay: "#232833",
        accent: "#bd93f9",
        text: "#e6e6eb",
        subtext: "#8b94a3",
    },
    ThemePreset {
        name: "Slate",
        background: "#1e293b",
        surface: "#273449",
        overlay: "#334158",
        accent: "#38bdf8",
        text: "#f1f5f9",
        subtext: "#94a3b8",
    },
    ThemePreset {
        name: "Charcoal",
        background: "#17171a",
        surface: "#212125",
        overlay: "#2d2d33",
        accent: "#1db954",
        text: "#f5f5f5",
        subtext: "#a1a1aa",
    },
];

/// Resolves a preset palette by name, returning `None` for unknown names.
pub fn resolve(name: &str) -> Option<Palette> {
    ALL.iter()
        .find(|p| p.name == name)
        .map(ThemePreset::palette)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT_NAMES: [&str; 4] = ["Snow", "Pearl", "Cloud", "Porcelain"];
    const DARK_NAMES: [&str; 3] = ["Midnight", "Slate", "Charcoal"];

    #[test]
    fn catalog_has_seven_presets() {
        assert_eq!(ALL.len(), 7);
    }

    #[test]
    fn split_is_four_light_three_dark() {
        let names: Vec<&str> = ALL.iter().map(|p| p.name).collect();
        for name in LIGHT_NAMES {
            assert!(names.contains(&name), "missing light preset {name}");
            let preset = ALL.iter().find(|p| p.name == name).unwrap();
            assert!(
                luminance(preset.background) > 0.5,
                "{name} background is too dark"
            );
        }
        for name in DARK_NAMES {
            assert!(names.contains(&name), "missing dark preset {name}");
            let preset = ALL.iter().find(|p| p.name == name).unwrap();
            assert!(
                luminance(preset.background) < 0.5,
                "{name} background is too light"
            );
        }
    }

    #[test]
    fn resolve_finds_every_preset() {
        for preset in ALL {
            assert_eq!(resolve(preset.name), Some(preset.palette()));
        }
    }

    #[test]
    fn resolve_unknown_name_returns_none() {
        assert_eq!(resolve("not-a-preset"), None);
        assert_eq!(resolve(""), None);
    }

    #[test]
    fn every_palette_color_is_a_nonempty_hex() {
        for preset in ALL {
            let palette = preset.palette();
            for color in [
                &palette.background,
                &palette.surface,
                &palette.overlay,
                &palette.accent,
                &palette.text,
                &palette.subtext,
            ] {
                assert!(!color.is_empty(), "empty color in preset {}", preset.name);
                assert!(
                    color.starts_with('#'),
                    "{} does not start with '#' in preset {}",
                    color,
                    preset.name
                );
            }
        }
    }

    /// Perceived luminance of a `#rrggbb` hex in the [0, 1] range.
    fn luminance(hex: &str) -> f64 {
        let hex = hex.trim_start_matches('#');
        let byte = |i: usize| {
            u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f64
        };
        let lin = |c: f64| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let r = lin(byte(0) / 255.0);
        let g = lin(byte(2) / 255.0);
        let b = lin(byte(4) / 255.0);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }
}
