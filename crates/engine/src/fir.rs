//! Windowed-sinc FIR low-pass filtering — the converter's decimation/reconstruction kernel.
//!
//! This is **designed DSP**, not emergent physics: it is the steep anti-alias / reconstruction
//! filter a modern oversampling converter runs *digitally* (the gentle analog pre-filter sits up
//! near the analog Nyquist and we don't model it). Unlike the cable's recursive
//! [`OnePole`](crate::OnePole), an FIR is **feed-forward**: its output is a weighted sum of
//! the last `L` inputs (the taps), so it can be made arbitrarily steep and **linear-phase**
//! (symmetric taps ⇒ constant group delay) — exactly what band-limiting at the AD/DA boundary needs.
//!
//! The taps are a **Kaiser-windowed sinc**, designed once at construction (an `exp`/Bessel cost paid
//! off the hot path); [`Decimator::process`] then only multiplies and accumulates. The number of
//! taps is the demonstrable **"weak filter" knob**: a short kernel widens the transition band and
//! lifts the stopband floor, so content above the decimated Nyquist leaks through and folds back
//! (the audible aliasing of Story 1.6.5).

use std::f64::consts::PI;

/// The modified Bessel function of the first kind, order 0 — `I₀(x)` — by its power series
/// `Σ ((x/2)^(2m) / m!²)`. Used to shape the Kaiser window; evaluated only at design time.
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0_f64;
    let mut term = 1.0_f64;
    let half_sq = (x / 2.0) * (x / 2.0);
    for m in 1..64 {
        term *= half_sq / (m as f64 * m as f64);
        sum += term;
        if term < 1e-14 * sum {
            break;
        }
    }
    sum
}

/// Design an `num_taps`-tap linear-phase low-pass: a sinc at `cutoff` (in cycles/sample, i.e. a
/// fraction of the sample rate) shaped by a Kaiser window of parameter `beta`, normalized to unity
/// DC gain. Symmetric, so the group delay is `(num_taps − 1) / 2` samples. Off the hot path.
///
/// # Panics
/// Panics if `num_taps == 0` or `cutoff` is not in `(0, 0.5)`. A construction-time check.
fn design_lowpass(num_taps: usize, cutoff: f64, beta: f64) -> Vec<f32> {
    assert!(num_taps >= 1, "an FIR needs at least one tap");
    assert!(
        cutoff > 0.0 && cutoff < 0.5,
        "cutoff must be in (0, 0.5) cycles/sample, got {cutoff}"
    );
    let center = (num_taps - 1) as f64 / 2.0;
    let denom = (num_taps - 1).max(1) as f64;
    let i0_beta = bessel_i0(beta);
    let mut taps = vec![0.0_f32; num_taps];
    let mut sum = 0.0_f64;
    for (n, tap) in taps.iter_mut().enumerate() {
        let x = n as f64 - center;
        // Ideal low-pass impulse response: 2·cutoff·sinc(2·cutoff·x), sinc(y) = sin(πy)/(πy).
        let arg = 2.0 * cutoff * x;
        let sinc = if arg == 0.0 {
            1.0
        } else {
            (PI * arg).sin() / (PI * arg)
        };
        let ideal = 2.0 * cutoff * sinc;
        // Kaiser window: I₀(beta·√(1 − r²)) / I₀(beta), with r ∈ [−1, 1] across the taps.
        let r = 2.0 * n as f64 / denom - 1.0;
        let w = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0_beta;
        let val = ideal * w;
        *tap = val as f32;
        sum += val;
    }
    // Normalize so the taps sum to 1 (exact unity gain at DC).
    let inv = 1.0 / sum;
    for tap in &mut taps {
        *tap = (f64::from(*tap) * inv) as f32;
    }
    taps
}

/// Kaiser `beta` for a target stopband attenuation `stopband_db`, per Kaiser's empirical formula.
/// Off the hot path (used at converter construction to size the AA/reconstruction filter).
#[must_use]
pub fn kaiser_beta(stopband_db: f64) -> f64 {
    if stopband_db > 50.0 {
        0.1102 * (stopband_db - 8.7)
    } else if stopband_db >= 21.0 {
        0.5842 * (stopband_db - 21.0).powf(0.4) + 0.07886 * (stopband_db - 21.0)
    } else {
        0.0
    }
}

