//! Asynchronous stream downloader for remote sources (YouTube, SoundCloud).
//!
//! A worker task accepts [`DownloadRequest`]s, runs `yt-dlp` for each with a
//! configurable concurrency limit, parses progress from the process output and
//! reports [`DownloadEvent`]s to a single receiver.
//!
//! The GUI owns the event receiver; nothing here touches the GTK thread.
#![allow(dead_code)] // TODO(loki): consumed by the GUI once it lands

use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use tokio::io::{AsyncBufReadExt, AsyncRead};
use tokio::process::Command;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use crate::library::Track;
use crate::sources::format_selector;

/// Maximum number of simultaneous yt-dlp processes.
pub const MAX_CONCURRENT_DOWNLOADS: usize = 3;

/// A request to download a remote track into a directory.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// The remote track (must carry a `url`).
    pub track: Track,
    /// Directory the audio file is saved into.
    pub destination_dir: PathBuf,
    /// Preferred quality (`best`, `320k`, `256k`, `128k`).
    pub quality: String,
}

/// Download lifecycle events emitted on the shared receiver.
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    /// A download task was accepted.
    Started { id: u64 },
    /// Download progress, 0.0-100.0.
    Progress { id: u64, percent: f32 },
    /// The download completed successfully.
    Finished { id: u64, path: PathBuf },
    /// The download failed or was cancelled.
    Failed { id: u64, error: String },
}

/// A handle for enqueuing and cancelling downloads.
#[derive(Clone)]
pub struct DownloadManager {
    command_tx: mpsc::Sender<DownloadCommand>,
    next_id: Arc<AtomicU64>,
}

/// Internal worker commands.
#[allow(clippy::large_enum_variant)] // Enqueue legitimately carries a full request
enum DownloadCommand {
    Enqueue { id: u64, request: DownloadRequest },
    Cancel(u64),
}

/// A line parsed from yt-dlp output.
#[derive(Debug, PartialEq)]
enum DownloadLine {
    Progress(f32),
    FilePath(PathBuf),
}

impl DownloadManager {
    /// Starts the download worker and returns its handle plus the event
    /// receiver. Must be called from within a running Tokio runtime.
    pub fn spawn() -> (Self, mpsc::Receiver<DownloadEvent>) {
        let (command_tx, command_rx) = mpsc::channel(64);
        let (event_tx, event_rx) = mpsc::channel(256);
        tokio::spawn(download_worker(command_rx, event_tx));
        (
            Self {
                command_tx,
                next_id: Arc::new(AtomicU64::new(1)),
            },
            event_rx,
        )
    }

    /// Enqueues `request` and returns its download id.
    pub async fn download(&self, request: DownloadRequest) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let _ = self
            .command_tx
            .send(DownloadCommand::Enqueue { id, request })
            .await;
        id
    }

    /// Requests cancellation of the download with `id`.
    pub async fn cancel(&self, id: u64) {
        let _ = self.command_tx.send(DownloadCommand::Cancel(id)).await;
    }
}

/// The worker loop: schedules downloads with a concurrency limit, keeps a
/// bounded queue of waiting requests and propagates cancels.
async fn download_worker(
    mut commands: mpsc::Receiver<DownloadCommand>,
    events: mpsc::Sender<DownloadEvent>,
) {
    let mut active: JoinSet<()> = JoinSet::new();
    let mut cancels: HashMap<u64, watch::Sender<bool>> = HashMap::new();
    let mut queue: VecDeque<(u64, DownloadRequest)> = VecDeque::new();

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break; };
                match command {
                    DownloadCommand::Enqueue { id, request } => {
                        if active.len() < MAX_CONCURRENT_DOWNLOADS {
                            spawn_one(id, request, &events, &mut active, &mut cancels);
                        } else {
                            queue.push_back((id, request));
                        }
                    }
                    DownloadCommand::Cancel(id) => {
                        if let Some(cancel) = cancels.remove(&id) {
                            let _ = cancel.send(true);
                        }
                    }
                }
            }
            _ = active.join_next(), if !active.is_empty() => {
                if let Some((id, request)) = queue.pop_front() {
                    spawn_one(id, request, &events, &mut active, &mut cancels);
                }
            }
        }
    }
}

/// Starts one download task and registers its cancel signal.
fn spawn_one(
    id: u64,
    request: DownloadRequest,
    events: &mpsc::Sender<DownloadEvent>,
    active: &mut JoinSet<()>,
    cancels: &mut HashMap<u64, watch::Sender<bool>>,
) {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    cancels.insert(id, cancel_tx);
    let events = events.clone();
    active.spawn(run_download(id, request, events, cancel_rx));
}

/// Runs a single download, emitting lifecycle events.
async fn run_download(
    id: u64,
    request: DownloadRequest,
    events: mpsc::Sender<DownloadEvent>,
    cancel: watch::Receiver<bool>,
) {
    let _ = events.send(DownloadEvent::Started { id }).await;
    match download_to(id, &request, &events, &cancel).await {
        Ok(path) => {
            let _ = events.send(DownloadEvent::Finished { id, path }).await;
        }
        Err(err) => {
            let _ = events
                .send(DownloadEvent::Failed {
                    id,
                    error: format!("{err:#}"),
                })
                .await;
        }
    }
}

