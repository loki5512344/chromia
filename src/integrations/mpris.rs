//! MPRIS2 D-Bus service.
//!
//! Exports the standard `org.mpris.MediaPlayer2` (identity) and
//! `org.mpris.MediaPlayer2.Player` interfaces on the session bus so media
//! keys, waybar, eww and similar clients can control chromia.

#![cfg(feature = "mpris")]

use std::collections::HashMap;
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use zbus::object_server::InterfaceRef;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Str, Value};
use zbus::{Connection, interface};

use crate::audio::{PlaybackState, PlayerCommand, PlayerEvent, RepeatMode};
use crate::library::Track;

/// D-Bus object path every MPRIS interface lives on.
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";

/// Well-known name of this player on the session bus.
const MPRIS_NAME: &str = "org.mpris.MediaPlayer2.chromia";

/// Playback status reported over MPRIS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl PlaybackStatus {
    /// The MPRIS string representation.
    fn as_str(self) -> &'static str {
        match self {
            Self::Playing => "Playing",
            Self::Paused => "Paused",
            Self::Stopped => "Stopped",
        }
    }
}

impl From<PlaybackState> for PlaybackStatus {
    fn from(state: PlaybackState) -> Self {
        match state {
            PlaybackState::Playing => Self::Playing,
            PlaybackState::Paused => Self::Paused,
            PlaybackState::Stopped => Self::Stopped,
        }
    }
}

/// Loop status reported over MPRIS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LoopStatus {
    #[default]
    None,
    Track,
    Playlist,
}

impl LoopStatus {
    /// The MPRIS string representation.
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Track => "Track",
            Self::Playlist => "Playlist",
        }
    }

    /// Parses a case-insensitive MPRIS loop status string.
    fn from_str_mpris(status: &str) -> Option<Self> {
        match status.to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "track" => Some(Self::Track),
            "playlist" => Some(Self::Playlist),
            _ => None,
        }
    }
}

impl From<RepeatMode> for LoopStatus {
    fn from(mode: RepeatMode) -> Self {
        match mode {
            RepeatMode::Off => Self::None,
            RepeatMode::One => Self::Track,
            RepeatMode::All => Self::Playlist,
        }
    }
}

impl From<LoopStatus> for RepeatMode {
    fn from(status: LoopStatus) -> Self {
        match status {
            LoopStatus::None => RepeatMode::Off,
            LoopStatus::Track => RepeatMode::One,
            LoopStatus::Playlist => RepeatMode::All,
        }
    }
}

/// Shared player state, updated from the audio event stream.
#[derive(Default)]
struct State {
    status: PlaybackStatus,
    track: Option<Track>,
    position: Duration,
    volume: f64,
    shuffle: bool,
    loop_status: LoopStatus,
}

/// `org.mpris.MediaPlayer2` root interface (player identity).
struct ChromiaMprisRoot {
    command_tx: mpsc::Sender<PlayerCommand>,
}

impl ChromiaMprisRoot {
    fn new(command_tx: mpsc::Sender<PlayerCommand>) -> Self {
        Self { command_tx }
    }

    async fn send_command(&self, command: PlayerCommand) -> zbus::fdo::Result<()> {
        self.command_tx
            .send(command)
            .await
            .map_err(|_| zbus::fdo::Error::Failed("audio task is unavailable".into()))
    }
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl ChromiaMprisRoot {
    /// Player name shown to MPRIS clients.
    #[zbus(property)]
    fn identity(&self) -> String {
        "chromia".to_owned()
    }

    /// The player has no window to raise.
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    /// The player can be asked to quit.
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    /// Chromia does not expose a track list.
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    /// Only local files are opened via URI schemes.
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["file".to_owned()]
    }

    /// Formats chromia's audio backend can decode.
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        [
            "audio/mpeg",
            "audio/flac",
            "audio/x-flac",
            "audio/ogg",
            "audio/opus",
            "audio/wav",
            "audio/x-wav",
            "audio/mp4",
            "audio/aac",
            "audio/mp4a-latm",
            "audio/x-m4a",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    /// Quit request: stop playback.
    async fn quit(&self) -> zbus::fdo::Result<()> {
        self.send_command(PlayerCommand::Stop).await
    }
}

/// `org.mpris.MediaPlayer2.Player` interface.
struct ChromiaMprisPlayer {
    state: Arc<parking_lot::Mutex<State>>,
    command_tx: mpsc::Sender<PlayerCommand>,
}

impl ChromiaMprisPlayer {
    fn new(state: Arc<parking_lot::Mutex<State>>, command_tx: mpsc::Sender<PlayerCommand>) -> Self {
        Self { state, command_tx }
    }

    async fn send_command(&self, command: PlayerCommand) -> zbus::fdo::Result<()> {
        self.command_tx
            .send(command)
            .await
            .map_err(|_| zbus::fdo::Error::Failed("audio task is unavailable".into()))
    }

