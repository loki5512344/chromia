//! Main playback widget: cover art, track info, transport and volume controls.
//!
//! All playback control flows through [`PlayerCommand`]s sent to the audio
//! task; the widget never touches the audio stack directly.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use glib::clone;
use gtk::prelude::*;

use crate::audio::{PlaybackState, PlayerCommand, PlayerEvent, RepeatMode};
use crate::library::Track;
use crate::library::metadata::extract_cover;
use crate::ui::UiContext;

/// Formats a [`Duration`] as `m:ss`.
pub(crate) fn fmt_duration(d: Duration) -> String {
    let total = d.as_secs();
    format!("{}:{:02}", total / 60, total % 60)
}

/// Builds a square `size`x`size` pixbuf from encoded cover-art bytes.
///
/// Returns `None` when the bytes cannot be decoded by GDK.
pub(crate) fn cover_pixbuf(bytes: &[u8], size: i32) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(bytes.to_vec())).ok()?;
    pixbuf.scale_simple(size, size, gtk::gdk_pixbuf::InterpType::Bilinear)
}

/// Main transport and track-info widget placed in the layout panel.
pub struct PlayerCore {
    root: gtk::Box,
    cover: gtk::Picture,
    title_label: gtk::Label,
    artist_label: gtk::Label,
    start_label: gtk::Label,
    end_label: gtk::Label,
    progress: gtk::Scale,
    play_icon: gtk::Image,
    play_button: gtk::ToggleButton,
    shuffle_button: gtk::ToggleButton,
    repeat_button: gtk::ToggleButton,
    volume_icon: gtk::Image,
    volume_scale: gtk::Scale,
    duration: Cell<Duration>,
    dragging: Rc<Cell<bool>>,
    volume_dragging: Rc<Cell<bool>>,
    repeat_mode: Rc<Cell<RepeatMode>>,
    rt: tokio::runtime::Handle,
}

impl PlayerCore {
    /// Builds the player widget and wires every control to the audio task.
    pub fn new(ctx: &UiContext) -> Self {
        let command_tx = ctx.command_tx.clone();
        let rt = ctx.rt.clone();
        let initial_volume = ctx.config.borrow().audio.volume.clamp(0.0, 1.0);

        let cover = gtk::Picture::builder()
            .width_request(56)
            .height_request(56)
            .css_classes(vec!["chromia-cover"])
            .valign(gtk::Align::Center)
            .build();

        let title_label = gtk::Label::builder()
            .label("Nothing playing")
            .css_classes(vec!["chromia-title"])
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let artist_label = gtk::Label::builder()
            .label("")
            .css_classes(vec!["chromia-subtitle"])
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let info = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Center)
            .spacing(0)
            .build();
        info.append(&title_label);
        info.append(&artist_label);

        let start_label = gtk::Label::builder()
            .label("0:00")
            .css_classes(vec!["chromia-time"])
            .build();
        let end_label = gtk::Label::builder()
            .label("0:00")
            .css_classes(vec!["chromia-time"])
            .build();
        let progress = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .adjustment(&gtk::Adjustment::new(0.0, 0.0, 1.0, 0.001, 0.1, 0.0))
            .draw_value(false)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        let time_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        time_row.append(&start_label);
        time_row.append(&progress);
        time_row.append(&end_label);

        let info_column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        info_column.append(&info);
        info_column.append(&time_row);

        let shuffle_button = gtk::ToggleButton::builder()
            .icon_name("media-playlist-shuffle-symbolic")
            .tooltip_text("Shuffle")
            .build();
        let prev_button = gtk::Button::from_icon_name("media-skip-backward-symbolic");
        prev_button.set_tooltip_text(Some("Previous"));
        let play_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
        play_icon.set_icon_size(gtk::IconSize::Normal);
        let play_button = gtk::ToggleButton::builder()
            .child(&play_icon)
            .tooltip_text("Play / Pause")
            .build();
        let next_button = gtk::Button::from_icon_name("media-skip-forward-symbolic");
        next_button.set_tooltip_text(Some("Next"));
        let repeat_button = gtk::ToggleButton::builder()
            .icon_name("media-playlist-repeat-symbolic")
            .tooltip_text("Repeat")
            .build();
        let transport = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .halign(gtk::Align::Center)
            .spacing(8)
            .build();
        transport.append(&shuffle_button);
        transport.append(&prev_button);
        transport.append(&play_button);
        transport.append(&next_button);
        transport.append(&repeat_button);

