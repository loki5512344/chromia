//! AudioInfo widget - codec, bitrate, sample rate and channel readout.
//!
//! Displayed in the right panel under the "Audiophile" workspace (see
//! `CHROMIA.md`). Extracts technical metadata from the track path via
//! `lofty` in a background thread and presents it as a compact grid.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use lofty::prelude::AudioFile;

use crate::audio::PlayerEvent;
use crate::library::Track;
use crate::ui::UiContext;

/// One labelled readout cell.
struct InfoCell {
    label: gtk::Label,
    value: gtk::Label,
}

impl InfoCell {
    fn new(label_text: &str) -> Self {
        let label = gtk::Label::builder()
            .label(label_text)
            .css_classes(vec!["chromia-audio-info-label"])
            .halign(gtk::Align::Start)
            .build();
        let value = gtk::Label::builder()
            .label("—")
            .css_classes(vec!["chromia-audio-info-value"])
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        Self { label, value }
    }

    fn attach(&self, grid: &gtk::Grid, col: i32, row: i32) {
        grid.attach(&self.label, col, row, 1, 1);
        grid.attach(&self.value, col + 1, row, 1, 1);
    }
}

/// Extracted audio technical metadata (best-effort from lofty tags).
#[derive(Debug, Clone, Default)]
struct AudioMeta {
    codec: String,
    bitrate_kbps: Option<u32>,
    sample_rate_hz: Option<u32>,
    channels: Option<u8>,
    bit_depth: Option<u8>,
}

/// Reads coarse technical information from the file at `path` using lofty.
fn read_meta(path: &std::path::Path) -> AudioMeta {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let codec = match ext.as_str() {
        "mp3" => "MP3",
        "flac" => "FLAC",
        "ogg" => "OGG Vorbis",
        "opus" => "Opus",
        "aac" | "m4a" => "AAC",
        "wav" => "PCM / WAV",
        "aiff" | "aif" => "AIFF",
        "wma" => "WMA",
        _ => "Unknown",
    }
    .to_owned();

    // Try to pull sample rate / bitrate from lofty properties.
    let (sample_rate_hz, channels, bit_depth, bitrate_kbps) =
        if let Ok(tagged) = lofty::read_from_path(path) {
            let props = tagged.properties();
            (
                props.sample_rate(),
                props.channels(),
                props.bit_depth(),
                props.audio_bitrate(),
            )
        } else {
            (None, None, None, None)
        };

    AudioMeta {
        codec,
        bitrate_kbps,
        sample_rate_hz,
        channels,
        bit_depth,
    }
}

/// Format a sample rate value as a human-readable string.
fn fmt_sample_rate(hz: u32) -> String {
    if hz % 1_000 == 0 {
        format!("{} kHz", hz / 1_000)
    } else {
        format!("{:.1} kHz", hz as f64 / 1_000.0)
    }
}

/// Format a bitrate as kbps string.
fn fmt_bitrate(kbps: u32) -> String {
    format!("{} kbps", kbps)
}

/// Format channel count as "Mono", "Stereo", or "N ch".
fn fmt_channels(ch: u8) -> String {
    match ch {
        1 => "Mono".into(),
        2 => "Stereo".into(),
        n => format!("{} ch", n),
    }
}

/// The AudioInfo right-panel widget.
pub struct AudioInfo {
    #[allow(dead_code)] // TODO(loki): rendered when the AudioInfo slot is enabled
    root: gtk::Box,
    codec_cell: InfoCell,
    bitrate_cell: InfoCell,
    sample_rate_cell: InfoCell,
    channels_cell: InfoCell,
    bit_depth_cell: InfoCell,
    current_track: Rc<RefCell<Option<Track>>>,
    rt: tokio::runtime::Handle,
}

impl AudioInfo {
    /// Builds the AudioInfo widget.
    pub fn new(ctx: &UiContext) -> Self {
        let header = gtk::Label::builder()
            .label("Audio Info")
            .css_classes(vec!["chromia-header"])
            .halign(gtk::Align::Start)
            .build();

        let codec_cell = InfoCell::new("Codec");
        let bitrate_cell = InfoCell::new("Bitrate");
        let sample_rate_cell = InfoCell::new("Sample rate");
        let channels_cell = InfoCell::new("Channels");
        let bit_depth_cell = InfoCell::new("Bit depth");

        let grid = gtk::Grid::builder()
            .row_spacing(6)
            .column_spacing(12)
            .css_classes(vec!["chromia-audio-info-grid"])
            .build();

        codec_cell.attach(&grid, 0, 0);
        bitrate_cell.attach(&grid, 0, 1);
        sample_rate_cell.attach(&grid, 0, 2);
        channels_cell.attach(&grid, 0, 3);
        bit_depth_cell.attach(&grid, 0, 4);

        // Source badge - shows "Local", "YouTube", "SoundCloud"
        let source_badge = gtk::Label::builder()
            .label("No track")
            .css_classes(vec!["chromia-source-badge"])
            .halign(gtk::Align::Start)
            .build();

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-audio-info"])
            .spacing(10)
            .build();
        root.append(&header);
        root.append(&source_badge);
        root.append(&grid);

        Self {
            root,
            codec_cell,
            bitrate_cell,
            sample_rate_cell,
            channels_cell,
            bit_depth_cell,
            current_track: Rc::new(RefCell::new(None)),
            rt: ctx.rt.clone(),
        }
    }

    /// Returns the widget root for embedding in the right panel.
    #[allow(dead_code)] // TODO(loki): rendered when the AudioInfo slot is enabled
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Reacts to playback events - loads audio metadata on track start.
    pub fn update(&self, event: &PlayerEvent) {
        if let PlayerEvent::TrackStarted(track) = event {
            *self.current_track.borrow_mut() = Some(track.clone());
            self.load_meta(track);
        }
    }

    /// Spawns a background task to read audio metadata from the file.
    fn load_meta(&self, track: &Track) {
        let path = track.path.clone();
        let codec_lbl = self.codec_cell.value.clone();
        let bitrate_lbl = self.bitrate_cell.value.clone();
        let sr_lbl = self.sample_rate_cell.value.clone();
        let ch_lbl = self.channels_cell.value.clone();
        let bd_lbl = self.bit_depth_cell.value.clone();

        // For remote tracks we don't have a local path - show source only.
        if path.as_os_str().is_empty() {
            codec_lbl.set_label("Stream");
            bitrate_lbl.set_label("—");
            sr_lbl.set_label("—");
            ch_lbl.set_label("—");
            bd_lbl.set_label("—");
            return;
        }

        let rt = self.rt.clone();
        glib::MainContext::default().spawn_local(async move {
            let path_clone = path.clone();
            let meta = rt
                .spawn_blocking(move || read_meta(&path_clone))
                .await
                .unwrap_or_default();

            codec_lbl.set_label(&meta.codec);
            bitrate_lbl.set_label(
                &meta
                    .bitrate_kbps
                    .map(fmt_bitrate)
                    .unwrap_or_else(|| "—".into()),
            );
            sr_lbl.set_label(
                &meta
                    .sample_rate_hz
                    .map(fmt_sample_rate)
                    .unwrap_or_else(|| "—".into()),
            );
            ch_lbl.set_label(
                &meta
                    .channels
                    .map(fmt_channels)
                    .unwrap_or_else(|| "—".into()),
            );
            bd_lbl.set_label(
                &meta
                    .bit_depth
                    .map(|d| format!("{} bit", d))
                    .unwrap_or_else(|| "—".into()),
            );
        });
    }
}
