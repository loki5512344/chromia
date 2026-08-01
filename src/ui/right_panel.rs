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

use std::cell::RefCell;
use std::rc::Rc;

use glib::clone;
use gtk::gdk::ContentProvider;
use gtk::prelude::*;

use crate::audio::PlayerEvent;
use crate::config::schema::AppearanceConfig;
use crate::ui::UiContext;
use crate::ui::layout::slots::{SlotWidget, default_slots};
use crate::ui::widgets::album_art::AlbumArt;
use crate::ui::widgets::equalizer::EqualizerWidget;
use crate::ui::widgets::lyrics::Lyrics;
use crate::ui::widgets::queue::Queue as QueueWidget;

/// String payload shipped through the drag-and-drop channel.
type SlotPayload = String;

/// Shared, mutable state used by every slot wrapper so the drop handler can
/// trigger a rebuild without owning `RightPanel` itself.
struct SharedState {
    root: gtk::Box,
    slots: RefCell<Vec<SlotWidget>>,
    edit_mode: RefCell<bool>,
    album_art: AlbumArt,
    lyrics: Lyrics,
    queue: QueueWidget,
    equalizer: EqualizerWidget,
}

/// The right-panel container hosting the customizable vertical slot stack.
pub struct RightPanel {
    state: Rc<SharedState>,
}

impl RightPanel {
    /// Builds the right panel using the layout stored in `ctx.config`.
    pub fn new(ctx: &UiContext) -> Self {
        let state = Rc::new(SharedState {
            root: gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .css_classes(vec!["chromia-right-panel"])
                .spacing(0)
                .hexpand(true)
                .vexpand(true)
                .build(),
            slots: RefCell::new(default_slots()),
            edit_mode: RefCell::new(ctx.config.borrow().appearance.edit_mode),
            album_art: AlbumArt::new(ctx),
            lyrics: Lyrics::new(ctx),
            queue: QueueWidget::new(ctx),
            equalizer: EqualizerWidget::new(ctx),
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

    /// Replaces the slot order and rebuilds the panel.
    #[allow(dead_code)] // TODO(loki): consumed by the config sync layer
    pub fn set_slots(&self, slots: Vec<SlotWidget>) {
        *self.state.slots.borrow_mut() = slots;
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

    let slots = state.slots.borrow().clone();
    let edit_mode = *state.edit_mode.borrow();
    for slot in slots {
        let widget = match slot {
            SlotWidget::AlbumArt => state.album_art.root(),
            SlotWidget::Lyrics => state.lyrics.root(),
            SlotWidget::Queue => state.queue.root(),
            SlotWidget::Equalizer => state.equalizer.root(),
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