        let volume_icon = gtk::Image::from_icon_name("audio-volume-high-symbolic");
        volume_icon.set_icon_size(gtk::IconSize::Normal);
        let volume_button = gtk::Button::builder()
            .child(&volume_icon)
            .tooltip_text("Mute")
            .build();
        let volume_scale = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .adjustment(&gtk::Adjustment::new(
                f64::from(initial_volume),
                0.0,
                1.0,
                0.01,
                0.1,
                0.0,
            ))
            .width_request(110)
            .draw_value(false)
            .build();
        let volume_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        volume_row.append(&volume_button);
        volume_row.append(&volume_scale);

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(vec!["chromia-player-bar"])
            .spacing(14)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .hexpand(true)
            .build();
        root.append(&cover);
        root.append(&info_column);
        root.append(&transport);
        root.append(&volume_row);

        let dragging = Rc::new(Cell::new(false));
        let click = gtk::GestureClick::new();
        click.connect_pressed(clone!(
            #[weak]
            dragging,
            move |_, _, _, _| dragging.set(true)
        ));
        click.connect_released(clone!(
            #[weak]
            dragging,
            move |_, _, _, _| dragging.set(false)
        ));
        progress.add_controller(click);
        progress.connect_change_value(clone!(
            #[strong]
            command_tx,
            move |_, _, value| {
                let _ =
                    command_tx.blocking_send(PlayerCommand::Seek(Duration::from_secs_f64(value)));
                glib::Propagation::Proceed
            }
        ));

        let volume_dragging = Rc::new(Cell::new(false));
        let vclick = gtk::GestureClick::new();
        vclick.connect_pressed(clone!(
            #[weak]
            volume_dragging,
            move |_, _, _, _| {
                volume_dragging.set(true);
            }
        ));
        vclick.connect_released(clone!(
            #[weak]
            volume_dragging,
            move |_, _, _, _| {
                volume_dragging.set(false);
            }
        ));
        volume_scale.add_controller(vclick);
        volume_scale.connect_change_value(clone!(
            #[strong]
            command_tx,
            move |_, _, value| {
                let _ = command_tx.blocking_send(PlayerCommand::SetVolume(value as f32));
                glib::Propagation::Proceed
            }
        ));

        shuffle_button.connect_clicked(clone!(
            #[strong]
            command_tx,
            move |btn| {
                let _ = command_tx.blocking_send(PlayerCommand::SetShuffle(btn.is_active()));
            }
        ));

        prev_button.connect_clicked(clone!(
            #[strong]
            command_tx,
            move |_| {
                let _ = command_tx.blocking_send(PlayerCommand::Previous);
            }
        ));
        next_button.connect_clicked(clone!(
            #[strong]
            command_tx,
            move |_| {
                let _ = command_tx.blocking_send(PlayerCommand::Next);
            }
        ));
        play_button.connect_clicked(clone!(
            #[strong]
            command_tx,
            move |_| {
                let _ = command_tx.blocking_send(PlayerCommand::PlayPause);
            }
        ));

        let repeat_mode = Rc::new(Cell::new(RepeatMode::Off));
        repeat_button.connect_clicked(clone!(
            #[strong]
            command_tx,
            #[weak]
            repeat_mode,
            move |btn| {
                let next = match repeat_mode.get() {
                    RepeatMode::Off => RepeatMode::All,
                    RepeatMode::All => RepeatMode::One,
                    RepeatMode::One => RepeatMode::Off,
                };
                repeat_mode.set(next);
                btn.set_active(next != RepeatMode::Off);
                let _ = command_tx.blocking_send(PlayerCommand::SetRepeat(next));
            }
        ));

