//! Main application window: a three-panel layout
//! (`Sidebar | Center | RightPanel`) with a persistent `BottomPlayer`.
//!
//! Architecture mirrors the spec in `CHROMIA.md`:
//!
//! ```text
//! ┌─────────────┬───────────────────────────────┬──────────────────┐
//! │  Sidebar    │  Center (pages)                │  Right Panel     │
//! │  (fixed)    │  (fixed)                       │  (customisable)  │
//! ├─────────────┴───────────────────────────────┴──────────────────┤
//! │  Bottom Player (preset-driven)                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gtk::prelude::*;
use tokio::sync::mpsc;

use crate::audio::PlayerEvent;
use crate::config::{
    self,
    schema::{AppearanceConfig, GlassBackground, GlassMode, ThemeConfig, ThemeMode},
};
use crate::library::Track;
use crate::library::database::Database;
use crate::theme::css;
use crate::theme::{Palette, palette_for, palette_from_image};
use crate::ui::UiContext;
use crate::ui::bottom_player::BottomPlayer;
use crate::ui::center::Center;
use crate::ui::right_panel::RightPanel;
use crate::ui::sidebar::{NavPage, Sidebar};
use crate::ui::widgets::equalizer::EqualizerWidget;

/// The main window tying together the sidebar, center, right panel, bottom
/// player and the event pump.
pub struct ChromiaWindow {
    app_window: adw::ApplicationWindow,
    root: gtk::Box,
    #[allow(dead_code)] // currently read for setup only; future page swaps
    sidebar: Sidebar,
    center: Rc<Center>,
    right_panel: RightPanel,
    bottom_player: BottomPlayer,
    equalizer: EqualizerWidget,
    event_rx: Rc<RefCell<mpsc::Receiver<PlayerEvent>>>,
    scan_rx: Rc<RefCell<mpsc::Receiver<Vec<Track>>>>,
    palette: Rc<RefCell<Palette>>,
    /// Palette extracted from the current cover in `dynamic` mode; kept across
    /// non-dynamic theme changes so switching back restores the cover colors.
    cover_palette: Rc<RefCell<Option<Palette>>>,
    config: Rc<RefCell<crate::config::schema::Config>>,
    database: Arc<Database>,
    rt: tokio::runtime::Handle,
}

