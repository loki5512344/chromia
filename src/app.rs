//! Application entry point: configuration, runtime, audio engine, integrations
//! and the GTK main loop.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gio::prelude::*;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use crate::audio::{Player, PlayerCommand, PlayerEvent, dsp::Spectrum, player::PlayerSettings};
use crate::config::{self, schema::Config};
use crate::library::Track;
use crate::library::database::Database;
use crate::sources::local::LocalSource;
use crate::sources::watch::spawn_library_watcher;
use crate::ui::UiContext;
use crate::ui::window::ChromiaWindow;

/// Initialises the tracing subscriber from `RUST_LOG`, defaulting to `info`.
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("chromia=info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

/// Starts the whole application and blocks in the GTK main loop.
pub fn run() {
    init_tracing();
    tracing::info!("chromia starting");

    let config = Rc::new(RefCell::new(Config::load().unwrap_or_default()));

    let runtime = Runtime::new().expect("failed to start the tokio runtime");
    let rt_handle = runtime.handle().clone();
    let _guard = runtime.enter();
    // The runtime drives the audio engine and background tasks for the whole
    // lifetime of the app; forgetting it keeps its worker threads alive.
    std::mem::forget(runtime);

    let cache_dir = config::cache_dir().expect("XDG cache directory unavailable");
    std::fs::create_dir_all(&cache_dir).expect("failed to create the cache directory");
    let database = Arc::new(
        Database::open(&cache_dir.join("library.db")).expect("failed to open the library database"),
    );

    let (event_tx, event_rx) = mpsc::channel::<PlayerEvent>(256);
    #[cfg(feature = "mpris")]
    let (mpris_tx, mpris_rx) = mpsc::channel::<PlayerEvent>(256);
    #[cfg(feature = "discord")]
    let (discord_tx, discord_rx) = mpsc::channel::<PlayerEvent>(256);

    #[cfg(any(feature = "mpris", feature = "discord"))]
    let mut event_senders: Vec<mpsc::Sender<PlayerEvent>> = vec![event_tx.clone()];
    #[cfg(not(any(feature = "mpris", feature = "discord")))]
    let event_senders: Vec<mpsc::Sender<PlayerEvent>> = vec![event_tx.clone()];
    #[cfg(feature = "mpris")]
    event_senders.push(mpris_tx.clone());
    #[cfg(feature = "discord")]
    event_senders.push(discord_tx.clone());

    let (audio_config, youtube_config) = {
        let config = config.borrow();
        (config.audio.clone(), config.sources.youtube.clone())
    };
    let spectrum = Arc::new(parking_lot::Mutex::new(Spectrum::new()));
    let settings = PlayerSettings {
        cache_dir: config::expand_path(&youtube_config.cache_dir),
        quality: youtube_config.quality.clone(),
        volume: audio_config.volume.clamp(0.0, 1.0),
        crossfade_ms: audio_config.crossfade_ms,
        replaygain: audio_config.replaygain,
        spectrum: Some(spectrum.clone()),
    };
    let command_tx =
        Player::spawn(settings, event_senders).expect("failed to start the audio engine");

    // The integrations hold their audio-event receivers here so they can be
    // started now (if enabled in config) and later toggled at runtime from the
    // Settings page. Because the event senders are fixed at startup, each
    // receiver may be consumed at most once; restarting a service after its
    // receiver was already consumed is not possible (see `set_mpris`).
    let control = Rc::new(IntegrationController::new(
        rt_handle.clone(),
        command_tx.clone(),
        #[cfg(feature = "mpris")]
        mpris_rx,
        #[cfg(feature = "discord")]
        discord_rx,
    ));

    #[cfg(feature = "mpris")]
    control.set_mpris(config.borrow().integrations.mpris);
    #[cfg(feature = "discord")]
    control.set_discord(config.borrow().integrations.discord);

    let (scan_tx, scan_rx) = mpsc::channel::<Vec<Track>>(16);
    if !config.borrow().first_run {
        spawn_library_scan(&config, &database, scan_tx.clone(), &rt_handle);
        spawn_library_watcher_from_config(&config, &database, scan_tx.clone());
    }

    let app = adw::Application::builder()
        .application_id("dev.chromia.player")
        .build();

    let context = UiContext::new(
        config.clone(),
        command_tx,
        event_rx,
        database,
        rt_handle,
        spectrum,
    );

    {
        let mut integration_control = context.integration_control.borrow_mut();
        *integration_control = Some(Box::new(move |mpris_enabled, discord_enabled| {
            #[cfg(feature = "mpris")]
            control.set_mpris(mpris_enabled);
            #[cfg(feature = "discord")]
            control.set_discord(discord_enabled);
        }));
    }
    let scan_rx = Rc::new(RefCell::new(Some(scan_rx)));
    app.connect_activate(move |app| {
        let first_run = context.config.borrow().first_run;
        if first_run {
            let on_done = {
                let context = context.clone();
                let app = app.clone();
                let scan_rx = scan_rx.clone();
                let scan_tx = scan_tx.clone();
                move || {
                    spawn_library_scan(&context.config, &context.database, scan_tx, &context.rt);
                    let receiver = scan_rx.borrow_mut().take().expect("window already built");
                    let window = ChromiaWindow::new(&app, &context, receiver);
                    window.present();
                }
            };
            let onboarding =
                crate::ui::onboarding::Onboarding::new(app, &context, Box::new(on_done));
            onboarding.present();
        } else {
            let receiver = scan_rx.borrow_mut().take().expect("window already built");
            let window = ChromiaWindow::new(app, &context, receiver);
            window.present();
        }
    });

    app.run();
}

/// Scans the configured local folders in the background and stores the results.
fn spawn_library_scan(
    config: &Rc<RefCell<Config>>,
    database: &Arc<Database>,
    scan_tx: mpsc::Sender<Vec<Track>>,
    rt: &tokio::runtime::Handle,
) {
    let local_paths: Vec<PathBuf> = config
        .borrow()
        .sources
        .local
        .paths
        .iter()
        .map(|path| config::expand_path(path))
        .collect();
    if local_paths.is_empty() {
        return;
    }
    let database = database.clone();
    rt.spawn_blocking(move || {
        tracing::info!("scanning local library…");
        let tracks = LocalSource::new(local_paths).scan();
        tracing::info!(count = tracks.len(), "library scan finished");
        if let Err(err) = database.upsert_tracks(&tracks) {
            tracing::warn!(error = %err, "failed to store the library");
        }
        let _ = scan_tx.blocking_send(tracks);
    });
}

/// Starts an inotify watcher over the configured local folders so the library
/// re-scans (and prunes removed files) when tracks change on disk.
fn spawn_library_watcher_from_config(
    config: &Rc<RefCell<Config>>,
    database: &Arc<Database>,
    scan_tx: mpsc::Sender<Vec<Track>>,
) {
    if !config.borrow().sources.local.watch {
        return;
    }
    let local_paths: Vec<PathBuf> = config
        .borrow()
        .sources
        .local
        .paths
        .iter()
        .map(|path| config::expand_path(path))
        .collect();
    if local_paths.is_empty() {
        return;
    }
    spawn_library_watcher(local_paths, database.clone(), scan_tx);
}

/// Runtime controller for the optional MPRIS2 and Discord integrations.
///
/// Owns each service's audio-event receiver so it can be started on demand,
/// keeps the running service's handle for a clean stop, and records whether it
/// is currently enabled. The audio event channels are fixed at startup, so a
/// receiver is consumed at most once: a service that was running and then
/// stopped cannot be restarted in the same session (the integration simply
/// stays off until the next launch).
#[allow(dead_code)] // fields only exist under their respective feature gates
struct IntegrationController {
    rt: tokio::runtime::Handle,
    command_tx: mpsc::Sender<PlayerCommand>,
    #[cfg(feature = "mpris")]
    mpris_ev: Rc<RefCell<Option<mpsc::Receiver<PlayerEvent>>>>,
    #[cfg(feature = "mpris")]
    mpris_join: Rc<RefCell<Option<tokio::task::JoinHandle<()>>>>,
    #[cfg(feature = "mpris")]
    mpris_on: Rc<Cell<bool>>,
    #[cfg(feature = "discord")]
    discord_ev: Rc<RefCell<Option<mpsc::Receiver<PlayerEvent>>>>,
    #[cfg(feature = "discord")]
    discord_stop: Rc<RefCell<Option<std::sync::mpsc::Sender<()>>>>,
    #[cfg(feature = "discord")]
    discord_join: Rc<RefCell<Option<std::thread::JoinHandle<()>>>>,
    #[cfg(feature = "discord")]
    discord_on: Rc<Cell<bool>>,
}

impl IntegrationController {
    fn new(
        rt: tokio::runtime::Handle,
        command_tx: mpsc::Sender<PlayerCommand>,
        #[cfg(feature = "mpris")] mpris_ev: mpsc::Receiver<PlayerEvent>,
        #[cfg(feature = "discord")] discord_ev: mpsc::Receiver<PlayerEvent>,
    ) -> Self {
        Self {
            rt,
            command_tx,
            #[cfg(feature = "mpris")]
            mpris_ev: Rc::new(RefCell::new(Some(mpris_ev))),
            #[cfg(feature = "mpris")]
            mpris_join: Rc::new(RefCell::new(None)),
            #[cfg(feature = "mpris")]
            mpris_on: Rc::new(Cell::new(false)),
            #[cfg(feature = "discord")]
            discord_ev: Rc::new(RefCell::new(Some(discord_ev))),
            #[cfg(feature = "discord")]
            discord_stop: Rc::new(RefCell::new(None)),
            #[cfg(feature = "discord")]
            discord_join: Rc::new(RefCell::new(None)),
            #[cfg(feature = "discord")]
            discord_on: Rc::new(Cell::new(false)),
        }
    }

    /// Starts or stops the MPRIS2 D-Bus service to match `enabled`.
    #[cfg(feature = "mpris")]
    fn set_mpris(&self, enabled: bool) {
        if self.mpris_on.get() == enabled {
            return;
        }
        if enabled {
            let Some(events) = self.mpris_ev.borrow_mut().take() else {
                tracing::debug!(
                    "mpris: audio event channel already consumed; \
                     cannot restart this session, will start on next launch"
                );
                return;
            };
            let command_tx = self.command_tx.clone();
            let join = self.rt.spawn(async move {
                if let Err(err) = crate::integrations::run_mpris(command_tx, events).await {
                    tracing::warn!(error = %err, "mpris integration stopped");
                }
            });
            *self.mpris_join.borrow_mut() = Some(join);
            self.mpris_on.set(true);
            tracing::info!("mpris2 integration enabled");
        } else {
            if let Some(join) = self.mpris_join.borrow_mut().take() {
                join.abort();
            }
            self.mpris_on.set(false);
            tracing::info!("mpris2 integration disabled");
        }
    }

    /// Starts or stops the Discord Rich Presence worker to match `enabled`.
    #[cfg(feature = "discord")]
    fn set_discord(&self, enabled: bool) {
        if self.discord_on.get() == enabled {
            return;
        }
        if enabled {
            let Some(events) = self.discord_ev.borrow_mut().take() else {
                tracing::debug!(
                    "discord: audio event channel already consumed; \
                     cannot restart on this session, integration starts next launch"
                );
                return;
            };
            let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
            match crate::integrations::Discord::spawn(events, stop_rx) {
                Ok(join) => {
                    *self.discord_join.borrow_mut() = Some(join);
                    *self.discord_stop.borrow_mut() = Some(stop_tx);
                    self.discord_on.set(true);
                    tracing::info!("discord integration enabled");
                }
                Err(err) => {
                    tracing::warn!(error = %err, "discord integration unavailable");
                }
            }
        } else {
            // Dropping the stop sender disconnects the worker's receiver so the
            // loop exits; join the thread to let it finish cleanly.
            if let Some(stop) = self.discord_stop.borrow_mut().take() {
                drop(stop);
            }
            if let Some(join) = self.discord_join.borrow_mut().take() {
                let _ = join.join();
            }
            self.discord_on.set(false);
            tracing::info!("discord integration disabled");
        }
    }
}
