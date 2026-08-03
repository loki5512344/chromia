//! rodio audio engine running on a dedicated blocking thread.
//!
//! The GTK thread never touches the audio stack directly; it sends
//! [`PlayerCommand`]s over a channel and receives [`PlayerEvent`]s.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rodio::{Decoder, OutputStream, Sink, Source};
use tokio::sync::mpsc;

use crate::audio::dsp::{EqualizerSource, Spectrum};
use crate::audio::equalizer::Equalizer;
use crate::audio::queue::Queue;
use crate::audio::{PlaybackState, PlayerCommand, PlayerEvent, RepeatMode};
use crate::library::Track;

/// Engine loop cadence.
const TICK: Duration = Duration::from_millis(25);
/// Interval between position reports.
const POSITION_TICK: Duration = Duration::from_millis(250);

/// Configuration for the audio engine.
pub struct PlayerSettings {
    /// Directory where streamed tracks are downloaded.
    pub cache_dir: PathBuf,
    /// Preferred stream quality (`best`, `320k`, `256k`, `128k`).
    pub quality: String,
    /// Initial playback volume, 0.0-1.0.
    pub volume: f32,
    /// Crossfade length between tracks, in ms (0 disables).
    pub crossfade_ms: u32,
    /// Whether ReplayGain normalisation is applied.
    pub replaygain: bool,
    /// Optional shared spectrum accumulator fed by the playback source; the UI
    /// visualizer reads it. `None` disables the live analyser.
    pub spectrum: Option<Arc<::parking_lot::Mutex<Spectrum>>>,
}

/// Opaque handle to the audio engine.
pub struct Player;

impl Player {
    /// Spawns the engine on a blocking thread; every event is broadcast to all
    /// senders in `event_tx`. Returns the command sender for the engine.
    ///
    /// Must be called from within a running Tokio runtime.
    pub fn spawn(
        settings: PlayerSettings,
        event_tx: Vec<mpsc::Sender<PlayerEvent>>,
    ) -> anyhow::Result<mpsc::Sender<PlayerCommand>> {
        let (tx, rx) = mpsc::channel::<PlayerCommand>(64);
        let engine = tokio::task::spawn_blocking(move || run_engine(settings, event_tx, rx));
        tokio::task::spawn(async move {
            match engine.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::error!("audio engine stopped with error: {e:#}"),
                Err(e) => tracing::error!("audio engine panicked: {e}"),
            }
        });
        Ok(tx)
    }
}

/// Live playback state mutated only by the engine thread.
struct Engine {
    sink: Sink,
    queue: Queue,
    equalizer: Equalizer,
    state: PlaybackState,
    current_track: Option<Track>,
    appended_duration: Duration,
    has_sound: bool,
    failed_tracks: usize,
    senders: Vec<mpsc::Sender<PlayerEvent>>,
    settings: PlayerSettings,
}

impl Engine {
    fn new(sink: Sink, senders: Vec<mpsc::Sender<PlayerEvent>>, settings: PlayerSettings) -> Self {
        Self {
            sink,
            queue: Queue::new(),
            equalizer: Equalizer::new(),
            state: PlaybackState::Stopped,
            current_track: None,
            appended_duration: Duration::ZERO,
            has_sound: false,
            failed_tracks: 0,
            senders,
            settings,
        }
    }

    fn emit(&self, event: PlayerEvent) {
        for sender in &self.senders {
            let _ = sender.try_send(event.clone());
        }
    }

