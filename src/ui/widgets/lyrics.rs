//! Synchronised lyrics view with current-line highlighting.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use crate::audio::PlayerEvent;
use crate::library::Track;
use crate::lyrics::lrclib::{Lrclib, LyricLine};
use crate::ui::UiContext;

/// Shows the lyrics of the current track and highlights the active line.
pub struct Lyrics {
    root: gtk::Box,
    text_view: gtk::TextView,
    lines: Rc<RefCell<Vec<LyricLine>>>,
    line_starts: Rc<RefCell<Vec<usize>>>,
    current_tag: gtk::TextTag,
    rt: tokio::runtime::Handle,
}

impl Lyrics {
    /// Builds the lyrics widget, adding a "current" line tag to its buffer.
    pub fn new(ctx: &UiContext) -> Self {
        let text_view = gtk::TextView::builder()
            .editable(false)
            .wrap_mode(gtk::WrapMode::WordChar)
            .css_classes(vec!["chromia-lyrics-text"])
            .hexpand(true)
            .vexpand(true)
            .build();
        text_view.set_cursor_visible(false);

        let buffer = text_view.buffer();
        let accent = ctx.config.borrow().theme.custom.accent.clone();
        let accent = if accent.is_empty() {
            "#cba6f7".to_owned()
        } else {
            accent
        };
        let accent_rgba = gtk::gdk::RGBA::parse(&accent)
            .unwrap_or_else(|_| gtk::gdk::RGBA::new(0.79, 0.65, 0.97, 1.0));
        let current_tag = gtk::TextTag::builder()
            .name("current")
            .weight(700)
            .foreground_set(true)
            .foreground_rgba(&accent_rgba)
            .build();
        buffer.tag_table().add(&current_tag);
        buffer.set_text("No lyrics found");

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();
        scrolled.set_child(Some(&text_view));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["lyrics"])
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        root.append(&scrolled);

        Self {
            root,
            text_view,
            lines: Rc::new(RefCell::new(Vec::new())),
            line_starts: Rc::new(RefCell::new(Vec::new())),
            current_tag,
            rt: ctx.rt.clone(),
        }
    }

    /// Returns the widget to place in the layout panel.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Loads lyrics on track start and scrolls to the active line on position
    /// updates.
    pub fn update(&self, event: &PlayerEvent) {
        match event {
            PlayerEvent::TrackStarted(track) => {
                self.lines.replace(Vec::new());
                self.line_starts.replace(Vec::new());
                self.text_view.buffer().set_text("Loading lyrics…");
                self.fetch_lyrics(track);
            }
            PlayerEvent::PositionChanged(pos) => self.highlight(*pos),
            _ => {}
        }
    }

    /// Fetches lyrics from LRCLIB on the background runtime and renders them on
    /// the GTK thread once they arrive.
    fn fetch_lyrics(&self, track: &Track) {
        let rt = self.rt.clone();
        let artist = track.artist.clone();
        let title = track.title.clone();
        let album = track.album.clone();
        let duration = track.duration;
        let text_view = self.text_view.clone();
        let lines = self.lines.clone();
        let line_starts = self.line_starts.clone();
        let current_tag = self.current_tag.clone();

        glib::MainContext::default().spawn_local(async move {
            let handle = rt.spawn(async move {
                let lrclib = Lrclib::new();
                lrclib.get_lyrics(&artist, &title, &album, duration).await
            });
            let fetched = match handle.await {
                Ok(Ok(Some(lyrics))) => lyrics.lines,
                _ => Vec::new(),
            };

            let buffer = text_view.buffer();
            buffer.set_text("");
            if fetched.is_empty() {
                lines.replace(Vec::new());
                line_starts.replace(Vec::new());
                buffer.set_text("No lyrics found");
                return;
            }
            lines.replace(fetched.clone());
            let mut iter = buffer.start_iter();
            let mut starts = Vec::with_capacity(fetched.len());
            for line in &fetched {
                starts.push(iter.offset() as usize);
                buffer.insert(&mut iter, &format!("{}\n", line.text));
            }
            line_starts.replace(starts);
            let end = line_starts.borrow().get(1).copied().unwrap_or_default() as i32;
            if end > 0 {
                let start_iter = buffer.iter_at_offset(0);
                let end_iter = buffer.iter_at_offset(end);
                buffer.apply_tag(&current_tag, &start_iter, &end_iter);
            }
        });
    }

    /// Highlights the line active at `pos` and scrolls it into view.
    fn highlight(&self, pos: Duration) {
        let lines = self.lines.borrow();
        if lines.is_empty() {
            return;
        }
        let idx = lines.partition_point(|line| line.time <= pos);
        if idx == 0 {
            return;
        }
        let current = idx - 1;
        let buffer = self.text_view.buffer();
        buffer.remove_tag(&self.current_tag, &buffer.start_iter(), &buffer.end_iter());
        let starts = self.line_starts.borrow();
        let start = match starts.get(current) {
            Some(&offset) => offset as i32,
            None => return,
        };
        let end = starts
            .get(current + 1)
            .map(|&offset| offset as i32)
            .unwrap_or_else(|| buffer.end_iter().offset());
        let mut start_iter = buffer.iter_at_offset(start);
        let end_iter = buffer.iter_at_offset(end);
        buffer.apply_tag(&self.current_tag, &start_iter, &end_iter);
        self.text_view
            .scroll_to_iter(&mut start_iter, 0.0, false, 0.0, 0.0);
    }
}
