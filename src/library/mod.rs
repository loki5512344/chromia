//! Music library: domain model, SQLite storage, metadata reading and scanning.

pub mod database;
pub mod metadata;
pub mod scanner;

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Where a [`Track`] originates from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// A file on the local filesystem.
    Local,
    /// A stream resolved via yt-dlp (YouTube / YouTube Music).
    Youtube,
    /// A stream resolved via yt-dlp (SoundCloud).
    Soundcloud,
}

impl std::fmt::Display for SourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Local => "local",
            Self::Youtube => "youtube",
            Self::Soundcloud => "soundcloud",
        })
    }
}

/// A single playable track, regardless of its source.
///
/// Local tracks carry a file `path`; streamed tracks carry a `url` and are
/// downloaded to the cache before playback.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Track {
    /// Database row id; `0` for tracks not yet persisted.
    pub id: i64,
    /// Source of the track.
    pub source: SourceKind,
    /// File path (local tracks) or cached stream path.
    pub path: PathBuf,
    /// Remote stream / info URL (yt-dlp sources).
    pub url: Option<String>,
    /// Remote cover art URL (yt-dlp sources), if known.
    pub thumbnail: Option<String>,
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
    /// How many times the track has been played.
    pub play_count: u32,
    /// Last time the track was played.
    pub last_played: Option<DateTime<Utc>>,
}

impl Track {
    /// Creates a track from a local file.
    pub fn new_local(
        path: PathBuf,
        title: String,
        artist: String,
        album: String,
        duration: Duration,
    ) -> Self {
        Self {
            id: 0,
            source: SourceKind::Local,
            path,
            url: None,
            thumbnail: None,
            title,
            artist,
            album,
            album_artist: String::new(),
            duration,
            track_no: None,
            disc_no: None,
            genre: None,
            year: None,
            bpm: None,
            play_count: 0,
            last_played: None,
        }
    }

    /// Creates a remote track resolved through yt-dlp.
    pub fn new_remote(source: SourceKind, url: String, title: String) -> Self {
        Self {
            id: 0,
            source,
            path: PathBuf::new(),
            url: Some(url),
            thumbnail: None,
            title,
            artist: String::new(),
            album: String::new(),
            album_artist: String::new(),
            duration: Duration::ZERO,
            track_no: None,
            disc_no: None,
            genre: None,
            year: None,
            bpm: None,
            play_count: 0,
            last_played: None,
        }
    }

    /// Whether the track is currently resolvable to an audio stream.
    pub fn is_playable(&self) -> bool {
        !self.path.as_os_str().is_empty() || self.url.is_some()
    }

    /// Convenience label: `Artist - Title`.
    pub fn label(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} - {}", self.artist, self.title)
        }
    }
}

impl std::fmt::Display for Track {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}
