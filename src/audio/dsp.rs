//! Real-time DSP: a biquad equalizer applied as a rodio `Source` wrapper, plus
//! a shared spectrum analyzer for the UI visualizer.
//!
//! * [`Biquad`] is a second-order RBJ peaking filter in transposed direct-form
//!   II, with per-channel state so stereo stays interleaved correctly.
//! * [`EqualizerSource`] wraps an `f32` rodio source and applies one biquad per
//!   band on every sample. Its coefficients are rebuilt from the live gains in
//!   a shared [`EqParams`], it feeds a shared [`Spectrum`], and applies a
//!   pre-gain (ReplayGain) plus an optional crossfade ramp at the head.
//!
//! All filtering uses `f32` linear samples; dB gains are converted through the
//! RBJ formulas.

use std::sync::Arc;

use rodio::Source;

use crate::audio::equalizer::{BAND_FREQUENCIES, BANDS, MAX_GAIN_DB, MIN_GAIN_DB};

/// Equalizer band Q used for the peaking filters.
const BAND_Q: f32 = 1.5;

// ─────────────────────────── Shared filter parameters ──────────────────

/// Live equalizer + pre-gain parameters shared between the engine and a
/// running [`EqualizerSource`]. Wrapped in a `parking_lot` mutex so the sink's
/// playback thread reads changes made from the engine thread on `next()`.
#[derive(Debug, Clone)]
pub struct EqParams {
    /// Whether the equalizer chain is active.
    pub enabled: bool,
    /// One gain (dB) per band.
    pub gains: [f32; BANDS],
    /// Linear (not dB) pre-gain used for ReplayGain, 1.0 = none.
    pub pre_gain: f32,
}

/// Shared, thread-safe handle to [`EqParams`].
pub type EqHandle = Arc<::parking_lot::Mutex<EqParams>>;

/// Creates a fresh, flat, disabled equalizer handle.
pub fn new_eq() -> EqHandle {
    Arc::new(::parking_lot::Mutex::new(EqParams {
        enabled: false,
        gains: [0.0; BANDS],
        pre_gain: 1.0,
    }))
}

// ─────────────────────────── Biquad filter ──────────────────────────────────

