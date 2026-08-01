//! Configuration loading, saving and filesystem helpers.
//!
//! The config lives at `~/.config/chromia/config.toml`; if it does not exist
//! yet, a copy of the bundled [`DEFAULT_CONFIG`] is written on first run.

pub mod schema;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::{debug, warn};

use self::schema::{Config, WidgetPlacement};

/// Bundled default configuration, embedded at compile time.
pub const DEFAULT_CONFIG: &str = include_str!("../../config/default.toml");

/// Returns the config directory (`~/.config/chromia`).
pub fn config_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|d| d.join("chromia"))
        .context("could not resolve the XDG config directory")
}

/// Returns the cache directory (`~/.cache/chromia`).
pub fn cache_dir() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|d| d.join("chromia"))
        .context("could not resolve the XDG cache directory")
}

/// Expands a leading `~` in a path against `$HOME`.
pub fn expand_path(path: &Path) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    path.to_path_buf()
}

impl Config {
    /// Loads the user config, creating it from [`DEFAULT_CONFIG`] if missing.
    ///
    /// Missing or invalid fields silently fall back to defaults so a broken
    /// config never prevents the player from starting.
    pub fn load() -> Result<Self> {
        let path = config_dir()?.join("config.toml");
        debug!(path = %path.display(), "loading configuration");

        if !path.exists() {
            if let Err(err) = Self::write_defaults(&path) {
                warn!(error = %err, "could not write default config");
            }
        }

        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;

        match toml::from_str::<Config>(&contents) {
            Ok(config) => Ok(config),
            Err(err) => {
                warn!(error = %err, "invalid config, falling back to defaults");
                Ok(Config::default())
            }
        }
    }

    /// Saves this configuration back to disk, preserving the TOML structure.
    pub fn save(&self) -> Result<()> {
        let path = config_dir()?.join("config.toml");
        Self::write_to(&path, self)
    }

    /// Updates the widget layout in this config and persists it.
    ///
    /// Reserved for a future layout editor; the window currently uses a fixed
    /// single-screen arrangement.
    #[allow(dead_code)]
    pub fn update_layout(&mut self, widgets: &[WidgetPlacement]) -> Result<()> {
        self.layout.widgets = widgets.to_vec();
        self.save()
    }

    /// Writes the bundled defaults to `path`, creating parent directories.
    pub fn write_defaults(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        std::fs::write(path, DEFAULT_CONFIG)
            .with_context(|| format!("failed to write config to {}", path.display()))?;
        debug!(path = %path.display(), "default configuration written");
        Ok(())
    }

    fn write_to(path: &Path, config: &Config) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let serialized =
            toml::to_string_pretty(config).context("failed to serialize configuration to TOML")?;
        std::fs::write(path, serialized)
            .with_context(|| format!("failed to write config to {}", path.display()))?;
        debug!(path = %path.display(), "configuration saved");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let parsed: Config = toml::from_str(DEFAULT_CONFIG).expect("default toml is valid");
        assert_eq!(parsed.theme.mode, schema::ThemeMode::Dynamic);
        assert_eq!(parsed.theme.catppuccin.flavor, schema::Flavor::Mocha);
        assert_eq!(parsed.audio.volume, 0.8);
    }

    #[test]
    fn config_roundtrips_through_toml() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).expect("serializes");
        let parsed: Config = toml::from_str(&serialized).expect("parses back");
        assert_eq!(config, parsed);
    }

    #[test]
    fn invalid_config_falls_back_to_defaults() {
        let parsed: Result<Config, _> = toml::from_str("not [valid toml at all !!");
        assert!(parsed.is_err());
    }

    #[test]
    fn expand_path_expands_tilde() {
        let home = dirs::home_dir().expect("home dir exists");
        assert_eq!(expand_path(Path::new("~/Music")), home.join("Music"));
        assert_eq!(
            expand_path(Path::new("/abs/path")),
            PathBuf::from("/abs/path")
        );
    }
}
