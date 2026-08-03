//! First-run onboarding window: welcome, music-folder selection and quick
//! settings before the main player screen appears.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gio::prelude::*;
use glib::clone;
use gtk::prelude::*;

use crate::audio::PlayerCommand;
use crate::config::expand_path;
use crate::config::schema::{Config, ThemeMode};
use crate::ui::UiContext;

/// Default music folder used when the user does not pick one.
pub const DEFAULT_MUSIC_FOLDER: &str = "~/Music";

/// Theme modes selectable in the onboarding dropdown.
const THEME_MODES: [ThemeMode; 3] = [ThemeMode::Dynamic, ThemeMode::Catppuccin, ThemeMode::Custom];

/// Callback invoked once onboarding completes, on the GTK main thread.
type DoneCallback = Box<dyn FnOnce()>;

/// The first-run welcome window.
pub struct Onboarding {
    window: gtk::ApplicationWindow,
    folder_label: gtk::Label,
    config: Rc<RefCell<Config>>,
    on_done: Rc<RefCell<Option<DoneCallback>>>,
}

impl Onboarding {
    /// Builds the onboarding window. `on_done` runs once on the main thread
    /// after the user finishes and the config is persisted.
    pub fn new(app: &adw::Application, ctx: &UiContext, on_done: Box<dyn FnOnce()>) -> Rc<Self> {
        let config = ctx.config.clone();
        let command_tx = ctx.command_tx.clone();

        let welcome_title = gtk::Label::builder()
            .label("Chromia")
            .css_classes(vec!["chromia-welcome-title"])
            .build();
        let welcome_subtitle = gtk::Label::builder()
            .label("Музыкальный плеер с динамическими цветами")
            .css_classes(vec!["chromia-welcome-subtitle"])
            .build();

        let folder_heading = gtk::Label::builder()
            .label("Папка с музыкой")
            .css_classes(vec!["chromia-header"])
            .halign(gtk::Align::Start)
            .build();
        let initial_folder = config
            .borrow()
            .sources
            .local
            .paths
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_MUSIC_FOLDER));
        let folder_label = gtk::Label::builder()
            .label(expand_path(&initial_folder).display().to_string())
            .css_classes(vec!["chromia-folder"])
            .halign(gtk::Align::Start)
            .build();
        let browse_button = gtk::Button::builder()
            .label("Обзор…")
            .tooltip_text("Выбрать папку с музыкой")
            .build();
        let default_button = gtk::Button::builder()
            .label("По умолчанию (~/Music)")
            .tooltip_text("Использовать стандартную папку")
            .build();
        let folder_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        folder_row.append(&browse_button);
        folder_row.append(&default_button);

        let folder_column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        folder_column.append(&folder_heading);
        folder_column.append(&folder_label);
        folder_column.append(&folder_row);

        let settings_expander = build_settings_expander(&config, &command_tx);

        let start_button = gtk::Button::builder()
            .label("Начать")
            .css_classes(vec!["suggested-action"])
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-onboarding"])
            .spacing(16)
            .margin_start(48)
            .margin_end(48)
            .margin_top(40)
            .margin_bottom(40)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();
        content.append(&welcome_title);
        content.append(&welcome_subtitle);
        content.append(&folder_column);
        content.append(&settings_expander);
        content.append(&start_button);

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Добро пожаловать в Chromia")
            .default_width(520)
            .default_height(520)
            .child(&content)
            .build();

        let theme = ctx.config.borrow().theme.clone();
        let palette = crate::ui::window::resolve_initial_palette(&theme);
        if let Err(err) = crate::theme::css::apply_css(&crate::theme::css::full_css(&palette)) {
            tracing::warn!(error = %err, "could not apply theme css");
        }

        let onboarding = Rc::new(Self {
            window,
            folder_label,
            config,
            on_done: Rc::new(RefCell::new(Some(on_done))),
        });

        onboarding.wire(&browse_button, &default_button, &start_button);
        onboarding
    }

    /// Shows the onboarding window.
    pub fn present(&self) {
        self.window.present();
    }

    /// Sets the music folder in the config and on the label.
    fn set_folder(&self, path: &std::path::Path) {
        self.config.borrow_mut().sources.local.paths = vec![path.to_path_buf()];
        self.folder_label
            .set_text(&expand_path(path).display().to_string());
    }

    /// Wires folder buttons and the start action.
    fn wire(self: &Rc<Self>, browse: &gtk::Button, default: &gtk::Button, start: &gtk::Button) {
        let config = self.config.clone();
        let folder_label = self.folder_label.clone();
        let window = self.window.clone();
        browse.connect_clicked(clone!(
            #[strong]
            config,
            #[strong]
            folder_label,
            #[strong]
            window,
            move |_| {
                let dialog = gtk::FileDialog::builder()
                    .title("Выберите папку с музыкой")
                    .build();
                let config = config.clone();
                let folder_label = folder_label.clone();
                dialog.select_folder(Some(&window), None::<&gio::Cancellable>, move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            config.borrow_mut().sources.local.paths = vec![path.clone()];
                            folder_label.set_text(&path.display().to_string());
                        }
                    }
                });
            }
        ));

        let onboarding = Rc::downgrade(self);
        default.connect_clicked(clone!(
            #[strong]
            onboarding,
            move |_| {
                if let Some(onboarding) = onboarding.upgrade() {
                    onboarding.set_folder(&PathBuf::from(DEFAULT_MUSIC_FOLDER));
                }
            }
        ));

        let config = self.config.clone();
        let on_done = self.on_done.clone();
        let window = self.window.clone();
        start.connect_clicked(clone!(
            #[strong]
            config,
            #[strong]
            on_done,
            #[strong]
            window,
            move |_| {
                config.borrow_mut().first_run = false;
                if let Err(err) = config.borrow().save() {
                    tracing::warn!(error = %err, "could not save configuration");
                }
                window.close();
                if let Some(callback) = on_done.borrow_mut().take() {
                    callback();
                }
            }
        ));
    }
}

