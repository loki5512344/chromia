//! Settings page - GUI for every user-facing knob in `Config`.
//!
//! Split into collapsible sections matching the TOML hierarchy:
//!
//! 1. **Appearance** - theme mode, Catppuccin flavor / accent, Glass UI
//! 2. **Audio** - volume
//! 3. **Sources** - local music path, YouTube quality
//! 4. **Integrations** - MPRIS2, Discord Rich Presence
#![allow(deprecated)] // ComboBoxText is deprecated since GTK 4.10 but is the
// most compact dropdown API for small option lists.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::audio::{PlayerCommand, PlayerEvent};
use crate::config::schema::{Config, Flavor, ThemeMode};
use crate::ui::UiContext;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn section_header(title: &str) -> gtk::Box {
    let label = gtk::Label::builder()
        .label(title)
        .css_classes(vec!["chromia-settings-section"])
        .halign(gtk::Align::Start)
        .build();
    let sep = gtk::Separator::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(vec!["chromia-settings-sep"])
        .build();
    let bx = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(16)
        .build();
    bx.append(&sep);
    bx.append(&label);
    bx
}

fn setting_row(label_text: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let label = gtk::Label::builder()
        .label(label_text)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .css_classes(vec!["chromia-settings-label"])
        .build();
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(16)
        .css_classes(vec!["chromia-settings-row"])
        .build();
    row.append(&label);
    row.append(control);
    row
}

fn combo(items: &[(&str, &str)], active_id: &str) -> gtk::ComboBoxText {
    let cb = gtk::ComboBoxText::new();
    for (id, label) in items {
        cb.append(Some(id), label);
    }
    cb.set_active_id(Some(active_id));
    cb.add_css_class("chromia-settings-combo");
    cb
}

// ─── Settings widget ──────────────────────────────────────────────────────────

pub struct Settings {
    root: gtk::Box,
    #[allow(dead_code)]
    config: Rc<RefCell<Config>>,
    #[allow(dead_code)]
    command_tx: tokio::sync::mpsc::Sender<PlayerCommand>,
}

