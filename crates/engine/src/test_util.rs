//! Test-only signal generators and measurements.
//!
//! Not part of the public API — shared infrastructure for unit tests that need real audio
//! signals rather than scalar asserts (filter magnitude response now; SNR in Story 1.4).
//! Gated behind `#[cfg(test)]`, so it's compiled only for tests and never ships.

use crate::electrical::{Ohms, OutputZ};
use crate::node::Node;
use crate::port::{InputPort, OutputPort};
use crate::signal::{AnalogRate, Lane, VoltageBuffer, Volts};

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

/// Drive a node's [`Node::process`] with plain [`VoltageBuffer`]s: wrap them as voltage [`Lane`]s,
/// run, and copy the results back. Lets an all-analog node's unit test read as it did before the
/// carrier seam (`process_voltage(&mut node, &in, &mut out)`). Clones the buffers — test-only,
/// never the hot path.
pub fn process_voltage(
    node: &mut impl Node,
    inputs: &[VoltageBuffer],
    outputs: &mut [VoltageBuffer],
) {
    let in_lanes: Vec<Lane> = inputs.iter().cloned().map(Lane::Voltage).collect();
    let mut out_lanes: Vec<Lane> = outputs.iter().cloned().map(Lane::Voltage).collect();
    node.process(&in_lanes, &mut out_lanes);
    for (slot, lane) in outputs.iter_mut().zip(out_lanes) {
        *slot = match lane {
            Lane::Voltage(b) => b,
            Lane::Sample(_) => unreachable!("process_voltage drives only voltage lanes"),
        };
    }
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

/// Peak amplitude of the `freq_hz` sinusoidal component of `samples`, by a single-bin DFT
/// (correlate the signal against `cos` and `sin` at that frequency). Empty slice → 0.
///
/// For a buffer spanning whole cycles of `freq_hz` this returns the component's true peak
/// amplitude — and harmonics, being whole-cycle too, stay orthogonal and don't leak. It's the
/// oracle for harmonic content: clipping distortion can't be heard in a unit test, so the
/// amplitudes at `f`, `3f`, `5f`… are measured and asserted against the hand calc. Dependency-
/// free (no FFT crate) and `f64`-accumulated; one bin is all the named-harmonic checks need.
pub fn tone_amplitude(samples: &[f32], freq_hz: f64, rate: AnalogRate) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let dt = rate.seconds_per_sample();
    let omega = std::f64::consts::TAU * freq_hz;
    let (mut re, mut im) = (0.0_f64, 0.0_f64);
    for (n, &x) in samples.iter().enumerate() {
        let phase = omega * (n as f64 * dt);
        re += f64::from(x) * phase.cos();
        im += f64::from(x) * phase.sin();
    }
    let n = samples.len() as f64;
    // For A·sin(ωt) over whole cycles: Σx·sin = A·N/2, Σx·cos ≈ 0 ⇒ amplitude = (2/N)·√(re²+im²).
    (2.0 / n * (re * re + im * im).sqrt()) as f32
}

/// A test-only source node emitting a free-running sine on a DC pedestal: `offset + amp·sin`.
///
/// The engine's [`TestSource`](crate::TestSource) emits pure DC; this drives **AC** (optionally
/// with a DC offset) through a real compiled patch — enough to test the analog chain on signals
/// that move, without pulling the real event-driven oscillator forward from Story 1.7. With
/// `offset = 0` it's a plain tone; with `amp = 0` a DC source; together, "DC riding on the AC"
/// for the DC-blocker tests.
///
/// Phase is held in `f64` and **persists across blocks**, so the tone is continuous from one
/// `process` call to the next. The sample period is read off the output buffer (the rate
/// `compile` sized it with), so the source stores no rate of its own. No inputs; one output.
pub struct SineSource {
    amp: f64,
    offset: f64,
    freq_hz: f64,
    phase: f64,
    outputs: [OutputPort; 1],
}

impl SineSource {
    /// A sine of peak amplitude `amp` at `freq_hz` on a DC pedestal `offset`, driving from
    /// output impedance `z_out`.
    pub fn new(freq_hz: f64, amp: Volts, offset: Volts, z_out: Ohms) -> Self {
        Self {
            amp: f64::from(amp.get()),
            offset: f64::from(offset.get()),
            freq_hz,
            phase: 0.0,
            outputs: [OutputZ::new(z_out).into()],
        }
    }
}

impl Node for SineSource {
    fn inputs(&self) -> &[InputPort] {
        &[]
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _inputs: &[Lane], outputs: &mut [Lane]) {
        let dt = outputs[0].voltage().rate().seconds_per_sample();
        let dphase = std::f64::consts::TAU * self.freq_hz * dt;
        let (amp, offset) = (self.amp, self.offset);
        let mut phase = self.phase;
        for s in outputs[0].voltage_mut().as_mut_slice() {
            *s = (offset + amp * phase.sin()) as f32;
            phase += dphase;
            if phase >= std::f64::consts::TAU {
                phase -= std::f64::consts::TAU;
            }
        }
        self.phase = phase; // carry phase into the next block
    }
}

/// A test-only **balanced** source: a differential `signal` riding a common-mode `cm` offset.
///
/// Emits `V+ = cm + signal/2`, `V− = cm − signal/2` on a two-conductor (balanced) output, so a
/// test can inject a known common-mode voltage by hand and prove a [`BalancedReceiver`] rejects
/// it (Story 1.5.1) — before the edge-injection seam that supplies pickup/hum exists (1.5.2). No
/// inputs; one balanced output.
///
/// [`BalancedReceiver`]: crate::BalancedReceiver
pub struct BalancedTestSource {
    signal: f32,
    cm: f32,
    outputs: [OutputPort; 1],
}

impl BalancedTestSource {
    /// A balanced source emitting differential `signal` with common-mode offset `cm`, driving
    /// from differential output impedance `z_out`.
    pub fn new(signal: Volts, cm: Volts, z_out: Ohms) -> Self {
        Self {
            signal: signal.get(),
            cm: cm.get(),
            outputs: [OutputZ::balanced(z_out).into()],
        }
    }
}

impl Node for BalancedTestSource {
    fn inputs(&self) -> &[InputPort] {
        &[]
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _inputs: &[Lane], outputs: &mut [Lane]) {
        let half = self.signal * 0.5;
        let (hot, cold) = outputs.split_at_mut(1);
        hot[0].voltage_mut().fill(Volts::new(self.cm + half));
        cold[0].voltage_mut().fill(Volts::new(self.cm - half));
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
    fn tone_amplitude_reads_the_fundamental_and_sees_no_harmonics() {
        // A clean 0.8 V sine at 1 kHz over whole cycles: the 1 kHz bin reads the amplitude
        // (0.8), and the 3 kHz bin reads ≈ 0 — a pure tone has no harmonics.
        let s = sine(1_000.0, Volts::new(0.8), 3_840, rate()); // 10 whole cycles at 384 kHz
        assert_relative_eq!(
            tone_amplitude(s.as_slice(), 1_000.0, rate()),
            0.8,
            epsilon = 1e-3
        );
        assert!(tone_amplitude(s.as_slice(), 3_000.0, rate()) < 1e-3);
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
