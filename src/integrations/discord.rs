//! Discord Rich Presence integration.
//!
//! Publishes the currently playing track to the Discord client via the IPC
//! socket, driven by [`PlayerEvent`]s from the audio task.

#![cfg(feature = "discord")]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use discord_presence::Client;
use discord_presence::models::{Activity, ActivityTimestamps};
use tokio::sync::mpsc;

use crate::audio::{PlaybackState, PlayerEvent};
use crate::library::Track;

/// TODO(loki): replace with the real Discord application id.
pub const CLIENT_ID: u64 = 1_103_153_029_914_382_372;

/// How long the worker thread sleeps between event polls.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Marker type; the presence worker is spawned via [`Discord::spawn`].
pub struct Discord;

impl Discord {
    /// Spawns a dedicated thread that publishes Rich Presence. `events` is one
    /// of the broadcast receivers fed by the audio task.
    ///
    /// # Errors
    ///
    /// Fails if the worker thread could not be spawned.
    pub fn spawn(events: mpsc::Receiver<PlayerEvent>) -> anyhow::Result<()> {
        std::thread::Builder::new()
            .name("chromia-discord".into())
            .spawn(run_presence_worker(events))?;
        Ok(())
    }
}

/// The presence worker loop. Runs until the audio task closes the channel.
fn run_presence_worker(events: mpsc::Receiver<PlayerEvent>) -> impl FnOnce() {
    move || {
        let mut events = events;
        let mut client = Client::new(CLIENT_ID);
        client.start();

        let mut track: Option<Track> = None;
        let mut start_ms: u64 = now_ms();

        loop {
            match events.try_recv() {
                Ok(PlayerEvent::TrackStarted(new_track)) => {
                    track = Some(new_track);
                    start_ms = now_ms();
                    if let Some(track) = &track {
                        publish(&mut client, track, start_ms, None);
                    }
                }
                Ok(PlayerEvent::PlaybackStateChanged(PlaybackState::Playing)) => {
                    start_ms = now_ms();
                    if let Some(track) = &track {
                        publish(&mut client, track, start_ms, None);
                    }
                }
                Ok(PlayerEvent::PlaybackStateChanged(PlaybackState::Paused)) => {
                    if let Some(track) = &track {
                        publish(&mut client, track, start_ms, Some(now_ms()));
                    }
                }
                Ok(PlayerEvent::PlaybackStateChanged(PlaybackState::Stopped)) => {
                    track = None;
                    if Client::is_ready() {
                        if let Err(e) = client.clear_activity() {
                            tracing::warn!("discord: clear_activity failed: {e}");
                        }
                    }
                }
                Ok(_) => {}
                Err(mpsc::error::TryRecvError::Empty) => {}
                Err(mpsc::error::TryRecvError::Disconnected) => break,
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

/// (Re)sets the activity for `track`, optionally ending the elapsed timer.
fn publish(client: &mut Client, track: &Track, start: u64, end: Option<u64>) {
    if !Client::is_ready() {
        return;
    }
    let activity = |activity: Activity| {
        activity
            .details(&track.title)
            .state(format!("{} - {}", track.artist, track.album))
            .timestamps(|timestamps: ActivityTimestamps| match end {
                Some(end) => timestamps.start(start).end(end),
                None => timestamps.start(start),
            })
    };
    if let Err(e) = client.set_activity(activity) {
        tracing::warn!("discord: set_activity failed: {e}");
    }
}

/// Current UNIX time in milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}