/// Drives the `yt-dlp` process, streaming progress events to `events`.
async fn download_to(
    id: u64,
    request: &DownloadRequest,
    events: &mpsc::Sender<DownloadEvent>,
    cancel: &watch::Receiver<bool>,
) -> Result<PathBuf> {
    let dir = &request.destination_dir;
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("failed to create {}", dir.display()))?;
    let format = format_selector(&request.quality);
    let url = request
        .track
        .url
        .as_deref()
        .context("track has no remote URL")?;
    let output_template = dir.join("%(title).%(ext)s");

    let mut child = Command::new("yt-dlp")
        .arg("--no-playlist")
        .arg("-f")
        .arg(&format)
        .arg("-o")
        .arg(&output_template)
        .arg("--progress")
        .arg("--newline")
        .arg("--progress-template")
        .arg("download:%(progress._percent_str)s")
        .arg("--print")
        .arg("after_download:filepath:%(filepath)s")
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                anyhow!("yt-dlp not found; install it to use this source")
            } else {
                anyhow!("failed to start yt-dlp: {err}")
            }
        })?;

    let stdout = child
        .stdout
        .take()
        .context("failed to read yt-dlp stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("failed to read yt-dlp stderr")?;
    let (line_tx, mut line_rx) = mpsc::channel::<DownloadLine>(128);
    let mut readers = JoinSet::new();
    readers.spawn(read_lines(stderr, line_tx.clone()));
    readers.spawn(read_lines(stdout, line_tx));

    let mut filepath: Option<PathBuf> = None;
    loop {
        if cancel.has_changed().unwrap_or(true) {
            let _ = child.kill().await;
            anyhow::bail!("download cancelled");
        }
        if let Some(status) = child.try_wait().context("failed to poll yt-dlp")? {
            if !status.success() {
                anyhow::bail!("yt-dlp exited with {status}");
            }
            break;
        }
        tokio::select! {
            line = line_rx.recv() => {
                if let Some(DownloadLine::Progress(percent)) = line {
                    // Progress is dropped rather than awaited so a slow GUI
                    // consumer cannot stall the downloader.
                    let _ = events.try_send(DownloadEvent::Progress { id, percent });
                }
                if let Some(DownloadLine::FilePath(path)) = line {
                    filepath = Some(path);
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    // Reap the line readers and drain any buffered filepath.
    while readers.join_next().await.is_some() {}
    while let Ok(DownloadLine::FilePath(path)) = line_rx.try_recv() {
        filepath = Some(path);
    }

    let filepath = filepath.context("yt-dlp did not report the downloaded file")?;
    if filepath.exists() {
        Ok(filepath)
    } else {
        anyhow::bail!(
            "yt-dlp reported {}, but the file does not exist",
            filepath.display()
        )
    }
}

/// Streams lines from `stream`, forwarding parsed download lines to `tx`.
async fn read_lines(stream: impl AsyncRead + Unpin, tx: mpsc::Sender<DownloadLine>) {
    use tokio::io::BufReader;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if let Some(parsed) = parse_download_line(&line) {
                    let _ = tx.send(parsed).await;
                }
            }
        }
    }
}

/// Parses a single line of yt-dlp output into a typed download line.
fn parse_download_line(line: &str) -> Option<DownloadLine> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("download:") {
        let percent: f32 = rest.trim_end_matches('%').trim().parse().ok()?;
        return Some(DownloadLine::Progress(percent.clamp(0.0, 100.0)));
    }
    if let Some(rest) = line.strip_prefix("filepath:") {
        let path = PathBuf::from(rest.trim());
        if !path.as_os_str().is_empty() {
            return Some(DownloadLine::FilePath(path));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{DownloadLine, parse_download_line};

    #[test]
    fn parses_progress_lines() {
        assert!(matches!(
            parse_download_line("download:45.2%"),
            Some(DownloadLine::Progress(45.2))
        ));
        assert!(matches!(
            parse_download_line("download:100.0%\r"),
            Some(DownloadLine::Progress(p)) if (p - 100.0).abs() < f32::EPSILON
        ));
        assert!(parse_download_line("download:not-a-number").is_none());
    }

    #[test]
    fn parses_filepath_lines() {
        assert_eq!(
            parse_download_line("filepath:/tmp/Music/Song.webm"),
            Some(DownloadLine::FilePath(PathBuf::from(
                "/tmp/Music/Song.webm"
            )))
        );
        assert_eq!(parse_download_line("filepath:"), None);
    }

    #[test]
    fn ignores_noise() {
        assert_eq!(parse_download_line("[youtube] Extracting URL: x"), None);
        assert_eq!(parse_download_line(""), None);
    }
}
