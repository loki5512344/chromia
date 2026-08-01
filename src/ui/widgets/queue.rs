//! Up-next queue view.

use std::cell::Cell;

use glib::clone;
use gtk::prelude::*;

use crate::audio::{PlayerCommand, PlayerEvent};
use crate::library::Track;
use crate::ui::UiContext;

/// Rebuilds `list` from `tracks`, wiring each row to start playback at its own
/// index.
fn populate(list: &gtk::ListBox, tracks: &[Track], tx: &tokio::sync::mpsc::Sender<PlayerCommand>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for (i, track) in tracks.iter().enumerate() {
        let title = gtk::Label::builder()
            .label(&track.title)
            .xalign(0.0)
            .css_classes(vec!["chromia-row-title"])
            .build();
        let subtitle = if track.artist.is_empty() {
            track.album.clone()
        } else {
            track.artist.clone()
        };
        let subtitle_label = gtk::Label::builder()
            .label(&subtitle)
            .xalign(0.0)
            .css_classes(vec!["chromia-row-subtitle"])
            .build();
        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_start(6)
            .margin_end(6)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        text_box.append(&title);
        text_box.append(&subtitle_label);

        let row = gtk::ListBoxRow::builder().child(&text_box).build();
        let tx = tx.clone();
        row.connect_activate(clone!(
            #[strong]
            tx,
            move |_| {
                let _ = tx.blocking_send(PlayerCommand::PlayAt(i));
            }
        ));
        list.append(&row);
    }
}

/// The playback queue: a list of upcoming tracks with the current one
/// highlighted.
pub struct Queue {
    root: gtk::Box,
    list: gtk::ListBox,
    current: Cell<Option<usize>>,
    command_tx: tokio::sync::mpsc::Sender<PlayerCommand>,
}

impl Queue {
    /// Builds the queue widget.
    pub fn new(ctx: &UiContext) -> Self {
        let header = gtk::Label::builder()
            .label("Up next")
            .css_classes(vec!["chromia-header"])
            .halign(gtk::Align::Start)
            .build();
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
            .css_classes(vec!["queue"])
            .spacing(6)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        root.append(&header);
        root.append(&scrolled);

        Self {
            root,
            list,
            current: Cell::new(None),
            command_tx: ctx.command_tx.clone(),
        }
    }

    /// Returns the widget to place in the layout panel.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Rebuilds the list on queue changes and restyles the current row when the
    /// playhead moves.
    pub fn update(&self, event: &PlayerEvent) {
        match event {
            PlayerEvent::QueueChanged(tracks) => {
                self.current.set(None);
                populate(&self.list, tracks, &self.command_tx);
            }
            PlayerEvent::CurrentIndexChanged(index) => {
                self.current.set(*index);
                let mut position = 0usize;
                let mut child = self.list.first_child();
                while let Some(widget) = child {
                    if let Some(row) = widget.downcast_ref::<gtk::ListBoxRow>() {
                        if Some(position) == self.current.get() {
                            row.add_css_class("current");
                        } else {
                            row.remove_css_class("current");
                        }
                    }
                    position += 1;
                    child = widget.next_sibling();
                }
            }
            _ => {}
        }
    }
}