/// Builds the collapsible quick-settings section.
fn build_settings_expander(
    config: &Rc<RefCell<Config>>,
    command_tx: &tokio::sync::mpsc::Sender<PlayerCommand>,
) -> gtk::Expander {
    let theme_label = gtk::Label::builder()
        .label("Тема")
        .halign(gtk::Align::Start)
        .build();
    let theme_names = ["Динамическая", "Catppuccin", "Своя"];
    let theme_dropdown = gtk::DropDown::from_strings(&theme_names);
    let initial = match config.borrow().theme.mode {
        ThemeMode::Dynamic => 0,
        ThemeMode::Catppuccin => 1,
        ThemeMode::Preset => 0,
        ThemeMode::Custom => 2,
    };
    theme_dropdown.set_selected(initial);
    let theme_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    theme_row.append(&theme_label);
    theme_row.append(&theme_dropdown);

    let volume_label = gtk::Label::builder()
        .label("Громкость")
        .halign(gtk::Align::Start)
        .build();
    let volume = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&gtk::Adjustment::new(
            f64::from(config.borrow().audio.volume.clamp(0.0, 1.0)),
            0.0,
            1.0,
            0.01,
            0.1,
            0.0,
        ))
        .width_request(200)
        .draw_value(false)
        .build();
    let volume_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    volume_row.append(&volume_label);
    volume_row.append(&volume);

    let mpris_switch = gtk::Switch::builder()
        .active(config.borrow().integrations.mpris)
        .build();
    let discord_switch = gtk::Switch::builder()
        .active(config.borrow().integrations.discord)
        .build();
    let integration_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(16)
        .build();
    let mpris_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    mpris_box.append(&gtk::Label::new(Some("MPRIS")));
    mpris_box.append(&mpris_switch);
    let discord_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    discord_box.append(&gtk::Label::new(Some("Discord")));
    discord_box.append(&discord_switch);
    integration_row.append(&mpris_box);
    integration_row.append(&discord_box);

    let settings_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .hexpand(true)
        .build();
    settings_box.append(&theme_row);
    settings_box.append(&volume_row);
    settings_box.append(&integration_row);

    let expander = gtk::Expander::builder().label("Настройки").build();
    expander.set_child(Some(&settings_box));

    let config = config.clone();
    theme_dropdown.connect_selected_notify(clone!(
        #[strong]
        config,
        move |dropdown| {
            if let Some(mode) = THEME_MODES.get(dropdown.selected() as usize) {
                config.borrow_mut().theme.mode = *mode;
            }
        }
    ));

    let config = config.clone();
    let command_tx = command_tx.clone();
    volume.connect_change_value(clone!(
        #[strong]
        config,
        #[strong]
        command_tx,
        move |_, _, value| {
            let volume = value.clamp(0.0, 1.0) as f32;
            config.borrow_mut().audio.volume = volume;
            let _ = command_tx.blocking_send(PlayerCommand::SetVolume(volume));
            glib::Propagation::Proceed
        }
    ));

    let config = config.clone();
    mpris_switch.connect_state_notify(clone!(
        #[strong]
        config,
        move |switch| config.borrow_mut().integrations.mpris = switch.state()
    ));
    discord_switch.connect_state_notify(clone!(
        #[strong]
        config,
        move |switch| config.borrow_mut().integrations.discord = switch.state()
    ));

    expander
}
