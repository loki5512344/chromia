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
            Self::Search => "system-search-symbolic",
            Self::Queue => "view-list-symbolic",
            Self::Settings => "preferences-system-symbolic",
        }
    }

    /// Returns the human-readable label shown in the sidebar entry.
    fn label(self) -> &'static str {
        match self {
            Self::Library => "Library",
            Self::Search => "Search",
            Self::Queue => "Queue",
            Self::Settings => "Settings",
        }
    }

    /// Iterates over every navigation page in display order.
    const fn all() -> [Self; 4] {
        [Self::Library, Self::Search, Self::Queue, Self::Settings]
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
        .build();

    let inner = gtk::Label::builder()
        .label("C")
        .css_classes(vec!["chromia-logo-letter"])
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
    playlists_box: gtk::Box,
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

        let playlists_section = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();
        playlists_section.append(&playlist_heading);
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

        let widget = Self {
            root,
            nav_buttons,
            page,
            on_page_changed,
            playlists_box,
        };

        widget.mark_active(NavPage::Library);
        widget.wire_nav();
        widget.load_playlists(ctx);
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

    /// Loads the user's playlists into the sidebar placeholder.
    ///
    /// For now this is a best-effort read; if the database has no playlists the
    /// empty-state label is kept in place.
    fn load_playlists(&self, ctx: &UiContext) {
        let playlists = ctx.database.list_playlists().unwrap_or_default();
        if playlists.is_empty() {
            return;
        }
        // Clear the placeholder before appending real entries.
        while let Some(child) = self.playlists_box.first_child() {
            self.playlists_box.remove(&child);
        }
        for playlist in playlists {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(10)
                .css_classes(vec!["chromia-playlist-row"])
                .build();
            let icon = gtk::Image::from_icon_name("media-playlist-repeat-symbolic");
            icon.set_icon_size(gtk::IconSize::Normal);
            let label = gtk::Label::builder()
                .label(&playlist.name)
                .halign(gtk::Align::Start)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            row.append(&icon);
            row.append(&label);
            self.playlists_box.append(&row);
        }
    }
}
