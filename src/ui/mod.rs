//! GTK user interface: main window, onboarding, columns and widgets.

pub mod onboarding;
pub mod widgets;
pub mod window;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::audio::{PlayerCommand, PlayerEvent};
use crate::config::schema::Config;
use crate::library::database::Database;

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
    /// Receiver for playback events, drained on the GTK main loop.
    pub event_rx: Rc<RefCell<mpsc::Receiver<PlayerEvent>>>,
    /// Shared library database.
    pub database: Arc<Database>,
    /// Tokio runtime handle for spawning short-lived background tasks
    /// (cover extraction, lyrics fetching) from widgets.
    pub rt: tokio::runtime::Handle,
}

impl UiContext {
    /// Builds a new context.
    pub fn new(
        config: Rc<RefCell<Config>>,
        command_tx: mpsc::Sender<PlayerCommand>,
        event_rx: mpsc::Receiver<PlayerEvent>,
        database: Arc<Database>,
        rt: tokio::runtime::Handle,
    ) -> Self {
        Self {
            config,
            command_tx,
            event_rx: Rc::new(RefCell::new(event_rx)),
            database,
            rt,
        }
    }
}
