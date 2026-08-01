//! Filesystem scanner that discovers local audio tracks.

use std::fs;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::library::Track;
use crate::library::metadata;

/// File extensions treated as audio during a scan (case-insensitive).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "opus", "m4a", "aac", "wav", "mka", "webm",
];

/// Recursively walks directories and reads metadata for every audio file.
pub struct Scanner;

impl Scanner {
    /// Creates a new scanner.
    pub fn new() -> Self {
        Self
    }

    /// Scans each of the given paths, returning every playable track found.
    ///
    /// Directories are walked recursively; entries whose name starts with `.`
    /// (hidden files and directories) are skipped.
    pub fn scan_paths(&self, paths: &[PathBuf]) -> Vec<Track> {
        let mut tracks = Vec::new();
        for path in paths {
            self.scan_path(path, &mut tracks);
        }
        tracks
    }

    /// Reads metadata for a single audio file, if it is parseable.
    pub fn scan_file(&self, path: &Path) -> Option<Track> {
        if !is_audio_file(path) {
            return None;
        }
        let meta = match metadata::Metadata::read(path) {
            Ok(meta) => meta,
            Err(error) => {
                warn!(path = %path.display(), error = %error, "failed to read audio metadata");
                return None;
            }
        };
        let mut track = Track::new_local(
            path.to_path_buf(),
            meta.title,
            meta.artist,
            meta.album,
            meta.duration,
        );
        track.track_no = meta.track_no;
        track.disc_no = meta.disc_no;
        track.genre = meta.genre;
        track.year = meta.year;
        track.bpm = meta.bpm;
        Some(track)
    }

    /// Recursively walks `path`, pushing found tracks onto `out`.
    fn scan_path(&self, path: &Path, out: &mut Vec<Track>) {
        let entry = match fs::metadata(path) {
            Ok(entry) => entry,
            Err(error) => {
                warn!(path = %path.display(), error = %error, "failed to stat path during scan");
                return;
            }
        };

        if entry.is_file() {
            if let Some(track) = self.scan_file(path) {
                out.push(track);
            }
        } else if entry.is_dir() {
            let entries = match fs::read_dir(path) {
                Ok(entries) => entries,
                Err(error) => {
                    warn!(path = %path.display(), error = %error, "failed to read directory during scan");
                    return;
                }
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with('.') {
                    continue;
                }
                self.scan_path(&entry.path(), out);
            }
        }
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `path` has an extension listed in [`AUDIO_EXTENSIONS`].
fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            AUDIO_EXTENSIONS
                .iter()
                .any(|audio| audio.eq_ignore_ascii_case(ext))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{AUDIO_EXTENSIONS, Scanner};

    /// Builds a minimal valid PCM WAV file (16-bit mono, 44100 Hz, 1 second).
    fn wav_bytes() -> Vec<u8> {
        const SAMPLE_RATE: u32 = 44_100;
        const CHANNELS: u16 = 1;
        const BITS_PER_SAMPLE: u16 = 16;
        const DURATION_SECS: u32 = 1;

        let bytes_per_sample = u32::from(BITS_PER_SAMPLE / 8);
        let data_len = SAMPLE_RATE * u32::from(CHANNELS) * bytes_per_sample * DURATION_SECS;
        let byte_rate = SAMPLE_RATE * u32::from(CHANNELS) * bytes_per_sample;
        let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);

        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&CHANNELS.to_le_bytes());
        wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..data_len / u32::from(block_align) {
            let sample = (i as i16).wrapping_mul(2);
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        wav
    }

    #[test]
    fn scan_file_parses_a_real_wav() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("test.wav");
        fs::write(&path, wav_bytes()).expect("write wav");

        let track = Scanner::new()
            .scan_file(&path)
            .expect("scan_file should parse the wav");
        assert_eq!(track.path, path);
        assert!(
            !track.title.is_empty(),
            "title should fall back to the file stem"
        );
        assert!(
            track.duration.as_millis() > 0,
            "duration should be parsed from the wav"
        );
    }

    #[test]
    fn scan_paths_walks_recursively_and_filters() {
        let dir = tempdir().expect("tempdir");
        let sub = dir.path().join("nested");
        fs::create_dir_all(&sub).expect("create nested dir");
        fs::write(dir.path().join("fake.mp3"), b"not a real mp3").expect("write mp3");
        fs::write(sub.join("real.wav"), wav_bytes()).expect("write wav");
        fs::write(dir.path().join("notes.txt"), b"hello").expect("write text");
        fs::write(dir.path().join(".hidden"), wav_bytes()).expect("write hidden file");

        let tracks = Scanner::new().scan_paths(&[dir.path().to_path_buf()]);
        let names: Vec<String> = tracks
            .iter()
            .map(|t| t.path.to_string_lossy().into_owned())
            .collect();

        assert!(
            names.iter().any(|n| n.ends_with("real.wav")),
            "missing real.wav in {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.ends_with("fake.mp3")),
            "invalid mp3 must be skipped"
        );
        assert!(
            !names.iter().any(|n| n.ends_with("notes.txt")),
            "non-audio must be skipped"
        );
        assert!(
            !names.iter().any(|n| n.contains(".hidden")),
            "hidden files must be skipped"
        );
    }

    #[test]
    fn scan_file_on_non_audio_returns_none() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("notes.txt");
        fs::write(&path, b"hello").expect("write text");
        assert!(Scanner::new().scan_file(&path).is_none());
    }

    #[test]
    fn audio_extensions_are_non_empty() {
        assert!(!AUDIO_EXTENSIONS.is_empty());
        assert!(AUDIO_EXTENSIONS.iter().all(|ext| !ext.is_empty()));
    }
}
