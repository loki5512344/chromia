//! Sidebar — fixed navigation rail hosting the Chromia logo, the primary
//! navigation buttons (Library / Search / Queue / Settings), the user's
//! playlists and a user-profile row at the bottom.
//!
//! The sidebar is the visual anchor of the new three-panel layout. It is
//! intentionally static — the buttons drive the Center page but never move
//! themselves, mirroring the contract described in `CHROMIA.md` ("Sidebar —
//! навигация, плейлисты, пользователь. Не двигается.").

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use glib::clone;
use gtk::prelude::*;

use crate::audio::PlayerEvent;
use crate::ui::UiContext;

/// Identifier of the active primary page.
///
/// The values mirror the labels shown in the sidebar and the page keys used in
/// the layout config; they are intentionally cheap to copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavPage {
    /// The local library / search view.
    Library,
    /// Artist browser (library page drill-down).
    Artists,
    /// Genre browser (library page drill-down).
    Genres,
    /// Search-only view (separate from library browsing).
    Search,
    /// The playback queue.
    Queue,
    /// Application settings (placeholder for v1.0 GUI settings).
    Settings,
}

impl NavPage {
    /// Returns the icon name used by the sidebar entry.
    fn icon_name(self) -> &'static str {
        match self {
            Self::Library => "media-optical-cd-audio-symbolic",
            Self::Artists => "avatar-default-symbolic",
            Self::Genres => "emblem-music-symbolic",
            Self::Search => "system-search-symbolic",
            Self::Queue => "view-list-symbolic",
            Self::Settings => "preferences-system-symbolic",
        }
    }

    /// Returns the human-readable label shown in the sidebar entry.
    fn label(self) -> &'static str {
        match self {
            Self::Library => "Library",
            Self::Artists => "Artists",
            Self::Genres => "Genres",
            Self::Search => "Search",
            Self::Queue => "Queue",
            Self::Settings => "Settings",
        }
    }

    /// Iterates over every navigation page in display order.
    const fn all() -> [Self; 6] {
        [
            Self::Library,
            Self::Artists,
            Self::Genres,
            Self::Search,
            Self::Queue,
            Self::Settings,
        ]
    }
}

/// Builds a single navigation button with a leading icon and trailing label.
///
/// The button uses `chromia-navbutton` so the stylesheet can drive the hover
/// and active states without further code.
fn nav_button(page: NavPage) -> gtk::Button {
    let icon = gtk::Image::from_icon_name(page.icon_name());
    icon.set_icon_size(gtk::IconSize::Normal);

    let label = gtk::Label::builder()
        .label(page.label())
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .hexpand(true)
        .build();
    content.append(&icon);
    content.append(&label);

    gtk::Button::builder()
        .css_classes(vec!["chromia-navbutton"])
        .child(&content)
        .hexpand(true)
        .build()
}

/// Builds the Chromia wordmark + logo header at the top of the sidebar.
fn build_logo() -> gtk::Box {
    let mark = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(vec!["chromia-logo-mark"])
        .width_request(38)
        .height_request(38)
        .valign(gtk::Align::Center)
        .build();

    let inner = gtk::Label::builder()
        .label("C")
        .css_classes(vec!["chromia-logo-letter"])
        .width_request(38)
        .height_request(38)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .build();
    mark.append(&inner);

    let title = gtk::Label::builder()
        .label("Chromia")
        .css_classes(vec!["chromia-logo-title"])
        .halign(gtk::Align::Start)
        .build();

    let subtitle = gtk::Label::builder()
        .label("your colors, your music")
        .css_classes(vec!["chromia-logo-subtitle"])
        .halign(gtk::Align::Start)
        .build();

    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .hexpand(true)
        .build();
    text.append(&title);
    text.append(&subtitle);

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(vec!["chromia-logo"])
        .build();
    row.append(&mark);
    row.append(&text);
    row
}

/// Builds the section heading used for "Playlists" and similar blocks.
fn section_heading(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .css_classes(vec!["chromia-section-heading"])
        .halign(gtk::Align::Start)
        .build()
}

/// Builds the empty playlists placeholder shown when the database has none.
fn empty_playlists() -> gtk::Label {
    gtk::Label::builder()
        .label("No playlists yet")
        .css_classes(vec!["chromia-empty"])
        .halign(gtk::Align::Start)
        .build()
}