    fn handle_command(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::LoadQueue(tracks) => {
                self.queue.load(tracks);
                self.notify_queue();
            }
            PlayerCommand::PlayAt(index) => self.play_index(index),
            PlayerCommand::PlayPause => self.toggle_play_pause(),
            PlayerCommand::Pause => {
                self.sink.pause();
                self.state = PlaybackState::Paused;
                self.emit(PlayerEvent::PlaybackStateChanged(PlaybackState::Paused));
            }
            PlayerCommand::Resume => self.resume(),
            PlayerCommand::Stop => self.stop(),
            PlayerCommand::Next => match self.queue.next() {
                Some(index) => self.play_index(index),
                None => self.stop(),
            },
            PlayerCommand::Previous => {
                if let Some(index) = self.queue.previous() {
                    self.play_index(index);
                }
            }
            PlayerCommand::Seek(position) => {
                match self.sink.try_seek(position) {
                    Ok(()) => {}
                    Err(e) => tracing::warn!("seek failed: {e}"),
                }
                self.emit(PlayerEvent::PositionChanged(position));
            }
            PlayerCommand::SetVolume(volume) => {
                let volume = volume.clamp(0.0, 1.0);
                self.sink.set_volume(volume);
                self.emit(PlayerEvent::VolumeChanged(volume));
            }
            PlayerCommand::SetShuffle(shuffle) => {
                self.queue.set_shuffle(shuffle);
                self.emit(PlayerEvent::ShuffleChanged(self.queue.shuffle()));
            }
            PlayerCommand::SetRepeat(repeat) => {
                self.queue.set_repeat(repeat);
                self.emit(PlayerEvent::RepeatChanged(self.queue.repeat()));
            }
            PlayerCommand::SetBand { index, gain_db } => self.equalizer.set_band(index, gain_db),
            PlayerCommand::SetEqualizerPreset(name) => {
                if !self.equalizer.set_preset(&name) {
                    tracing::warn!("unknown equalizer preset: {name}");
                }
            }
            PlayerCommand::SetEqualizerEnabled(enabled) => self.equalizer.set_enabled(enabled),
        }
    }

    fn toggle_play_pause(&mut self) {
        match self.state {
            PlaybackState::Playing => {
                self.sink.pause();
                self.state = PlaybackState::Paused;
                self.emit(PlayerEvent::PlaybackStateChanged(PlaybackState::Paused));
            }
            PlaybackState::Paused => {
                self.sink.play();
                self.state = PlaybackState::Playing;
                self.emit(PlayerEvent::PlaybackStateChanged(PlaybackState::Playing));
            }
            PlaybackState::Stopped => {
                if let Some(index) = self.queue.current_index() {
                    self.play_index(index);
                }
            }
        }
    }

    fn resume(&mut self) {
        if self.state == PlaybackState::Stopped {
            if let Some(index) = self.queue.current_index() {
                self.play_index(index);
            }
            return;
        }
        if self.has_sound && self.state == PlaybackState::Paused {
            self.sink.play();
            self.state = PlaybackState::Playing;
            self.emit(PlayerEvent::PlaybackStateChanged(PlaybackState::Playing));
        }
    }

    fn stop(&mut self) {
        self.sink.clear();
        self.sink.stop();
        self.has_sound = false;
        self.failed_tracks = 0;
        self.state = PlaybackState::Stopped;
        self.emit(PlayerEvent::PlaybackStateChanged(PlaybackState::Stopped));
    }

    fn notify_queue(&self) {
        self.emit(PlayerEvent::QueueChanged(self.queue.tracks()));
        self.emit(PlayerEvent::CurrentIndexChanged(self.queue.current_index()));
    }

    fn play_index(&mut self, index: usize) {
        let Some(mut track) = self.queue.play_at(index) else {
            return;
        };
        self.emit(PlayerEvent::CurrentIndexChanged(Some(index)));
        if !track.is_playable() {
            self.handle_play_failure(format!("{} has no source", track.label()));
            return;
        }
        if track.path.as_os_str().is_empty() {
            let resolved = match track.source {
                crate::library::SourceKind::Youtube => crate::sources::youtube::YoutubeSource::new(
                    &self.settings.quality,
                    self.settings.cache_dir.clone(),
                )
                .resolve_stream(&track),
                crate::library::SourceKind::Soundcloud => {
                    crate::sources::soundcloud::SoundcloudSource::new(
                        &self.settings.quality,
                        self.settings.cache_dir.clone(),
                    )
                    .resolve_stream(&track)
                }
                crate::library::SourceKind::Local => {
                    self.handle_play_failure(format!("{} has no local file", track.label()));
                    return;
                }
            };
            match resolved {
                Ok(path) => {
                    track.path = path.clone();
                    self.queue.set_track_path(index, path);
                }
                Err(e) => {
                    self.handle_play_failure(format!("download failed: {e}"));
                    return;
                }
            }
        }
        let path = track.path.clone();
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(e) => {
                self.handle_play_failure(format!("cannot open {}: {e}", path.display()));
                return;
            }
        };
        let decoder = match Decoder::new(BufReader::new(file)) {
            Ok(decoder) => decoder,
            Err(e) => {
                self.handle_play_failure(format!("decode error: {e}"));
                return;
            }
        };
        let duration = decoder.total_duration();
        self.appended_duration = duration.unwrap_or(Duration::ZERO);
        self.failed_tracks = 0;

        // Build the DSP-wrapped playback source: ReplayGain pre-gain, the live
        // equalizer chain, an optional crossfade head ramp, and the spectrum
        // feed all live inside a single rodio `Source`.
        let replaygain_linear = if self.settings.replaygain {
            crate::library::metadata::replaygain_gain_db(&path)
                .ok()
                .flatten()
                .map(|db| 10f32.powf(db / 20.0))
                .unwrap_or(1.0)
        } else {
            1.0
        };
        self.equalizer.set_pre_gain(replaygain_linear);

        let spectrum = self.settings.spectrum.clone();
        if let Some(spec) = &spectrum {
            spec.lock().reset();
        }
        let mut source =
            EqualizerSource::new(decoder.convert_samples(), self.equalizer.handle(), spectrum);
        source.crossfade_secs(self.settings.crossfade_ms as f32 / 1000.0);

        self.sink.clear();
        self.sink.append(source);
        self.sink.play();
        self.has_sound = true;
        self.state = PlaybackState::Playing;
        self.current_track = Some(track.clone());
        self.emit(PlayerEvent::TrackStarted(track));
        if let Some(duration) = duration {
            self.emit(PlayerEvent::DurationChanged(duration));
        }
        self.emit(PlayerEvent::PlaybackStateChanged(PlaybackState::Playing));
    }

    fn handle_play_failure(&mut self, message: String) {
        self.has_sound = false;
        self.emit(PlayerEvent::Error(message));
        self.failed_tracks += 1;
        if self.failed_tracks >= self.queue.len().max(1) {
            self.stop();
        } else if let Some(index) = self.queue.next() {
            self.play_index(index);
        } else {
            self.stop();
        }
    }

    fn handle_track_end(&mut self) {
        self.has_sound = false;
        match self.queue.repeat() {
            RepeatMode::One => {
                if let Some(track) = self.current_track.clone() {
                    self.restart_track(track);
                } else {
                    self.stop();
                }
            }
            _ => {
                tracing::debug!(
                    "track ended after {:?}, advancing queue",
                    self.appended_duration
                );
                match self.queue.next() {
                    Some(index) => self.play_index(index),
                    None => {
                        self.emit(PlayerEvent::TrackEnded);
                        self.stop();
                    }
                }
            }
        }
    }

    fn restart_track(&mut self, track: Track) {
        let path = track.path.clone();
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(e) => {
                self.handle_play_failure(format!("cannot open {}: {e}", path.display()));
                return;
            }
        };
        match Decoder::new(BufReader::new(file)) {
            Ok(decoder) => {
                let spectrum = self.settings.spectrum.clone();
                let source = EqualizerSource::new(
                    decoder.convert_samples(),
                    self.equalizer.handle(),
                    spectrum,
                );
                self.sink.clear();
                self.sink.append(source);
                self.sink.play();
                self.has_sound = true;
                self.state = PlaybackState::Playing;
            }
            Err(e) => self.handle_play_failure(format!("decode error: {e}")),
        }
    }
}