    /// Absolute seek position from a (possibly negative) microsecond value.
    fn seek_duration(micros: i64) -> Duration {
        Duration::from_micros(micros.max(0) as u64)
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl ChromiaMprisPlayer {
    /// Resume playback.
    async fn play(&self) -> zbus::fdo::Result<()> {
        self.send_command(PlayerCommand::Resume).await
    }

    /// Pause playback.
    async fn pause(&self) -> zbus::fdo::Result<()> {
        self.send_command(PlayerCommand::Pause).await
    }

    /// Toggle play / pause.
    async fn play_pause(&self) -> zbus::fdo::Result<()> {
        self.send_command(PlayerCommand::PlayPause).await
    }

    /// Stop playback and unload the track.
    async fn stop(&self) -> zbus::fdo::Result<()> {
        self.send_command(PlayerCommand::Stop).await
    }

    /// Skip to the next track.
    async fn next(&self) -> zbus::fdo::Result<()> {
        self.send_command(PlayerCommand::Next).await
    }

    /// Go back to the previous track.
    async fn previous(&self) -> zbus::fdo::Result<()> {
        self.send_command(PlayerCommand::Previous).await
    }

    /// Seek to `offset` microseconds, clamped to the start of the track.
    async fn seek(&self, offset: i64) -> zbus::fdo::Result<()> {
        self.send_command(PlayerCommand::Seek(Self::seek_duration(offset)))
            .await
    }

    /// Seek to an absolute `position` in microseconds; the track id is ignored.
    async fn set_position(
        &self,
        _track_id: OwnedObjectPath,
        position: i64,
    ) -> zbus::fdo::Result<()> {
        self.send_command(PlayerCommand::Seek(Self::seek_duration(position)))
            .await
    }

    /// Open a URI.
    ///
    /// TODO(loki): resolve non-file URIs (e.g. `https://`) through yt-dlp.
    async fn open_uri(&self, _uri: String) -> zbus::fdo::Result<()> {
        Ok(())
    }

    /// Current playback status.
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.state.lock().status.as_str().to_owned()
    }

    /// Loop behaviour.
    #[zbus(property)]
    fn loop_status(&self) -> String {
        self.state.lock().loop_status.as_str().to_owned()
    }

    #[zbus(property)]
    async fn set_loop_status(&self, status: String) -> zbus::Result<()> {
        let Some(loop_status) = LoopStatus::from_str_mpris(&status) else {
            return Err(zbus::fdo::Error::InvalidArgs("unknown loop status".into()).into());
        };
        self.send_command(PlayerCommand::SetRepeat(loop_status.into()))
            .await
            .map_err(Into::into)
    }

    /// Deprecated alias for `LoopStatus`.
    #[zbus(property)]
    fn repeat_status(&self) -> String {
        self.state.lock().loop_status.as_str().to_owned()
    }

    /// Whether playback order is shuffled.
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.state.lock().shuffle
    }

    #[zbus(property)]
    async fn set_shuffle(&self, shuffle: bool) -> zbus::Result<()> {
        self.send_command(PlayerCommand::SetShuffle(shuffle))
            .await
            .map_err(Into::into)
    }

    /// Playback volume, `0.0`-`1.0`.
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.state.lock().volume
    }

    #[zbus(property)]
    async fn set_volume(&self, volume: f64) -> zbus::Result<()> {
        self.send_command(PlayerCommand::SetVolume(volume.clamp(0.0, 1.0) as f32))
            .await
            .map_err(Into::into)
    }

    /// Current position in microseconds.
    #[zbus(property)]
    fn position(&self) -> i64 {
        self.state.lock().position.as_micros() as i64
    }

    /// Minimum playback rate (no speed control).
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    /// Maximum playback rate (no speed control).
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    /// Playback rate.
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    /// Current track metadata as an `a{sv}` dictionary.
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        build_metadata(&self.state.lock().track)
    }
}

/// Builds the MPRIS metadata dictionary for `track` (empty when no track).
fn build_metadata(track: &Option<Track>) -> HashMap<String, OwnedValue> {
    let Some(track) = track else {
        return HashMap::new();
    };

    let mut metadata = HashMap::new();

    if let Ok(path) = ObjectPath::try_from(format!("/org/chromia/Track/{}", track.id)) {
        metadata.insert("mpris:trackid".to_owned(), OwnedValue::from(path));
    }
    metadata.insert(
        "mpris:length".to_owned(),
        OwnedValue::from(track.duration.as_micros() as u64),
    );

    if let Some(uri) = file_uri(&track.path) {
        metadata.insert(
            "mpris:artUrl".to_owned(),
            OwnedValue::from(Str::from(uri.clone())),
        );
        metadata.insert("xesam:url".to_owned(), OwnedValue::from(Str::from(uri)));
    }

    if !track.title.is_empty() {
        metadata.insert(
            "xesam:title".to_owned(),
            OwnedValue::from(Str::from(track.title.clone())),
        );
    }
    if !track.album.is_empty() {
        metadata.insert(
            "xesam:album".to_owned(),
            OwnedValue::from(Str::from(track.album.clone())),
        );
    }
    if !track.artist.is_empty() {
        if let Ok(artists) = OwnedValue::try_from(Value::new(vec![track.artist.clone()])) {
            metadata.insert("xesam:artist".to_owned(), artists);
        }
    }

    metadata
}