/// Builds the user profile row at the bottom of the sidebar.
fn build_profile() -> gtk::Box {
    let avatar = gtk::Box::builder()
        .css_classes(vec!["chromia-avatar"])
        .valign(gtk::Align::Center)
        .build();
    let avatar_label = gtk::Label::builder()
        .label("U")
        .css_classes(vec!["chromia-avatar-letter"])
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .build();
    avatar.append(&avatar_label);

    let name = gtk::Label::builder()
        .label("Listener")
        .css_classes(vec!["chromia-profile-name"])
        .halign(gtk::Align::Start)
        .build();
    let detail = gtk::Label::builder()
        .label("Local library")
        .css_classes(vec!["chromia-profile-detail"])
        .halign(gtk::Align::Start)
        .build();

    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .hexpand(true)
        .build();
    text.append(&name);
    text.append(&detail);

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .css_classes(vec!["chromia-profile"])
        .build();
    row.append(&avatar);
    row.append(&text);
    row
}

/// Callback invoked when the active navigation page changes.
type PageCallback = Rc<RefCell<Option<Box<dyn Fn(NavPage)>>>>;

/// Callback invoked when a playlist should be opened in the center.
type PlaylistOpenCallback = Rc<RefCell<Option<Box<dyn Fn(i64)>>>>;

/// The sidebar widget — fixed navigation rail.
///
/// Holds a `Rc<RefCell<NavPage>>` so the rest of the UI can query the current
/// page and so other panels (the Center panel in particular) can subscribe to
/// page changes through `connect_page_changed`.
pub struct Sidebar {
    root: gtk::Box,
    nav_buttons: Vec<gtk::Button>,
    page: Rc<RefCell<NavPage>>,
    on_page_changed: PageCallback,
    on_playlist_open: PlaylistOpenCallback,
    playlists_box: gtk::Box,
    database: Arc<crate::library::database::Database>,
    playlist_entry: gtk::Entry,
    playlist_add: gtk::Button,
}

impl Sidebar {
    /// Builds the sidebar widget.
    pub fn new(ctx: &UiContext) -> Self {
        let logo = build_logo();

        let nav_buttons: Vec<gtk::Button> = NavPage::all().iter().map(|&p| nav_button(p)).collect();
        let nav_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(vec!["chromia-nav"])
            .build();
        for button in &nav_buttons {
            nav_box.append(button);
        }

        let playlist_heading = section_heading("Playlists");
        let playlists_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .css_classes(vec!["chromia-playlists"])
            .build();
        playlists_box.append(&empty_playlists());

        let playlist_entry = gtk::Entry::builder()
            .placeholder_text("New playlist…")
            .css_classes(vec!["chromia-playlist-new"])
            .build();
        let playlist_add = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("Create a playlist")
            .build();
        let new_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        new_row.append(&playlist_entry);
        new_row.append(&playlist_add);

        let playlists_section = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();
        playlists_section.append(&playlist_heading);
        playlists_section.append(&new_row);
        playlists_section.append(&playlists_box);

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_vexpand(true);

        let profile = build_profile();

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-sidebar"])
            .spacing(18)
            .build();
        root.append(&logo);
        root.append(&nav_box);
        root.append(&playlists_section);
        root.append(&spacer);
        root.append(&profile);

        let page = Rc::new(RefCell::new(NavPage::Library));
        let on_page_changed: PageCallback = Rc::new(RefCell::new(None));
        let on_playlist_open: PlaylistOpenCallback = Rc::new(RefCell::new(None));
        let database = ctx.database.clone();

        let widget = Self {
            root,
            nav_buttons,
            page,
            on_page_changed,
            on_playlist_open,
            playlists_box,
            database,
            playlist_entry,
            playlist_add,
        };

        widget.mark_active(NavPage::Library);
        widget.wire_nav();
        widget.wire_playlist_entry();
        widget.reload_playlists();
        widget
    }

    /// Returns the widget to embed in the window.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Returns the currently selected page.
    #[allow(dead_code)] // TODO(loki): consumed by the window page switcher
    pub fn current_page(&self) -> NavPage {
        *self.page.borrow()
    }

