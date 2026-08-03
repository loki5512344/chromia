//! GTK user interface: main window, onboarding, columns and widgets.

pub mod bottom_player;
pub mod center;
pub mod layout;
pub mod onboarding;
pub mod right_panel;
pub mod sidebar;
pub mod widgets;
pub mod window;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::audio::dsp::Spectrum;
use crate::audio::{PlayerCommand, PlayerEvent};
use crate::config::schema::Config;
use crate::library::database::Database;

/// A hook the main window populates once it exists, so the Settings page (and
/// onboarding) can push a theme re-apply onto the live interface without
/// holding a reference to the window.
pub type ThemeApplier = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// Shared context handed to every widget at construction time.
///
/// Widgets read config / database through this context and control playback
/// by sending [`PlayerCommand`]s; they never await on the audio task.
#[derive(Clone)]
pub struct UiContext {
    /// Mutable application config (edited by onboarding and settings).
    pub config: Rc<RefCell<Config>>,
    /// Channel for sending playback commands to the audio task.
    pub command_tx: mpsc::Sender<PlayerCommand>,
    /// Receiver = playback events separate, drained on the GTK main loop.
    pub event_rx: Rc<RefCell<mpsc::Receiver<PlayerEvent>>>,
    /// Shared library database.
    pub database: Arc<Database>,
    /// Tokio runtime handle for spawning short-lived background tasks
    /// (cover extraction, lyrics fetching) from widgets.
    pub rt: tokio::runtime::Handle,
    /// Optional hook the window registers to re-apply the theme live after a
    /// config-driven change (theme mode, flavor, accent, …).
    pub theme_applier: ThemeApplier,
    /// Optional hook the window registers to re-apply appearance knobs that
    /// are not palette-driven (Glass UI, animations, blur, border radius).
    pub appearance_applier: ThemeApplier,
    /// Optional hook that toggles the MPRIS2 / Discord integrations live from
    /// the Settings page. The closure receives the requested `(mpris_enabled,
    /// discord_enabled)` state and stops/starts the services accordingly.
    pub integration_control: IntegrationControl,
    /// Shared live spectrum for the visualizer, fed by the audio engine's
    /// DSP source on the playback thread and read by the UI on a timer.
    pub spectrum: Arc<::parking_lot::Mutex<Spectrum>>,
}

/// Runtime control for the optional integrations (MPRIS2, Discord RPC).
///
/// Populated by the application entry point (see `app.rs`); the Settings page
/// calls it to stop / start a service without restarting the whole player.
/// The closure argument is `(mpris_enabled, discord_enabled)`.
pub type IntegrationControl = Rc<RefCell<Option<Box<dyn Fn(bool, bool)>>>>;

impl UiContext {
    /// Builds a new context.
    pub fn new(
        config: Rc<RefCell<Config>>,
        command_tx: mpsc::Sender<PlayerCommand>,
        event_rx: mpsc::Receiver<PlayerEvent>,
        database: Arc<Database>,
        rt: tokio::runtime::Handle,
        spectrum: Arc<::parking_lot::Mutex<Spectrum>>,
    ) -> Self {
        Self {
            config,
            command_tx,
            event_rx: Rc::new(RefCell::new(event_rx)),
            database,
            rt,
            theme_applier: Rc::new(RefCell::new(None)),
            appearance_applier: Rc::new(RefCell::new(None)),
            integration_control: Rc::new(RefCell::new(None)),
            spectrum,
        }
    }
}