        let last_volume = Rc::new(Cell::new(initial_volume));
        volume_button.connect_clicked(clone!(
            #[strong]
            command_tx,
            #[weak]
            last_volume,
            #[weak]
            volume_scale,
            move |_| {
                if volume_scale.value() > 0.001 {
                    last_volume.set(volume_scale.value() as f32);
                    let _ = command_tx.blocking_send(PlayerCommand::SetVolume(0.0));
                } else {
                    let restored = if last_volume.get() > 0.0 {
                        last_volume.get()
                    } else {
                        1.0
                    };
                    let _ = command_tx.blocking_send(PlayerCommand::SetVolume(restored));
                }
            }
        ));

        Self {
            root,
            cover,
            title_label,
            artist_label,
            start_label,
            end_label,
            progress,
            play_icon,
            play_button,
            shuffle_button,
            repeat_button,
            volume_icon,
            volume_scale,
            duration: Cell::new(Duration::ZERO),
            dragging,
            volume_dragging,
            repeat_mode,
            rt,
        }
    }

    /// Returns the widget to place in the layout panel.
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
                self.artist_label.set_text(&subtitle);
                self.duration.set(track.duration);
                self.progress.set_value(0.0);
                self.start_label.set_text("0:00");
                self.end_label.set_text(&fmt_duration(track.duration));
                self.play_icon
                    .set_icon_name(Some("media-playback-pause-symbolic"));
                self.play_button.set_active(true);
                self.load_cover(track);
            }
            PlayerEvent::DurationChanged(d) => {
                self.duration.set(*d);
                self.end_label.set_text(&fmt_duration(*d));
            }
            PlayerEvent::PositionChanged(d) => {
                self.start_label.set_text(&fmt_duration(*d));
                let duration = self.duration.get();
                if !self.dragging.get() && !duration.is_zero() {
                    let ratio = d.as_secs_f64() / duration.as_secs_f64();
                    self.progress.set_value(ratio);
                }
            }
            PlayerEvent::PlaybackStateChanged(state) => match state {
                PlaybackState::Playing => {
                    self.play_icon
                        .set_icon_name(Some("media-playback-pause-symbolic"));
                    self.play_button.set_active(true);
                }
                PlaybackState::Paused | PlaybackState::Stopped => {
                    self.play_icon
                        .set_icon_name(Some("media-playback-start-symbolic"));
                    self.play_button.set_active(false);
                }
            },
            PlayerEvent::VolumeChanged(v) => {
                if !self.volume_dragging.get() {
                    self.volume_scale.set_value(f64::from(*v));
                }
                let icon = if *v <= 0.001 {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                };
                self.volume_icon.set_icon_name(Some(icon));
            }
            PlayerEvent::ShuffleChanged(v) => self.shuffle_button.set_active(*v),
            PlayerEvent::RepeatChanged(m) => {
                self.repeat_mode.set(*m);
                self.repeat_button.set_active(*m != RepeatMode::Off);
            }
            PlayerEvent::TrackEnded
            | PlayerEvent::QueueChanged(_)
            | PlayerEvent::CurrentIndexChanged(_)
            | PlayerEvent::Error(_) => {}
        }
    }

    /// Extracts and displays the cover art for a local track in the background.
    fn load_cover(&self, track: &Track) {
        let cover = self.cover.clone();
        let rt = self.rt.clone();
        let path = track.path.clone();
        if path.as_os_str().is_empty() {
            return;
        }
        glib::MainContext::default().spawn_local(async move {
            let handle = rt.spawn_blocking(move || extract_cover(&path).ok().flatten());
            let bytes = match handle.await {
                Ok(Some(bytes)) => bytes,
                _ => return,
            };
            if let Some(pixbuf) = cover_pixbuf(&bytes, 56) {
                cover.set_paintable(Some(&gtk::gdk::Texture::for_pixbuf(&pixbuf)));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::fmt_duration;
    use std::time::Duration;

    #[test]
    fn formats_mm_ss() {
        assert_eq!(fmt_duration(Duration::from_secs(0)), "0:00");
        assert_eq!(fmt_duration(Duration::from_secs(59)), "0:59");
        assert_eq!(fmt_duration(Duration::from_secs(60)), "1:00");
        assert_eq!(fmt_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(fmt_duration(Duration::from_secs(600)), "10:00");
    }
}
