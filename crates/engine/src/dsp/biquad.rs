//! A second-order IIR (biquad) filter and the RBJ-cookbook designers that build one.

use super::flush_denormal;
use crate::signal::SampleRate;
use std::f64::consts::PI;

/// A second-order IIR filter in **Transposed Direct Form II**, the digital-domain peer of the
/// analog [`OnePole`](crate::OnePole).
///
/// The recurrence (with `a0` normalized to 1) is
/// ```text
///   y[n]  = b0·x[n] + z1
///   z1[n] = b1·x[n] − a1·y[n] + z2
///   z2[n] = b2·x[n] − a2·y[n]
/// ```
/// TDF-II is the form of choice for `f64` audio biquads: only two state words, and good numerical
/// behaviour (the state holds the *output* history, so coefficient round-off doesn't accumulate the
/// way it can in Direct Form I). Coefficients are **designed once** by one of the cookbook
/// constructors ([`peaking`](Self::peaking) / [`low_shelf`](Self::low_shelf) /
/// [`high_shelf`](Self::high_shelf)) — paying the `cos`/`sin`/`powf` there, never on the hot path.
///
/// Coefficients and state are `f64` (the accumulator policy — state feeds back every sample).
/// [`process`](Self::process) is the hot path: zero-alloc, panic-free, denormals flushed.
#[derive(Debug, Clone)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// Build from raw coefficients, normalizing by `a0` so the recurrence can assume `a0 = 1`.
    ///
    /// The single place coefficients enter the filter; every cookbook designer routes through here.
    #[must_use]
    fn from_coeffs(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
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

    /// A **peaking** (bell) EQ band: `gain_db` of boost/cut centered on `freq_hz`, bandwidth set by
    /// `q`. At `gain_db = 0` the design collapses to unity (numerator = denominator) — exactly
    /// transparent. RBJ Audio-EQ Cookbook.
    #[must_use]
    pub fn peaking(rate: SampleRate, freq_hz: f64, q: f64, gain_db: f64) -> Self {
        let (cos_w0, alpha) = w0(rate, freq_hz, q);
        let a = 10.0_f64.powf(gain_db / 40.0); // amplitude at the peak = √(power gain)
        Self::from_coeffs(
            1.0 + alpha * a, // b0
            -2.0 * cos_w0,   // b1
            1.0 - alpha * a, // b2
            1.0 + alpha / a, // a0
            -2.0 * cos_w0,   // a1
            1.0 - alpha / a, // a2
        )
    }

    /// A **low-shelf** EQ band: `gain_db` applied below `freq_hz`, asymptotically unity above; `q`
    /// sets the transition steepness (≈ 0.707 for a flat, Butterworth-like shelf). RBJ cookbook.
    #[must_use]
    pub fn low_shelf(rate: SampleRate, freq_hz: f64, q: f64, gain_db: f64) -> Self {
        let (cos_w0, alpha) = w0(rate, freq_hz, q);
        let a = 10.0_f64.powf(gain_db / 40.0);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        Self::from_coeffs(
            a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha), // b0
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),              // b1
            a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha), // b2
            (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,       // a0
            -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),                 // a1
            (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,       // a2
        )
    }

    /// A **high-shelf** EQ band: `gain_db` applied above `freq_hz`, asymptotically unity below; `q`
    /// sets the transition steepness (≈ 0.707 for a flat shelf). RBJ cookbook.
    #[must_use]
    pub fn high_shelf(rate: SampleRate, freq_hz: f64, q: f64, gain_db: f64) -> Self {
        let (cos_w0, alpha) = w0(rate, freq_hz, q);
        let a = 10.0_f64.powf(gain_db / 40.0);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        Self::from_coeffs(
            a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha), // b0
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),             // b1
            a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha), // b2
            (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,       // a0
            2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),                  // a1
            (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,       // a2
        )
    }

    /// Clear the filter state. Off the hot path.
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Advance the filter by one sample (TDF-II). Hot path: `#[inline]`, denormal-flushed state.
    #[inline]
    pub(crate) fn step(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = flush_denormal(self.b1 * x - self.a1 * y + self.z2);
        self.z2 = flush_denormal(self.b2 * x - self.a2 * y);
        y
    }

    /// Filter a block of samples in place. Zero-alloc, panic-free, denormals flushed.
    pub fn process(&mut self, samples: &mut [f32]) {
        for s in samples {
            *s = self.step(f64::from(*s)) as f32;
        }
    }
}