impl ChromiaWindow {
    /// Builds the window, its panels, the initial theme and the event pump.
    pub fn new(
        app: &adw::Application,
        ctx: &UiContext,
        scan_rx: mpsc::Receiver<Vec<Track>>,
    ) -> Rc<Self> {
        let sidebar = Sidebar::new(ctx);
        let center = Rc::new(Center::new(ctx));
        let right_panel = RightPanel::new(ctx);
        let bottom_player = BottomPlayer::new(ctx);
        let equalizer = EqualizerWidget::new(ctx);

        // Wire sidebar → center page header. The actual page contents still
        // live in the Center widget; swapping them is a v1.0 milestone.
        {
            let center_weak = Rc::downgrade(&center);
            sidebar.connect_page_changed(move |page| {
                if let Some(center) = center_weak.upgrade() {
                    center.set_page(page);
                }
            });
        }

        // Opening a playlist replaces the center track list with the playlist's
        // tracks and jumps to the Library page.
        {
            let center = center.clone();
            let database = ctx.database.clone();
            sidebar.connect_playlist_open(move |id| {
                let tracks = match database.playlist_tracks(id) {
                    Ok(tracks) => tracks,
                    Err(err) => {
                        tracing::warn!(error = %err, "failed to open playlist");
                        return;
                    }
                };
                center.load_tracks(tracks);
                center.set_page(NavPage::Library);
            });
        }

        let body_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        body_paned.set_start_child(Some(&sidebar.root()));
        body_paned.set_position(220);
        body_paned.set_resize_start_child(false);
        body_paned.set_shrink_start_child(false);

        let center_right = gtk::Paned::new(gtk::Orientation::Horizontal);
        center_right.set_start_child(Some(&center.root()));
        center_right.set_end_child(Some(&right_panel.root()));
        center_right.set_position(560);
        center_right.set_hexpand(true);
        center_right.set_vexpand(true);
        center_right.set_resize_end_child(false);
        center_right.set_shrink_end_child(false);
        body_paned.set_end_child(Some(&center_right));
        body_paned.set_hexpand(true);
        body_paned.set_vexpand(true);

        let show_equalizer = ctx
            .config
            .borrow()
            .layout
            .widgets
            .iter()
            .any(|w| w.id == "Equalizer" && w.visible);

        let body_wrapper = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .css_classes(vec!["chromia-body"])
            .build();
        body_wrapper.append(&body_paned);
        if show_equalizer {
            body_wrapper.append(&equalizer.root());
        }

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .css_classes(vec!["chromia-shell"])
            .build();
        root.append(&body_wrapper);
        root.append(&bottom_player.root());

        let palette = resolve_initial_palette(&ctx.config.borrow().theme);
        let palette = Rc::new(RefCell::new(palette));
        let cover_palette = Rc::new(RefCell::new(None));

        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::PreferDark);
        // The legacy GtkSettings dark flag triggers a libadwaita warning; the
        // color scheme above is the supported way to request a dark theme.
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_application_prefer_dark_theme(false);
        }

        let app_window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Chromia")
            .default_width(1280)
            .default_height(820)
            .content(&root)
            .build();

        let window = Rc::new(Self {
            app_window,
            root,
            sidebar,
            center,
            right_panel,
            bottom_player,
            equalizer,
            event_rx: ctx.event_rx.clone(),
            scan_rx: Rc::new(RefCell::new(scan_rx)),
            palette,
            cover_palette,
            config: ctx.config.clone(),
            database: ctx.database.clone(),
            rt: ctx.rt.clone(),
        });

        if let Err(err) = css::apply_css(&css::full_css(&window.palette.borrow())) {
            tracing::warn!(error = %err, "could not apply theme css");
        }

        // Register the live theme re-apply hook used by the Settings page. The
        // window clones a weak ref so it never outlives the panels.
        {
            let weak = Rc::downgrade(&window);
            *ctx.theme_applier.borrow_mut() = Some(Rc::new(move || {
                if let Some(w) = weak.upgrade() {
                    w.reapply_theme();
                }
            }));
        }

        // Register the live appearance re-apply hook (Glass UI, animations,
        // blur, border radius). The Settings page calls this after mutating
        // `config.appearance` so the UI updates without a restart.
        {
            let weak = Rc::downgrade(&window);
            *ctx.appearance_applier.borrow_mut() = Some(Rc::new(move || {
                if let Some(w) = weak.upgrade() {
                    w.apply_appearance();
                }
            }));
        }

        // Apply the appearance state that was loaded from disk on startup.
        window.apply_appearance();

        window.start_event_pump();
        window
    }

    /// Shows the window.
    pub fn present(&self) {
        self.app_window.present();
    }

    /// Drains playback and library-scan events off their shared receivers on
    /// the GTK main loop.
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

    /// Consumes completed library scans and refreshes the center panel.
    fn pump_scans(&self) {
        let mut receiver = self.scan_rx.borrow_mut();
        while let Ok(tracks) = receiver.try_recv() {
            self.center.load_tracks(tracks);
        }
    }

    /// Forwards an event to every widget and refreshes the dynamic theme.
    fn dispatch(&self, event: PlayerEvent) {
        if let PlayerEvent::Error(message) = &event {
            tracing::warn!(error = %message, "playback error");
        }
        self.sidebar.update(&event);
        self.center.update(&event);
        self.right_panel.update(&event);
        self.bottom_player.update(&event);
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

    /// Re-extracts the palette from the current track's cover when the theme
    /// is in `dynamic` mode. Local tracks use embedded art; remote tracks
    /// download and cache their cover via `sources::download_thumbnail`.
    fn refresh_dynamic_theme(&self, track: &Track) {
        if !matches!(self.config.borrow().theme.mode, ThemeMode::Dynamic) {
            return;
        }
        let local_path = track.path.clone();
        let thumbnail = track.thumbnail.clone();
        let cache_dir = config::expand_path(&self.config.borrow().paths.cache_dir);
        let palette = self.palette.clone();
        let cover_palette = self.cover_palette.clone();
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
            *cover_palette.borrow_mut() = Some(next.clone());
            *cover_palette.borrow_mut() = Some(next.clone());
            if let Err(err) = css::apply_css(&css::full_css(&next)) {
                tracing::warn!(error = %err, "could not apply dynamic theme");
            }
        });
    }

    /// Re-applies the resolved theme from the current config.
    ///
    /// Called live from the Settings page via the [`ThemeApplier`] hook. In
    /// `catppuccin` / `custom` mode the palette is recomputed from config; in
    /// `dynamic` mode the last cover palette is kept when one exists
    /// (falling back to Catppuccin before any cover is extracted).
    fn reapply_theme(&self) {
        let mode = self.config.borrow().theme.mode;
        let resolved = match mode {
            ThemeMode::Dynamic => match self.cover_palette.borrow().clone() {
                Some(cover) => cover,
                None => palette_for(
                    self.config.borrow().theme.catppuccin.flavor,
                    &self.config.borrow().theme.catppuccin.accent,
                ),
            },
            ThemeMode::Catppuccin => palette_for(
                self.config.borrow().theme.catppuccin.flavor,
                &self.config.borrow().theme.catppuccin.accent,
            ),
            ThemeMode::Custom => Palette::from_custom(&self.config.borrow().theme.custom),
        };
        *self.palette.borrow_mut() = resolved.clone();
        if let Err(err) = css::apply_css(&css::full_css(&resolved)) {
            tracing::warn!(error = %err, "could not re-apply theme css");
        }
    }

    /// Applies the appearance knobs (Glass UI, animations, border radius,
    /// blur) to the shell root as CSS classes plus a generated appearance
    /// block.
    ///
    /// Called once at startup from the persisted config and live from the
    /// Settings page via the [`crate::ui::ThemeApplier`] hook. Mirroring the
    /// palette re-apply, it mutates only the shell root and re-installs the
    /// appearance CSS.
    fn apply_appearance(&self) {
        let appearance: AppearanceConfig = self.config.borrow().appearance.clone();
        let glass_on = appearance.glass && appearance.glass_mode != GlassMode::Disabled;
        let root = &self.root;

        if glass_on {
            root.add_css_class("glass");
        } else {
            root.remove_css_class("glass");
        }

        if glass_on && appearance.glass_mode == GlassMode::Strong {
            root.add_css_class("glass-strong");
        } else {
            root.remove_css_class("glass-strong");
        }

        if appearance.animations {
            root.remove_css_class("no-anim");
        } else {
            root.add_css_class("no-anim");
        }

        // Glass surfaces tinted by the dynamic palette; `Solid` glass gives
        // an opaque tint that needs no blur and no extra overlay.
        if glass_on && appearance.glass_background == GlassBackground::Solid {
            root.add_css_class("glass-solid");
        } else {
            root.remove_css_class("glass-solid");
        }

        if appearance.noise {
            root.add_css_class("noise");
        } else {
            root.remove_css_class("noise");
        }

        if let Err(err) = css::apply_css(&css::appearance_css(&appearance)) {
            tracing::warn!(error = %err, "could not apply appearance css");
        }
    }
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
