//! Library browsers: grouped lists of artists or genres with a drill-down track
//! list. Selecting an item loads its tracks into the lower list, ready to play.

use std::rc::Rc;
use std::sync::Arc;

use glib::clone;
use gtk::prelude::*;

use crate::audio::PlayerCommand;
use crate::library::Track;
use crate::library::database::Database;
use crate::ui::UiContext;

/// What a [`Browser`] aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseKind {
    /// Group tracks by `artist`.
    Artists,
    /// Group tracks by `genre`.
    Genres,
}

impl BrowseKind {
    fn title(self) -> &'static str {
        match self {
            Self::Artists => "Artists",
            Self::Genres => "Genres",
        }
    }
}

/// A scrollable card grid plus a drill-down track list for one aggregation key.
pub struct Browser {
    root: gtk::Box,
    grid: gtk::FlowBox,
    list: gtk::ListBox,
    command_tx: tokio::sync::mpsc::Sender<PlayerCommand>,
    database: Arc<Database>,
}

impl Browser {
    /// Builds the browser for `kind`, populating the grid from the database.
    pub fn new(ctx: &UiContext, kind: BrowseKind) -> Self {
        let command_tx = ctx.command_tx.clone();
        let database = ctx.database.clone();

        let title = gtk::Label::builder()
            .label(kind.title())
            .css_classes(vec!["chromia-page-title"])
            .halign(gtk::Align::Start)
            .build();

        let grid = gtk::FlowBox::builder()
            .css_classes(vec!["chromia-browse-grid"])
            .max_children_per_line(4)
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .build();
        let grid_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .max_content_height(300)
            .hexpand(true)
            .build();
        grid_scroll.set_child(Some(&grid));

        let list = gtk::ListBox::builder()
            .css_classes(vec!["chromia-list"])
            .activate_on_single_click(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        let list_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();
        list_scroll.set_child(Some(&list));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .margin_start(12)
            .margin_end(12)
            .margin_top(4)
            .margin_bottom(12)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&title);
        root.append(&grid_scroll);
        root.append(&list_scroll);

        Self {
            root,
            grid,
            list,
            command_tx,
            database,
        }
    }

    /// Returns the widget to embed in the center panel.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Re-queries the database and rebuilds the browsing grid.
    pub fn reload(&self, kind: BrowseKind) {
        self.rebuild_grid(kind);
    }

    /// Fills the grid from `grouped_artists` / `grouped_genres`; clicking a card
    /// loads that key's tracks into the lower list.
    fn rebuild_grid(&self, kind: BrowseKind) {
        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }
        let groups: Vec<(String, usize)> = match kind {
            BrowseKind::Artists => self.database.grouped_artists().unwrap_or_default(),
            BrowseKind::Genres => self.database.grouped_genres().unwrap_or_default(),
        };
        for (name, count) in groups {
            let label = gtk::Label::builder()
                .label(&name)
                .xalign(0.0)
                .css_classes(vec!["chromia-browse-name"])
                .max_width_chars(20)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let count_label = gtk::Label::builder()
                .label(format!("{count} tracks"))
                .xalign(0.0)
                .css_classes(vec!["chromia-row-subtitle"])
                .build();
            let text = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .halign(gtk::Align::Fill)
                .spacing(2)
                .build();
            text.append(&label);
            text.append(&count_label);

            let button = gtk::Button::builder()
                .css_classes(vec!["chromia-browse-card"])
                .child(&text)
                .build();

            let database = self.database.clone();
            let list = self.list.clone();
            let tx = self.command_tx.clone();
            let name = name.clone();
            button.connect_clicked(clone!(
                #[strong]
                database,
                #[strong]
                list,
                #[strong]
                tx,
                #[strong]
                name,
                move |_| {
                    let tracks = match kind {
                        BrowseKind::Artists => database.tracks_by_artist(&name),
                        BrowseKind::Genres => database.tracks_by_genre(&name),
                    }
                    .unwrap_or_default();
                    populate(&list, &tracks, &tx);
                }
            ));

            self.grid.append(&button);
        }
    }
}

/// Replaces the content of `list` with `tracks`, wiring each row to queue them
/// and start playback at its own index.
fn populate(
    list: &gtk::ListBox,
    tracks: &[Track],
    command_tx: &tokio::sync::mpsc::Sender<PlayerCommand>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let snapshot = Rc::new(tracks.to_vec());
    for (i, track) in snapshot.iter().enumerate() {
        let title = gtk::Label::builder()
            .label(&track.title)
            .xalign(0.0)
            .css_classes(vec!["chromia-row-title"])
            .build();
        let subtitle = if track.artist.is_empty() {
            track.album.clone()
        } else if track.album.is_empty() {
            track.artist.clone()
        } else {
            format!("{} — {}", track.artist, track.album)
        };
        let sub = gtk::Label::builder()
            .label(&subtitle)
            .xalign(0.0)
            .css_classes(vec!["chromia-row-subtitle"])
            .build();
        let text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_start(6)
            .margin_end(6)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        text.append(&title);
        text.append(&sub);

        let row = gtk::ListBoxRow::builder().child(&text).build();
        let tx = command_tx.clone();
        let snapshot = snapshot.clone();
        row.connect_activate(clone!(
            #[strong]
            tx,
            #[strong]
            snapshot,
            move |_| {
                let _ = tx.blocking_send(PlayerCommand::LoadQueue((*snapshot).clone()));
                let _ = tx.blocking_send(PlayerCommand::PlayAt(i));
            }
        ));
        list.append(&row);
    }
}
