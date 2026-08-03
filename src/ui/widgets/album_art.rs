//! AlbumArt — large cover widget shown at the top of the right panel.
//!
//! The widget keeps the latest cover bytes and re-renders a `gtk::Picture`
//! whenever a new track starts. It is intentionally lightweight: the heavy
//! lifting (cover extraction, palette generation) happens in
//! `library::metadata` and `theme::dynamic`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::audio::PlayerEvent;
use crate::library::Track;
use crate::library::metadata::extract_cover;
use crate::ui::UiContext;

/// Size (in pixels) of the album-art square shown in the right panel.
const ART_SIZE: i32 = 220;

/// Large album-art widget for the right panel slot.
pub struct AlbumArt {
    root: gtk::Box,
    picture: gtk::Picture,
    title_label: gtk::Label,
    subtitle_label: gtk::Label,
    fallback: Rc<RefCell<Option<gtk::gdk::Texture>>>,
    rt: tokio::runtime::Handle,
}

impl AlbumArt {
    /// Builds the album-art widget.
    pub fn new(ctx: &UiContext) -> Self {
        let picture = gtk::Picture::builder()
            .width_request(ART_SIZE)
            .height_request(ART_SIZE)
            .css_classes(vec!["chromia-album-art"])
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();

        let title_label = gtk::Label::builder()
            .label("Nothing playing")
            .css_classes(vec!["chromia-art-title"])
            .halign(gtk::Align::Center)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let subtitle_label = gtk::Label::builder()
            .label("")
            .css_classes(vec!["chromia-art-subtitle"])
            .halign(gtk::Align::Center)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();

        let text_column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .halign(gtk::Align::Center)
            .build();
        text_column.append(&title_label);
        text_column.append(&subtitle_label);

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-album-art-card"])
            .spacing(12)
            .halign(gtk::Align::Center)
            .build();
        root.append(&picture);
        root.append(&text_column);

        Self {
            root,
            picture,
            title_label,
            subtitle_label,
            fallback: Rc::new(RefCell::new(None)),
            rt: ctx.rt.clone(),
        }
    }

    /// Returns the widget to embed in the right panel slot.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Applies a playback event to the widget state.
    pub fn update(&self, event: &PlayerEvent) {
        match event {
            PlayerEvent::TrackStarted(track) => {
                self.title_label.set_text(&track.title);
                let subtitle = if track.artist.is_empty() {
                    track.album.clone()
                } else if track.album.is_empty() {
                    track.artist.clone()
                } else {
                    format!("{} — {}", track.artist, track.album)
                };
                self.subtitle_label.set_text(&subtitle);
                self.load_cover(track);
            }
            PlayerEvent::Loading(title) => {
                self.title_label.set_text("Loading stream…");
                self.subtitle_label.set_text(title);
                self.show_fallback();
            }
            _ => {}
        }
    }

    /// Loads the cover for the current track on a background thread.
    fn load_cover(&self, track: &Track) {
        let picture = self.picture.clone();
        let rt = self.rt.clone();
        let path = track.path.clone();
        if path.as_os_str().is_empty() {
            self.show_fallback();
            return;
        }
        glib::MainContext::default().spawn_local(async move {
            let handle = rt.spawn_blocking(move || extract_cover(&path).ok().flatten());
            let bytes = match handle.await {
                Ok(Some(bytes)) => bytes,
                _ => return,
            };
            if let Some(pixbuf) = crate::ui::widgets::player::cover_pixbuf(&bytes, ART_SIZE * 2) {
                let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
                picture.set_paintable(Some(&texture));
            }
        });
    }

    /// Falls back to the placeholder gradient when no cover is available.
    fn show_fallback(&self) {
        if let Some(texture) = self.fallback.borrow().clone() {
            self.picture.set_paintable(Some(&texture));
        } else {
            self.picture.set_paintable(None::<&gtk::gdk::Texture>);
        }
    }
}