/// A **decimating** linear-phase FIR low-pass: consumes `factor` input samples per output sample,
/// band-limiting to the decimated Nyquist first so nothing folds back.
///
/// It computes **one length-`L` dot product per retained output** — the polyphase decimation
/// saving (the `factor − 1` discarded outputs are never computed), without the explicit phase-bank
/// reorganization (a later micro-optimization that doesn't change the result). The tap history is a
/// ring buffer carried across blocks; [`process`](Self::process) is zero-allocation and panic-free.
pub struct Decimator {
    /// Symmetric windowed-sinc taps; `taps[k]` weights the input `k` samples before the newest.
    taps: Vec<f32>,
    /// Ring buffer of the last `taps.len()` inputs.
    history: Vec<f32>,
    /// Next write position in `history`.
    pos: usize,
    /// Decimation factor `M`: inputs consumed per output produced.
    factor: usize,
}

impl Decimator {
    /// Build a decimator from explicit `taps` and decimation `factor`.
    ///
    /// # Panics
    /// Panics if `taps` is empty or `factor == 0`.
    #[must_use]
    pub fn new(taps: Vec<f32>, factor: usize) -> Self {
        assert!(!taps.is_empty(), "a decimator needs at least one tap");
        assert!(factor >= 1, "decimation factor must be >= 1");
        let len = taps.len();
        Self {
            taps,
            history: vec![0.0; len],
            pos: 0,
            factor,
        }
    }

    /// An anti-alias decimator: a `num_taps` Kaiser-windowed sinc whose cutoff sits at the
    /// **decimated Nyquist** (`0.5 / factor` of the input rate), with window `beta` (see
    /// [`kaiser_beta`]).
    #[must_use]
    pub fn lowpass(num_taps: usize, factor: usize, beta: f64) -> Self {
        let taps = design_lowpass(num_taps, 0.5 / factor as f64, beta);
        Self::new(taps, factor)
    }

    /// The decimation factor `M`.
    pub fn factor(&self) -> usize {
        self.factor
    }

    /// The filter's group delay, `(taps − 1) / 2` samples — exact and constant because the kernel is
    /// symmetric (linear phase). A decimator's taps live at the **input** (here analog) rate, so this
    /// is in analog-rate samples: the latency the anti-alias decimation adds. Off the hot path.
    #[must_use]
    pub fn group_delay(&self) -> f64 {
        (self.taps.len() - 1) as f64 / 2.0
    }

    /// Clear the tap history (zeroed state), as at the start of a fresh run.
    pub fn reset(&mut self) {
        self.history.iter_mut().for_each(|h| *h = 0.0);
        self.pos = 0;
    }

    /// Decimate `input` into `output`, where `input.len() == output.len() * factor`. Hot path:
    /// no allocation, no panic; the per-output convolution accumulates in `f64` and flushes a
    /// denormal result to zero.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let len = self.taps.len();
        let mut oi = 0;
        for (i, &x) in input.iter().enumerate() {
            self.history[self.pos] = x;
            self.pos += 1;
            if self.pos == len {
                self.pos = 0;
            }
            // Retain one output every `factor` inputs; only these cost a dot product (polyphase).
            if (i + 1).is_multiple_of(self.factor) && oi < output.len() {
                let mut acc = 0.0_f64;
                // Newest input sits at pos−1; taps[k] weights the input k samples back.
                let mut idx = if self.pos == 0 { len - 1 } else { self.pos - 1 };
                for &tap in &self.taps {
                    acc += f64::from(tap) * f64::from(self.history[idx]);
                    idx = if idx == 0 { len - 1 } else { idx - 1 };
                }
                let y = acc as f32;
                output[oi] = if y != 0.0 && !y.is_normal() { 0.0 } else { y };
                oi += 1;
            }
        }
    }
}

