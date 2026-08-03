//! GTK CSS generation and application.
//!
//! [`generate_css`] emits the palette-dependent stylesheet; [`full_css`]
//! prepends the bundled base stylesheet; [`apply_css`] installs a stylesheet
//! on the default display.

use crate::config::schema::AppearanceConfig;
use crate::config::schema::GlassMode;
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

/// Emits the appearance-driven CSS block: corner radius, blur radius and the
/// Glass UI tint applied by the live `appearance_applier` hook.
///
/// Unlike the palette block, this is regenerated on every appearance change
/// (Glass toggle, animations, blur, border radius) rather than bundled in the
/// base sheet, because the values are numeric and configurable at runtime.
/// It only ships translucent fills that reference the palette `@define-color`s
/// from [`generate_css`]; `backdrop-filter` merely *enriches* those surfaces on
/// compositors that support blur (Hyprland, KDE) and harmlessly no-ops
/// elsewhere, keeping an opaque-ish fallback for everyone.
pub fn appearance_css(appearance: &AppearanceConfig) -> String {
    let radius = appearance.border_radius.max(1);
    let blur = appearance.blur;
    let opacity = appearance.glass_opacity.clamp(0.0, 1.0);
    let on = appearance.glass && appearance.glass_mode != GlassMode::Disabled;
    let strong = on && appearance.glass_mode == GlassMode::Strong;

    // Strong mode dims the fill a touch more and doubles the blur for depth.
    let (tint, glass_blur) = if strong {
        ((opacity * 0.65).min(0.55), (blur as u16 * 2).max(28))
    } else {
        (opacity, blur as u16)
    };

    format!(
        r#"/* ═══ Appearance knobs (radius / blur / glass) ═══ */
.chromia-shell.appearance {{
    border-radius: {radius}px;
}}
window.appearance {{
    border-radius: {radius}px;
}}
.chromia-card,
.chromia-slot,
.chromia-cover,
.chromia-album-card {{
    border-radius: {radius}px;
}}

/* Glass is opt-in via the `.glass` root class set on the shell. */
.chromia-shell.glass .chromia-sidebar,
.chromia-shell.glass .chromia-right-panel,
.chromia-shell.glass .chromia-bottom-player {{
    background-color: alpha(@surface, {tint});
    backdrop-filter: blur({glass_blur}px);
}}
.chromia-shell.glass .chromia-shell {{
    background-color: alpha(@background, 0.78);
}}
.chromia-shell.glass .chromia-card,
.chromia-shell.glass .card {{
    background-color: alpha(@surface, {tint});
    backdrop-filter: blur({glass_blur}px);
}}

/* Muted, hairline edges so translucent panels stay legible without blur. */
.chromia-shell.glass .chromia-sidebar,
.chromia-shell.glass .chromia-right-panel,
.chromia-shell.glass .chromia-bottom-player {{
    box-shadow: inset 0 0 0 1px alpha(white, 0.04);
}}
"#,
        radius = radius,
        tint = tint,
        glass_blur = glass_blur,
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

    #[test]
    fn appearance_css_emits_radius_and_blur() {
        let app = crate::config::schema::AppearanceConfig {
            glass: true,
            glass_opacity: 0.8,
            blur: 24,
            noise: true,
            glass_mode: crate::config::schema::GlassMode::Light,
            border_radius: 16,
            animations: true,
            edit_mode: false,
            follow_wallpaper: false,
            glass_background: crate::config::schema::GlassBackground::Dynamic,
        };
        let css = appearance_css(&app);
        assert!(css.contains("border-radius: 16px"));
        assert!(css.contains("blur(24px)"));
        assert!(css.contains("alpha(@surface"));
        assert!(css.contains(".chromia-shell.glass"));
    }
}
