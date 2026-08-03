//! Right Panel — the customizable vertical container on the right side of
//! the window.
//!
//! Iteration 2 adds drag-and-drop slot reordering: each slot registers a
//! `gtk::DragSource` and a `gtk::DropTarget`, both keyed to a string
//! payload (`SlotWidget::as_str`). When the user drags a slot onto another,
//! the panel reorders the underlying `Vec<SlotWidget>` and rebuilds the
//! widget tree on the next idle tick (so we don't destroy widgets mid-event).
//!
//! The DnD flow is gated by `appearance.edit_mode` so it never gets in the
//! way of normal listening. When `edit_mode` is `false` the slots behave
//! exactly like in iteration 1.
//!
//! **Config sync (v1.1):** the panel is now a first-class consumer of the
//! `[layout.right_panel]` config section:
//!
//! - The initial slot order comes from `config.layout.right_panel.slots`
//!   (falls back to [`default_slots`] when empty / not configured).
//! - The `Edit layout` toggle reads and writes `config.appearance.edit_mode`.
//! - In edit mode a palette row lets the user append widgets from the full
//!   [`SlotWidget::all`] catalogue.
//! - Reordering via drag-and-drop and palette additions are persisted back to
//!   `config.layout.right_panel.slots`, so layout survives a restart.

use std::cell::RefCell;
use std::rc::Rc;

use glib::clone;
use gtk::gdk::ContentProvider;
use gtk::prelude::*;

use crate::audio::PlayerEvent;
use crate::config::schema::{AppearanceConfig, Config};
use crate::ui::UiContext;
use crate::ui::layout::slots::{SlotWidget, default_slots, parse_slots};
use crate::ui::widgets::album_art::AlbumArt;
use crate::ui::widgets::audio_info::AudioInfo;
use crate::ui::widgets::equalizer::EqualizerWidget;
use crate::ui::widgets::lyrics::Lyrics;
use crate::ui::widgets::queue::Queue as QueueWidget;
use crate::ui::widgets::visualizer::Visualizer;

/// String payload shipped through the drag-and-drop channel.
type SlotPayload = String;

/// Shared, mutable state used by every slot wrapper so the drop handler can
/// trigger a rebuild without owning `RightPanel` itself.
struct SharedState {
    root: gtk::Box,
    slots: RefCell<Vec<SlotWidget>>,
    edit_mode: RefCell<bool>,
    config: Rc<RefCell<Config>>,
    album_art: AlbumArt,
    lyrics: Lyrics,
    queue: QueueWidget,
    equalizer: EqualizerWidget,
    audio_info: AudioInfo,
    visualizer: Visualizer,
}

/// The right-panel container hosting the customizable vertical slot stack.
pub struct RightPanel {
    state: Rc<SharedState>,
}

impl RightPanel {
    /// Builds the right panel using the layout stored in `ctx.config`.
    pub fn new(ctx: &UiContext) -> Self {
        // The slot order is user-configurable via `[layout.right_panel]`.
        // Unknown / empty lists fall back to the curated defaults so the
        // panel always renders something useful.
        let configured = parse_slots(&ctx.config.borrow().layout.right_panel.slots.clone());
        let slots = if configured.is_empty() {
            default_slots()
        } else {
            configured
        };

        let state = Rc::new(SharedState {
            root: gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .css_classes(vec!["chromia-right-panel"])
                .spacing(0)
                .hexpand(true)
                .vexpand(true)
                .build(),
            slots: RefCell::new(slots),
            edit_mode: RefCell::new(ctx.config.borrow().appearance.edit_mode),
            config: ctx.config.clone(),
            album_art: AlbumArt::new(ctx),
            lyrics: Lyrics::new(ctx),
            queue: QueueWidget::new(ctx),
            equalizer: EqualizerWidget::new(ctx),
            audio_info: AudioInfo::new(ctx),
            visualizer: Visualizer::new(ctx),
        });

        rebuild(&state);

        Self { state }
    }

    /// Returns the widget to embed in the window.
    pub fn root(&self) -> gtk::Box {
        self.state.root.clone()
    }

    /// Returns the current slot order.
    #[allow(dead_code)] // TODO(loki): consumed by the config sync layer
    pub fn slots(&self) -> Vec<SlotWidget> {
        self.state.slots.borrow().clone()
    }

    /// Replaces the slot order and rebuilds the panel, persisting the change
    /// back to `[layout.right_panel]`. Reserved for programmatic layout
    /// changes (the layout editor uses DnD + the palette instead).
    #[allow(dead_code)]
    pub fn set_slots(&self, slots: Vec<SlotWidget>) {
        *self.state.slots.borrow_mut() = slots;
        persist_slots(&self.state);
        rebuild(&self.state);
    }

