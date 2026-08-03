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
//!
//! **Persistent slot containers:** each implemented slot owns a [`SlotContainer`]
//! whose `wrapper` is created once. The hosted widget is appended to its
//! wrapper exactly once and never re-parented; `rebuild` only re-appends the
//! (distinct) wrappers inside the persistent `content` box and toggles the
//! edit-mode headers. This avoids the `gtk_box_append` parent assertion that
//! rebuilding freshly-constructed wrappers around live singletons produced.

use std::cell::RefCell;
use std::rc::Rc;

use glib::clone;
use gtk::gdk::ContentProvider;
use gtk::prelude::*;

use crate::audio::PlayerEvent;
use crate::config::schema::{AppearanceConfig, Config};
use crate::ui::layout::slots::{default_slots, parse_slots, SlotWidget};
use crate::ui::widgets::album_art::AlbumArt;
use crate::ui::widgets::audio_info::AudioInfo;
use crate::ui::widgets::equalizer::EqualizerWidget;
use crate::ui::widgets::lyrics::Lyrics;
use crate::ui::widgets::queue::Queue as QueueWidget;
use crate::ui::widgets::visualizer::Visualizer;
use crate::ui::UiContext;

/// String payload shipped through the drag-and-drop channel.
type SlotPayload = String;

/// Shared, mutable state used by every slot wrapper so the drop handler can
/// trigger a rebuild without owning `RightPanel` itself.
struct SharedState {
    root: gtk::Box,
    /// Persistent container holding the slot wrappers. The transient rows
    /// (edit toggle / palette) are appended after it and rebuilt on demand,
    /// so `rebuild` never has to touch `root`'s persistent children.
    content: gtk::Box,
    slots: RefCell<Vec<SlotWidget>>,
    /// One container per implemented slot, built once in `new` (and on demand
    /// for palette additions). The hosted widget never changes parent.
    containers: RefCell<Vec<SlotContainer>>,
    edit_mode: RefCell<bool>,
    config: Rc<RefCell<Config>>,
    album_art: AlbumArt,
    lyrics: Lyrics,
    queue: QueueWidget,
    equalizer: EqualizerWidget,
    audio_info: AudioInfo,
    visualizer: Visualizer,
}

/// A right-panel slot's persistent wrapper.
///
/// The `header` (drag handle + title) is hidden in normal mode and shown in
/// edit mode; the `widget` is appended to `wrapper` exactly once and never
/// re-parented, which keeps the panel safe to rebuild repeatedly.
#[derive(Clone)]
struct SlotContainer {
    slot: SlotWidget,
    wrapper: gtk::Box,
    header: gtk::Box,
    /// The hosted widget root, kept as a reference handle so the container
    /// owns its lifetime without ever re-parenting it.
    #[allow(dead_code)]
    widget: gtk::Widget,
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

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-right-panel"])
            .spacing(0)
            .hexpand(true)
            .vexpand(true)
            .build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();
        root.append(&content);

        let state = Rc::new(SharedState {
            root,
            content,
            slots: RefCell::new(slots),
            containers: RefCell::new(Vec::new()),
            edit_mode: RefCell::new(ctx.config.borrow().appearance.edit_mode),
            config: ctx.config.clone(),
            album_art: AlbumArt::new(ctx),
            lyrics: Lyrics::new(ctx),
            queue: QueueWidget::new(ctx),
            equalizer: EqualizerWidget::new(ctx),
            audio_info: AudioInfo::new(ctx),
            visualizer: Visualizer::new(ctx),
        });

