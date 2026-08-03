//! Typed configuration schema (serde + toml).
//!
//! Every field mirrors a section of `~/.config/chromia/config.toml`.
//! All structs implement [`serde::Deserialize`] with kebab-case keys and
//! fall back to defaults for any missing field via `#[serde(default)]`.

use serde::{Deserialize, Serialize};

/// Top-level application configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct Config {
    /// Whether the first-run onboarding has been completed.
    #[serde(default = "default_true")]
    pub first_run: bool,
    /// Theme selection (dynamic / catppuccin / custom).
    pub theme: ThemeConfig,
    /// Widget layout for the main window.
    pub layout: LayoutConfig,
    /// Music sources (local, youtube, soundcloud).
    pub sources: SourcesConfig,
    /// Audio playback settings.
    pub audio: AudioConfig,
    /// Optional integrations (MPRIS2, Discord RPC).
    pub integrations: IntegrationsConfig,
    /// Filesystem locations (cache, etc).
    pub paths: PathsConfig,
    /// Appearance options (glass UI, animations, edit mode).
    #[serde(default)]
    pub appearance: AppearanceConfig,
}

/// `serde` default for booleans that should start as `true`.
fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            first_run: true,
            theme: ThemeConfig::default(),
            layout: LayoutConfig::default(),
            sources: SourcesConfig::default(),
            audio: AudioConfig::default(),
            integrations: IntegrationsConfig::default(),
            paths: PathsConfig::default(),
            appearance: AppearanceConfig::default(),
        }
    }
}

/// Theme configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct ThemeConfig {
    /// Active theme mode.
    pub mode: ThemeMode,
    /// Crossfade duration for color transitions, in milliseconds.
    pub transition_ms: u32,
    /// Whether to blur the album art behind the window.
    pub blur_background: bool,
    /// Blur strength, 0-100.
    pub blur_strength: u8,
    /// Catppuccin flavor settings (used in `catppuccin` and as fallback).
    pub catppuccin: CatppuccinConfig,
    /// Ready-made preset name (used in `preset` mode).
    pub preset: String,
    /// User-defined palette (used in `custom` mode).
    pub custom: CustomThemeConfig,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dynamic,
            transition_ms: 300,
            blur_background: true,
            blur_strength: 20,
            catppuccin: CatppuccinConfig::default(),
            preset: "Midnight".into(),
            custom: CustomThemeConfig::default(),
        }
    }
}

/// Theme resolution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Colors extracted from the current album art via color-thief.
    Dynamic,
    /// Static Catppuccin palette.
    Catppuccin,
    /// Ready-made preset palette from `theme.preset`.
    Preset,
    /// User-supplied colors from `theme.custom`.
    Custom,
}

/// Catppuccin palette settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct CatppuccinConfig {
    /// Flavor: mocha / macchiato / frappe / latte.
    pub flavor: Flavor,
    /// Accent color name, e.g. `mauve`, `blue`, `green`.
    pub accent: String,
}

impl Default for CatppuccinConfig {
    fn default() -> Self {
        Self {
            flavor: Flavor::Mocha,
            accent: "mauve".into(),
        }
    }
}

/// Catppuccin flavor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    Mocha,
    Macchiato,
    Frappe,
    Latte,
}

/// User-defined theme palette (hex strings like `#1e1e2e`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct CustomThemeConfig {
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

impl Default for CustomThemeConfig {
    fn default() -> Self {
        Self {
            background: "#1e1e2e".into(),
            surface: "#313244".into(),
            overlay: "#45475a".into(),
            accent: "#cba6f7".into(),
            text: "#cdd6f4".into(),
            subtext: "#a6adc8".into(),
        }
    }
}

