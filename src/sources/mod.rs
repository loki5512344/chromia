//! Music sources: local files, YouTube and SoundCloud (both via yt-dlp).

pub mod local;
pub mod soundcloud;
pub mod watch;
pub mod youtube;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde_json::Value;

use crate::library::{SourceKind, Track};

/// Returns the sources enabled in `config`, in priority order.
///
/// Unknown entries in `sources.enabled` are ignored.
pub fn enabled_sources(config: &crate::config::schema::SourcesConfig) -> Vec<SourceKind> {
    config
        .enabled
        .iter()
        .filter_map(|name| match name.as_str() {
            "local" => Some(SourceKind::Local),
            "youtube" => Some(SourceKind::Youtube),
            "soundcloud" => Some(SourceKind::Soundcloud),
            _ => None,
        })
        .collect()
}

/// Returns the source named by `sources.default`, falling back to `Local`.
pub fn default_source(config: &crate::config::schema::SourcesConfig) -> SourceKind {
    match config.default.as_str() {
        "youtube" => SourceKind::Youtube,
        "soundcloud" => SourceKind::Soundcloud,
        _ => SourceKind::Local,
    }
}

/// Downloads a remote stream via yt-dlp into `cache_dir` and returns the final
/// file path.
///
/// Runs synchronously so the audio engine can use it on its blocking thread.
pub fn download_stream(
    url: &str,
    cache_dir: &Path,
    prefix: &str,
    quality: &str,
) -> Result<PathBuf> {
    let file_id = derive_file_id(url);
    std::fs::create_dir_all(cache_dir)
        .with_context(|| format!("failed to create cache dir {}", cache_dir.display()))?;
    let output_template = cache_dir.join(format!("{prefix}-{file_id}.%(ext)s"));
    let format = format_selector(quality);
    let output = std::process::Command::new("yt-dlp")
        .arg("--no-playlist")
        .arg("-f")
        .arg(&format)
        .arg("-o")
        .arg(&output_template)
        .arg("--print-json")
        .arg(url)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "yt-dlp exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json = String::from_utf8_lossy(&output.stdout);
    let filepath =
        extract_filepath(json.as_ref()).context("yt-dlp did not report a downloaded file")?;
    if filepath.exists() {
        Ok(filepath)
    } else {
        anyhow::bail!(
            "yt-dlp reported {}, but the file does not exist",
            filepath.display()
        )
    }
}

/// Parses a `yt-dlp --flat-playlist -J` search response into [`Track`]s.
pub(crate) fn parse_search_response(json: &str, source: SourceKind) -> Vec<Track> {
    let Ok(root) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let Some(entries) = root.get("entries").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| track_from_info(entry, source))
        .collect()
}

/// Downloads a remote track's cover art into `cache_dir/thumbnails` and returns
/// its path, reusing the cached file on repeat requests.
#[allow(dead_code)] // TODO(loki): consumed by the GUI for remote cover art
pub async fn download_thumbnail(url: &str, cache_dir: &Path) -> Result<PathBuf> {
    let thumbs_dir = cache_dir.join("thumbnails");
    std::fs::create_dir_all(&thumbs_dir)
        .with_context(|| format!("failed to create thumbnail dir {}", thumbs_dir.display()))?;
    let ext = Path::new(url)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .filter(|ext| matches!(ext.as_str(), "jpg" | "jpeg" | "webp" | "png"))
        .unwrap_or_else(|| "jpg".to_string());
    let file = thumbs_dir.join(format!("{}.{ext}", derive_file_id(url)));
    if file.exists() {
        return Ok(file);
    }
    let response = reqwest::get(url).await?.error_for_status()?;
    let bytes = response.bytes().await?;
    std::fs::write(&file, &bytes)
        .with_context(|| format!("failed to cache thumbnail to {}", file.display()))?;
    Ok(file)
}

/// Builds a [`Track`] from a single yt-dlp info / flat-playlist entry.
fn track_from_info(entry: &Value, source: SourceKind) -> Option<Track> {
    let title = entry.get("title")?.as_str()?.to_string();
    let url = entry
        .get("url")
        .or_else(|| entry.get("webpage_url"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .map(|id| entry_url(source, id))
        })
        .unwrap_or_default();
    if url.is_empty() {
        return None;
    }
    let mut track = Track::new_remote(source, url, title);
    track.artist = entry
        .get("uploader")
        .or_else(|| entry.get("channel"))
        .or_else(|| entry.get("artist"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    track.duration =
        Duration::from_secs(entry.get("duration").and_then(Value::as_f64).unwrap_or(0.0) as u64);
    track.thumbnail = entry
        .get("thumbnail")
        .and_then(Value::as_str)
        .map(str::to_string);
    track.album = match source {
        SourceKind::Youtube => "YouTube",
        SourceKind::Soundcloud => "SoundCloud",
        SourceKind::Local => "Local",
    }
    .to_string();
    Some(track)
}

/// Builds a source-specific URL from a flat-playlist entry id.
fn entry_url(source: SourceKind, id: &str) -> String {
    match source {
        SourceKind::Youtube => format!("https://www.youtube.com/watch?v={id}"),
        SourceKind::Soundcloud => format!("https://soundcloud.com/{id}"),
        SourceKind::Local => String::new(),
    }
}

/// Extracts the produced file path from a `yt-dlp --print-json` response.
///
/// Prefers `requested_downloads[0].filepath`, falling back to `filepath`.
pub(crate) fn extract_filepath(json: &str) -> Option<PathBuf> {
    let root: Value = serde_json::from_str(json).ok()?;
    if let Some(first) = root
        .get("requested_downloads")
        .and_then(Value::as_array)
        .and_then(|downloads| downloads.first())
    {
        if let Some(path) = first.get("filepath").and_then(Value::as_str) {
            return Some(PathBuf::from(path));
        }
    }
    root.get("filepath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

/// Stable hex id derived from a remote URL.
///
/// Used for cache file names so they never contain URL characters that are
/// unsafe in file paths.
pub(crate) fn derive_file_id(url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Maps a configured quality string to a yt-dlp format selector.
///
/// `best` and empty strings map to plain `bestaudio`; bitrates like `320k`
/// prefer audio at most that bitrate and fall back to `bestaudio`.
pub(crate) fn format_selector(quality: &str) -> String {
    match quality.trim() {
        "" | "best" => "bestaudio".to_string(),
        bitrate => format!("bestaudio[abr<={bitrate}]/bestaudio"),
    }
}

/// Runs `yt-dlp` and returns its stdout on success.
///
/// A missing binary maps to a helpful error; non-zero exits include stderr.
pub(crate) async fn run_ytdlp(args: &[&str]) -> Result<Vec<u8>> {
    let output = match tokio::process::Command::new("yt-dlp")
        .args(args)
        .output()
        .await
    {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(anyhow!("yt-dlp not found; install it to use this source"));
        }
        Err(err) => return Err(err).context("failed to run yt-dlp"),
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp failed: {}", stderr.trim()));
    }
    Ok(output.stdout)
}