    /// Registers a callback that fires whenever the active page changes.
    pub fn connect_page_changed<F: Fn(NavPage) + 'static>(&self, callback: F) {
        *self.on_page_changed.borrow_mut() = Some(Box::new(callback));
    }

    /// Updates the sidebar in response to playback events.
    ///
    /// The sidebar itself does not react to most events today, but the hook is
    /// in place so the profile row can later show the "now playing" track.
    pub fn update(&self, _event: &PlayerEvent) {}

    /// Toggles the active styling on the navigation button matching `page` and
    /// clears it on every other button.
    fn mark_active(&self, page: NavPage) {
        for (index, button) in self.nav_buttons.iter().enumerate() {
            let active = NavPage::all()[index] == page;
            if active {
                button.add_css_class("active");
            } else {
                button.remove_css_class("active");
            }
        }
    }

    /// Wires every nav button to update `page` and fire the change callback.
    fn wire_nav(&self) {
        for (index, button) in self.nav_buttons.iter().enumerate() {
            let page = NavPage::all()[index];
            let page_cell = self.page.clone();
            let on_page_changed = self.on_page_changed.clone();
            let buttons = self.nav_buttons.clone();
            button.connect_clicked(clone!(
                #[strong]
                page_cell,
                #[strong]
                on_page_changed,
                #[strong]
                buttons,
                move |_| {
                    *page_cell.borrow_mut() = page;
                    for (i, b) in buttons.iter().enumerate() {
                        if NavPage::all()[i] == page {
                            b.add_css_class("active");
                        } else {
                            b.remove_css_class("active");
                        }
                    }
                    if let Some(cb) = on_page_changed.borrow().as_ref() {
                        cb(page);
                    }
                }
            ));
        }
    }

    /// Registers a callback that fires whenever a playlist should be opened.
    pub fn connect_playlist_open<F: Fn(i64) + 'static>(&self, callback: F) {
        *self.on_playlist_open.borrow_mut() = Some(Box::new(callback));
    }

    /// Wires the "new playlist" row: pressing Enter or the button creates the
    /// playlist and rebuilds the list.
    fn wire_playlist_entry(&self) {
        let entry = self.playlist_entry.clone();
        let add_button = self.playlist_add.clone();
        let playlists_box = self.playlists_box.clone();
        let database = self.database.clone();
        let on_playlist_open = self.on_playlist_open.clone();

        let create = {
            let entry = entry.clone();
            move || {
                let name = entry.text().trim().to_string();
                if name.is_empty() {
                    return;
                }
                if let Err(err) = database.create_playlist(&name) {
                    tracing::warn!(error = %err, "failed to create playlist");
                    return;
                }
                entry.set_text("");
                populate_playlists(&playlists_box, &database, &on_playlist_open);
            }
        };

        let activate = {
            let create = create.clone();
            move |_entry: &gtk::Entry| create()
        };
        let clicked = {
            let create = create.clone();
            move |_button: &gtk::Button| create()
        };
        entry.connect_activate(activate);
        add_button.connect_clicked(clicked);
    }

    /// Re-loads the playlist list from the database, rebuilding every row.
    pub fn reload_playlists(&self) {
        populate_playlists(&self.playlists_box, &self.database, &self.on_playlist_open);
    }
}

/// Rebuilds the playlist list inside `playlists_box` from `database`, wiring
/// each row to open or delete the playlist via `on_playlist_open`.
fn populate_playlists(
    playlists_box: &gtk::Box,
    database: &Arc<crate::library::database::Database>,
    on_playlist_open: &PlaylistOpenCallback,
) {
    while let Some(child) = playlists_box.first_child() {
        playlists_box.remove(&child);
    }
    let playlists = database.list_playlists().unwrap_or_default();
    if playlists.is_empty() {
        playlists_box.append(&empty_playlists());
        return;
    }
    for playlist in playlists {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .css_classes(vec!["chromia-playlist-row"])
            .build();
        let icon = gtk::Image::from_icon_name("media-playlist-repeat-symbolic");
        icon.set_icon_size(gtk::IconSize::Normal);
        let open = gtk::Button::builder()
            .child(
                &gtk::Label::builder()
                    .label(&playlist.name)
                    .halign(gtk::Align::Start)
                    .hexpand(true)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build(),
            )
            .css_classes(vec!["chromia-playlist-open"])
            .hexpand(true)
            .build();
        let delete = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(vec!["chromia-playlist-delete"])
            .build();
        row.append(&icon);
        row.append(&open);
        row.append(&delete);

        let id = playlist.id;
        open.connect_clicked(clone!(
            #[strong]
            on_playlist_open,
            move |_| {
                if let Some(cb) = on_playlist_open.borrow().as_ref() {
                    cb(id);
                }
            }
        ));
        let database = database.clone();
        let playlists_box = playlists_box.clone();
        delete.connect_clicked(clone!(
            #[strong]
            database,
            #[strong]
            playlists_box,
            #[strong]
            on_playlist_open,
            move |_| {
                if let Err(err) = database.delete_playlist(id) {
                    tracing::warn!(error = %err, "failed to delete playlist");
                    return;
                }
                populate_playlists(&playlists_box, &database, &on_playlist_open);
            }
        ));
        playlists_box.append(&row);
    }
}
