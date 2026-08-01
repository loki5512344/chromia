//! Bottom Player presets.
//!
//! Presets are pure UI affordances: each preset maps to an ordered list of
//! [`BottomPlayerElement`]s that the bottom player reveals or hides. They do
//! not change the underlying playback engine, they only change what is
//! visible on the bar.
//!
//! The values mirror the spec in `CHROMIA.md`:
//!
//! | Preset      | Elements                                                            |
//! |-------------|---------------------------------------------------------------------|
//! | minimal     | `Cover Song Play`                                                   |
//! | default     | `Cover Song Artist Progress Controls Volume`                        |
//! | audiophile  | `Cover Song Artist Waveform Bitrate SampleRate Codec Device ...`    |

use std::fmt;

/// Identifiers for every element that can appear in the bottom player bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomPlayerElement {
    /// Square album cover thumbnail.
    Cover,
    /// Track title.
    Song,
    /// Track artist.
    Artist,
    /// Seek bar with timestamps.
    Progress,
    /// Transport controls (shuffle / prev / play / next / repeat).
    Controls,
    /// Volume icon + scale.
    Volume,
    /// Audio waveform (placeholder for future DSP visualizer).
    Waveform,
    /// Bitrate readout, e.g. `320 kbps`.
    Bitrate,
    /// Sample-rate readout, e.g. `44.1 kHz`.
    SampleRate,
    /// Codec readout, e.g. `FLAC`.
    Codec,
    /// Output device selector.
    Device,
}

impl BottomPlayerElement {
    /// Returns the canonical TOML identifier for this element.
    #[allow(dead_code)] // TODO(loki): consumed by the config sync layer
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "Cover",
            Self::Song => "Song",
            Self::Artist => "Artist",
            Self::Progress => "Progress",
            Self::Controls => "Controls",
            Self::Volume => "Volume",
            Self::Waveform => "Waveform",
            Self::Bitrate => "Bitrate",
            Self::SampleRate => "SampleRate",
            Self::Codec => "Codec",
            Self::Device => "Device",
        }
    }
}

/// Available bottom-player presets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BottomPlayerPreset {
    /// `Cover Song Play` — minimal footprint.
    Minimal,
    /// `Cover Song Artist Progress Controls Volume` — the recommended default.
    #[default]
    Default,
    /// Maximum information density (bitrate, codec, sample rate, device, …).
    Audiophile,
}

impl BottomPlayerPreset {
    /// Returns the preset's display name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Default => "default",
            Self::Audiophile => "audiophile",
        }
    }

    /// Returns the index of this preset in [`Self::all`].
    pub fn as_index(self) -> usize {
        Self::all().iter().position(|p| *p == self).unwrap_or(1)
    }

    /// Iterates over every preset in display order.
    pub const fn all() -> [Self; 3] {
        [Self::Minimal, Self::Default, Self::Audiophile]
    }

    /// Returns the display names of every preset, in order.
    pub fn all_names() -> Vec<String> {
        Self::all().iter().map(|p| p.as_str().to_string()).collect()
    }

    /// Parses a preset from its display name.
    #[allow(dead_code)] // TODO(loki): consumed by the config sync layer
    pub fn from_str(name: &str) -> Option<Self> {
        match name.trim() {
            "minimal" => Some(Self::Minimal),
            "default" => Some(Self::Default),
            "audiophile" => Some(Self::Audiophile),
            _ => None,
        }
    }

    /// Resolves a preset from a `DropDown` index.
    pub fn from_index(index: usize) -> Option<Self> {
        Self::all().get(index).copied()
    }

    /// Returns the ordered list of elements this preset exposes.
    pub fn elements(self) -> Vec<BottomPlayerElement> {
        match self {
            Self::Minimal => vec![
                BottomPlayerElement::Cover,
                BottomPlayerElement::Song,
                BottomPlayerElement::Controls,
            ],
            Self::Default => vec![
                BottomPlayerElement::Cover,
                BottomPlayerElement::Song,
                BottomPlayerElement::Artist,
                BottomPlayerElement::Progress,
                BottomPlayerElement::Controls,
                BottomPlayerElement::Volume,
            ],
            Self::Audiophile => vec![
                BottomPlayerElement::Cover,
                BottomPlayerElement::Song,
                BottomPlayerElement::Artist,
                BottomPlayerElement::Waveform,
                BottomPlayerElement::Bitrate,
                BottomPlayerElement::SampleRate,
                BottomPlayerElement::Codec,
                BottomPlayerElement::Device,
                BottomPlayerElement::Progress,
                BottomPlayerElement::Controls,
                BottomPlayerElement::Volume,
            ],
        }
    }
}

impl fmt::Display for BottomPlayerPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returns the canonical element list for the [`BottomPlayerPreset::Default`]
/// preset — useful as a fallback when applying presets programmatically.
#[allow(dead_code)] // TODO(loki): consumed by the config sync layer
pub fn default_elements() -> Vec<BottomPlayerElement> {
    BottomPlayerPreset::default().elements()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preset_is_default() {
        assert_eq!(BottomPlayerPreset::default(), BottomPlayerPreset::Default);
    }

    #[test]
    fn preset_round_trips_through_name() {
        for preset in BottomPlayerPreset::all() {
            assert_eq!(
                BottomPlayerPreset::from_str(preset.as_str()),
                Some(preset),
                "{} should round-trip through name",
                preset.as_str()
            );
        }
    }

    #[test]
    fn preset_index_matches_all_order() {
        for (i, preset) in BottomPlayerPreset::all().iter().enumerate() {
            assert_eq!(preset.as_index(), i);
            assert_eq!(BottomPlayerPreset::from_index(i), Some(*preset));
        }
    }

    #[test]
    fn minimal_preset_omits_volume_and_progress() {
        let minimal = BottomPlayerPreset::Minimal.elements();
        assert!(!minimal.contains(&BottomPlayerElement::Volume));
        assert!(!minimal.contains(&BottomPlayerElement::Progress));
        assert!(minimal.contains(&BottomPlayerElement::Cover));
    }

    #[test]
    fn audiophile_preset_includes_codec_and_sample_rate() {
        let audiophile = BottomPlayerPreset::Audiophile.elements();
        assert!(audiophile.contains(&BottomPlayerElement::Codec));
        assert!(audiophile.contains(&BottomPlayerElement::SampleRate));
        assert!(audiophile.contains(&BottomPlayerElement::Device));
    }
}