/// Runs the engine loop forever, driving playback on this blocking thread.
fn run_engine(
    settings: PlayerSettings,
    event_tx: Vec<mpsc::Sender<PlayerEvent>>,
    mut rx: mpsc::Receiver<PlayerCommand>,
) -> anyhow::Result<()> {
    let (_stream, handle) = match OutputStream::try_default() {
        Ok(stream) => stream,
        Err(e) => {
            let message = format!("audio output: {e}");
            tracing::error!("{message}");
            emit_raw(&event_tx, PlayerEvent::Error(message.clone()));
            return Err(anyhow::anyhow!("{message}"));
        }
    };
    tracing::debug!(
        cache_dir = %settings.cache_dir.display(),
        quality = %settings.quality,
        volume = settings.volume,
        crossfade_ms = settings.crossfade_ms,
        replaygain = settings.replaygain,
        "audio engine started"
    );
    let sink = match Sink::try_new(&handle) {
        Ok(sink) => sink,
        Err(e) => {
            let message = format!("audio sink: {e}");
            tracing::error!("{message}");
            emit_raw(&event_tx, PlayerEvent::Error(message.clone()));
            return Err(anyhow::anyhow!("{message}"));
        }
    };
    sink.set_volume(settings.volume);

    let mut engine = Engine::new(sink, event_tx, settings);
    let mut last_position_tick = Instant::now();
    loop {
        while let Ok(command) = rx.try_recv() {
            engine.handle_command(command);
        }
        if engine.state == PlaybackState::Playing && engine.has_sound && engine.sink.len() == 0 {
            engine.handle_track_end();
        }
        if engine.state == PlaybackState::Playing && last_position_tick.elapsed() >= POSITION_TICK {
            last_position_tick = Instant::now();
            engine.emit(PlayerEvent::PositionChanged(engine.sink.get_pos()));
        }
        std::thread::sleep(TICK);
    }
}

fn emit_raw(senders: &[mpsc::Sender<PlayerEvent>], event: PlayerEvent) {
    for sender in senders {
        let _ = sender.try_send(event.clone());
    }
}