/// An **interpolating** linear-phase FIR low-pass: produces `factor` output samples per input
/// sample, the mirror of [`Decimator`]. It upsamples by inserting `factor − 1` zeros between
/// inputs (which replicates the spectrum into `factor` images) and low-passes at the original
/// Nyquist to keep only the baseband — the reconstruction filter of an oversampling DA.
///
/// It computes the polyphase form directly: for each input it emits the `factor` output phases,
/// where phase `p` convolves the input history with the sub-filter `taps[p], taps[p+factor], …`.
/// Because the inserted zeros contribute nothing, only the nonzero inputs are ever multiplied —
/// the same saving as the decimator, from the other side. To undo the `1/factor` energy loss of
/// zero-stuffing, the taps are scaled by `factor` so the passband gain is unity. The input
/// history is a ring buffer carried across blocks; [`process`](Self::process) is zero-allocation
/// and panic-free.
pub struct Interpolator {
    /// Windowed-sinc taps, scaled by `factor` for unity passband gain after zero-stuffing.
    taps: Vec<f32>,
    /// Ring buffer of the last `ceil(taps.len() / factor)` input samples.
    history: Vec<f32>,
    /// Next write position in `history`.
    pos: usize,
    /// Interpolation factor `M`: outputs produced per input consumed.
    factor: usize,
}

impl Interpolator {
    /// Build an interpolator from explicit `taps` (already scaled for unity gain) and `factor`.
    ///
    /// # Panics
    /// Panics if `taps` is empty or `factor == 0`.
    #[must_use]
    pub fn new(taps: Vec<f32>, factor: usize) -> Self {
        assert!(!taps.is_empty(), "an interpolator needs at least one tap");
        assert!(factor >= 1, "interpolation factor must be >= 1");
        // History need only span the taps one phase reaches: ceil(L / factor) input samples.
        let span = taps.len().div_ceil(factor);
        Self {
            taps,
            history: vec![0.0; span],
            pos: 0,
            factor,
        }
    }

    /// A reconstruction interpolator: a `num_taps` Kaiser-windowed sinc whose cutoff sits at the
    /// **pre-upsampling Nyquist** (`0.5 / factor` of the output rate), window `beta` (see
    /// [`kaiser_beta`]), scaled by `factor` so the passband gain is unity.
    #[must_use]
    pub fn lowpass(num_taps: usize, factor: usize, beta: f64) -> Self {
        let mut taps = design_lowpass(num_taps, 0.5 / factor as f64, beta);
        // Zero-stuffing drops average energy by `factor`; scale the taps to restore unity gain.
        let gain = factor as f32;
        for tap in &mut taps {
            *tap *= gain;
        }
        Self::new(taps, factor)
    }

    /// The interpolation factor `M`.
    pub fn factor(&self) -> usize {
        self.factor
    }

    /// The filter's group delay, `(taps − 1) / 2` samples — exact and constant (symmetric kernel ⇒
    /// linear phase). An interpolator's taps live at the **output** (here analog) rate, so this is in
    /// analog-rate samples: the latency the reconstruction adds. Off the hot path.
    #[must_use]
    pub fn group_delay(&self) -> f64 {
        (self.taps.len() - 1) as f64 / 2.0
    }

    /// Clear the input history (zeroed state), as at the start of a fresh run.
    pub fn reset(&mut self) {
        self.history.iter_mut().for_each(|h| *h = 0.0);
        self.pos = 0;
    }

