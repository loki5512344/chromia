//! Bottom Player — the persistent transport bar pinned to the bottom of the
//! window. Holds the cover thumbnail, the track title/artist, the seek bar,
//! the transport controls (shuffle / prev / play / next / repeat), the volume
//! slider and the preset selector described in `CHROMIA.md`.
//!
//! Presets (`minimal` / `default` / `audiophile`) are pure UI affordances in
//! this iteration: they hide or reveal groups of elements without changing
//! the underlying playback engine. The `audiophile` preset exposes a
//! placeholder for the future bitrate / sample-rate readout.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use glib::clone;
use gtk::prelude::*;

use crate::audio::{PlaybackState, PlayerCommand, PlayerEvent, RepeatMode};
use crate::library::Track;
use crate::library::metadata::extract_cover;
use crate::ui::UiContext;
use crate::ui::layout::presets::{BottomPlayerElement, BottomPlayerPreset};

/// Re-exports the duration formatter so callers can reuse it without pulling
/// the original `widgets::player` module.
pub(crate) use crate::ui::widgets::player::fmt_duration;

/// Cover thumbnail size in pixels for the bottom player.
const COVER_SIZE: i32 = 56;

/// The bottom transport bar widget.
pub struct BottomPlayer {
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

impl BottomPlayer {
    /// Builds the bottom player widget and wires every control.
    pub fn new(ctx: &UiContext) -> Self {
        let command_tx = ctx.command_tx.clone();
        let rt = ctx.rt.clone();
        let initial_volume = ctx.config.borrow().audio.volume.clamp(0.0, 1.0);
        let initial_preset = BottomPlayerPreset::default();

        // ── Cover ───────────────────────────────────────────────────────────
        let cover = gtk::Picture::builder()
            .width_request(COVER_SIZE)
            .height_request(COVER_SIZE)
            .css_classes(vec!["chromia-cover", "chromia-bottom-cover"])
            .valign(gtk::Align::Center)
            .build();

        // ── Title / artist ─────────────────────────────────────────────────
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
        let track_info = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();
        track_info.append(&title_label);
        track_info.append(&artist_label);

        // ── Progress row ───────────────────────────────────────────────────
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
            .css_classes(vec!["chromia-progress"])
            .build();
        let progress_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        progress_row.append(&start_label);
        progress_row.append(&progress);
        progress_row.append(&end_label);

        // ── Transport ──────────────────────────────────────────────────────
        let shuffle_button = gtk::ToggleButton::builder()
            .icon_name("media-playlist-shuffle-symbolic")
            .tooltip_text("Shuffle")
            .css_classes(vec!["chromia-transport"])
            .build();
        let prev_button = gtk::Button::from_icon_name("media-skip-backward-symbolic");
        prev_button.set_tooltip_text(Some("Previous"));
        prev_button.add_css_class("chromia-transport");
        let play_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
        play_icon.set_icon_size(gtk::IconSize::Normal);
        let play_button = gtk::ToggleButton::builder()
            .child(&play_icon)
            .tooltip_text("Play / Pause")
            .css_classes(vec!["chromia-transport", "chromia-play"])
            .build();
        let next_button = gtk::Button::from_icon_name("media-skip-forward-symbolic");
        next_button.set_tooltip_text(Some("Next"));
        next_button.add_css_class("chromia-transport");
        let repeat_button = gtk::ToggleButton::builder()
            .icon_name("media-playlist-repeat-symbolic")
            .tooltip_text("Repeat")
            .css_classes(vec!["chromia-transport"])
            .build();
        let transport = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .halign(gtk::Align::Center)
            .spacing(6)
            .build();
        transport.append(&shuffle_button);
        transport.append(&prev_button);
        transport.append(&play_button);
        transport.append(&next_button);
        transport.append(&repeat_button);

        // ── Volume ─────────────────────────────────────────────────────────
        let volume_icon = gtk::Image::from_icon_name("audio-volume-high-symbolic");
        volume_icon.set_icon_size(gtk::IconSize::Normal);
        let volume_button = gtk::Button::builder()
            .child(&volume_icon)
            .tooltip_text("Mute")
            .css_classes(vec!["chromia-transport"])
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
            .width_request(96)
            .draw_value(false)
            .build();
        let volume_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        volume_row.append(&volume_button);
        volume_row.append(&volume_scale);

        // ── Preset selector ────────────────────────────────────────────────
        let preset_names = BottomPlayerPreset::all_names();
        let name_refs: Vec<&str> = preset_names.iter().map(String::as_str).collect();
        let preset_combo = gtk::DropDown::from_strings(&name_refs);
        preset_combo.set_tooltip_text(Some("Bottom player preset"));
        preset_combo.set_selected(initial_preset.as_index() as u32);
        preset_combo.add_css_class("chromia-preset");

        // ── Extra info (audiophile preset placeholder) ─────────────────────
        let info_extra = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .css_classes(vec!["chromia-bottom-info-extra"])
            .build();
        let bitrate_label = gtk::Label::builder()
            .label("—")
            .css_classes(vec!["chromia-info-pill"])
            .tooltip_text("Bitrate")
            .build();
        let codec_label = gtk::Label::builder()
            .label("—")
            .css_classes(vec!["chromia-info-pill"])
            .tooltip_text("Codec")
            .build();
        info_extra.append(&bitrate_label);
        info_extra.append(&codec_label);

        // ── Root layout ────────────────────────────────────────────────────
        let left = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build();
        left.append(&cover);
        left.append(&track_info);

        let center = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .halign(gtk::Align::Center)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        center.append(&progress_row);
        center.append(&transport);

        let right = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk::Align::End)
            .build();
        right.append(&info_extra);
        right.append(&preset_combo);
        right.append(&volume_row);

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(vec!["chromia-bottom-player"])
            .spacing(16)
            .hexpand(true)
            .build();
        root.append(&left);
        root.append(&center);
        root.append(&right);