/// Drag-and-drop widget layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct LayoutConfig {
    /// Ordered placement of every widget in the window.
    pub widgets: Vec<WidgetPlacement>,
    /// Right-panel slot configuration.
    #[serde(default)]
    pub right_panel: RightPanelLayout,
    /// Bottom-player preset configuration.
    #[serde(default)]
    pub bottom_player: BottomPlayerLayout,
    /// Per-page slot overrides (keyed by page name).
    #[serde(default)]
    pub pages: std::collections::HashMap<String, PageLayout>,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            widgets: vec![
                WidgetPlacement {
                    id: "PlayerCore".into(),
                    x: 0,
                    y: 0,
                    width: 3,
                    height: 1,
                    visible: true,
                },
                WidgetPlacement {
                    id: "Library".into(),
                    x: 3,
                    y: 0,
                    width: 3,
                    height: 2,
                    visible: true,
                },
                WidgetPlacement {
                    id: "Queue".into(),
                    x: 6,
                    y: 0,
                    width: 2,
                    height: 2,
                    visible: true,
                },
                WidgetPlacement {
                    id: "Lyrics".into(),
                    x: 0,
                    y: 1,
                    width: 3,
                    height: 2,
                    visible: true,
                },
                WidgetPlacement {
                    id: "Equalizer".into(),
                    x: 0,
                    y: 2,
                    width: 8,
                    height: 1,
                    visible: false,
                },
                WidgetPlacement {
                    id: "MiniPlayer".into(),
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 1,
                    visible: false,
                },
            ],
            right_panel: RightPanelLayout::default(),
            bottom_player: BottomPlayerLayout::default(),
            pages: std::collections::HashMap::new(),
        }
    }
}

/// Right-panel slot configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct RightPanelLayout {
    /// Ordered widget identifiers, e.g. `["AlbumArt", "Player", "Lyrics"]`.
    pub slots: Vec<String>,
}

impl Default for RightPanelLayout {
    fn default() -> Self {
        Self {
            slots: vec![
                "AlbumArt".into(),
                "Player".into(),
                "Lyrics".into(),
                "Queue".into(),
            ],
        }
    }
}

/// Bottom-player preset configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct BottomPlayerLayout {
    /// Preset name: `minimal`, `default` or `audiophile`.
    pub preset: String,
    /// Optional explicit element list (overrides `preset` when non-empty).
    #[serde(default)]
    pub elements: Vec<String>,
}

impl Default for BottomPlayerLayout {
    fn default() -> Self {
        Self {
            preset: "default".into(),
            elements: Vec::new(),
        }
    }
}

/// Per-page layout (center + right slots) overrides.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct PageLayout {
    /// Center-page widget identifiers.
    #[serde(default)]
    pub center: Vec<String>,
    /// Right-panel slot identifiers for this page.
    #[serde(default)]
    pub right: Vec<String>,
}

/// A single widget placement in the layout grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct WidgetPlacement {
    /// Widget identifier, e.g. `PlayerCore`.
    pub id: String,
    /// Horizontal grid position.
    pub x: i32,
    /// Vertical grid position.
    pub y: i32,
    /// Widget width in grid cells.
    pub width: i32,
    /// Widget height in grid cells.
    pub height: i32,
    /// Whether the widget is shown.
    pub visible: bool,
}

impl Default for WidgetPlacement {
    fn default() -> Self {
        Self {
            id: "PlayerCore".into(),
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            visible: true,
        }
    }
}

/// Music source configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct SourcesConfig {
    /// Enabled sources, in priority order.
    pub enabled: Vec<String>,
    /// Default source used for lookups.
    pub default: String,
    /// Local folder scanning.
    pub local: LocalSourceConfig,
    /// YouTube via yt-dlp.
    pub youtube: YoutubeSourceConfig,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            enabled: vec!["local".into(), "youtube".into(), "soundcloud".into()],
            default: "local".into(),
            local: LocalSourceConfig::default(),
            youtube: YoutubeSourceConfig::default(),
        }
    }
}

