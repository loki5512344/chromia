//! Ten-band graphical equalizer with an enable switch and presets.

use std::cell::Cell;
use std::rc::Rc;

use glib::clone;
use gtk::prelude::*;

use crate::audio::equalizer::{BAND_FREQUENCIES, BANDS, MAX_GAIN_DB, MIN_GAIN_DB, presets};
use crate::audio::{PlayerCommand, PlayerEvent};
use crate::ui::UiContext;

/// Band sliders plus an enable switch and preset picker.
pub struct EqualizerWidget {
    root: gtk::Box,
    sliders: Vec<gtk::Scale>,
    enabled: Rc<Cell<bool>>,
    preset_names: Vec<String>,
    command_tx: tokio::sync::mpsc::Sender<PlayerCommand>,
}

impl EqualizerWidget {
    /// Builds the equalizer widget and wires all controls to the audio task.
    pub fn new(ctx: &UiContext) -> Self {
        let command_tx = ctx.command_tx.clone();
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["equalizer"])
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();

        let enable_switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
        let enable_label = gtk::Label::builder().label("Enable").build();
        let enable_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        enable_row.append(&enable_label);
        enable_row.append(&enable_switch);

        let preset_names: Vec<String> = presets().iter().map(|p| p.name.to_owned()).collect();
        let name_refs: Vec<&str> = preset_names.iter().map(String::as_str).collect();
        let combo = gtk::DropDown::from_strings(&name_refs);
        combo.set_selected(0);

        let top_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        top_row.append(&enable_row);
        top_row.append(&combo);
        root.append(&top_row);

        let bands_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .halign(gtk::Align::Center)
            .hexpand(true)
            .vexpand(true)
            .spacing(6)
            .build();
        let mut sliders = Vec::with_capacity(BANDS);
        for frequency in BAND_FREQUENCIES {
            let adjustment =
                gtk::Adjustment::new(0.0, MIN_GAIN_DB as f64, MAX_GAIN_DB as f64, 0.5, 2.0, 0.0);
            let scale = gtk::Scale::builder()
                .orientation(gtk::Orientation::Vertical)
                .adjustment(&adjustment)
                .width_request(40)
                .draw_value(false)
                .hexpand(true)
                .vexpand(true)
                .build();
            let label = gtk::Label::builder()
                .label(format!("{:.0}", frequency))
                .build();
            let band = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(2)
                .vexpand(true)
                .build();
            band.append(&scale);
            band.append(&label);
            bands_row.append(&band);
            sliders.push(scale);
        }
        root.append(&bands_row);

        let widget = Self {
            root,
            sliders,
            enabled: Rc::new(Cell::new(true)),
            preset_names,
            command_tx,
        };
        widget.wire(&enable_switch, &combo);
        widget
    }

    /// Returns the widget to place in the layout panel.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// Equalizer state is pushed from this widget, not from playback events.
    pub fn update(&self, _event: &PlayerEvent) {}

    /// Connects every control to a [`PlayerCommand`].
    fn wire(&self, enable_switch: &gtk::Switch, combo: &gtk::DropDown) {
        let tx = self.command_tx.clone();
        let enabled = self.enabled.clone();
        let sliders = self.sliders.clone();
        enable_switch.connect_state_notify(clone!(
            #[strong]
            tx,
            #[strong]
            enabled,
            #[strong]
            sliders,
            move |switch| {
                let active = switch.state();
                enabled.set(active);
                for slider in &sliders {
                    slider.set_sensitive(active);
                }
                let _ = tx.blocking_send(PlayerCommand::SetEqualizerEnabled(active));
            }
        ));

        let tx = self.command_tx.clone();
        let preset_names = self.preset_names.clone();
        let sliders = self.sliders.clone();
        let all_presets = presets();
        combo.connect_selected_notify(clone!(
            #[strong]
            tx,
            #[strong]
            preset_names,
            #[strong]
            sliders,
            #[strong]
            all_presets,
            move |combo| {
                let index = combo.selected() as usize;
                if let Some(preset) = all_presets.get(index) {
                    for (slider, gain) in sliders.iter().zip(preset.gains) {
                        slider.set_value(f64::from(gain));
                    }
                }
                if let Some(name) = preset_names.get(index) {
                    let _ = tx.blocking_send(PlayerCommand::SetEqualizerPreset(name.clone()));
                }
            }
        ));

        for (index, slider) in self.sliders.iter().enumerate() {
            let tx = self.command_tx.clone();
            slider.connect_change_value(clone!(
                #[strong]
                tx,
                move |_, _, value| {
                    let _ = tx.blocking_send(PlayerCommand::SetBand {
                        index,
                        gain_db: value as f32,
                    });
                    glib::Propagation::Proceed
                }
            ));
        }
    }
}
