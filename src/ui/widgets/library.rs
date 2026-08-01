//! Searchable library list: local database plus online sources (YouTube,
//! SoundCloud) resolved through yt-dlp.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use glib::clone;
use gtk::prelude::*;
use tracing::warn;

use crate::audio::{PlayerCommand, PlayerEvent};
use crate::config::expand_path;
use crate::library::SourceKind;
use crate::library::Track;
use crate::sources::soundcloud::SoundcloudSource;
use crate::sources::youtube::YoutubeSource;
use crate::sources::{default_source, enabled_sources};
use crate::ui::UiContext;

/// Rebuilds `list` from `tracks`, wiring each row to load the list as the
/// queue and start playback at its own index.
fn populate(
    list: &gtk::ListBox,
    tracks: &Rc<RefCell<Vec<Track>>>,
    tx: &tokio::sync::mpsc::Sender<PlayerCommand>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let snapshot = Rc::new(tracks.borrow().clone());
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
        let snapshot = snapshot.clone();
        let tx = tx.clone();
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

/// One of the yt-dlp backed online sources.
enum OnlineSource {
    Youtube(YoutubeSource),
    Soundcloud(SoundcloudSource),
}

impl OnlineSource {
    /// Runs a search against the underlying service.
    async fn search(&self, query: &str) -> anyhow::Result<Vec<Track>> {
        match self {
            Self::Youtube(source) => source.search(query).await,
            Self::Soundcloud(source) => source.search(query).await,
        }
    }
}

/// The searchable track list with a source selector.
pub struct Library {
    root: gtk::Box,
    search: gtk::SearchEntry,
    list: gtk::ListBox,
    tracks: Rc<RefCell<Vec<Track>>>,
    command_tx: tokio::sync::mpsc::Sender<PlayerCommand>,
    modes: Rc<Vec<SourceKind>>,
    dropdown: gtk::DropDown,
    youtube: YoutubeSource,
    soundcloud: SoundcloudSource,
    rt: tokio::runtime::Handle,
}

impl Library {
    /// Builds the library widget, loads the full library and wires the search
    /// entry to re-query the active source on every keystroke.
    pub fn new(ctx: &UiContext) -> Self {
        let command_tx = ctx.command_tx.clone();

        let modes = {
            let config = ctx.config.borrow();
            Rc::new(enabled_sources(&config.sources))
        };
        let names: Vec<String> = modes.iter().map(|mode| mode.to_string()).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let dropdown = gtk::DropDown::from_strings(&name_refs);
        let initial = {
            let config = ctx.config.borrow();
            modes
                .iter()
                .position(|mode| *mode == default_source(&config.sources))
                .unwrap_or(0)
        };
        dropdown.set_selected(initial as u32);

        let (quality, cache_dir) = {
            let config = ctx.config.borrow();
            (
                config.sources.youtube.quality.clone(),
                expand_path(&config.sources.youtube.cache_dir),
            )
        };
        let youtube = YoutubeSource::new(&quality, cache_dir.clone());
        let soundcloud = SoundcloudSource::new(&quality, cache_dir);

        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search…")
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

        let search_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        search_row.append(&dropdown);
        search_row.append(&search);

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["library"])
            .spacing(6)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        root.append(&search_row);
        root.append(&scrolled);

        let widget = Self {
            root,
            search,
            list,
            tracks: Rc::new(RefCell::new(Vec::new())),
            command_tx,
            modes,
            dropdown,
            youtube,
            soundcloud,
            rt: ctx.rt.clone(),
        };
        widget.initial_load(ctx);
        widget.wire(&ctx.database);
        widget
    }

    /// Returns the widget to place in the layout panel.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Replaces the displayed track list.
    pub fn load_tracks(&self, tracks: Vec<Track>) {
        *self.tracks.borrow_mut() = tracks;
        populate(&self.list, &self.tracks, &self.command_tx);
    }

    /// The library does not react to playback events.
    pub fn update(&self, _event: &PlayerEvent) {}

    /// Loads the full library from the database, logging failures instead of
    /// panicking.
    fn initial_load(&self, ctx: &UiContext) {
        match ctx.database.all_tracks() {
            Ok(tracks) => self.load_tracks(tracks),
            Err(err) => warn!(error = %err, "failed to load the library"),
        }
    }

    /// Wires the source selector and search entry to reload the list.
    fn wire(&self, database: &Arc<crate::library::database::Database>) {
        let search_changed = {
            let database = database.clone();
            let search = self.search.clone();
            let dropdown = self.dropdown.clone();
            let modes = self.modes.clone();
            let list = self.list.clone();
            let tracks = self.tracks.clone();
            let tx = self.command_tx.clone();
            let youtube = self.youtube.clone();
            let soundcloud = self.soundcloud.clone();
            let rt = self.rt.clone();
            move |_entry: &gtk::SearchEntry| {
                reload(
                    &database,
                    &search,
                    &dropdown,
                    &modes,
                    &list,
                    &tracks,
                    &tx,
                    &youtube,
                    &soundcloud,
                    &rt,
                );
            }
        };
        self.search.connect_search_changed(search_changed);

        let selected = {
            let database = database.clone();
            let search = self.search.clone();
            let dropdown = self.dropdown.clone();
            let modes = self.modes.clone();
            let list = self.list.clone();
            let tracks = self.tracks.clone();
            let tx = self.command_tx.clone();
            let youtube = self.youtube.clone();
            let soundcloud = self.soundcloud.clone();
            let rt = self.rt.clone();
            move |_dropdown: &gtk::DropDown| {
                reload(
                    &database,
                    &search,
                    &dropdown,
                    &modes,
                    &list,
                    &tracks,
                    &tx,
                    &youtube,
                    &soundcloud,
                    &rt,
                );
            }
        };
        self.dropdown.connect_selected_notify(selected);
    }
}