/// Shared cookbook intermediates: the cosine of the normalized angular frequency `w0 = 2π·f0/fs`,
/// and `alpha = sin(w0) / (2Q)` (the bandwidth term). The single place `f0`/`Q` become coefficients.
fn w0(rate: SampleRate, freq_hz: f64, q: f64) -> (f64, f64) {
    let w0 = 2.0 * PI * freq_hz / rate.as_hz();
    let alpha = w0.sin() / (2.0 * q);
    (w0.cos(), alpha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, Volts};
    use crate::test_util::{sine, tone_amplitude};
    use approx::assert_relative_eq;

    fn fs() -> SampleRate {
        SampleRate::new(48_000.0)
    }

    /// 48 kHz expressed as an `AnalogRate`, purely so the test oracles (`sine` / `tone_amplitude`)
    /// run their DFT at the digital sample period — the filter itself never sees an `AnalogRate`.
    fn fs_as_analog() -> AnalogRate {
        AnalogRate::new(48_000.0)
    }

    /// Linear gain a biquad applies to a `freq_hz` tone: drive a unit sine through it and return
    /// the settled output amplitude **relative to the input amplitude measured by the same oracle**.
    /// Taking the ratio cancels the single-bin DFT's spectral leakage at frequencies that don't
    /// divide the window (e.g. 15 kHz at 48 kHz is 3.2 samples/cycle), so the gain is the filter's,
    /// not the measurement's. The first half is dropped as the filter's startup transient.
    fn gain_at(mut f: Biquad, freq_hz: f64) -> f32 {
        let n = 8_192;
        let input = sine(freq_hz, Volts::new(1.0), n, fs_as_analog());
        let in_amp = tone_amplitude(&input.as_slice()[n / 2..], freq_hz, fs_as_analog());
        let mut buf = input.as_slice().to_vec();
        f.process(&mut buf);
        let out_amp = tone_amplitude(&buf[n / 2..], freq_hz, fs_as_analog());
        out_amp / in_amp
    }

    #[test]
    fn zero_db_peaking_is_transparent_everywhere() {
        // gain_db = 0 ⇒ A = 1 ⇒ numerator coeffs == denominator coeffs ⇒ H(z) = 1 exactly.
        let flat = Biquad::peaking(fs(), 1_000.0, 1.0, 0.0);
        assert_relative_eq!(flat.b0, 1.0, epsilon = 1e-12);
        assert_relative_eq!(flat.b1, flat.a1, epsilon = 1e-12);
        assert_relative_eq!(flat.b2, flat.a2, epsilon = 1e-12);
        for &freq in &[100.0, 1_000.0, 5_000.0, 15_000.0] {
            let g = gain_at(Biquad::peaking(fs(), 1_000.0, 1.0, 0.0), freq);
            assert_relative_eq!(g, 1.0, epsilon = 1e-3);
        }
    }

    #[test]
    fn peaking_boosts_its_center_and_passes_far_tones() {
        // +6 dB at the center = 10^(6/20) ≈ 1.995 linear; a decade below should be ~unity.
        let center = 1_000.0;
        let g_center = gain_at(Biquad::peaking(fs(), center, 2.0, 6.0), center);
        assert_relative_eq!(g_center, 10.0_f32.powf(6.0 / 20.0), epsilon = 0.03);

        let g_far = gain_at(Biquad::peaking(fs(), center, 2.0, 6.0), center / 10.0);
        assert!(
            (g_far - 1.0).abs() < 0.05,
            "a decade below center should be ~unity, got {g_far}"
        );
    }

    #[test]
    fn peaking_cut_attenuates_its_center() {
        // −12 dB at center = 10^(−12/20) ≈ 0.251 linear.
        let center = 2_000.0;
        let g = gain_at(Biquad::peaking(fs(), center, 2.0, -12.0), center);
        assert_relative_eq!(g, 10.0_f32.powf(-12.0 / 20.0), epsilon = 0.02);
    }

    #[test]
    fn low_shelf_boosts_lows_and_leaves_highs() {
        // +6 dB low shelf at 200 Hz: a 50 Hz tone (well below) sees ≈ +6 dB; a 10 kHz tone
        // (well above) sees ≈ unity.
        let shelf = || Biquad::low_shelf(fs(), 200.0, 0.707, 6.0);
        let g_low = gain_at(shelf(), 50.0);
        assert_relative_eq!(g_low, 10.0_f32.powf(6.0 / 20.0), epsilon = 0.05);

        let g_high = gain_at(shelf(), 10_000.0);
        assert!(
            (g_high - 1.0).abs() < 0.05,
            "highs should be untouched by a low shelf, got {g_high}"
        );
    }

    #[test]
    fn high_shelf_boosts_highs_and_leaves_lows() {
        // +6 dB high shelf at 5 kHz: a 15 kHz tone sees ≈ +6 dB; a 100 Hz tone sees ≈ unity.
        let shelf = || Biquad::high_shelf(fs(), 5_000.0, 0.707, 6.0);
        let g_high = gain_at(shelf(), 15_000.0);
        assert_relative_eq!(g_high, 10.0_f32.powf(6.0 / 20.0), epsilon = 0.06);

        let g_low = gain_at(shelf(), 100.0);
        assert!(
            (g_low - 1.0).abs() < 0.05,
            "lows should be untouched by a high shelf, got {g_low}"
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut f = Biquad::peaking(fs(), 1_000.0, 1.0, 12.0);
        let mut warm = vec![1.0_f32; 256];
        f.process(&mut warm);
        f.reset();
        let mut silence = vec![0.0_f32; 64];
        f.process(&mut silence);
        assert!(silence.iter().all(|&s| s == 0.0));
    }
}
