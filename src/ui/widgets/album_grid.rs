//! AlbumGrid — Spotify-style grid of album cards shown above the track list
//! in the Center panel.
//!
//! The grid groups the supplied tracks by `(album, album_artist)` and renders
//! one card per group. Covers are loaded lazily from the first track of each
//! album; while the cover is loading (or if there is none) the card shows a
//! stylised placeholder built from the album initial.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use glib::clone;
use gtk::prelude::*;

use crate::audio::{PlayerCommand, PlayerEvent};
use crate::library::Track;
use crate::library::metadata::extract_cover;
use crate::ui::UiContext;

/// Square size of the album cover thumbnail (px).
const COVER_SIZE: i32 = 160;

/// A grouping key derived from `(album, album_artist)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct AlbumKey {
    album: String,
    artist: String,
}

/// Aggregate info for a single album card.
#[derive(Debug, Clone)]
struct AlbumCard {
    key: AlbumKey,
    /// All tracks belonging to this album, in playback order.
    tracks: Vec<Track>,
}

/// Builds the grouping map for the supplied tracks.
fn group_by_album(tracks: &[Track]) -> Vec<AlbumCard> {
    let mut buckets: BTreeMap<AlbumKey, Vec<Track>> = BTreeMap::new();
    for track in tracks {
        let album = if track.album.is_empty() {
            "Unknown album".to_string()
        } else {
            track.album.clone()
        };
        let artist = if track.album_artist.is_empty() {
            if track.artist.is_empty() {
                "Unknown artist".to_string()
            } else {
                track.artist.clone()
            }
        } else {
            track.album_artist.clone()
        };
        buckets
            .entry(AlbumKey { album, artist })
            .or_default()
            .push(track.clone());
    }
    buckets
        .into_iter()
        .map(|(key, mut tracks)| {
            // Stable-ish order: disc, track number, then title.
            tracks.sort_by(|a, b| {
                a.disc_no
                    .cmp(&b.disc_no)
                    .then(a.track_no.cmp(&b.track_no))
                    .then(a.title.cmp(&b.title))
            });
            AlbumCard { key, tracks }
        })
        .collect()
}

/// Builds the placeholder cover: a square gradient box with the album initial.
fn build_placeholder(initial: char, accent: &str) -> gtk::Box {
    let box_ = gtk::Box::builder()
        .css_classes(vec!["chromia-album-placeholder"])
        .width_request(COVER_SIZE)
        .height_request(COVER_SIZE)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .build();
    let label = gtk::Label::builder()
        .label(initial.to_uppercase().to_string())
        .css_classes(vec!["chromia-album-placeholder-letter"])
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .build();
    box_.append(&label);
    let _ = accent; // accent tint applied via CSS using the global @accent var
    box_
}

/// Builds a single album card widget.
fn build_card(
    album: &AlbumCard,
    tx: &tokio::sync::mpsc::Sender<PlayerCommand>,
    rt: &tokio::runtime::Handle,
) -> gtk::FlowBoxChild {
    let initial = album.key.album.chars().next().unwrap_or('?');

    let cover_box = gtk::Box::builder()
        .css_classes(vec!["chromia-album-cover-wrap"])
        .width_request(COVER_SIZE)
        .height_request(COVER_SIZE)
        .build();
    let placeholder = build_placeholder(initial, "");
    cover_box.append(&placeholder);

    let title = gtk::Label::builder()
        .label(&album.key.album)
        .css_classes(vec!["chromia-album-title"])
        .halign(gtk::Align::Start)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(20)
        .build();
    let artist = gtk::Label::builder()
        .label(&album.key.artist)
        .css_classes(vec!["chromia-album-artist"])
        .halign(gtk::Align::Start)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(20)
        .build();

    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .margin_top(8)
        .build();
    text.append(&title);
    text.append(&artist);

    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(vec!["chromia-album-card"])
        .hexpand(false)
        .build();
    card.append(&cover_box);
    card.append(&text);

    // Lazy cover load — kick off a background task that swaps the placeholder
    // for a real cover once extraction completes.
    let first_track = album.tracks.first().cloned();
    if let Some(track) = first_track {
        if !track.path.as_os_str().is_empty() {
            let cover_box = cover_box.clone();
            let rt = rt.clone();
            let path: PathBuf = track.path.clone();
            glib::MainContext::default().spawn_local(async move {
                let handle = rt.spawn_blocking(move || extract_cover(&path).ok().flatten());
                let bytes = match handle.await {
                    Ok(Some(bytes)) => bytes,
                    _ => return,
                };
                if let Some(pixbuf) =
                    crate::ui::widgets::player::cover_pixbuf(&bytes, COVER_SIZE * 2)
                {
                    let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_css_classes(&["chromia-album-cover"]);
                    picture.set_size_request(COVER_SIZE, COVER_SIZE);
                    // Replace the placeholder with the real cover.
                    while let Some(child) = cover_box.first_child() {
                        cover_box.remove(&child);
                    }
                    cover_box.append(&picture);
                }
            });
        }
    }

    let tx = tx.clone();
    let tracks = album.tracks.clone();
    let click = gtk::GestureClick::new();
    click.connect_released(clone!(
        #[strong]
        tx,
        move |_, _, _, _| {
            let _ = tx.blocking_send(PlayerCommand::LoadQueue(tracks.clone()));
            let _ = tx.blocking_send(PlayerCommand::PlayAt(0));
        }
    ));
    card.add_controller(click);

    // Wrap in a FlowBoxChild so the FlowBox can manage selection / reflow.
    gtk::FlowBoxChild::builder()
        .child(&card)
        .css_classes(vec!["chromia-album-item"])
        .build()
}

