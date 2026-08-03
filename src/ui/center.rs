//! Center panel - the fixed area between the sidebar and the right panel.
//!
//! v1.0 upgrade: real page switching via a `gtk::Stack`. Each page renders
//! a dedicated widget:
//!
//! - **Library** - AlbumGrid + track list
//! - **Search**  - hint panel (full search lives in the Library page bar)
//! - **Queue**   - recently-played history widget
//! - **Settings** - full GUI settings page

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::audio::PlayerEvent;
use crate::ui::UiContext;
use crate::ui::sidebar::NavPage;
use crate::ui::widgets::album_grid::AlbumGrid;
use crate::ui::widgets::browser::{BrowseKind, Browser};
use crate::ui::widgets::history::History;
use crate::ui::widgets::library::Library;
use crate::ui::widgets::settings::Settings;

/// The Center panel container.
pub struct Center {
    root: gtk::Box,
    header_label: gtk::Label,
    subheader_label: gtk::Label,
    stack: gtk::Stack,
    album_grid: AlbumGrid,
    library: Library,
    history: History,
    settings: Settings,
    browser_artists: Browser,
    browser_genres: Browser,
    page: Rc<RefCell<NavPage>>,
}

impl Center {
    /// Builds the center panel.
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

        // ── Pages ─────────────────────────────────────────────────────────

        let album_grid = AlbumGrid::new(ctx);
        let library = Library::new(ctx);

        // Library page - album grid + track list stacked vertically.
        let library_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .hexpand(true)
            .vexpand(true)
            .build();
        library_page.append(&album_grid.root());
        library_page.append(&library.root());

        // Search page - a hint panel. The full search bar (local DB +
        // YouTube / SoundCloud) lives in the Library page, so this page just
        // points the user there instead of duplicating a second Library
        // instance that would not receive scan updates.
        let search_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .spacing(12)
            .hexpand(true)
            .vexpand(true)
            .build();
        let search_icon = gtk::Image::from_icon_name("system-search-symbolic");
        search_icon.set_icon_size(gtk::IconSize::Large);
        search_icon.set_css_classes(&["chromia-search-icon"]);
        let search_hint = gtk::Label::builder()
            .label("Search your library, YouTube and SoundCloud\nfrom the bar on the Library page")
            .css_classes(vec!["chromia-page-subtitle"])
            .halign(gtk::Align::Center)
            .justify(gtk::Justification::Center)
            .build();
        search_page.append(&search_icon);
        search_page.append(&search_hint);

        // Queue / History page.
        let history = History::new(ctx);
        let queue_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        queue_page.append(&history.root());

        // Artist / Genre browser pages.
        let browser_artists = Browser::new(ctx, BrowseKind::Artists);
        let browser_genres = Browser::new(ctx, BrowseKind::Genres);
        let artists_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        artists_page.append(&browser_artists.root());
        let genres_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        genres_page.append(&browser_genres.root());

        // Settings page.
        let settings = Settings::new(ctx);

        // ── Stack ────────────────────────────────────────────────────────
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(180)
            .hexpand(true)
            .vexpand(true)
            .build();
        stack.add_named(&library_page, Some("library"));
        stack.add_named(&artists_page, Some("artists"));
        stack.add_named(&genres_page, Some("genres"));
        stack.add_named(&search_page, Some("search"));
        stack.add_named(&queue_page, Some("queue"));
        stack.add_named(&settings.root(), Some("settings"));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-center"])
            .spacing(0)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&header);
        root.append(&stack);

        Self {
            root,
            header_label,
            subheader_label,
            stack,
            album_grid,
            library,
            history,
            settings,
            browser_artists,
            browser_genres,
            page: Rc::new(RefCell::new(NavPage::Library)),
        }
    }

    /// Returns the widget to embed in the window.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Switches the active page - updates the header and the stack child.
    pub fn set_page(&self, page: NavPage) {
        *self.page.borrow_mut() = page;
        let (title, subtitle, stack_name, refresh) = match page {
            NavPage::Library => (
                "Library",
                "Your collection, ready to play",
                "library",
                false,
            ),
            NavPage::Artists => ("Artists", "Browse by artist", "artists", true),
            NavPage::Genres => ("Genres", "Browse by genre", "genres", true),
            NavPage::Search => ("Search", "Find tracks across every source", "search", false),
            NavPage::Queue => (
                "History",
                "Tracks you've listened to recently",
                "queue",
                false,
            ),
            NavPage::Settings => ("Settings", "Tune Chromia to your taste", "settings", false),
        };
        self.header_label.set_label(title);
        self.subheader_label.set_label(subtitle);
        self.stack.set_visible_child_name(stack_name);

        // Reload browser grids and history whenever their page is opened so the
        // list is always fresh after a background rescan.
        if refresh || page == NavPage::Artists {
            self.browser_artists.reload(BrowseKind::Artists);
        }
        if refresh || page == NavPage::Genres {
            self.browser_genres.reload(BrowseKind::Genres);
        }
        if page == NavPage::Queue {
            self.history.reload();
        }
    }

    /// Returns the underlying library widget.
    #[allow(dead_code)]
    pub fn library(&self) -> &Library {
        &self.library
    }

    /// Forwards a playback event to every page that cares.
    pub fn update(&self, event: &PlayerEvent) {
        self.library.update(event);
        self.album_grid.update(event);
        self.history.update(event);
        self.settings.update(event);
    }

    /// Replaces the displayed track list and refreshes the album grid.
    pub fn load_tracks(&self, tracks: Vec<crate::library::Track>) {
        self.album_grid.load_tracks(tracks.clone());
        self.library.load_tracks(tracks);
        self.browser_artists.reload(BrowseKind::Artists);
        self.browser_genres.reload(BrowseKind::Genres);
    }
}
