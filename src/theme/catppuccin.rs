//! Official Catppuccin palette data for all four flavors.
//!
//! Values are taken from the Catppuccin reference palette
//! (https://github.com/catppuccin/catppuccin). For every flavor the accent
//! lookup falls back to `mauve` when an unknown name is requested.

use crate::config::schema::Flavor;
use crate::theme::Palette;

/// Valid accent names, sorted alphabetically.
#[allow(dead_code)] // TODO(loki): theme settings UI
pub const ALL_ACCENTS: &[&str] = &[
    "blue",
    "flamingo",
    "green",
    "lavender",
    "maroon",
    "mauve",
    "peach",
    "pink",
    "red",
    "rosewater",
    "sapphire",
    "sky",
    "teal",
    "yellow",
];

/// Base colors per flavor, indexed by [`flavor_index`]:
/// 0 = mocha, 1 = macchiato, 2 = frappe, 3 = latte.
struct FlavorColors {
    background: [&'static str; 4],
    surface: [&'static str; 4],
    overlay: [&'static str; 4],
    text: [&'static str; 4],
    subtext: [&'static str; 4],
}

const BASE: FlavorColors = FlavorColors {
    background: ["#1e1e2e", "#24273a", "#303446", "#eff1f5"],
    surface: ["#313244", "#363a4f", "#414559", "#e6e9ef"],
    overlay: ["#45475a", "#494d64", "#51576d", "#dce0e8"],
    text: ["#cdd6f4", "#cad3f5", "#c6d0f5", "#4c4f69"],
    subtext: ["#a6adc8", "#b8c0e0", "#b5bfe2", "#6c6f85"],
};

/// Accent hex per flavor, indexed as [`FlavorColors`] arrays are.
const ACCENTS: [(&str, [&str; 4]); 14] = [
    ("rosewater", ["#f5e0dc", "#f4dbd6", "#f2d5cf", "#dc8a78"]),
    ("flamingo", ["#f2cdcd", "#f0c6c6", "#eebebe", "#dd7878"]),
    ("pink", ["#f5c2e7", "#f5bde6", "#f4b8e4", "#ea76cb"]),
    ("mauve", ["#cba6f7", "#c6a0f6", "#ca9ee6", "#8839ef"]),
    ("red", ["#f38ba8", "#ed8796", "#e78284", "#d20f39"]),
    ("maroon", ["#eba0ac", "#ee99a0", "#ea999c", "#e64553"]),
    ("peach", ["#fab387", "#f5a97f", "#ef9f76", "#fe640b"]),
    ("yellow", ["#f9e2af", "#eed49f", "#e5c890", "#df8e1d"]),
    ("green", ["#a6e3a1", "#a6da95", "#a6d189", "#40a02b"]),
    ("teal", ["#94e2d5", "#8bd5ca", "#81c8be", "#179299"]),
    ("sky", ["#89dceb", "#91d7e3", "#99d1db", "#04a5e5"]),
    ("sapphire", ["#74c7ec", "#7dc4e4", "#85c1dc", "#209fb5"]),
    ("blue", ["#89b4fa", "#8caaee", "#8caaee", "#1e66f5"]),
    ("lavender", ["#b4befe", "#b7bdf8", "#babbf1", "#7287fd"]),
];

/// Resolves a palette for a flavor, honoring `accent` (falling back to
/// `mauve` for unknown accent names).
pub fn palette_for(flavor: Flavor, accent: &str) -> Palette {
    let idx = flavor_index(flavor);
    let accent_hex = ACCENTS
        .iter()
        .find(|(name, _)| *name == accent)
        .map(|(_, hexes)| hexes[idx])
        .unwrap_or_else(|| mauve_for(idx));
    Palette {
        background: BASE.background[idx].into(),
        surface: BASE.surface[idx].into(),
        overlay: BASE.overlay[idx].into(),
        accent: accent_hex.into(),
        text: BASE.text[idx].into(),
        subtext: BASE.subtext[idx].into(),
    }
}

/// Returns the accent hex for the fallback `mauve` accent of a flavor.
fn mauve_for(idx: usize) -> &'static str {
    ACCENTS
        .iter()
        .find(|(name, _)| *name == "mauve")
        .map(|(_, hexes)| hexes[idx])
        .expect("mauve is always present")
}

/// Maps a [`Flavor`] to the array index used throughout this module.
fn flavor_index(flavor: Flavor) -> usize {
    match flavor {
        Flavor::Mocha => 0,
        Flavor::Macchiato => 1,
        Flavor::Frappe => 2,
        Flavor::Latte => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mocha_mauve_default_matches_expected_hexes() {
        let palette = palette_for(Flavor::Mocha, "mauve");
        assert_eq!(palette.background, "#1e1e2e");
        assert_eq!(palette.surface, "#313244");
        assert_eq!(palette.overlay, "#45475a");
        assert_eq!(palette.accent, "#cba6f7");
        assert_eq!(palette.text, "#cdd6f4");
        assert_eq!(palette.subtext, "#a6adc8");
    }

    #[test]
    fn unknown_accent_falls_back_to_mauve() {
        let palette = palette_for(Flavor::Macchiato, "not-a-color");
        assert_eq!(palette.accent, "#c6a0f6");
    }

    #[test]
    fn each_flavor_has_a_distinct_background() {
        let backgrounds: Vec<String> = [
            Flavor::Mocha,
            Flavor::Macchiato,
            Flavor::Frappe,
            Flavor::Latte,
        ]
        .into_iter()
        .map(|f| palette_for(f, "mauve").background)
        .collect();
        assert_eq!(
            backgrounds,
            vec!["#1e1e2e", "#24273a", "#303446", "#eff1f5"]
        );
    }

    #[test]
    fn all_accents_is_sorted() {
        let mut sorted = ALL_ACCENTS.to_vec();
        sorted.sort_unstable();
        assert_eq!(ALL_ACCENTS, sorted);
    }

    #[test]
    fn known_accent_names_resolve_per_flavor() {
        assert_eq!(palette_for(Flavor::Latte, "blue").accent, "#1e66f5");
        assert_eq!(palette_for(Flavor::Frappe, "green").accent, "#a6d189");
        assert_eq!(palette_for(Flavor::Mocha, "pink").accent, "#f5c2e7");
    }
}
