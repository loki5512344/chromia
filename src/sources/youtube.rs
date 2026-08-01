//! YouTube / YouTube Music source backed by yt-dlp.

use std::path::PathBuf;

use anyhow::Result;

use crate::library::{SourceKind, Track};
use crate::sources::{download_stream, parse_search_response, run_ytdlp, track_from_info};

/// Searches YouTube and resolves streams through yt-dlp.
#[derive(Debug, Clone)]
pub struct YoutubeSource {
    /// Preferred quality (`best`, `320k`, `256k`, `128k`).
    quality: String,
    /// Directory where resolved streams are cached.
    cache_dir: PathBuf,
}

impl YoutubeSource {
    /// Creates a source with the given quality and cache directory.
    pub fn new(quality: &str, cache_dir: PathBuf) -> Self {
        Self {
            quality: quality.to_string(),
            cache_dir,
        }
    }

    /// Runs a YouTube search and returns up to 25 matching tracks.
    pub async fn search(&self, query: &str) -> Result<Vec<Track>> {
        let query_arg = format!("ytsearch25:{query}");
        let stdout = run_ytdlp(&["--flat-playlist", "-J", query_arg.as_str()]).await?;
        let stdout = String::from_utf8_lossy(&stdout);
        Ok(parse_search_response(stdout.as_ref(), SourceKind::Youtube))
    }

    /// Resolves full metadata (including the cover URL) for a single video.
    #[allow(dead_code)] // TODO(loki): consumed by the GUI before playing/downloading
    pub async fn fetch_info(&self, url: &str) -> Result<Track> {
        let stdout = run_ytdlp(&["--no-playlist", "-J", url]).await?;
        let stdout = String::from_utf8_lossy(&stdout);
        let value: serde_json::Value = serde_json::from_str(&stdout)?;
        track_from_info(&value, SourceKind::Youtube)
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
        download_stream(url, &self.cache_dir, "yt", &self.quality)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::library::SourceKind;
    use crate::sources::parse_search_response;

    #[test]
    fn parses_search_response() {
        let json = r#"{
            "entries": [
                {
                    "id": "abc123",
                    "title": "First Track",
                    "url": "https://www.youtube.com/watch?v=abc123",
                    "duration": 214,
                    "channel": "Channel One"
                },
                {
                    "id": "def456",
                    "title": "Second Track",
                    "duration": 95.5,
                    "uploader": "Uploader Two"
                }
            ]
        }"#;
        let tracks = parse_search_response(json, SourceKind::Youtube);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].source, SourceKind::Youtube);
        assert_eq!(tracks[0].title, "First Track");
        assert_eq!(
            tracks[0].url.as_deref(),
            Some("https://www.youtube.com/watch?v=abc123")
        );
        assert_eq!(tracks[0].artist, "Channel One");
        assert_eq!(tracks[0].duration, Duration::from_secs(214));
        assert_eq!(tracks[0].album, "YouTube");
        assert_eq!(
            tracks[1].url.as_deref(),
            Some("https://www.youtube.com/watch?v=def456")
        );
        assert_eq!(tracks[1].artist, "Uploader Two");
        assert_eq!(tracks[1].duration, Duration::from_secs(95));
    }

    #[test]
    fn empty_response_yields_no_tracks() {
        let tracks = parse_search_response("{}", SourceKind::Youtube);
        assert!(tracks.is_empty());
    }
}
