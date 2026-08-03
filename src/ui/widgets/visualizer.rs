//! Spectrum visualizer: draws vertical bars from the shared live spectrum the
//! audio engine's DSP source feeds. Self-animates via a periodic redraw.

use std::time::Duration;

use gtk::prelude::*;

use crate::audio::dsp::SPECTRUM_BINS;
use crate::ui::UiContext;

/// Bar count actually drawn (downsampled from the analysis bins).
const BARS: usize = 16;
/// How often the drawing is refreshed, in ms.
const TICK_MS: u64 = 33;

/// A self-animating bar visualizer for the current playback spectrum.
pub struct Visualizer {
    root: gtk::Box,
}

impl Visualizer {
    /// Builds the widget, wiring it to `ctx.spectrum` and starting its timer.
    pub fn new(ctx: &UiContext) -> Self {
        let spectrum = ctx.spectrum.clone();
        let area = gtk::DrawingArea::builder()
            .css_classes(vec!["chromia-visualizer"])
            .hexpand(true)
            .vexpand(true)
            .build();

        area.set_draw_func(move |_, cr, width, height| {
            let bins = spectrum.lock().snapshot();
            draw_bars(cr, width, height, &bins);
        });

        // Self-animate: request a redraw on a tight timer while alive.
        let area_tick = area.clone();
        glib::timeout_add_local(Duration::from_millis(TICK_MS), move || {
            if area_tick.is_visible() {
                area_tick.queue_draw();
            }
            glib::ControlFlow::Continue
        });

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["chromia-visualizer-shell"])
            .hexpand(true)
            .vexpand(true)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        root.append(&area);
        Self { root }
    }

    /// Returns the widget to embed in a slot.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }
}

/// Draws `BARS` vertical bars whose heights track the analysis bins.
fn draw_bars(cr: &gtk::cairo::Context, width: i32, height: i32, bins: &[f32]) {
    if width <= 0 || height <= 0 {
        return;
    }
    let w = width as f64;
    let h = height as f64;

    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    cr.paint().ok();

    let slot = w / (BARS as f64 * 1.4);
    let gap = slot * 0.4;
    let bar_w = slot - gap;
    let max = bins.iter().fold(0.0f32, |a, &b| a.max(b)).max(1.0);

    for i in 0..BARS {
        // Merge consecutive bins; take the average to smooth flicker.
        let start = i * SPECTRUM_BINS / BARS;
        let end = ((i + 1) * SPECTRUM_BINS / BARS).min(SPECTRUM_BINS);
        let mut acc = 0.0f32;
        for b in &bins[start..end] {
            acc += b;
        }
        let avg = acc / (end.saturating_sub(start).max(1) as f32);
        let norm = (avg / max).min(1.0);
        // Perceptual-ish rise so quiet music still shows bars.
        let level = (norm.sqrt() as f64).clamp(0.0, 1.0);
        let bar_h = 0.05 * h + 0.85 * h * level;
        let x = i as f64 * slot;
        let y = h - bar_h;
        cr.set_source_rgba(0.79, 0.65, 0.97, 0.9);
        cr.rectangle(x, y, bar_w, bar_h);
        cr.fill().expect("fill failed");
    }
}