/// Local filesystem source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct LocalSourceConfig {
    /// Folders scanned for audio files.
    pub paths: Vec<std::path::PathBuf>,
    /// Re-scan folders on startup when the library changed.
    pub watch: bool,
}

impl Default for LocalSourceConfig {
    fn default() -> Self {
        Self {
            paths: vec!["~/Music".into()],
            watch: true,
        }
    }
}

/// YouTube (Music) source backed by yt-dlp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct YoutubeSourceConfig {
    /// Preferred quality: `best`, `320k`, `256k` or `128k`.
    pub quality: String,
    /// Directory for cached stream downloads.
    pub cache_dir: std::path::PathBuf,
}

impl Default for YoutubeSourceConfig {
    fn default() -> Self {
        Self {
            quality: "best".into(),
            cache_dir: std::path::PathBuf::from("~/.cache/chromia/youtube"),
        }
    }
}

/// Audio playback settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct AudioConfig {
    /// Initial volume, 0.0-1.0.
    pub volume: f32,
    /// Crossfade between tracks, in milliseconds.
    pub crossfade_ms: u32,
    /// Apply ReplayGain tags when available.
    pub replaygain: bool,
    /// Output device name override (None = system default).
    pub device: Option<String>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            volume: 0.8,
            crossfade_ms: 0,
            replaygain: false,
            device: None,
        }
    }
}

/// Optional integrations toggle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct IntegrationsConfig {
    /// Export MPRIS2 over D-Bus (media keys, waybar, eww).
    pub mpris: bool,
    /// Publish Discord Rich Presence.
    pub discord: bool,
}

impl Default for IntegrationsConfig {
    fn default() -> Self {
        Self {
            mpris: true,
            discord: true,
        }
    }
}

/// Filesystem paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct PathsConfig {
    /// Cache directory for cover art, streams and DB snapshots.
    pub cache_dir: std::path::PathBuf,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            cache_dir: std::path::PathBuf::from("~/.cache/chromia"),
        }
    }
}

/// Appearance / Glass UI options.
///
/// Mirrors the `[appearance]` section of `CHROMIA.md`. Glass UI only takes
/// effect when the compositor supports blur (Hyprland, KDE); on unsupported
/// compositors the surfaces fall back to a translucent fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct AppearanceConfig {
    /// Enable the Glass UI look.
    pub glass: bool,
    /// Opacity of glass surfaces, 0.0–1.0.
    pub glass_opacity: f32,
    /// Blur radius in pixels applied behind glass surfaces.
    pub blur: u8,
    /// Add a subtle noise texture on top of glass surfaces for depth.
    pub noise: bool,
    /// Glass intensity preset.
    pub glass_mode: GlassMode,
    /// Corner radius (px) used by cards and panels.
    pub border_radius: u8,
    /// Master switch for all animations.
    pub animations: bool,
    /// Whether the layout editor (drag-and-drop slots) is active.
    pub edit_mode: bool,
    /// When glass is on, follow the wallpaper instead of the dynamic palette.
    pub follow_wallpaper: bool,
    /// Source of the glass background.
    pub glass_background: GlassBackground,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            glass: false,
            glass_opacity: 0.82,
            blur: 24,
            noise: true,
            glass_mode: GlassMode::Light,
            border_radius: 14,
            animations: true,
            edit_mode: false,
            follow_wallpaper: false,
            glass_background: GlassBackground::Dynamic,
        }
    }
}

/// Glass intensity preset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlassMode {
    /// Subtle translucency.
    #[default]
    Light,
    /// Stronger blur and dimmer tint.
    Strong,
    /// Glass UI disabled (the option is kept for symmetry).
    Disabled,
}

/// Source of the glass background.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlassBackground {
    /// Dynamic accent color mixed into the glass tint.
    #[default]
    Dynamic,
    /// Wallpaper / album art blurred behind the window.
    Wallpaper,
    /// Solid palette color (no transparency).
    Solid,
}
