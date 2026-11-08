//! Shared signal helpers for the phenomena tests — the integration-test counterpart of the engine's
//! in-crate `test_util` (which is `#[cfg(test)]`, so unreachable from here). Built on the public API.

use engine::{AnalogRate, InputPort, Lane, Node, Ohms, OutputPort, OutputZ, Params, Volts};

/// The analog rate the phenomena run at (384 kHz — 8× a 48 kHz converter clock).
pub fn rate() -> AnalogRate {
    AnalogRate::new(384_000.0)
}

/// Root-mean-square of a slice. Empty slice → 0.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
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
/// that move, without involving the event-driven oscillator. With `offset = 0` it's a plain tone;
/// with `amp = 0` a DC source; together, "DC riding on the AC" for the DC-blocker tests.
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

    fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
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
/// it — independent of the edge-injection seam that supplies cable pickup/hum. No inputs; one
/// balanced output.
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

    fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
        let half = self.signal * 0.5;
        let (hot, cold) = outputs.split_at_mut(1);
        hot[0].voltage_mut().fill(Volts::new(self.cm + half));
        cold[0].voltage_mut().fill(Volts::new(self.cm - half));
    }
}
