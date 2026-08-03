//! Watches the configured local music folders with `notify` (inotify on Linux)
//! and triggers a rescan + store + broadcast whenever files change on disk.
//!
//! The heavy work (scanning, upserting, pruning, broadcasting to the UI) runs on
//! a dedicated background thread so the GTK loop never blocks. Changes are
//! debounced: a burst of filesystem events (e.g. copying an album) collapses
//! into a single rescan once the folder has been quiet for a moment.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::library::Track;
use crate::library::database::Database;
use crate::sources::local::LocalSource;

/// How long the folder must stay quiet before a rescan is triggered.
const DEBOUNCE: Duration = Duration::from_millis(600);

/// Starts a background thread that watches each folder in `paths` and rescan the
/// library on any (debounced) change.
///
/// Scans and stores are performed on the watcher thread; results are pushed to
/// `scan_tx`, which the GTK loop drains to refresh the UI.
pub fn spawn_library_watcher(
    paths: Vec<PathBuf>,
    database: Arc<Database>,
    scan_tx: mpsc::Sender<Vec<Track>>,
) {
    let (events_tx, events_rx) = std::sync::mpsc::channel::<()>();
    let mut watcher =
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
            Ok(event) if event_kind_relevant(&event.kind) => {
                let _ = events_tx.send(());
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(error = %err, "library watcher event error"),
        }) {
            Ok(watcher) => watcher,
            Err(err) => {
                tracing::warn!(error = %err, "failed to start the library watcher");
                return;
            }
        };

    for path in &paths {
        if let Err(err) = watcher.watch(path, RecursiveMode::Recursive) {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "failed to watch library folder"
            );
        }
    }

    std::thread::Builder::new()
        .name("chromia-library-watch".to_string())
        .spawn(move || {
            // Keep the watcher registered for the lifetime of this thread.
            let _watcher = watcher;
            loop {
                // Wait for the first event…
                if events_rx.recv().is_err() {
                    // All senders dropped; nothing left to watch.
                    tracing::info!("library watcher shutting down");
                    return;
                }
                // …then collapse a burst of events into one quiet period.
                while events_rx.recv_timeout(DEBOUNCE).is_ok() {}
                tracing::info!("library changed, rescanning");
                scan_and_publish(&paths, &database, &scan_tx);
            }
        })
        .expect("failed to spawn the library watcher thread");
}

/// Filters out noise (access, metadata) events we don't care about.
fn event_kind_relevant(kind: &notify::EventKind) -> bool {
    use notify::EventKind::*;
    // Renames and chmods arrive as `Modify`; we only ignore pure metadata noise.
    matches!(kind, Create(_) | Modify(_) | Remove(_))
}

/// Scans `paths`, stores the result, prunes removed files and broadcasts the
/// refreshed track list to the UI channel. Runs on the watcher thread.
fn scan_and_publish(paths: &[PathBuf], database: &Database, scan_tx: &mpsc::Sender<Vec<Track>>) {
    let tracks = LocalSource::new(paths.to_vec()).scan();
    let keep: Vec<PathBuf> = tracks.iter().map(|t| t.path.clone()).collect();
    if let Err(err) = database.upsert_tracks(&tracks) {
        tracing::warn!(error = %err, "failed to store rescanned library");
        return;
    }
    match database.prune_local_except(&keep) {
        Ok(0) => {}
        Ok(n) => tracing::info!(removed = n, "pruned removed local tracks"),
        Err(err) => tracing::warn!(error = %err, "failed to prune removed tracks"),
    }
    let _ = scan_tx.blocking_send(tracks);
}
