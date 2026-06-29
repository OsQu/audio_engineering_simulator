//! `SineSource` — a sine oscillator, defined **here in the harness as demo scaffolding**.
//!
//! This is *not* the engine's oscillator. It lives in the harness only so the plot scenarios
//! have a bare continuous AC tone to push through a real compiled schedule — the engine's
//! [`TestSource`](engine::TestSource) emits DC (no treble rolloff, no curved wave to show), and
//! its real voice [`SynthVoice`](engine::SynthVoice) is event-driven and enveloped (great for the
//! audible first-sound renders, but not the steady tone the waveform plots want).

use engine::{InputPort, Lane, Node, Ohms, OutputPort, OutputZ, Params, Volts};
use std::f64::consts::TAU;

/// A continuous sine oscillator with a real Thévenin output impedance.
///
/// Zero inputs, one output. The output impedance is real so downstream loading (the
/// connection divider) still applies — the demo's point is that the physics emerge, so the
/// source must present a real electrical face just like [`TestSource`](engine::TestSource).
///
/// Phase is held in `f64` radians and **persists across blocks** so the tone is continuous
/// from one `process` call to the next (no per-block discontinuity); it is wrapped back into
/// `[0, 2π)` each sample to keep the accumulator small and precise.
pub struct SineSource {
    /// Peak amplitude (the open-circuit swing, before any downstream loading).
    amp: Volts,
    /// Frequency in hertz.
    freq_hz: f64,
    /// The single output face: a real `Zout` the next stage divides against.
    outputs: [OutputPort; 1],
    /// Running phase in radians, persisted across blocks for a continuous tone.
    phase: f64,
}

impl SineSource {
    /// A sine of peak amplitude `amp` at `freq_hz`, driving from output impedance `z_out`.
    #[must_use]
    pub fn new(amp: Volts, freq_hz: f64, z_out: Ohms) -> Self {
        Self {
            amp,
            freq_hz,
            outputs: [OutputZ::new(z_out).into()],
            phase: 0.0,
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
        // The output buffer carries the analog rate `compile` sized it with, so the sample
        // period comes straight off it — the source never stores a rate of its own.
        let dt = outputs[0].voltage().rate().seconds_per_sample();
        let dphase = TAU * self.freq_hz * dt; // radians advanced per sample
        let amp = self.amp.get();
        let mut phase = self.phase;
        for s in outputs[0].voltage_mut().as_mut_slice() {
            *s = amp * (phase.sin() as f32);
            phase += dphase;
            // `dphase < 2π` for any sane freq ≪ rate, so one subtraction is enough to wrap.
            if phase >= TAU {
                phase -= TAU;
            }
        }
        self.phase = phase; // carry the phase into the next block
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::{AnalogRate, VoltageBuffer};

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// One voltage lane sized `len` at the demo rate.
    fn lane(len: usize) -> Lane {
        Lane::Voltage(VoltageBuffer::zeros(len, rate()))
    }

    #[test]
    fn one_block_of_a_1khz_sine_has_the_expected_peak_and_period() {
        // 1 kHz at 384 kHz ⇒ 384 samples/cycle. Fill two cycles (768 samples).
        let amp = Volts::new(2.0);
        let mut src = SineSource::new(amp, 1_000.0, Ohms::new(100.0));
        let mut out = [lane(768)];
        src.process(&Params::EMPTY, &[], &mut out);
        let samples = out[0].voltage().as_slice();

        // Peak ≈ amp: with 384 samples/cycle a sample lands within a whisker of the crest,
        // so |max| sits just under 2.0 V. Allow 1 %.
        let peak = samples.iter().fold(0.0_f32, |m, &v| m.max(v.abs()));
        assert!(
            (peak - 2.0).abs() < 0.02,
            "peak {peak} V should be ≈ amp 2.0 V"
        );

        // Period: sin peaks a quarter-cycle in, at sample rate/(4·freq) = 384000/4000 = 96.
        // The index of the *first* cycle's crest pins the frequency (hence the period) without
        // trig in the test. (Search only the first 384 samples — the second cycle has an equal
        // crest at 480 that would otherwise win the tie.)
        let argmax = samples[..384]
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            argmax.abs_diff(96) <= 1,
            "first crest at sample {argmax} should be ≈ a quarter period (96)"
        );
    }

    #[test]
    fn phase_is_continuous_across_blocks() {
        // Two back-to-back 100-sample blocks must equal one continuous 200-sample block —
        // i.e. block 2 picks up exactly where block 1 left off (no per-block phase reset).
        let make = || SineSource::new(Volts::new(1.0), 1_000.0, Ohms::new(100.0));

        let mut split = make();
        let mut b1 = [lane(100)];
        let mut b2 = [lane(100)];
        split.process(&Params::EMPTY, &[], &mut b1);
        split.process(&Params::EMPTY, &[], &mut b2);

        let mut whole = make();
        let mut both = [lane(200)];
        whole.process(&Params::EMPTY, &[], &mut both);

        for (i, &v) in both[0].voltage().as_slice().iter().enumerate() {
            let got = if i < 100 {
                b1[0].voltage().as_slice()[i]
            } else {
                b2[0].voltage().as_slice()[i - 100]
            };
            assert!(
                (got - v).abs() < 1e-6,
                "sample {i}: split {got} vs whole {v}"
            );
        }
    }
}