/// The album grid widget shown at the top of the Center panel.
pub struct AlbumGrid {
    root: gtk::Box,
    flow: gtk::FlowBox,
    count_label: gtk::Label,
    cards: Rc<RefCell<Vec<Track>>>,
    command_tx: tokio::sync::mpsc::Sender<PlayerCommand>,
    rt: tokio::runtime::Handle,
}

impl AlbumGrid {
    /// Builds the empty album grid.
    pub fn new(ctx: &UiContext) -> Self {
        let heading = gtk::Label::builder()
            .label("Albums")
            .css_classes(vec!["chromia-section-heading", "chromia-albums-heading"])
            .halign(gtk::Align::Start)
            .build();
        let count_label = gtk::Label::builder()
            .label("")
            .css_classes(vec!["chromia-albums-count"])
            .halign(gtk::Align::Start)
            .build();

        let header_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk::Align::Start)
            .margin_bottom(4)
            .build();
        header_row.append(&heading);
        header_row.append(&count_label);

        let flow = gtk::FlowBox::builder()
            .orientation(gtk::Orientation::Horizontal)
            .homogeneous(true)
            .min_children_per_line(2)
            .max_children_per_line(6)
            .selection_mode(gtk::SelectionMode::None)
            .column_spacing(14)
            .row_spacing(14)
            .hexpand(true)
            .vexpand(false)
            .css_classes(vec!["chromia-album-grid"])
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .vexpand(false)
            .propagate_natural_height(true)
            .build();
        scrolled.set_child(Some(&flow));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-album-grid-wrap"])
            .spacing(8)
            .margin_bottom(12)
            .build();
        root.append(&header_row);
        root.append(&scrolled);

        Self {
            root,
            flow,
            count_label,
            cards: Rc::new(RefCell::new(Vec::new())),
            command_tx: ctx.command_tx.clone(),
            rt: ctx.rt.clone(),
        }
    }

    /// Returns the widget to embed in the center panel.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Replaces the displayed tracks and regenerates the album cards.
    pub fn load_tracks(&self, tracks: Vec<Track>) {
        *self.cards.borrow_mut() = tracks.clone();
        self.rebuild(&tracks);
    }

    /// Updates the grid in response to playback events.
    ///
    /// Today the grid does not react to events; the hook exists so future
    /// iterations can highlight the currently playing album.
    pub fn update(&self, _event: &PlayerEvent) {}

    /// Returns the number of cards currently rendered.
    #[allow(dead_code)] // TODO(loki): consumed by the GUI
    pub fn len(&self) -> usize {
        self.cards.borrow().len()
    }

    /// Returns `true` when no cards are rendered.
    #[allow(dead_code)] // TODO(loki): consumed by the GUI
    pub fn is_empty(&self) -> bool {
        self.cards.borrow().is_empty()
    }

    /// Clears the flow box and rebuilds the cards from `tracks`.
    fn rebuild(&self, tracks: &[Track]) {
        while let Some(child) = self.flow.first_child() {
            self.flow.remove(&child);
        }
        let albums = group_by_album(tracks);
        for album in &albums {
            let card = build_card(album, &self.command_tx, &self.rt);
            self.flow.insert(&card, -1);
        }
        let suffix = if albums.len() == 1 { "album" } else { "albums" };
        self.count_label
            .set_label(&format!("{} {}", albums.len(), suffix));
        let visible = !albums.is_empty();
        self.root.set_visible(visible);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::SourceKind;
    use std::path::PathBuf;
    use std::time::Duration;

    fn track(album: &str, artist: &str, title: &str, track_no: u32) -> Track {
        Track {
            id: 0,
            source: SourceKind::Local,
            path: PathBuf::new(),
            url: None,
            thumbnail: None,
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            album_artist: artist.into(),
            duration: Duration::ZERO,
            track_no: Some(track_no),
            disc_no: None,
            genre: None,
            year: None,
            bpm: None,
            play_count: 0,
            last_played: None,
        }
    }

    #[test]
    fn group_by_album_merges_same_album() {
        let tracks = vec![
            track("Album A", "Artist X", "Song 1", 1),
            track("Album A", "Artist X", "Song 2", 2),
            track("Album B", "Artist Y", "Song 3", 1),
        ];
        let albums = group_by_album(&tracks);
        assert_eq!(albums.len(), 2);
        let a = albums
            .iter()
            .find(|a| a.key.album == "Album A")
            .expect("album A present");
        assert_eq!(a.tracks.len(), 2);
        // Tracks are sorted by track_no within the album.
        assert_eq!(a.tracks[0].track_no, Some(1));
        assert_eq!(a.tracks[1].track_no, Some(2));
    }

    #[test]
    fn group_by_album_handles_empty_album_name() {
        let tracks = vec![
            track("", "Artist X", "Song 1", 1),
            track("", "Artist X", "Song 2", 2),
        ];
        let albums = group_by_album(&tracks);
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].key.album, "Unknown album");
    }

    #[test]
    fn group_by_album_separates_same_album_different_artist() {
        let tracks = vec![
            track("Greatest Hits", "Artist X", "Song 1", 1),
            track("Greatest Hits", "Artist Y", "Song 2", 1),
        ];
        let albums = group_by_album(&tracks);
        assert_eq!(albums.len(), 2);
    }
}