    /// Toggles the layout editor. When enabled, each slot gains a header
    /// with a drag handle and accepts drops from other slots.
    #[allow(dead_code)] // TODO(loki): consumed by the layout editor
    pub fn set_edit_mode(&self, enabled: bool) {
        *self.state.edit_mode.borrow_mut() = enabled;
        rebuild(&self.state);
    }

    /// Returns `true` when the panel is in edit mode (slot reordering
    /// allowed).
    #[allow(dead_code)] // TODO(loki): consumed by the layout editor
    pub fn is_edit_mode(&self) -> bool {
        *self.state.edit_mode.borrow()
    }

    /// Syncs the panel state from the appearance config.
    #[allow(dead_code)] // TODO(loki): consumed by the config sync layer
    pub fn apply_appearance(&self, appearance: &AppearanceConfig) {
        let changed = *self.state.edit_mode.borrow() != appearance.edit_mode;
        if changed {
            self.set_edit_mode(appearance.edit_mode);
        }
    }

    /// Forwards a playback event to every hosted widget.
    pub fn update(&self, event: &PlayerEvent) {
        self.state.album_art.update(event);
        self.state.lyrics.update(event);
        self.state.queue.update(event);
        self.state.equalizer.update(event);
        self.state.audio_info.update(event);
    }
}

/// Clears the panel and re-appends every slot in the current order.
///
/// Slots that are not yet implemented (`Visualizer`, `AudioInfo`, etc.)
/// are skipped — the panel keeps rendering the ones that have a builder
/// today, so the right panel is always usable.
fn rebuild(state: &Rc<SharedState>) {
    while let Some(child) = state.root.first_child() {
        state.root.remove(&child);
    }

    // Edit-mode toggle button — always visible at the top of the right panel
    // so the user can switch into the layout editor without touching the
    // config file.
    {
        let toggle = gtk::ToggleButton::builder()
            .label("Edit layout")
            .tooltip_text("Toggle the layout editor (drag-and-drop slots)")
            .css_classes(vec!["chromia-edit-toggle"])
            .active(*state.edit_mode.borrow())
            .build();
        let state_for_toggle = state.clone();
        toggle.connect_clicked(clone!(
            #[strong]
            state_for_toggle,
            move |btn| {
                let next = btn.is_active();
                *state_for_toggle.edit_mode.borrow_mut() = next;
                // Persist the toggle so it survives a restart.
                state_for_toggle.config.borrow_mut().appearance.edit_mode = next;
                persist_config(&state_for_toggle);
                rebuild(&state_for_toggle);
            }
        ));
        let toggle_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .halign(gtk::Align::End)
            .margin_bottom(6)
            .build();
        toggle_row.append(&toggle);
        state.root.append(&toggle_row);
    }

    // Layout-editor palette — only in edit mode. Offers the full widget
    // catalogue so the user can append any slot, not just reorder the ones
    // that exist by default.
    if *state.edit_mode.borrow() {
        state.root.append(&palette_row(state));
    }

    let slots = state.slots.borrow().clone();
    let edit_mode = *state.edit_mode.borrow();
    for slot in slots {
        let widget = match slot {
            SlotWidget::AlbumArt => state.album_art.root(),
            SlotWidget::Lyrics => state.lyrics.root(),
            SlotWidget::Queue => state.queue.root(),
            SlotWidget::Equalizer => state.equalizer.root(),
            SlotWidget::Visualizer => state.visualizer.root(),
            // Future slots fall through silently — see CHROMIA.md roadmap.
            _ => continue,
        };

        let wrapper = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-slot"])
            .build();

        if edit_mode {
            let header = build_slot_header(slot);
            wrapper.append(&header);
        }
        wrapper.append(&widget);

        // Wire drag-and-drop in edit mode only.
        if edit_mode {
            wire_drag_source(&wrapper, slot);
            wire_drop_target(&wrapper, slot, state.clone());
        }

        state.root.append(&wrapper);
    }
}

/// Builds the edit-mode palette row: a dropdown of every catalogued widget
/// plus an "Add" button that appends the chosen widget to the slot stack.
fn palette_row(state: &Rc<SharedState>) -> gtk::Box {
    static ADD_INDEX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    let names: Vec<&str> = SlotWidget::all().iter().map(|w| w.as_str()).collect();
    let dropdown = gtk::DropDown::from_strings(&names);
    dropdown.set_selected(
        ADD_INDEX
            .load(std::sync::atomic::Ordering::Relaxed)
            .min(names.len().saturating_sub(1)) as u32,
    );
    dropdown.add_css_class("chromia-palette-dropdown");

    let add = gtk::Button::builder()
        .label("Add widget")
        .css_classes(vec!["chromia-palette-add"])
        .build();

    let state_for_add = state.clone();
    let catalogue = dynamic_catalogue();
    let dropdown_in_click = dropdown.clone();
    add.connect_clicked(move |_| {
        let index = dropdown_in_click.selected() as usize;
        let Some(widget) = catalogue.get(index).copied() else {
            return;
        };
        ADD_INDEX.store(index, std::sync::atomic::Ordering::Relaxed);
        // Ignore duplicates: a widget can only occupy one slot.
        {
            let mut slots = state_for_add.slots.borrow_mut();
            if !slots.contains(&widget) {
                slots.push(widget);
            }
        }
        persist_slots(&state_for_add);
        rebuild(&state_for_add);
    });

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .css_classes(vec!["chromia-palette"])
        .margin_bottom(6)
        .build();
    row.append(&dropdown);
    row.append(&add);
    row
}

