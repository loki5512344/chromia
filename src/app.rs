//! Application entry point: configuration, runtime, audio engine, integrations
//! and the GTK main loop.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gio::prelude::*;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use crate::audio::{Player, PlayerEvent, player::PlayerSettings};
use crate::config::{self, schema::Config};
use crate::library::Track;
use crate::library::database::Database;
use crate::sources::local::LocalSource;
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
    let settings = PlayerSettings {
        cache_dir: config::expand_path(&youtube_config.cache_dir),
        quality: youtube_config.quality.clone(),
        volume: audio_config.volume.clamp(0.0, 1.0),
        crossfade_ms: audio_config.crossfade_ms,
        replaygain: audio_config.replaygain,
    };
    let command_tx =
        Player::spawn(settings, event_senders).expect("failed to start the audio engine");

    #[cfg(feature = "mpris")]
    if config.borrow().integrations.mpris {
        let command_tx = command_tx.clone();
        rt_handle.spawn(async move {
            if let Err(err) = crate::integrations::run_mpris(command_tx, mpris_rx).await {
                tracing::warn!(error = %err, "mpris integration stopped");
            }
        });
    }

    #[cfg(feature = "discord")]
    if config.borrow().integrations.discord {
        if let Err(err) = crate::integrations::Discord::spawn(discord_rx) {
            tracing::warn!(error = %err, "discord integration unavailable");
        }
    }

    let (scan_tx, scan_rx) = mpsc::channel::<Vec<Track>>(16);
    if !config.borrow().first_run {
        spawn_library_scan(&config, &database, scan_tx.clone(), &rt_handle);
    }

    let app = adw::Application::builder()
        .application_id("dev.chromia.player")
        .build();

    let context = UiContext::new(config.clone(), command_tx, event_rx, database, rt_handle);
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
