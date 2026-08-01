//! Right-panel slot definitions.
//!
//! The right panel is composed of vertical slots. Each slot hosts one widget
//! from a fixed catalogue — the same catalogue used by the future
//! drag-and-drop editor. Today the catalogue only feeds the slot order: every
//! widget identifier declared here corresponds to a real builder in
//! `ui::right_panel`.

use std::fmt;

/// Catalogue of widgets that can live in the right panel.
///
/// The string form (`as_str`) is the canonical TOML identifier used in the
/// `layout.right_panel.slots` config section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotWidget {
    /// Large album cover with shadow.
    AlbumArt,
    /// Standalone transport widget (progress + controls).
    Player,
    /// Synchronised lyrics view.
    Lyrics,
    /// Up-next playback queue.
    Queue,
    /// 10-band graphical equalizer.
    Equalizer,
    /// Audio spectrum / waveform visualizer.
    Visualizer,
    /// Album year, genre, label readout.
    AlbumInfo,
    /// Artist biography readout.
    ArtistInfo,
    /// "Similar albums" recommendations.
    SimilarAlbums,
    /// Bitrate / codec / sample-rate readout.
    AudioInfo,
    /// Output device picker.
    Devices,
}

impl SlotWidget {
    /// Returns the canonical TOML identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AlbumArt => "AlbumArt",
            Self::Player => "Player",
            Self::Lyrics => "Lyrics",
            Self::Queue => "Queue",
            Self::Equalizer => "Equalizer",
            Self::Visualizer => "Visualizer",
            Self::AlbumInfo => "AlbumInfo",
            Self::ArtistInfo => "ArtistInfo",
            Self::SimilarAlbums => "SimilarAlbums",
            Self::AudioInfo => "AudioInfo",
            Self::Devices => "Devices",
        }
    }

    /// Parses a TOML identifier back into a [`SlotWidget`].
    pub fn from_str(name: &str) -> Option<Self> {
        match name.trim() {
            "AlbumArt" => Some(Self::AlbumArt),
            "Player" => Some(Self::Player),
            "Lyrics" => Some(Self::Lyrics),
            "Queue" => Some(Self::Queue),
            "Equalizer" => Some(Self::Equalizer),
            "Visualizer" => Some(Self::Visualizer),
            "AlbumInfo" => Some(Self::AlbumInfo),
            "ArtistInfo" => Some(Self::ArtistInfo),
            "SimilarAlbums" => Some(Self::SimilarAlbums),
            "AudioInfo" => Some(Self::AudioInfo),
            "Devices" => Some(Self::Devices),
            _ => None,
        }
    }

    /// Iterates over every slot widget in the canonical order.
    #[allow(dead_code)] // TODO(loki): used by the layout editor palette
    pub const fn all() -> [Self; 11] {
        [
            Self::AlbumArt,
            Self::Player,
            Self::Lyrics,
            Self::Queue,
            Self::Equalizer,
            Self::Visualizer,
            Self::AlbumInfo,
            Self::ArtistInfo,
            Self::SimilarAlbums,
            Self::AudioInfo,
            Self::Devices,
        ]
    }
}

impl fmt::Display for SlotWidget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Default slot order used when the user has no `[layout.right_panel]`
/// configuration.
pub fn default_slots() -> Vec<SlotWidget> {
    vec![
        SlotWidget::AlbumArt,
        SlotWidget::Player,
        SlotWidget::Lyrics,
        SlotWidget::Queue,
    ]
}

/// Parses a TOML slot list, ignoring unknown identifiers so the right panel
/// always renders even if the user mistyped a widget name.
#[allow(dead_code)] // TODO(loki): consumed by the config sync layer
pub fn parse_slots(raw: &[String]) -> Vec<SlotWidget> {
    raw.iter()
        .filter_map(|name| SlotWidget::from_str(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_widget_round_trips() {
        for widget in SlotWidget::all() {
            assert_eq!(
                SlotWidget::from_str(widget.as_str()),
                Some(widget),
                "{} should round-trip",
                widget.as_str()
            );
        }
    }

    #[test]
    fn unknown_slot_name_is_ignored() {
        assert!(SlotWidget::from_str("DefinitelyNotAWidget").is_none());
    }

    #[test]
    fn default_slots_match_spec() {
        let defaults = default_slots();
        assert_eq!(
            defaults,
            vec![
                SlotWidget::AlbumArt,
                SlotWidget::Player,
                SlotWidget::Lyrics,
                SlotWidget::Queue,
            ]
        );
    }

    #[test]
    fn parse_slots_filters_unknowns() {
        let raw = vec![
            "AlbumArt".to_string(),
            "Unknown".to_string(),
            "Queue".to_string(),
        ];
        let parsed = parse_slots(&raw);
        assert_eq!(parsed, vec![SlotWidget::AlbumArt, SlotWidget::Queue]);
    }
}
