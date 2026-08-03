//! Equalizer: per-band gain configuration with named presets.
//!
//! The model stores gains/preset state and exposes them through a shared
//! [`EqHandle`] that the real DSP source reads live from the playback thread.

use crate::audio::dsp::{EqHandle, new_eq};

/// Number of equalizer bands.
pub const BANDS: usize = 10;
/// Centre frequency of each band, in Hz.
pub const BAND_FREQUENCIES: [f32; BANDS] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
/// Minimum allowed band gain, in dB.
pub const MIN_GAIN_DB: f32 = -12.0;
/// Maximum allowed band gain, in dB.
pub const MAX_GAIN_DB: f32 = 12.0;

/// A named set of band gains.
#[derive(Debug, Clone, Copy)]
pub struct Preset {
    /// Preset display name.
    pub name: &'static str,
    /// One gain (dB) per band.
    pub gains: [f32; BANDS],
}

const PRESETS: [Preset; 6] = [
    Preset {
        name: "Flat",
        gains: [0.0; BANDS],
    },
    Preset {
        name: "Bass Boost",
        gains: [6.0, 5.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    },
    Preset {
        name: "Vocal",
        gains: [-2.0, -1.0, 0.0, 2.0, 3.0, 3.0, 2.0, 1.0, 0.0, -1.0],
    },
    Preset {
        name: "Treble Boost",
        gains: [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 6.0, 7.0],
    },
    Preset {
        name: "Rock",
        gains: [5.0, 4.0, 2.0, 0.0, -1.0, 1.0, 3.0, 4.0, 3.0, 2.0],
    },
    Preset {
        name: "Jazz",
        gains: [4.0, 3.0, 1.0, 0.0, 1.0, 2.0, 3.0, 2.0, 1.0, 0.0],
    },
];

/// All built-in presets.
pub fn presets() -> &'static [Preset] {
    &PRESETS
}

/// Per-band gain model that shares its state with the DSP source.
pub struct Equalizer {
    params: EqHandle,
}

impl Equalizer {
    /// Creates an equalizer with all bands flat and DSP disabled.
    pub fn new() -> Self {
        Self { params: new_eq() }
    }

    /// Returns a handle to the shared live parameters used by the DSP source.
    pub fn handle(&self) -> EqHandle {
        self.params.clone()
    }

    /// Sets a single band's gain in dB, clamped to the supported range.
    pub fn set_band(&mut self, index: usize, gain_db: f32) {
        if index >= BANDS {
            return;
        }
        let mut params = self.params.lock();
        params.gains[index] = gain_db.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
    }

    /// Gain of a band in dB; `0.0` for out-of-range indices.
    #[allow(dead_code)] // exercised by tests, used by a future settings UI
    pub fn gain(&self, index: usize) -> f32 {
        if index >= BANDS {
            0.0
        } else {
            self.params.lock().gains[index]
        }
    }

    /// Applies a named preset. Returns `false` if the name is unknown.
    pub fn set_preset(&mut self, name: &str) -> bool {
        for preset in PRESETS {
            if preset.name == name {
                self.params.lock().gains = preset.gains;
                return true;
            }
        }
        false
    }

    /// Enables or disables the equalizer.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.params.lock().enabled = enabled;
    }

    /// Whether the equalizer is enabled.
    #[allow(dead_code)] // exercised by tests, used by a future settings UI
    pub fn is_enabled(&self) -> bool {
        self.params.lock().enabled
    }

    /// Snapshot of all band gains in dB.
    #[allow(dead_code)] // exercised by tests, used by a future settings UI
    pub fn gains(&self) -> [f32; BANDS] {
        self.params.lock().gains
    }

    /// Applies a linear pre-gain (used for ReplayGain); 1.0 disables.
    pub fn set_pre_gain(&mut self, linear: f32) {
        if linear.is_finite() && linear > 0.0 {
            self.params.lock().pre_gain = linear;
        }
    }
}

impl Default for Equalizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{BANDS, Equalizer, MAX_GAIN_DB, MIN_GAIN_DB, presets};

    #[test]
    fn set_band_clamps_gain_and_index() {
        let mut eq = Equalizer::new();
        eq.set_band(3, 50.0);
        assert_eq!(eq.gain(3), MAX_GAIN_DB);
        eq.set_band(3, -50.0);
        assert_eq!(eq.gain(3), MIN_GAIN_DB);
        eq.set_band(3, 4.5);
        assert_eq!(eq.gain(3), 4.5);
        eq.set_band(999, 3.0);
        assert_eq!(eq.gain(999), 0.0);
    }

    #[test]
    fn set_preset_unknown_returns_false() {
        let mut eq = Equalizer::new();
        assert!(!eq.set_preset("No Such Preset"));
    }

    #[test]
    fn flat_preset_zeroes_gains() {
        let mut eq = Equalizer::new();
        for i in 0..BANDS {
            eq.set_band(i, 6.0);
        }
        assert!(eq.set_preset("Flat"));
        assert_eq!(eq.gains(), [0.0; BANDS]);
    }

    #[test]
    fn gains_roundtrip() {
        let mut eq = Equalizer::new();
        eq.set_band(1, -4.2);
        assert_eq!(eq.gain(1), -4.2);
        assert_eq!(eq.gains()[1], -4.2);
    }

    #[test]
    fn presets_expose_required_names() {
        let names: Vec<&str> = presets().iter().map(|p| p.name).collect();
        for required in [
            "Flat",
            "Bass Boost",
            "Vocal",
            "Treble Boost",
            "Rock",
            "Jazz",
        ] {
            assert!(names.contains(&required), "missing preset {required}");
        }
        assert!(presets().iter().all(|p| p.gains.len() == BANDS));
    }
}
