//! SoundCloud source backed by yt-dlp.

use std::path::PathBuf;

use anyhow::Result;

use crate::library::{SourceKind, Track};
use crate::sources::{download_stream, parse_search_response, run_ytdlp, track_from_info};

/// Searches SoundCloud and resolves streams through yt-dlp.
#[derive(Debug, Clone)]
pub struct SoundcloudSource {
    /// Preferred quality (`best`, `320k`, `256k`, `128k`).
    quality: String,
    /// Directory where resolved streams are cached.
    cache_dir: PathBuf,
}

impl SoundcloudSource {
    /// Creates a source with the given quality and cache directory.
    pub fn new(quality: &str, cache_dir: PathBuf) -> Self {
        Self {
            quality: quality.to_string(),
            cache_dir,
        }
    }

    /// Runs a SoundCloud search and returns up to 10 matching tracks.
    pub async fn search(&self, query: &str) -> Result<Vec<Track>> {
        let query_arg = format!("scsearch10:{query}");
        let stdout = run_ytdlp(&["--flat-playlist", "-J", query_arg.as_str()]).await?;
        let stdout = String::from_utf8_lossy(&stdout);
        Ok(parse_search_response(
            stdout.as_ref(),
            SourceKind::Soundcloud,
        ))
    }

    /// Resolves full metadata (including the cover URL) for a single track.
    #[allow(dead_code)] // TODO(loki): consumed by the GUI before playing/downloading
    pub async fn fetch_info(&self, url: &str) -> Result<Track> {
        let stdout = run_ytdlp(&["--no-playlist", "-J", url]).await?;
        let stdout = String::from_utf8_lossy(&stdout);
        let value: serde_json::Value = serde_json::from_str(&stdout)?;
        track_from_info(&value, SourceKind::Soundcloud)
            .ok_or_else(|| anyhow::anyhow!("yt-dlp returned no usable info for {url}"))
    }

    /// Downloads the best available audio for `track` into the cache directory.
    ///
    /// Runs synchronously so the audio engine can call it from its blocking
    /// thread when a streamed track starts playing.
    pub fn resolve_stream(&self, track: &Track) -> Result<PathBuf> {
        let url = track
            .url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("track has no remote URL"))?;
        download_stream(url, &self.cache_dir, "sc", &self.quality)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use crate::library::SourceKind;
    use crate::sources::{extract_filepath, parse_search_response};

    #[test]
    fn parses_soundcloud_search_response() {
        let json = r#"{
            "entries": [
                {
                    "id": "123456",
                    "title": "SC Track",
                    "url": "https://soundcloud.com/artist/track",
                    "duration": 180,
                    "uploader": "SC Artist"
                }
            ]
        }"#;
        let tracks = parse_search_response(json, SourceKind::Soundcloud);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].source, SourceKind::Soundcloud);
        assert_eq!(tracks[0].title, "SC Track");
        assert_eq!(
            tracks[0].url.as_deref(),
            Some("https://soundcloud.com/artist/track")
        );
        assert_eq!(tracks[0].artist, "SC Artist");
        assert_eq!(tracks[0].duration, Duration::from_secs(180));
        assert_eq!(tracks[0].album, "SoundCloud");
    }

    #[test]
    fn extracts_filepath_from_requested_downloads() {
        let json = r#"{
            "id": "xyz",
            "requested_downloads": [{"filepath": "/tmp/yt-abc123.mp3"}],
            "filepath": "/tmp/ignored.mp3"
        }"#;
        assert_eq!(
            extract_filepath(json),
            Some(PathBuf::from("/tmp/yt-abc123.mp3"))
        );
    }

    #[test]
    fn falls_back_to_top_level_filepath() {
        let json = r#"{"id": "xyz", "filepath": "/tmp/fallback.webm"}"#;
        assert_eq!(
            extract_filepath(json),
            Some(PathBuf::from("/tmp/fallback.webm"))
        );
    }
}
