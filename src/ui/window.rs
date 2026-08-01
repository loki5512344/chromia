//! Main application window: a single-screen player layout with a player bar,
//! resizable columns (library / lyrics / queue) and an optional equalizer.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gtk::prelude::*;
use tokio::sync::mpsc;

use crate::audio::PlayerEvent;
use crate::config::{
    self,
    schema::{ThemeConfig, ThemeMode},
};
use crate::library::Track;
use crate::library::database::Database;
use crate::theme::css;
use crate::theme::{Palette, palette_for, palette_from_image};
use crate::ui::UiContext;
use crate::ui::widgets::equalizer::EqualizerWidget;
use crate::ui::widgets::library::Library;
use crate::ui::widgets::lyrics::Lyrics;
use crate::ui::widgets::player::PlayerCore;
use crate::ui::widgets::queue::Queue as QueueWidget;

/// The main window tying together the player bar, columns and event loop.
pub struct ChromiaWindow {
    app_window: adw::ApplicationWindow,
    player: PlayerCore,
    library: Library,
    lyrics: Lyrics,
    queue: QueueWidget,
    equalizer: EqualizerWidget,
    event_rx: Rc<RefCell<mpsc::Receiver<PlayerEvent>>>,
    scan_rx: Rc<RefCell<mpsc::Receiver<Vec<Track>>>>,
    palette: Rc<RefCell<Palette>>,
    config: Rc<RefCell<crate::config::schema::Config>>,
    database: Arc<Database>,
    rt: tokio::runtime::Handle,
}

impl ChromiaWindow {
    /// Builds the window, its columns, the initial theme and the event pump.
    pub fn new(
        app: &adw::Application,
        ctx: &UiContext,
        scan_rx: mpsc::Receiver<Vec<Track>>,
    ) -> Rc<Self> {
        let player = PlayerCore::new(ctx);
        let library = Library::new(ctx);
        let lyrics = Lyrics::new(ctx);
        let queue = QueueWidget::new(ctx);
        let equalizer = EqualizerWidget::new(ctx);

        let middle = build_columns(&library, &lyrics, &queue);

        let show_equalizer = ctx
            .config
            .borrow()
            .layout
            .widgets
            .iter()
            .any(|w| w.id == "Equalizer" && w.visible);

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&player.root());
        root.append(&middle);
        if show_equalizer {
            root.append(&equalizer.root());
        }

