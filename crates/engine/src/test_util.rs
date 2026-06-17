//! Test-only signal generators and measurements.
//!
//! Not part of the public API — shared infrastructure for unit tests that need real audio
//! signals rather than scalar asserts (filter magnitude response now; SNR in Story 1.4).
//! Gated behind `#[cfg(test)]`, so it's compiled only for tests and never ships.

use crate::signal::{AnalogRate, VoltageBuffer, Volts};

/// A steady sine of `len` samples: `amp · sin(2π·freq·t)`, sampled at `rate`.
///
/// Computed in `f64` (phase accumulates over the block); stored as the buffer's `f32`.
pub fn sine(freq_hz: f64, amp: Volts, len: usize, rate: AnalogRate) -> VoltageBuffer {
    let mut buf = VoltageBuffer::zeros(len, rate);
    let dt = rate.seconds_per_sample();
    let omega = std::f64::consts::TAU * freq_hz;
    let a = f64::from(amp.get());
    for (n, s) in buf.as_mut_slice().iter_mut().enumerate() {
        let t = n as f64 * dt;
        *s = (a * (omega * t).sin()) as f32;
    }
    buf
}

/// Root-mean-square of a slice. Empty slice → 0.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

/// Steady-state magnitude response of `process` at `freq_hz`, as out-RMS / in-RMS.
///
/// Drives a unit sine through `process`, **discards the first half** as the settling
/// transient, and measures the steady second half. The buffer spans ~256 periods, so the
/// discarded half (~128 periods) dwarfs any filter time constant and the measured half
/// covers enough whole cycles that the RMS is accurate to well under a percent.
///
/// `process` is any in-place block operation — typically a stateful filter's `process`,
/// passed as a closure.
pub fn measure_gain<F>(freq_hz: f64, rate: AnalogRate, mut process: F) -> f32
where
    F: FnMut(&mut VoltageBuffer),
{
    let samples_per_period = rate.as_hz() / freq_hz;
    let len = (samples_per_period * 256.0).ceil() as usize;
    let input = sine(freq_hz, Volts::new(1.0), len, rate);
    let mut output = input.clone();
    process(&mut output);
    let half = len / 2;
    rms(&output.as_slice()[half..]) / rms(&input.as_slice()[half..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn rms_of_unit_sine_is_one_over_root_two() {
        // A full-scale sine has RMS = amp/√2 ≈ 0.7071.
        let s = sine(1_000.0, Volts::new(1.0), 384_000, rate());
        assert_relative_eq!(rms(s.as_slice()), 0.707_106_77, epsilon = 1e-3);
    }

    #[test]
    fn rms_of_a_constant_is_that_constant() {
        // RMS of a DC level equals the level itself.
        assert_relative_eq!(rms(&[2.0, 2.0, 2.0, 2.0]), 2.0);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn sine_starts_at_zero_and_stays_within_amplitude() {
        let s = sine(440.0, Volts::new(0.5), 2_000, rate());
        assert_eq!(s.len(), 2_000);
        assert_eq!(s.rate(), rate());
        assert_relative_eq!(s.get(0).get(), 0.0, epsilon = 1e-6);
        assert!(s.as_slice().iter().all(|&v| v.abs() <= 0.5 + 1e-6));
    }

    #[test]
    fn measure_gain_of_passthrough_is_unity() {
        // An identity process leaves the signal untouched → gain 1.0.
        let g = measure_gain(10_000.0, rate(), |_buf| {});
        assert_relative_eq!(g, 1.0, epsilon = 1e-3);
    }

    #[test]
    fn measure_gain_tracks_a_fixed_scaler() {
        // Halving every sample is a flat 0.5 (−6 dB) gain at any frequency.
        let g = measure_gain(10_000.0, rate(), |buf| {
            for s in buf.as_mut_slice() {
                *s *= 0.5;
            }
        });
        assert_relative_eq!(g, 0.5, epsilon = 1e-3);
    }
}
