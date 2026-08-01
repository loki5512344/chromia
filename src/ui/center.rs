//! Center panel — the fixed area between the sidebar and the right panel.
//!
//! Hosts the album grid (Spotify-style cards) and the Library widget below
//! it. The page header mirrors the active [`crate::ui::sidebar::NavPage`];
//! switching pages updates the header text today, with actual page
//! swapping scheduled for v1.0 (see `CHROMIA.md`).

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::audio::PlayerEvent;
use crate::ui::UiContext;
use crate::ui::sidebar::NavPage;
use crate::ui::widgets::album_grid::AlbumGrid;
use crate::ui::widgets::library::Library;

/// The Center panel container.
pub struct Center {
    root: gtk::Box,
    header_label: gtk::Label,
    subheader_label: gtk::Label,
    album_grid: AlbumGrid,
    library: Library,
    page: Rc<RefCell<NavPage>>,
}

impl Center {
    /// Builds the center panel wrapping the album grid + Library widget.
    pub fn new(ctx: &UiContext) -> Self {
        let header_label = gtk::Label::builder()
            .label("Library")
            .css_classes(vec!["chromia-page-title"])
            .halign(gtk::Align::Start)
            .build();
        let subheader_label = gtk::Label::builder()
            .label("Your collection, ready to play")
            .css_classes(vec!["chromia-page-subtitle"])
            .halign(gtk::Align::Start)
            .build();

        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .css_classes(vec!["chromia-page-header"])
            .build();
        header.append(&header_label);
        header.append(&subheader_label);

        let album_grid = AlbumGrid::new(ctx);
        let library = Library::new(ctx);

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-center"])
            .spacing(12)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&header);
        root.append(&album_grid.root());
        root.append(&library.root());

        Self {
            root,
            header_label,
            subheader_label,
            album_grid,
            library,
            page: Rc::new(RefCell::new(NavPage::Library)),
        }
    }

    /// Returns the widget to embed in the window.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Switches the active page (header only in this iteration).
    pub fn set_page(&self, page: NavPage) {
        *self.page.borrow_mut() = page;
        let (title, subtitle) = match page {
            NavPage::Library => ("Library", "Your collection, ready to play"),
            NavPage::Search => ("Search", "Find tracks across every source"),
            NavPage::Queue => ("Queue", "What's coming up next"),
            NavPage::Settings => ("Settings", "Tune Chromia to your taste"),
        };
        self.header_label.set_label(title);
        self.subheader_label.set_label(subtitle);
    }

    /// Returns the underlying library widget so the window can feed it
    /// scan results.
    #[allow(dead_code)] // reserved for the future layout editor
    pub fn library(&self) -> &Library {
        &self.library
    }

    /// Forwards a playback event to the library and the album grid.
    pub fn update(&self, event: &PlayerEvent) {
        self.library.update(event);
        self.album_grid.update(event);
    }

    /// Replaces the displayed track list and refreshes the album grid.
    pub fn load_tracks(&self, tracks: Vec<crate::library::Track>) {
        self.album_grid.load_tracks(tracks.clone());
        self.library.load_tracks(tracks);
    }
}