        let palette = resolve_initial_palette(&ctx.config.borrow().theme);
        let palette = Rc::new(RefCell::new(palette));

        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::PreferDark);
        // The legacy GtkSettings dark flag triggers a libadwaita warning; the
        // color scheme above is the supported way to request a dark theme.
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_application_prefer_dark_theme(false);
        }

        let app_window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Chromia")
            .default_width(1200)
            .default_height(800)
            .content(&root)
            .build();

        let window = Rc::new(Self {
            app_window,
            player,
            library,
            lyrics,
            queue,
            equalizer,
            event_rx: ctx.event_rx.clone(),
            scan_rx: Rc::new(RefCell::new(scan_rx)),
            palette,
            config: ctx.config.clone(),
            database: ctx.database.clone(),
            rt: ctx.rt.clone(),
        });

        if let Err(err) = css::apply_css(&css::full_css(&window.palette.borrow())) {
            tracing::warn!(error = %err, "could not apply theme css");
        }

        window.start_event_pump();
        window
    }

    /// Shows the window.
    pub fn present(&self) {
        self.app_window.present();
    }

    /// Drains playback and library-scan events off their shared receivers on the
    /// GTK main loop.
    fn start_event_pump(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(50), move || {
            let Some(window) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            window.pump_events();
            window.pump_scans();
            glib::ControlFlow::Continue
        });
    }

    /// Consumes every queued playback event and dispatches it to the widgets.
    fn pump_events(&self) {
        let mut receiver = self.event_rx.borrow_mut();
        while let Ok(event) = receiver.try_recv() {
            self.dispatch(event);
        }
    }

    /// Consumes completed library scans and refreshes the library widget.
    fn pump_scans(&self) {
        let mut receiver = self.scan_rx.borrow_mut();
        while let Ok(tracks) = receiver.try_recv() {
            self.library.load_tracks(tracks);
        }
    }

    /// Forwards an event to every widget and refreshes the dynamic theme.
    fn dispatch(&self, event: PlayerEvent) {
        if let PlayerEvent::Error(message) = &event {
            tracing::warn!(error = %message, "playback error");
        }
        self.player.update(&event);
        self.library.update(&event);
        self.lyrics.update(&event);
        self.queue.update(&event);
        self.equalizer.update(&event);
        if let PlayerEvent::TrackStarted(track) = &event {
            self.refresh_dynamic_theme(track);
            if track.id != 0 {
                if let Err(err) = self.database.increment_play_count(track.id) {
                    tracing::debug!(error = %err, "could not increment play count");
                }
                if let Err(err) = self.database.record_play(track.id) {
                    tracing::debug!(error = %err, "could not record play history");
                }
            }
        }
    }

    /// Re-extracts the palette from the current track's cover when the theme is
    /// in `dynamic` mode. Local tracks use embedded art; remote tracks download
    /// and cache their cover via `sources::download_thumbnail`.
    fn refresh_dynamic_theme(&self, track: &Track) {
        if !matches!(self.config.borrow().theme.mode, ThemeMode::Dynamic) {
            return;
        }
        let local_path = track.path.clone();
        let thumbnail = track.thumbnail.clone();
        let cache_dir = config::expand_path(&self.config.borrow().paths.cache_dir);
        let palette = self.palette.clone();
        let rt = self.rt.clone();
        glib::MainContext::default().spawn_local(async move {
            let bytes = if !local_path.as_os_str().is_empty() {
                let handle = rt.spawn_blocking(move || {
                    crate::library::metadata::extract_cover(&local_path)
                        .ok()
                        .flatten()
                });
                handle.await.ok().flatten()
            } else if let Some(url) = thumbnail {
                let handle = rt.spawn(async move {
                    crate::sources::download_thumbnail(&url, &cache_dir)
                        .await
                        .ok()
                        .and_then(|path| std::fs::read(path).ok())
                });
                handle.await.ok().flatten()
            } else {
                None
            };
            let Some(bytes) = bytes else {
                return;
            };
            let Some(next) = rt
                .spawn_blocking(move || palette_from_image(&bytes).ok())
                .await
                .ok()
                .flatten()
            else {
                return;
            };
            *palette.borrow_mut() = next.clone();
            if let Err(err) = css::apply_css(&css::full_css(&next)) {
                tracing::warn!(error = %err, "could not apply dynamic theme");
            }
        });
    }
}

/// Builds the resizable three-column body: library | lyrics | queue.
fn build_columns(library: &Library, lyrics: &Lyrics, queue: &QueueWidget) -> gtk::Paned {
    let queue_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    queue_paned.set_start_child(Some(&lyrics.root()));
    queue_paned.set_end_child(Some(&queue.root()));
    queue_paned.set_position(480);

    let main_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    main_paned.set_start_child(Some(&library.root()));
    main_paned.set_end_child(Some(&queue_paned));
    main_paned.set_position(320);
    main_paned.set_hexpand(true);
    main_paned.set_vexpand(true);
    main_paned
}

/// Resolves the palette shown before any track starts playing.
pub(crate) fn resolve_initial_palette(theme: &ThemeConfig) -> Palette {
    match theme.mode {
        ThemeMode::Custom => Palette::from_custom(&theme.custom),
        ThemeMode::Dynamic => {
            // Fall back to Catppuccin until the first cover is extracted.
            palette_for(theme.catppuccin.flavor, &theme.catppuccin.accent)
        }
        ThemeMode::Catppuccin => palette_for(theme.catppuccin.flavor, &theme.catppuccin.accent),
    }
}
