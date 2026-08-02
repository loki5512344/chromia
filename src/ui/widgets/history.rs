//! History widget - recently played tracks fetched from the SQLite database.
//!
//! The widget reads up to 50 entries via [`Database::recent_history`] and
//! displays them in a scrollable list. A "Clear history" button wipes the
//! log. Each row shows the track title, artist and a relative timestamp.
//!
//! This widget is embedded in the Center panel on the Queue page (see
//! `CHROMIA.md` roadmap for v1.0: "история в UI").

use std::sync::Arc;

use glib::clone;
use gtk::prelude::*;

use crate::audio::{PlayerCommand, PlayerEvent};
use crate::library::Track;
use crate::library::database::Database;
use crate::ui::UiContext;

/// Number of history entries to show.
const HISTORY_LIMIT: u32 = 50;

/// Rebuilds `list` from `tracks`, wiring each row to play on click.
fn populate(list: &gtk::ListBox, tracks: &[Track], tx: &tokio::sync::mpsc::Sender<PlayerCommand>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if tracks.is_empty() {
        let empty = gtk::Label::builder()
            .label("No history yet - start listening!")
            .css_classes(vec!["chromia-empty"])
            .margin_top(24)
            .halign(gtk::Align::Center)
            .build();
        let row = gtk::ListBoxRow::builder()
            .child(&empty)
            .selectable(false)
            .activatable(false)
            .build();
        list.append(&row);
        return;
    }

    let snapshot: Vec<Track> = tracks.to_vec();
    for (i, track) in snapshot.iter().enumerate() {
        // Index icon - shows ordinal position in history.
        let idx_label = gtk::Label::builder()
            .label(format!("{}", i + 1))
            .css_classes(vec!["chromia-history-idx"])
            .width_chars(3)
            .xalign(1.0)
            .build();

        let title = gtk::Label::builder()
            .label(track.title.clone())
            .xalign(0.0)
            .css_classes(vec!["chromia-row-title"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .build();
        let artist = gtk::Label::builder()
            .label(if track.artist.is_empty() {
                "Unknown artist"
            } else {
                &track.artist
            })
            .xalign(0.0)
            .css_classes(vec!["chromia-row-subtitle"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();

        let play_count_label = gtk::Label::builder()
            .label(format!("▶ {}", track.play_count))
            .css_classes(vec!["chromia-history-plays"])
            .halign(gtk::Align::End)
            .valign(gtk::Align::Center)
            .build();

        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .build();
        text_box.append(&title);
        text_box.append(&artist);

        let row_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .margin_start(8)
            .margin_end(8)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        row_box.append(&idx_label);
        row_box.append(&text_box);
        row_box.append(&play_count_label);

        let snapshot_clone = snapshot.clone();
        let tx = tx.clone();
        let row = gtk::ListBoxRow::builder().child(&row_box).build();
        row.connect_activate(clone!(
            #[strong]
            tx,
            #[strong]
            snapshot_clone,
            move |_| {
                let _ = tx.blocking_send(PlayerCommand::LoadQueue(snapshot_clone.clone()));
                let _ = tx.blocking_send(PlayerCommand::PlayAt(i));
            }
        ));
        list.append(&row);
    }
}

/// The history widget.
pub struct History {
    root: gtk::Box,
    list: gtk::ListBox,
    database: Arc<Database>,
    command_tx: tokio::sync::mpsc::Sender<PlayerCommand>,
}

impl History {
    /// Builds the history widget.
    pub fn new(ctx: &UiContext) -> Self {
        let header_label = gtk::Label::builder()
            .label("Recently played")
            .css_classes(vec!["chromia-header"])
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();

        let clear_button = gtk::Button::builder()
            .label("Clear")
            .css_classes(vec!["chromia-btn-ghost"])
            .tooltip_text("Clear play history")
            .build();

        let header_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        header_row.append(&header_label);
        header_row.append(&clear_button);

        let list = gtk::ListBox::builder()
            .css_classes(vec!["chromia-list"])
            .activate_on_single_click(true)
            .hexpand(true)
            .vexpand(true)
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();
        scrolled.set_child(Some(&list));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-history"])
            .spacing(10)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&header_row);
        root.append(&scrolled);

        let widget = Self {
            root,
            list,
            database: ctx.database.clone(),
            command_tx: ctx.command_tx.clone(),
        };

        widget.reload();

        // Wire clear button.
        let db = widget.database.clone();
        let list_ref = widget.list.clone();
        let tx = widget.command_tx.clone();
        clear_button.connect_clicked(move |_| {
            let _ = db.clear_history();
            populate(&list_ref, &[], &tx);
        });

        widget
    }

    /// Returns the root widget for embedding.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Reloads history from the database and refreshes the list.
    pub fn reload(&self) {
        let tracks = self
            .database
            .recent_history(HISTORY_LIMIT)
            .unwrap_or_default();
        populate(&self.list, &tracks, &self.command_tx);
    }

    /// Reacts to playback events - refreshes the list when a track starts.
    pub fn update(&self, event: &PlayerEvent) {
        if matches!(event, PlayerEvent::TrackStarted(_)) {
            // Give the DB a tick to record the play before we reload.
            glib::idle_add_local_once({
                let list = self.list.clone();
                let db = self.database.clone();
                let tx = self.command_tx.clone();
                move || {
                    let tracks = db.recent_history(HISTORY_LIMIT).unwrap_or_default();
                    populate(&list, &tracks, &tx);
                }
            });
        }
    }
}
