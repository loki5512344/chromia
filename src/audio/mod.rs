//! Audio engine: rodio playback, queue management and equalizer.
//!
//! The GTK main thread never touches the audio stack directly. All control
//! flows through [`PlayerCommand`] sent over a `tokio::sync::mpsc` channel to a
//! background task; playback events travel back as [`PlayerEvent`].

pub mod equalizer;
pub mod player;
pub mod queue;

use std::time::Duration;

use crate::library::Track;

pub use player::Player;

/// Playback state reported to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    /// Audio is currently playing.
    Playing,
    /// Playback paused in place.
    Paused,
    /// No track loaded.
    Stopped,
}

/// Repeat behaviour of the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    /// Play the queue once, then stop.
    Off,
    /// Restart the queue after the last track.
    All,
    /// Repeat the current track.
    One,
}

/// Commands sent from the UI (or integrations) to the audio task.
#[derive(Debug, Clone)]
pub enum PlayerCommand {
    /// Toggle play / pause.
    PlayPause,
    /// Pause playback.
    #[allow(dead_code)] // sent by the MPRIS and keyboard control layers
    Pause,
    /// Resume playback.
    #[allow(dead_code)] // sent by the MPRIS and keyboard control layers
    Resume,
    /// Stop playback and unload the track.
    #[allow(dead_code)] // sent by the MPRIS and keyboard control layers
    Stop,
    /// Advance to the next track in the queue.
    Next,
    /// Go back to the previous track.
    Previous,
    /// Seek to a position in the current track.
    Seek(Duration),
    /// Set the volume, 0.0-1.0.
    SetVolume(f32),
    /// Toggle shuffle.
    SetShuffle(bool),
    /// Change the repeat mode.
    SetRepeat(RepeatMode),
    /// Replace the queue contents.
    LoadQueue(Vec<Track>),
    /// Play the track at `index` in the current queue.
    PlayAt(usize),
    /// Set a single equalizer band gain, in dB.
    SetBand { index: usize, gain_db: f32 },
    /// Apply a named equalizer preset.
    SetEqualizerPreset(String),
    /// Toggle the equalizer on / off.
    SetEqualizerEnabled(bool),
}

/// Events emitted by the audio task and consumed by the UI thread.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // TrackStarted legitimately carries a full Track
pub enum PlayerEvent {
    /// A track started playing.
    TrackStarted(Track),
    /// Playback state changed.
    PlaybackStateChanged(PlaybackState),
    /// Playback position advanced.
    PositionChanged(Duration),
    /// The current track's duration became known.
    DurationChanged(Duration),
    /// Volume changed.
    VolumeChanged(f32),
    /// Shuffle mode changed.
    ShuffleChanged(bool),
    /// Repeat mode changed.
    RepeatChanged(RepeatMode),
    /// The queue changed.
    QueueChanged(Vec<Track>),
    /// The current track index changed.
    CurrentIndexChanged(Option<usize>),
    /// The current track finished and the queue advanced.
    TrackEnded,
    /// A non-fatal error occurred (e.g. decode failure).
    Error(String),
}