/// A stable catalogue snapshot order matching [`SlotWidget::all`].
fn dynamic_catalogue() -> Vec<SlotWidget> {
    SlotWidget::all().to_vec()
}

/// Persists the current slot order into `config.layout.right_panel.slots`.
fn persist_slots(state: &Rc<SharedState>) {
    let names: Vec<String> = state
        .slots
        .borrow()
        .iter()
        .map(|w| w.as_str().to_string())
        .collect();
    state.config.borrow_mut().layout.right_panel.slots = names;
    persist_config(state);
}

/// Writes the shared config back to disk, swallowing I/O errors.
fn persist_config(state: &Rc<SharedState>) {
    if let Err(err) = state.config.borrow().save() {
        tracing::warn!(error = %err, "could not persist layout config");
    }
}

/// Builds the slot header shown in edit mode (title + drag handle).
fn build_slot_header(slot: SlotWidget) -> gtk::Box {
    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(vec!["chromia-slot-header"])
        .spacing(6)
        .build();
    let handle = gtk::Image::builder()
        .icon_name("view-app-grid-symbolic")
        .css_classes(vec!["chromia-slot-handle"])
        .build();
    let title = gtk::Label::builder()
        .label(slot.as_str())
        .css_classes(vec!["chromia-slot-title"])
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();
    header.append(&handle);
    header.append(&title);
    header
}

/// Builds a `ContentProvider` for a slot drag.
fn slot_content_provider(slot: SlotWidget) -> ContentProvider {
    let value = slot.as_str().to_value();
    ContentProvider::for_value(&value)
}

/// Attaches a `DragSource` to a slot wrapper so the user can pick it up.
fn wire_drag_source(wrapper: &gtk::Box, slot: SlotWidget) {
    let source = gtk::DragSource::builder()
        .name("chromia-slot-drag")
        .actions(gtk::gdk::DragAction::MOVE)
        .build();
    let provider = slot_content_provider(slot);
    source.connect_prepare(move |_, _, _| Some(provider.clone()));

    let wrapper_clone = wrapper.clone();
    source.connect_drag_begin(clone!(
        #[weak]
        wrapper_clone,
        move |_, _| {
            wrapper_clone.add_css_class("dragging");
        }
    ));
    let wrapper_clone = wrapper.clone();
    source.connect_drag_end(clone!(
        #[weak]
        wrapper_clone,
        move |_, _, _| {
            wrapper_clone.remove_css_class("dragging");
        }
    ));
    wrapper.add_controller(source);
}

/// Attaches a `DropTarget` to a slot wrapper so the user can drop another
/// slot onto it. On drop, the slots vector is reordered and the panel is
/// rebuilt on the next idle tick.
fn wire_drop_target(wrapper: &gtk::Box, slot: SlotWidget, state: Rc<SharedState>) {
    let target = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::MOVE);
    let wrapper_clone = wrapper.clone();
    target.connect_enter(move |_, _, _| {
        wrapper_clone.add_css_class("drop-target");
        gtk::gdk::DragAction::MOVE
    });
    let wrapper_clone = wrapper.clone();
    target.connect_leave(clone!(
        #[weak]
        wrapper_clone,
        move |_| {
            wrapper_clone.remove_css_class("drop-target");
        }
    ));
    target.connect_drop(move |_, value, _, _| {
        let Some(name) = value.get::<SlotPayload>().ok() else {
            return false;
        };
        let Some(dragged) = SlotWidget::from_str(&name) else {
            return false;
        };
        // Reorder: remove dragged from old position, insert before the
        // target slot.
        {
            let mut current = state.slots.borrow_mut();
            if let Some(pos) = current.iter().position(|s| *s == dragged) {
                current.remove(pos);
            }
            let target_index = current
                .iter()
                .position(|s| *s == slot)
                .unwrap_or(current.len());
            current.insert(target_index, dragged);
        }
        persist_slots(&state);
        // Schedule the rebuild for the next idle tick so the current
        // event dispatch finishes before we destroy the wrapper that owns
        // this DropTarget.
        let state = state.clone();
        glib::idle_add_local_once(move || {
            rebuild(&state);
        });
        true
    });
    wrapper.add_controller(target);
}