/// Runs the current query against the selected source.
#[allow(clippy::too_many_arguments)]
fn reload(
    database: &Arc<crate::library::database::Database>,
    search: &gtk::SearchEntry,
    dropdown: &gtk::DropDown,
    modes: &[SourceKind],
    list: &gtk::ListBox,
    tracks: &Rc<RefCell<Vec<Track>>>,
    tx: &tokio::sync::mpsc::Sender<PlayerCommand>,
    youtube: &YoutubeSource,
    soundcloud: &SoundcloudSource,
    rt: &tokio::runtime::Handle,
) {
    let query = search.text().to_string();
    let mode = modes
        .get(dropdown.selected() as usize)
        .copied()
        .unwrap_or(SourceKind::Local);

    match mode {
        SourceKind::Local => {
            let result = if query.is_empty() {
                database.all_tracks()
            } else {
                database.search(&query)
            };
            match result {
                Ok(found) => {
                    *tracks.borrow_mut() = found;
                    populate(list, tracks, tx);
                }
                Err(err) => warn!(error = %err, "library search failed"),
            }
        }
        SourceKind::Youtube | SourceKind::Soundcloud => {
            let online = match mode {
                SourceKind::Youtube => OnlineSource::Youtube(youtube.clone()),
                SourceKind::Soundcloud => OnlineSource::Soundcloud(soundcloud.clone()),
                SourceKind::Local => unreachable!(),
            };
            let list = list.clone();
            let tracks = tracks.clone();
            let tx = tx.clone();
            let rt = rt.clone();
            glib::MainContext::default().spawn_local(async move {
                if query.is_empty() {
                    *tracks.borrow_mut() = Vec::new();
                    populate(&list, &tracks, &tx);
                    return;
                }
                let handle = rt.spawn(async move { online.search(&query).await });
                let results = match handle.await {
                    Ok(Ok(tracks)) => tracks,
                    Ok(Err(err)) => {
                        warn!(error = %err, "online search failed");
                        return;
                    }
                    Err(err) => {
                        warn!(error = %err, "online search task failed");
                        return;
                    }
                };
                *tracks.borrow_mut() = results.clone();
                populate(&list, &tracks, &tx);
            });
        }
    }
}