        // Build one persistent container per implemented slot, in the
        // configured order. Unimplemented slots are skipped just like the
        // original rebuild did.
        for slot in state.slots.borrow().iter().copied() {
            ensure_container(&state, slot);
        }

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
        sync_container_order(&self.state);
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

/// Returns the root widget for a slot, or `None` when the widget is not yet
/// implemented. Mirrors the match the original rebuild performed inline.
fn slot_root(state: &SharedState, slot: SlotWidget) -> Option<gtk::Widget> {
    match slot {
        SlotWidget::AlbumArt => Some(state.album_art.root().upcast()),
        SlotWidget::Lyrics => Some(state.lyrics.root().upcast()),
        SlotWidget::Queue => Some(state.queue.root().upcast()),
        SlotWidget::Equalizer => Some(state.equalizer.root().upcast()),
        SlotWidget::Visualizer => Some(state.visualizer.root().upcast()),
        // Future slots fall through silently — see CHROMIA.md roadmap.
        _ => None,
    }
}

/// Ensures a slot has a persistent [`SlotContainer`], building one on demand
/// (e.g. for palette additions). Unimplemented slots get no container.
fn ensure_container(state: &Rc<SharedState>, slot: SlotWidget) {
    {
        let containers = state.containers.borrow();
        if containers.iter().any(|c| c.slot == slot) {
            return;
        }
    }

    let Some(widget) = slot_root(state, slot) else {
        return;
    };

    let wrapper = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(vec!["chromia-slot"])
        .build();
    let header = build_slot_header(slot);
    // Headers only matter in edit mode; keep them hidden until then so the
    // normal panel renders exactly as before.
    header.set_visible(*state.edit_mode.borrow());
    wrapper.append(&header);
    wrapper.append(&widget);

    // Wire drag-and-drop once for the lifetime of the container. The
    // handlers themselves check `edit_mode`, so nothing is draggable or
    // droppable in normal mode.
    wire_drag_source(&wrapper, slot, state.clone());
    wire_drop_target(&wrapper, slot, state.clone());

    state.containers.borrow_mut().push(SlotContainer {
        slot,
        wrapper,
        header,
        widget,
    });
}

/// Rebuilds the panel's transient rows and re-orders the slot wrappers.
///
/// Slot wrappers are re-parented inside the persistent `content` box, but the
/// hosted widget instances never leave their wrapper. Toggling edit mode on
/// and off repeatedly therefore never hits the `gtk_box_append: assertion
/// 'gtk_widget_get_parent (child) == NULL'` failure the old per-rebuild
/// wrapper construction produced.
fn rebuild(state: &Rc<SharedState>) {
    // Remove only the transient rows from `root`; the persistent `content`
    // box (its first child) is kept so the wrappers inside it keep their
    // parentage.
    let content_widget = state.content.clone().upcast::<gtk::Widget>();
    let mut transient: Vec<gtk::Widget> = Vec::new();
    {
        let mut child = state.root.first_child();
        while let Some(c) = child {
            if c != content_widget {
                transient.push(c.clone());
            }
            child = c.next_sibling();
        }
    }
    for widget in transient {
        state.root.remove(&widget);
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

    let edit_mode = *state.edit_mode.borrow();

    // Layout-editor palette — only in edit mode. Offers the full widget
    // catalogue so the user can append any slot, not just reorder the ones
    // that exist by default.
    if edit_mode {
        state.root.append(&palette_row(state));
    }

    // Re-append the slot wrappers into `content` in the current order.
    // Moving a wrapper is safe: each wrapper is a distinct box whose inner
    // widget never gets re-parented.
    let content = &state.content;
    while let Some(child) = content.first_child() {
        content.remove(&child);
    }
    let slots = state.slots.borrow().clone();
    let containers = state.containers.borrow();
    for slot in slots {
        if let Some(container) = containers.iter().find(|c| c.slot == slot) {
            container.header.set_visible(edit_mode);
            content.append(&container.wrapper);
        }
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
        // Build the container on demand for newly added implemented slots.
        ensure_container(&state_for_add, widget);
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

/// Reorders the slot list after a drop, mirroring the container list so
/// `rebuild` renders both in the same sequence.
fn reorder_slots(state: &Rc<SharedState>, dragged: SlotWidget, target: SlotWidget) {
    {
        let mut slots = state.slots.borrow_mut();
        if let Some(pos) = slots.iter().position(|s| *s == dragged) {
            slots.remove(pos);
        }
        let target_index = slots
            .iter()
            .position(|s| *s == target)
            .unwrap_or(slots.len());
        slots.insert(target_index, dragged);
    }
    sync_container_order(state);
}

/// Rebuilds `containers` to match the current `slots` order, keeping each
/// container's widgets (and their parentage) untouched.
fn sync_container_order(state: &Rc<SharedState>) {
    let mut pool = state.containers.borrow().clone();
    let mut ordered = Vec::with_capacity(pool.len());
    for slot in state.slots.borrow().iter() {
        if let Some(pos) = pool.iter().position(|c| &c.slot == slot) {
            ordered.push(pool.remove(pos));
        }
    }
    *state.containers.borrow_mut() = ordered;
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
///
/// The source is attached once; `connect_prepare` refuses to start a drag
/// unless the panel is in edit mode.
fn wire_drag_source(wrapper: &gtk::Box, slot: SlotWidget, state: Rc<SharedState>) {
    let source = gtk::DragSource::builder()
        .name("chromia-slot-drag")
        .actions(gtk::gdk::DragAction::MOVE)
        .build();
    let provider = slot_content_provider(slot);
    let state_for_prepare = state.clone();
    source.connect_prepare(move |_, _, _| {
        if *state_for_prepare.edit_mode.borrow() {
            Some(provider.clone())
        } else {
            None
        }
    });

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
///
/// The target is attached once; it only reacts while the panel is in edit
/// mode.
fn wire_drop_target(wrapper: &gtk::Box, slot: SlotWidget, state: Rc<SharedState>) {
    let target = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::MOVE);
    let wrapper_clone = wrapper.clone();
    let state_for_enter = state.clone();
    target.connect_enter(move |_, _, _| {
        if !*state_for_enter.edit_mode.borrow() {
            return gtk::gdk::DragAction::empty();
        }
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
        if !*state.edit_mode.borrow() {
            return false;
        }
        let Some(name) = value.get::<SlotPayload>().ok() else {
            return false;
        };
        let Some(dragged) = SlotWidget::from_str(&name) else {
            return false;
        };
        // Reorder both the slot list and the container list, then rebuild.
        reorder_slots(&state, dragged, slot);
        persist_slots(&state);
        // Schedule the rebuild for the next idle tick so the current event
        // dispatch finishes before the wrappers are re-parented.
        let state = state.clone();
        glib::idle_add_local_once(move || {
            rebuild(&state);
        });
        true
    });
    wrapper.add_controller(target);
}