impl Settings {
    pub fn new(ctx: &UiContext) -> Self {
        let config = ctx.config.clone();
        let command_tx = ctx.command_tx.clone();

        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(vec!["chromia-settings-content"])
            .margin_start(24)
            .margin_end(24)
            .margin_top(8)
            .margin_bottom(24)
            .build();

        // ── Appearance ────────────────────────────────────────────────────
        content.append(&section_header("Appearance"));

        let theme_mode = {
            let cfg = config.borrow();
            match cfg.theme.mode {
                ThemeMode::Dynamic => "dynamic",
                ThemeMode::Catppuccin => "catppuccin",
                ThemeMode::Custom => "custom",
            }
        };
        let theme_combo = combo(
            &[
                ("dynamic", "Dynamic (album art)"),
                ("catppuccin", "Catppuccin"),
                ("custom", "Custom"),
            ],
            theme_mode,
        );
        {
            let config = config.clone();
            theme_combo.connect_changed(move |cb| {
                let mode = match cb.active_id().as_deref() {
                    Some("dynamic") => ThemeMode::Dynamic,
                    Some("catppuccin") => ThemeMode::Catppuccin,
                    Some("custom") => ThemeMode::Custom,
                    _ => return,
                };
                config.borrow_mut().theme.mode = mode;
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("Theme mode", &theme_combo));

        let flavor_id = {
            let cfg = config.borrow();
            match cfg.theme.catppuccin.flavor {
                Flavor::Mocha => "mocha",
                Flavor::Macchiato => "macchiato",
                Flavor::Frappe => "frappe",
                Flavor::Latte => "latte",
            }
        };
        let flavor_combo = combo(
            &[
                ("mocha", "Mocha"),
                ("macchiato", "Macchiato"),
                ("frappe", "Frappé"),
                ("latte", "Latte"),
            ],
            flavor_id,
        );
        {
            let config = config.clone();
            flavor_combo.connect_changed(move |cb| {
                let flavor = match cb.active_id().as_deref() {
                    Some("mocha") => Flavor::Mocha,
                    Some("macchiato") => Flavor::Macchiato,
                    Some("frappe") => Flavor::Frappe,
                    Some("latte") => Flavor::Latte,
                    _ => return,
                };
                config.borrow_mut().theme.catppuccin.flavor = flavor;
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("Catppuccin flavor", &flavor_combo));

        let accent_id = config.borrow().theme.catppuccin.accent.clone();
        let accent_combo = combo(
            &[
                ("rosewater", "Rosewater"),
                ("flamingo", "Flamingo"),
                ("pink", "Pink"),
                ("mauve", "Mauve"),
                ("red", "Red"),
                ("maroon", "Maroon"),
                ("peach", "Peach"),
                ("yellow", "Yellow"),
                ("green", "Green"),
                ("teal", "Teal"),
                ("sky", "Sky"),
                ("sapphire", "Sapphire"),
                ("blue", "Blue"),
                ("lavender", "Lavender"),
            ],
            &accent_id,
        );
        {
            let config = config.clone();
            accent_combo.connect_changed(move |cb| {
                if let Some(id) = cb.active_id() {
                    config.borrow_mut().theme.catppuccin.accent = id.to_string();
                    Self::save_config(&config);
                }
            });
        }
        content.append(&setting_row("Catppuccin accent", &accent_combo));

        let blur_switch = gtk::Switch::builder()
            .active(config.borrow().theme.blur_background)
            .valign(gtk::Align::Center)
            .build();
        {
            let config = config.clone();
            blur_switch.connect_active_notify(move |sw| {
                config.borrow_mut().theme.blur_background = sw.is_active();
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("Blur background", &blur_switch));

        let glass_switch = gtk::Switch::builder()
            .active(config.borrow().appearance.glass)
            .valign(gtk::Align::Center)
            .build();
        {
            let config = config.clone();
            glass_switch.connect_active_notify(move |sw| {
                config.borrow_mut().appearance.glass = sw.is_active();
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("Glass UI", &glass_switch));

        let anim_switch = gtk::Switch::builder()
            .active(config.borrow().appearance.animations)
            .valign(gtk::Align::Center)
            .build();
        {
            let config = config.clone();
            anim_switch.connect_active_notify(move |sw| {
                config.borrow_mut().appearance.animations = sw.is_active();
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("Animations", &anim_switch));

        // Transition duration (ms)
        let trans_val = config.borrow().theme.transition_ms as f64;
        let trans_adj = gtk::Adjustment::new(trans_val, 0.0, 1000.0, 50.0, 100.0, 0.0);
        let trans_scale = gtk::Scale::builder()
            .adjustment(&trans_adj)
            .orientation(gtk::Orientation::Horizontal)
            .draw_value(true)
            .width_request(180)
            .css_classes(vec!["chromia-settings-scale"])
            .build();
        trans_scale.set_format_value_func(|_, v| format!("{:.0} ms", v));
        {
            let config = config.clone();
            trans_adj.connect_value_changed(move |adj| {
                config.borrow_mut().theme.transition_ms = adj.value() as u32;
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("Color transition", &trans_scale));

        // ── Audio ─────────────────────────────────────────────────────────
        content.append(&section_header("Audio"));

        let vol_initial = (config.borrow().audio.volume.clamp(0.0, 1.0) * 100.0) as f64;
        let vol_adj = gtk::Adjustment::new(vol_initial, 0.0, 100.0, 1.0, 10.0, 0.0);
        let vol_scale = gtk::Scale::builder()
            .adjustment(&vol_adj)
            .orientation(gtk::Orientation::Horizontal)
            .draw_value(true)
            .width_request(180)
            .css_classes(vec!["chromia-settings-scale"])
            .build();
        vol_scale.set_format_value_func(|_, v| format!("{:.0}%", v));
        {
            let config = config.clone();
            let command_tx = command_tx.clone();
            vol_adj.connect_value_changed(move |adj| {
                let v = (adj.value() / 100.0) as f32;
                config.borrow_mut().audio.volume = v;
                let _ = command_tx.try_send(PlayerCommand::SetVolume(v));
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("Volume", &vol_scale));

        let rg_switch = gtk::Switch::builder()
            .active(config.borrow().audio.replaygain)
            .valign(gtk::Align::Center)
            .build();
        {
            let config = config.clone();
            rg_switch.connect_active_notify(move |sw| {
                config.borrow_mut().audio.replaygain = sw.is_active();
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("ReplayGain", &rg_switch));

        // ── Sources ───────────────────────────────────────────────────────
        content.append(&section_header("Sources"));

        let local_path = config
            .borrow()
            .sources
            .local
            .paths
            .first()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "~/Music".to_owned());
        let path_entry = gtk::Entry::builder()
            .text(&local_path)
            .hexpand(true)
            .placeholder_text("~/Music")
            .css_classes(vec!["chromia-settings-entry"])
            .build();
        {
            let config = config.clone();
            path_entry.connect_changed(move |entry| {
                let path = std::path::PathBuf::from(entry.text().as_str());
                config.borrow_mut().sources.local.paths = vec![path];
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("Music folder", &path_entry));

        let yt_quality = config.borrow().sources.youtube.quality.clone();
        let yt_combo = combo(
            &[
                ("best", "Best"),
                ("320k", "320 kbps"),
                ("256k", "256 kbps"),
                ("128k", "128 kbps"),
            ],
            &yt_quality,
        );
        {
            let config = config.clone();
            yt_combo.connect_changed(move |cb| {
                if let Some(id) = cb.active_id() {
                    config.borrow_mut().sources.youtube.quality = id.to_string();
                    Self::save_config(&config);
                }
            });
        }
        content.append(&setting_row("YouTube quality", &yt_combo));

        // ── Integrations ──────────────────────────────────────────────────
        content.append(&section_header("Integrations"));

        let mpris_switch = gtk::Switch::builder()
            .active(config.borrow().integrations.mpris)
            .valign(gtk::Align::Center)
            .build();
        {
            let config = config.clone();
            mpris_switch.connect_active_notify(move |sw| {
                config.borrow_mut().integrations.mpris = sw.is_active();
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("MPRIS2 (media keys, waybar)", &mpris_switch));

        let discord_switch = gtk::Switch::builder()
            .active(config.borrow().integrations.discord)
            .valign(gtk::Align::Center)
            .build();
        {
            let config = config.clone();
            discord_switch.connect_active_notify(move |sw| {
                config.borrow_mut().integrations.discord = sw.is_active();
                Self::save_config(&config);
            });
        }
        content.append(&setting_row("Discord Rich Presence", &discord_switch));

        // ── About ─────────────────────────────────────────────────────────
        content.append(&section_header("About"));
        let about_label = gtk::Label::builder()
            .label(concat!(
                "Chromia v",
                env!("CARGO_PKG_VERSION"),
                "  ·  GPL-3.0  ·  Rust + GTK4 + Libadwaita"
            ))
            .css_classes(vec!["chromia-settings-about"])
            .halign(gtk::Align::Start)
            .selectable(true)
            .build();
        content.append(&about_label);

        scroll.set_child(Some(&content));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-settings"])
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&scroll);

        Self {
            root,
            config,
            command_tx,
        }
    }

    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn update(&self, _event: &PlayerEvent) {}

    fn save_config(config: &Rc<RefCell<Config>>) {
        if let Err(err) = config.borrow().save() {
            tracing::warn!(error = %err, "could not persist config");
        }
    }
}