/// A second-order peaking filter in transposed direct-form II (RBJ cookbook).
/// Processes exactly one sample.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    /// Numerator (b0, b1, b2) and feedback (a1, a2) coefficients, normalised so
    /// `a0 == 1.0`, plus the two delay states.
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// Builds a peaking filter for `freq` (Hz), quality `q`, `gain_db` at
    /// `sample_rate` (Hz). A `gain_db` of 0 yields an allpass.
    pub fn peaking(freq: f32, q: f32, gain_db: f32, sample_rate: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * freq / sample_rate;
        let alpha = (w0 * 0.5).sin() * q;
        let cos_w0 = w0.cos();

        let a0 = 1.0 + alpha / a;
        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// An allpass identity filter — cheap way to bypass a band.
    pub fn bypass() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Applies the filter to one mono sample.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

// ─────────────────────────── equalizer source ───────────────────────────────

/// Wraps an `f32` source and applies the 10-band equalizer in real time.
///
/// Coefficients are rebuilt only when the shared gains change, so steady-state
/// playback does little work. A pre-gain (ReplayGain), a head-of-track crossfade
/// ramp, and the shared spectrum feed are all integrated here so the whole DSP
/// chain lives inside one Source.
pub struct EqualizerSource<S> {
    input: S,
    /// One biquad per band, per channel: `[band][channel]`.
    filters: Vec<Vec<Biquad>>,
    channels: u16,
    sample_rate: f32,
    params: EqHandle,
    /// Last gain snapshot used to detect coefficient changes.
    cache: EqParams,
    /// Samples since the last parameter refresh.
    tick: usize,
    /// Interleave cursor (which channel the next sample belongs to).
    ch: u16,
    /// Optional shared spectrum accumulator.
    spectrum: Option<Arc<::parking_lot::Mutex<Spectrum>>>,
    /// Remaining crossfade ramp samples; maps a 0..=1 envelope on the head.
    crossfade_left: usize,
    crossfade_total: usize,
}

impl<S> EqualizerSource<S>
where
    S: Source<Item = f32>,
{
    /// How often (in samples) the shared gains are re-read.
    const REFRESH: usize = 256;

    /// Wraps `input`, reflecting the shared [`EqHandle`] live. Builds the
    /// per-band filters at the source's sample rate. `spectrum` is an optional
    /// shared accumulator fed with mono samples on the first channel.
    pub fn new(
        input: S,
        eq: EqHandle,
        spectrum: Option<Arc<::parking_lot::Mutex<Spectrum>>>,
    ) -> Self {
        let channels = input.channels();
        let sample_rate = input.sample_rate();
        let cache = eq.lock().clone();
        let mut ready = Self {
            input,
            filters: Vec::new(),
            channels,
            sample_rate: sample_rate as f32,
            params: eq,
            cache,
            tick: 0,
            ch: 0,
            spectrum,
            crossfade_left: 0,
            crossfade_total: 0,
        };
        ready.rebuild_filters();
        ready
    }

    /// Enables a gentle head ramp of `seconds` (used for crossfade). Must be
    /// called before the source reaches the sink.
    pub fn crossfade_secs(&mut self, seconds: f32) {
        if seconds <= 0.0 {
            return;
        }
        let total = ((seconds * self.sample_rate) as u32).max(1) as usize;
        self.crossfade_total = total;
        self.crossfade_left = total;
    }

    /// Recomputes the per-band filters from the cached gains.
    fn rebuild_filters(&mut self) {
        let gains = self.cache.gains;
        let enabled = self.cache.enabled;
        self.filters.clear();
        for band in 0..BANDS {
            let gain_db = if enabled {
                gains[band].clamp(MIN_GAIN_DB, MAX_GAIN_DB)
            } else {
                0.0
            };
            let coeff = if gain_db == 0.0 {
                Biquad::bypass()
            } else {
                Biquad::peaking(BAND_FREQUENCIES[band], BAND_Q, gain_db, self.sample_rate)
            };
            let mut band_filters = Vec::with_capacity(self.channels as usize);
            for _ in 0..self.channels {
                band_filters.push(coeff);
            }
            self.filters.push(band_filters);
        }
    }

    /// Pulls one frame through every stage (pre-gain, biquads, head ramp,
    /// spectrum feed). Returns `None` at the end of the input.
    fn process_next(&mut self) -> Option<f32> {
        let mut x = self.input.next()?;
        let ch = self.ch as usize;

        // Refresh the coefficient snapshot periodically.
        self.tick += 1;
        if self.tick % Self::REFRESH == 0 {
            let fresh = self.params.lock().clone();
            if fresh.gains != self.cache.gains || fresh.enabled != self.cache.enabled {
                self.cache = fresh;
                self.rebuild_filters();
            }
        }

        // Apply pre-gain, then the per-channel equalizer chain.
        x *= self.cache.pre_gain;
        let chain_len = self.filters.len();
        for band in 0..chain_len {
            x = self.filters[band][ch].process(x);
        }

        // Crossfade ramp: a raised-cosine rise on the first `total` samples.
        if self.crossfade_left > 0 {
            let t = 1.0 - self.crossfade_left as f32 / self.crossfade_total as f32;
            let env = (std::f32::consts::PI * t).sin().max(0.0);
            x *= env;
            self.crossfade_left -= 1;
        }

        // Feed the spectrum on the first channel only.
        if ch == 0 {
            if let Some(spec) = self.spectrum.as_ref() {
                spec.lock().push(x.abs());
            }
        }

        self.ch += 1;
        if self.ch >= self.channels {
            self.ch = 0;
        }
        Some(x)
    }
}

impl<S> Iterator for EqualizerSource<S>
where
    S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.process_next()
    }
}

impl<S> Source for EqualizerSource<S>
where
    S: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.input.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        self.input.try_seek(pos)
    }
}

// ─────────────────────────── spectrum analyzer ──────────────────────────────

/// Number of bars shown by the visualizer.
pub const SPECTRUM_BINS: usize = 32;

/// Accumulates a sliding-window spectrum (magnitudes) from the playback sample
/// stream. The engine feeds mono amplitude; the UI reads bins on a timer.
pub struct Spectrum {
    bins: [f32; SPECTRUM_BINS],
    window: Vec<f32>,
    write: usize,
}

impl Spectrum {
    const FFT_SIZE: usize = 1 << 10;