    /// Interpolate `input` into `output`, where `output.len() == input.len() * factor`. Hot path:
    /// no allocation, no panic; each output phase accumulates in `f64` and flushes a denormal
    /// result to zero.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let len = self.taps.len();
        let factor = self.factor;
        let span = self.history.len();
        let mut oi = 0;
        for &x in input {
            self.history[self.pos] = x;
            self.pos += 1;
            if self.pos == span {
                self.pos = 0;
            }
            let newest = if self.pos == 0 {
                span - 1
            } else {
                self.pos - 1
            };
            // Emit the `factor` output phases for this input; phase `p` uses taps p, p+M, p+2M, …
            for p in 0..factor {
                if oi >= output.len() {
                    return;
                }
                let mut acc = 0.0_f64;
                let mut idx = newest;
                let mut t = p;
                while t < len {
                    acc += f64::from(self.taps[t]) * f64::from(self.history[idx]);
                    idx = if idx == 0 { span - 1 } else { idx - 1 };
                    t += factor;
                }
                let y = acc as f32;
                output[oi] = if y != 0.0 && !y.is_normal() { 0.0 } else { y };
                oi += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, Volts};
    use crate::test_util::{sine, tone_amplitude};
    use approx::assert_relative_eq;

    /// 8× decimation: 384 kHz → 48 kHz, the Story 1.6 default.
    const M: usize = 8;
    fn hi() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn lo() -> AnalogRate {
        AnalogRate::new(48_000.0)
    }

    /// Decimate a high-rate slice into a fresh low-rate `Vec`.
    fn decimate(dec: &mut Decimator, input: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0; input.len() / dec.factor()];
        dec.process(input, &mut out);
        out
    }

