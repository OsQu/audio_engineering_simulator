//! Noise specified as a **spectral density**, the rate-independent way to model a noise floor.
//!
//! A device's noise floor is given as a white-noise voltage spectral density `D` in V/√Hz —
//! the figure on a datasheet (e.g. a preamp's "input-referred noise"). White noise at the
//! analog rate has a flat one-sided power spectral density over `[0, fs/2]`, so the density and
//! the per-sample draw relate by
//!
//! ```text
//!   D = σ / √(fs/2)   ⇒   σ = D · √(fs/2)
//! ```
//!
//! where `σ` is the standard deviation of each independent Gaussian sample (and the wideband
//! RMS on the wire). We store the **density**, not `σ`, on purpose: the density is
//! rate-independent *in band*. When the AD converter band-limits to an audio bandwidth `B`, the
//! in-band noise becomes `D·√B`, and the oversampling SNR gain falls out of the physics with no
//! remodelling — there is no throwaway parameter to migrate later.

use crate::signal::AnalogRate;

/// A white-noise voltage spectral density in volts per √Hz (V/√Hz).
///
/// A measurement-domain spec (like a datasheet figure), turned into a per-sample standard
/// deviation by [`per_sample_sigma`](Self::per_sample_sigma) once the analog rate is known.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct NoiseDensity(f32);

impl NoiseDensity {
    /// No noise.
    pub const ZERO: Self = Self(0.0);

    /// A density of `volts_per_root_hz` V/√Hz.
    ///
    /// # Panics
    /// Panics unless the value is finite and `>= 0` — a negative or non-finite density is a
    /// setup bug. Checked here at construction, never on the hot path.
    #[must_use]
    pub fn new(volts_per_root_hz: f32) -> Self {
        assert!(
            volts_per_root_hz.is_finite() && volts_per_root_hz >= 0.0,
            "NoiseDensity must be finite and >= 0, got {volts_per_root_hz}"
        );
        Self(volts_per_root_hz)
    }

    /// The raw density in V/√Hz.
    pub fn get(self) -> f32 {
        self.0
    }

    /// The per-sample white-noise standard deviation in volts at `rate`: `σ = D·√(fs/2)`.
    ///
    /// This is also the wideband RMS the noise presents on the wire. Computed in `f64` (the
    /// `√(fs/2)` factor is large) and returned as the buffer's `f32`.
    #[must_use]
    pub fn per_sample_sigma(self, rate: AnalogRate) -> f32 {
        let nyquist = rate.as_hz() * 0.5;
        (f64::from(self.0) * nyquist.sqrt()) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn sigma_matches_hand_calc() {
        // D = 10 nV/√Hz at fs = 384 kHz:
        //   σ = 10e-9 · √(384000/2) = 10e-9 · √192000 = 10e-9 · 438.178 = 4.3818 µV.
        let d = NoiseDensity::new(10e-9);
        assert_relative_eq!(d.per_sample_sigma(rate()), 4.381_78e-6, epsilon = 1e-9);
    }

    #[test]
    fn zero_density_is_zero_sigma() {
        assert_eq!(NoiseDensity::ZERO.per_sample_sigma(rate()), 0.0);
    }

    #[test]
    fn sigma_scales_with_root_rate() {
        // Quadrupling the rate doubles σ (√ of the Nyquist span).
        let d = NoiseDensity::new(10e-9);
        let lo = d.per_sample_sigma(AnalogRate::new(96_000.0));
        let hi = d.per_sample_sigma(AnalogRate::new(384_000.0));
        assert_relative_eq!(hi / lo, 2.0, epsilon = 1e-5);
    }

    #[test]
    #[should_panic(expected = "must be finite and >= 0")]
    fn rejects_negative() {
        let _ = NoiseDensity::new(-1e-9);
    }
}