        // ── Wire seek bar ──────────────────────────────────────────────────
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

        // ── Wire volume ────────────────────────────────────────────────────
        let volume_dragging = Rc::new(Cell::new(false));
        let vclick = gtk::GestureClick::new();
        vclick.connect_pressed(clone!(
            #[weak]
            volume_dragging,
            move |_, _, _, _| volume_dragging.set(true)
        ));
        vclick.connect_released(clone!(
            #[weak]
            volume_dragging,
            move |_, _, _, _| volume_dragging.set(false)
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

        // ── Transport wiring ───────────────────────────────────────────────
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

        // ── Mute toggle ────────────────────────────────────────────────────
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

        // ── Preset selector ────────────────────────────────────────────────
        let preset = Rc::new(RefCell::new(initial_preset));

        let apply_state = {
            let preset = preset.clone();
            let cover = cover.clone();
            let title_label = title_label.clone();
            let artist_label = artist_label.clone();
            let progress_row = progress_row.clone();
            let transport = transport.clone();
            let volume_row = volume_row.clone();
            let info_extra = info_extra.clone();
            move |raw: u32| {
                let next = BottomPlayerPreset::from_index(raw as usize)
                    .unwrap_or(BottomPlayerPreset::Default);
                *preset.borrow_mut() = next;

                // Map each element to the widget that renders it. We use a
                // slice of references so the borrow checker is happy with the
                // mixed widget types (Picture, Label, Box…).
                let map: &[(&BottomPlayerElement, &gtk::Widget)] = &[
                    (&BottomPlayerElement::Cover, cover.upcast_ref()),
                    (&BottomPlayerElement::Song, title_label.upcast_ref()),
                    (&BottomPlayerElement::Artist, artist_label.upcast_ref()),
                    (&BottomPlayerElement::Progress, progress_row.upcast_ref()),
                    (&BottomPlayerElement::Controls, transport.upcast_ref()),
                    (&BottomPlayerElement::Volume, volume_row.upcast_ref()),
                    (&BottomPlayerElement::Bitrate, info_extra.upcast_ref()),
                ];
                let active = next.elements();
                for (element, widget) in map {
                    // The audiophile info pill hosts bitrate / codec / sample
                    // rate together, so it becomes visible when any of them
                    // is requested.
                    let visible = active.iter().any(|e| e == *element)
                        || (matches!(element, BottomPlayerElement::Bitrate)
                            && active.iter().any(|e| {
                                matches!(e, BottomPlayerElement::Codec)
                                    || matches!(e, BottomPlayerElement::SampleRate)
                            }));
                    widget.set_visible(visible);
                }
            }
        };
        apply_state(preset_combo.selected());

        preset_combo.connect_selected_notify(clone!(
            #[strong]
            apply_state,
            move |combo| apply_state(combo.selected())
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

    /// Returns the widget to embed in the window.
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

    /// Loads the cover thumbnail for a track on a background thread.
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
            if let Some(pixbuf) = crate::ui::widgets::player::cover_pixbuf(&bytes, COVER_SIZE) {
                cover.set_paintable(Some(&gtk::gdk::Texture::for_pixbuf(&pixbuf)));
            }
        });
    }
}