    /// Creates an empty spectrum with a 1024-sample analysis window.
    pub fn new() -> Self {
        Self {
            bins: [0.0; SPECTRUM_BINS],
            window: vec![0.0; Self::FFT_SIZE],
            write: 0,
        }
    }

    /// Feeds one mono amplitude, running the analysis when the window fills.
    pub fn push(&mut self, mag: f32) {
        self.window[self.write % Self::FFT_SIZE] = mag;
        self.write += 1;
        if self.write % Self::FFT_SIZE == 0 {
            self.bins = Self::transform(&self.window);
            for b in self.bins.iter_mut() {
                if *b < 0.003 {
                    *b = 0.0;
                }
            }
        }
    }

    /// Clears accumulated energy (called on a track change).
    pub fn reset(&mut self) {
        self.window.fill(0.0);
        self.write = 0;
        self.bins.fill(0.0);
    }

    /// Snapshot of the current band magnitudes.
    pub fn snapshot(&self) -> [f32; SPECTRUM_BINS] {
        self.bins
    }

    /// Pure; a slow DFT over SPECTRUM_BINS bins spread linearly across
    /// 0..Nyquist. Runs only when the window is full so it stays cheap.
    fn transform(samples: &[f32]) -> [f32; SPECTRUM_BINS] {
        let nyquist = Self::FFT_SIZE / 2;
        let mut out = [0.0f32; SPECTRUM_BINS];
        for (bin, slot) in out.iter_mut().enumerate() {
            let f_bin = nyquist as f32 * (bin as f32 + 0.5) / SPECTRUM_BINS as f32;
            let k = (f_bin * Self::FFT_SIZE as f32 / nyquist as f32).round() as usize;
            let k = k.clamp(1, nyquist - 1);
            let mut re = 0.0;
            let mut im = 0.0;
            let angle = -2.0 * std::f32::consts::PI * k as f32 / Self::FFT_SIZE as f32;
            for (n, &s) in samples.iter().enumerate() {
                let phase = angle * n as f32;
                re += s * phase.cos();
                im += s * phase.sin();
            }
            *slot = (re * re + im * im).sqrt() / Self::FFT_SIZE as f32;
        }
        out
    }
}

impl Default for Spectrum {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;

    #[test]
    fn bypass_passes_through_unchanged() {
        let mut f = Biquad::bypass();
        for x in [0.5f32, -0.25, 1.0, -1.0, 0.0] {
            assert!((f.process(x) - x).abs() < 1e-6, "bypass altered {x}");
        }
    }

    #[test]
    fn peaking_responses_at_centre_frequency() {
        // A 1000 Hz peaking filter should roughly pass a 1000 Hz tone.
        let mut f = Biquad::peaking(1000.0, 1.5, 3.0, 44100.0);
        let mut out: f32 = 0.0;
        // Let the filter settle over several periods before measuring.
        for n in 0..44100 {
            let x = (2.0 * std::f32::consts::PI * 1000.0 * n as f32 / 44100.0).sin();
            let y = f.process(x);
            if n > 4000 {
                out = out.max(y.abs());
            }
        }
        // With +3 dB peaking the steady amplitude should exceed ~1.3x the input.
        assert!(out > 1.3, "peaking didn't boost: {out}");
    }

    #[test]
    fn flat_equalizer_passes_through() {
        let eq = new_eq();
        let input = SamplesBuffer::new(1, 44100, vec![0.0, 0.1, -0.2, 0.3]);
        let mut source = EqualizerSource::new(input, eq, None);
        let samples: Vec<f32> = source.by_ref().collect();
        // Gains are 0 (disabled) so output should match input except tiny fp.
        assert_eq!(samples.len(), 4);
        assert!((samples[1] - 0.1).abs() < 1e-5);
        assert!((samples[2] - -0.2).abs() < 1e-5);
        assert!((samples[3] - 0.3).abs() < 1e-5);
    }

    #[test]
    fn spectrum_reset_clears_bins() {
        let mut spec = Spectrum::new();
        for n in 0..(Spectrum::FFT_SIZE * 2) {
            let s = (2.0 * std::f32::consts::PI * 1000.0 * n as f32 / 44100.0).sin();
            spec.push(s);
        }
        assert!(spec.snapshot().iter().any(|&b| b > 0.0), "spectrum empty");
        spec.reset();
        assert!(spec.snapshot().iter().all(|&b| b == 0.0), "reset failed");
    }
}