/// Builds a percent-encoded `file://` URI for `path`.
fn file_uri(path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        return None;
    }
    let mut uri = String::from("file://");
    if path.is_absolute() {
        uri.push('/');
    }
    let mut first = true;
    for component in path.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        if !first {
            uri.push('/');
        }
        first = false;
        uri.push_str(&urlencoding::encode(name.to_string_lossy().as_ref()));
    }
    Some(uri)
}

/// A property that changed in [`State`] and needs an MPRIS signal emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    PlaybackStatus,
    LoopStatus,
    Shuffle,
    Volume,
    Position,
    Metadata,
}

/// Applies an audio event to the shared state, returning the affected properties.
fn apply_event(state: &parking_lot::Mutex<State>, event: PlayerEvent) -> Vec<Change> {
    let mut state = state.lock();
    match event {
        PlayerEvent::TrackStarted(track) => {
            state.track = Some(track);
            state.status = PlaybackStatus::Playing;
            state.position = Duration::ZERO;
            vec![Change::PlaybackStatus, Change::Metadata, Change::Position]
        }
        PlayerEvent::PlaybackStateChanged(new_state) => {
            let status = PlaybackStatus::from(new_state);
            if state.status == status {
                return Vec::new();
            }
            state.status = status;
            vec![Change::PlaybackStatus]
        }
        PlayerEvent::PositionChanged(position) => {
            if state.position == position {
                return Vec::new();
            }
            state.position = position;
            vec![Change::Position]
        }
        PlayerEvent::DurationChanged(duration) => match &mut state.track {
            Some(track) => {
                if track.duration == duration {
                    return Vec::new();
                }
                track.duration = duration;
                vec![Change::Metadata]
            }
            None => Vec::new(),
        },
        PlayerEvent::VolumeChanged(volume) => {
            state.volume = f64::from(volume);
            vec![Change::Volume]
        }
        PlayerEvent::ShuffleChanged(shuffle) => {
            if state.shuffle == shuffle {
                return Vec::new();
            }
            state.shuffle = shuffle;
            vec![Change::Shuffle]
        }
        PlayerEvent::RepeatChanged(mode) => {
            let loop_status = LoopStatus::from(mode);
            if state.loop_status == loop_status {
                return Vec::new();
            }
            state.loop_status = loop_status;
            vec![Change::LoopStatus]
        }
        _ => Vec::new(),
    }
}

/// Emits the MPRIS `PropertiesChanged` signal for a single change.
async fn emit_change(
    player: &InterfaceRef<ChromiaMprisPlayer>,
    change: Change,
) -> zbus::Result<()> {
    match change {
        Change::PlaybackStatus => {
            player
                .get()
                .await
                .playback_status_changed(player.signal_emitter())
                .await
        }
        Change::LoopStatus => {
            let iface = player.get().await;
            iface.loop_status_changed(player.signal_emitter()).await?;
            iface.repeat_status_changed(player.signal_emitter()).await
        }
        Change::Shuffle => {
            player
                .get()
                .await
                .shuffle_changed(player.signal_emitter())
                .await
        }
        Change::Volume => {
            player
                .get()
                .await
                .volume_changed(player.signal_emitter())
                .await
        }
        Change::Position => {
            player
                .get()
                .await
                .position_changed(player.signal_emitter())
                .await
        }
        Change::Metadata => {
            player
                .get()
                .await
                .metadata_changed(player.signal_emitter())
                .await
        }
    }
}

/// Drives the shared state from the audio event stream and notifies clients.
async fn drive_state(
    state: Arc<parking_lot::Mutex<State>>,
    player: InterfaceRef<ChromiaMprisPlayer>,
    mut events: mpsc::Receiver<PlayerEvent>,
) {
    while let Some(event) = events.recv().await {
        for change in apply_event(&state, event) {
            if let Err(error) = emit_change(&player, change).await {
                tracing::warn!("mpris: failed to emit {change:?}: {error}");
            }
        }
    }
}

/// Serves MPRIS2 on the session bus until the connection is closed.
///
/// # Errors
///
/// Fails if the session bus cannot be reached or the well-known name is taken.
pub async fn run(
    command_tx: mpsc::Sender<PlayerCommand>,
    events: mpsc::Receiver<PlayerEvent>,
) -> anyhow::Result<()> {
    let connection = Connection::session().await?;
    connection.request_name(MPRIS_NAME).await?;

    let state = Arc::new(parking_lot::Mutex::new(State::default()));

    connection
        .object_server()
        .at(MPRIS_PATH, ChromiaMprisRoot::new(command_tx.clone()))
        .await?;
    connection
        .object_server()
        .at(
            MPRIS_PATH,
            ChromiaMprisPlayer::new(state.clone(), command_tx),
        )
        .await?;

    let player: InterfaceRef<ChromiaMprisPlayer> =
        connection.object_server().interface(MPRIS_PATH).await?;

    tokio::spawn(drive_state(state, player, events));

    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}
