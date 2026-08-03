//! Reading audio metadata (tags, duration, embedded cover art) via `lofty`.

use std::borrow::Cow;
use std::path::Path;
use std::time::Duration;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::read_from_path;
use lofty::tag::{Accessor, ItemKey, ItemValue};

/// Extracted metadata for a single audio file.
#[derive(Debug, Clone, PartialEq)]
pub struct Metadata {
    /// Track title.
    pub title: String,
    /// Performing artist.
    pub artist: String,
    /// Album name.
    pub album: String,
    /// Album artist (may differ from `artist` on compilations).
    pub album_artist: String,
    /// Playback duration.
    pub duration: Duration,
    /// Track number within the disc.
    pub track_no: Option<u32>,
    /// Disc number within the album.
    pub disc_no: Option<u32>,
    /// Primary genre.
    pub genre: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Beats per minute, when tagged.
    pub bpm: Option<f32>,
}

impl Metadata {
    /// Reads metadata from the audio file at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read or is not recognised as a
    /// supported audio format.
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let tagged_file = read_from_path(path).map_err(anyhow::Error::from)?;
        let tag = tagged_file.primary_tag();

        let title = tag
            .and_then(Accessor::title)
            .map(|cow| cow.into_owned())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map_or_else(String::new, str::to_owned)
            });

        Ok(Self {
            title,
            artist: tag
                .and_then(Accessor::artist)
                .map_or_else(String::new, Cow::into_owned),
            album: tag
                .and_then(Accessor::album)
                .map_or_else(String::new, Cow::into_owned),
            album_artist: tag
                .and_then(|tag| tag.get(ItemKey::AlbumArtist))
                .and_then(|item| item.value().text())
                .unwrap_or_default()
                .to_owned(),
            duration: tagged_file.properties().duration(),
            track_no: tag.and_then(Accessor::track),
            disc_no: tag.and_then(Accessor::disk),
            genre: tag.and_then(Accessor::genre).map(|cow| cow.into_owned()),
            year: tag.and_then(Accessor::date).map(|date| date.year as i32),
            bpm: tag.and_then(bpm_of),
        })
    }
}

/// Extracts a ReplayGain value in dB from a track's tags.
///
/// Prefers the track gain over the album gain. Values are clamped to a sane
/// range so a broken tag never distorts playback catastrophically.
///
/// # Errors
///
/// Returns an error when the file cannot be read or is not recognised.
pub fn replaygain_gain_db(path: &Path) -> anyhow::Result<Option<f32>> {
    let tagged_file = read_from_path(path).map_err(anyhow::Error::from)?;
    let Some(tag) = tagged_file.primary_tag() else {
        return Ok(None);
    };
    for key in [ItemKey::ReplayGainTrackGain, ItemKey::ReplayGainAlbumGain] {
        if let Some(value) = tag.get(key).and_then(|item| item.value().text()) {
            if let Some(db) = parse_gain_db(value) {
                return Ok(Some(db));
            }
        }
    }
    Ok(None)
}

/// Parses a `… dB` string like `-6.51 dB` into a finite dB value.
fn parse_gain_db(text: &str) -> Option<f32> {
    let head = text.split_ascii_whitespace().next()?;
    let value: f32 = head.trim().parse().ok()?;
    if value.is_finite() && (-24.0..=24.0).contains(&value) {
        Some(value)
    } else {
        None
    }
}

/// Extracts the first embedded cover picture, if any.
///
/// # Errors
///
/// Returns an error when the file cannot be read or is not recognised as a
/// supported audio format.
pub fn extract_cover(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    let tagged_file = read_from_path(path).map_err(anyhow::Error::from)?;
    let Some(tag) = tagged_file.primary_tag() else {
        return Ok(None);
    };
    Ok(tag
        .pictures()
        .first()
        .map(|picture| picture.data().to_vec()))
}

/// Extracts a decimal BPM value from the tag, if present.
fn bpm_of(tag: &lofty::tag::Tag) -> Option<f32> {
    for key in [ItemKey::Bpm, ItemKey::IntegerBpm] {
        let Some(item) = tag.get(key) else { continue };
        match item.value() {
            ItemValue::Text(text) => match text.trim().parse::<f32>() {
                Ok(bpm) if bpm > 0.0 => return Some(bpm),
                _ => {}
            },
            // MP4 stores BPM as a big-endian `u16` in the `tmpo` atom.
            ItemValue::Binary(bytes) if bytes.len() == 2 => {
                let bpm = u16::from_be_bytes([bytes[0], bytes[1]]);
                if bpm > 0 {
                    return Some(f32::from(bpm));
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::Metadata;

    #[test]
    fn read_missing_file_errors() {
        assert!(Metadata::read(std::path::Path::new("/nonexistent/foo.mp3")).is_err());
    }

    #[test]
    fn read_non_audio_file_errors_gracefully() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("not_audio.tmp");
        fs::write(&path, b"this is definitely not an audio file").expect("write temp file");
        assert!(Metadata::read(&path).is_err());
    }
}
