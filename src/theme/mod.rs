//! Theme resolution: palette selection and GTK CSS generation.
//!
//! Four modes exist (see [`crate::config::schema::ThemeMode`]):
//! * `dynamic` — colors extracted from album art via color-thief,
//! * `catppuccin` — static Catppuccin palette,
//! * `preset` — a ready-made palette from the bundled [`presets`] catalog,
//! * `custom` — user-supplied hex colors.

pub mod catppuccin;
pub mod css;
pub mod dynamic;
pub mod presets;

pub use catppuccin::palette_for;
pub use dynamic::palette_from_image;
pub use presets::{resolve as resolve_preset, ThemePreset, ALL as ALL_THEME_PRESETS};

/// A resolved color palette; every color is a hex string like `#1e1e2e`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// Window / page background.
    pub background: String,
    /// Raised surface background.
    pub surface: String,
    /// Overlay / elevated elements.
    pub overlay: String,
    /// Primary accent color.
    pub accent: String,
    /// Primary text color.
    pub text: String,
    /// Secondary (muted) text color.
    pub subtext: String,
}

impl Palette {
    /// Builds a palette straight from the user-supplied custom theme config.
    pub fn from_custom(custom: &crate::config::schema::CustomThemeConfig) -> Self {
        Self {
            background: custom.background.clone(),
            surface: custom.surface.clone(),
            overlay: custom.overlay.clone(),
            accent: custom.accent.clone(),
            text: custom.text.clone(),
            subtext: custom.subtext.clone(),
        }
    }
}