    /// Interpolate a low-rate slice into a fresh high-rate `Vec`.
    fn interpolate(interp: &mut Interpolator, input: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0; input.len() * interp.factor()];
        interp.process(input, &mut out);
        out
    }

    #[test]
    fn taps_are_symmetric_and_unity_dc() {
        let taps = design_lowpass(101, 0.5 / M as f64, 8.0);
        // Linear phase ⇒ symmetric taps.
        for k in 0..taps.len() / 2 {
            assert_relative_eq!(taps[k], taps[taps.len() - 1 - k], epsilon = 1e-7);
        }
        // Normalized to unity DC gain ⇒ taps sum to 1.
        let sum: f64 = taps.iter().map(|&t| f64::from(t)).sum();
        assert_relative_eq!(sum, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn dc_passes_at_unity() {
        // A constant 1.0 in ⇒ after the filter fills, a constant 1.0 out (unity DC gain).
        let mut dec = Decimator::lowpass(161, M, 8.0);
        let input = vec![1.0_f32; 8_000];
        let out = decimate(&mut dec, &input);
        // Past settling (the group delay is ~161/2 inputs = ~10 outputs), the tail is ≈ 1.0.
        assert!(
            out[500..].iter().all(|&v| (v - 1.0).abs() < 1e-4),
            "DC gain should be unity"
        );
    }

    #[test]
    fn passband_tone_passes_near_unity() {
        // 4 kHz is deep in the passband (cutoff is the 24 kHz decimated Nyquist) ⇒ ~unity.
        let mut dec = Decimator::lowpass(161, M, 8.0);
        let input = sine(4_000.0, Volts::new(1.0), 8_000, hi());
        let out = decimate(&mut dec, input.as_slice());
        let amp = tone_amplitude(&out[400..], 4_000.0, lo());
        assert!(amp > 0.97, "passband tone should pass ~unity, got {amp}");
    }

    #[test]
    fn above_nyquist_is_attenuated_so_it_barely_aliases() {
        // 40 kHz is above the 24 kHz decimated Nyquist; without filtering it would fold to
        // 48 − 40 = 8 kHz. A good AA filter attenuates it first, so the 8 kHz alias is tiny.
        let mut dec = Decimator::lowpass(241, M, 8.0);
        let input = sine(40_000.0, Volts::new(1.0), 16_000, hi());
        let out = decimate(&mut dec, input.as_slice());
        let alias = tone_amplitude(&out[400..], 8_000.0, lo());
        assert!(
            alias < 0.02,
            "alias of an out-of-band tone should be tiny, got {alias}"
        );
    }

    #[test]
    fn weak_filter_aliases_more_than_a_strong_one() {
        // The "weak filter" knob is the tap count: a short kernel can't reject the 40 kHz tone,
        // so much more of it folds back to 8 kHz than with a long kernel.
        let input = sine(40_000.0, Volts::new(1.0), 16_000, hi());
        let strong = {
            let mut d = Decimator::lowpass(241, M, 8.0);
            tone_amplitude(&decimate(&mut d, input.as_slice())[400..], 8_000.0, lo())
        };
        let weak = {
            let mut d = Decimator::lowpass(15, M, 8.0);
            tone_amplitude(&decimate(&mut d, input.as_slice())[400..], 8_000.0, lo())
        };
        assert!(
            weak > strong * 5.0,
            "a weak (short) filter must alias far more: weak {weak} vs strong {strong}"
        );
    }

    #[test]
    fn kaiser_beta_matches_known_points() {
        // Kaiser's formula: ~60 dB ⇒ beta ≈ 0.1102·(60−8.7) = 5.653.
        assert_relative_eq!(kaiser_beta(60.0), 0.1102 * 51.3, epsilon = 1e-9);
        // Below 21 dB a rectangular window suffices ⇒ beta = 0.
        assert_eq!(kaiser_beta(10.0), 0.0);
    }

    #[test]
    fn interpolator_dc_passes_at_unity() {
        // A constant 1.0 upsampled ×M is a pulse train averaging 1/M; the taps are scaled by M
        // so the reconstruction restores a constant ≈ 1.0 (images rejected by the stopband).
        let mut interp = Interpolator::lowpass(161, M, kaiser_beta(96.0));
        let out = interpolate(&mut interp, &[1.0_f32; 1_000]);
        // Past settling (group delay ~161/2 hi-rate samples = ~80 ≈ 10 inputs), the tail is ≈ 1.0.
        assert!(
            out[800..].iter().all(|&v| (v - 1.0).abs() < 1e-3),
            "interpolated DC gain should be unity"
        );
    }

    #[test]
    fn interpolator_preserves_a_passband_tone() {
        // A 4 kHz tone at 48 kHz (deep in the passband) upsampled ×8 to 384 kHz keeps unity
        // amplitude — the reconstruction filter passes the baseband and rejects the images.
        let mut interp = Interpolator::lowpass(161, M, kaiser_beta(96.0));
        let input = sine(4_000.0, Volts::new(1.0), 1_000, lo());
        let out = interpolate(&mut interp, input.as_slice());
        let amp = tone_amplitude(&out[800..], 4_000.0, hi());
        assert!(amp > 0.97, "passband tone should survive ~unity, got {amp}");
    }

    #[test]
    fn interpolator_rejects_the_first_image() {
        // Upsampling a 4 kHz tone by 8 puts spectral images at 48k ± 4k, 96k ± 4k, … The nearest
        // image (44 kHz) sits in the stopband, so it should be attenuated to near nothing.
        let mut interp = Interpolator::lowpass(161, M, kaiser_beta(96.0));
        let input = sine(4_000.0, Volts::new(1.0), 1_000, lo());
        let out = interpolate(&mut interp, input.as_slice());
        let image = tone_amplitude(&out[800..], 44_000.0, hi());
        assert!(image < 0.01, "first image should be rejected, got {image}");
    }

    #[test]
    fn interpolate_then_decimate_round_trips_a_passband_tone() {
        // Up ×8 then back down ×8 through matched filters is identity in the passband (the two
        // group delays add, so compare past the combined settling). 3 kHz is well in band.
        let input = sine(3_000.0, Volts::new(0.8), 1_000, lo());
        let mut interp = Interpolator::lowpass(161, M, kaiser_beta(96.0));
        let mut dec = Decimator::lowpass(161, M, kaiser_beta(96.0));
        let up = interpolate(&mut interp, input.as_slice());
        let down = decimate(&mut dec, &up);
        let amp = tone_amplitude(&down[400..], 3_000.0, lo());
        assert_relative_eq!(amp, 0.8, epsilon = 0.02);
    }

    #[test]
    fn group_delay_is_half_the_kernel() {
        // A symmetric (linear-phase) kernel of L taps delays by exactly (L − 1)/2 operating-rate
        // samples: 161 taps ⇒ 80. Same for both directions (the taps live at the high/analog rate).
        let dec = Decimator::lowpass(161, M, 8.0);
        assert_relative_eq!(dec.group_delay(), 80.0, epsilon = 1e-12);
        let interp = Interpolator::lowpass(161, M, kaiser_beta(96.0));
        assert_relative_eq!(interp.group_delay(), 80.0, epsilon = 1e-12);
    }
}
