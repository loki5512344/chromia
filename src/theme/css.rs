//! GTK CSS generation and application.
//!
//! [`generate_css`] emits the palette-dependent stylesheet; [`full_css`]
//! prepends the bundled base stylesheet; [`apply_css`] installs a stylesheet
//! on the default display.

use crate::theme::Palette;

/// Returns the bundled base stylesheet, embedded at compile time.
pub fn load_base_css() -> &'static str {
    include_str!("../../assets/style/base.css")
}

/// Builds the full stylesheet: base CSS followed by the palette block.
pub fn full_css(palette: &Palette) -> String {
    format!("{}\n{}", load_base_css(), generate_css(palette))
}

/// Generates the palette-specific CSS block for a resolved palette.
///
/// Emits `@define-color` directives for every palette color plus derived
/// colors (`border`, `accent-hover`), then widget rules targeting the
/// `chromia-*` style classes.
pub fn generate_css(palette: &Palette) -> String {
    let border = lighten(&palette.overlay, 0.15);
    let accent_hover = lighten(&palette.accent, 0.10);
    format!(
        r#"@define-color background {background};
@define-color surface {surface};
@define-color overlay {overlay};
@define-color accent {accent};
@define-color text {text};
@define-color subtext {subtext};
@define-color border {border};
@define-color accent-hover {accent_hover};

window {{
    background-color: @background;
    color: @text;
}}

.chromia-cover {{
    border-radius: 8px;
}}

.chromia-title {{
    color: @text;
    font-weight: bold;
}}

.chromia-subtitle {{
    color: @subtext;
}}

.chromia-row-title {{
    color: @text;
}}

.chromia-row-subtitle {{
    color: @subtext;
}}

.chromia-lyrics-text {{
    color: @text;
}}

.chromia-list row:hover {{
    background-color: @overlay;
}}

.chromia-list row.current {{
    background-color: alpha(@accent, 0.18);
}}
"#,
        background = palette.background,
        surface = palette.surface,
        overlay = palette.overlay,
        accent = palette.accent,
        text = palette.text,
        subtext = palette.subtext,
        border = border,
        accent_hover = accent_hover,
    )
}

/// Installs a stylesheet on the default display at application priority.
///
/// # Threading
///
/// **Must be called on the GTK main thread.** `CssProvider::new` asserts an
/// initialized main thread and the provider is otherwise not thread-safe.
/// Returns an error when no display is available (e.g. headless runs).
#[allow(deprecated)] // `load_from_data` is deprecated since GTK 4.12; still the
// documented API for string CSS in gtk4 0.11.
pub fn apply_css(css: &str) -> anyhow::Result<()> {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(css);
    match gtk::gdk::Display::default() {
        Some(display) => {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            Ok(())
        }
        None => Err(anyhow::anyhow!("no GTK display available")),
    }
}

/// Parses a `#rrggbb` hex string into a byte triplet.
fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
    (byte(0), byte(2), byte(4))
}

/// Lightens a `#rrggbb` color by mixing it `amount` (0.0-1.0) toward white.
fn lighten(hex: &str, amount: f64) -> String {
    let (r, g, b) = parse_hex(hex);
    let mix = |c: u8| (c as f64 + (255.0 - c as f64) * amount).round() as u8;
    format!("#{:02x}{:02x}{:02x}", mix(r), mix(g), mix(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_palette() -> Palette {
        Palette {
            background: "#1e1e2e".into(),
            surface: "#313244".into(),
            overlay: "#45475a".into(),
            accent: "#cba6f7".into(),
            text: "#cdd6f4".into(),
            subtext: "#a6adc8".into(),
        }
    }

    #[test]
    fn generate_css_defines_all_six_palette_colors() {
        let css = generate_css(&sample_palette());
        for (name, value) in [
            ("background", "#1e1e2e"),
            ("surface", "#313244"),
            ("overlay", "#45475a"),
            ("accent", "#cba6f7"),
            ("text", "#cdd6f4"),
            ("subtext", "#a6adc8"),
        ] {
            assert!(
                css.contains(&format!("@define-color {name} {value};")),
                "missing @define-color {name} {value};"
            );
        }
    }

    #[test]
    fn generate_css_includes_derived_colors_and_classes() {
        let css = generate_css(&sample_palette());
        assert!(css.contains("@define-color border"));
        assert!(css.contains("@define-color accent-hover"));
        for class in [
            "chromia-cover",
            "chromia-list",
            "chromia-title",
            "chromia-subtitle",
            "chromia-lyrics-text",
        ] {
            assert!(css.contains(class), "missing {class} rule");
        }
    }

    #[test]
    fn full_css_includes_the_base_stylesheet() {
        let css = full_css(&sample_palette());
        assert!(css.contains(".chromia-shell"), "base.css marker missing");
        assert!(css.starts_with(load_base_css()));
    }

    #[test]
    fn lighten_moves_toward_white() {
        assert_eq!(lighten("#45475a", 0.0), "#45475a");
        assert_eq!(lighten("#000000", 1.0), "#ffffff");
    }
}
