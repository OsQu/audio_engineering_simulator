//! A dynamic microphone: a passive balanced source with a deterministic capsule tone.

use super::Node;
use crate::electrical::{Ohms, OutputZ};
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{InputPort, OutputPort};
use crate::signal::{AnalogRate, Lane, Volts};

/// A dynamic microphone — a **passive** balanced source that needs **no power to operate**.
///
/// Where the [`CondenserMic`](super::CondenserMic) carries active impedance-converter electronics
/// that must be phantom-fed, a dynamic mic is just a coil moving in a magnet's field (a tiny
/// generator): it produces its own EMF from the diaphragm's motion, so it draws **no DC** and
/// declares **no [`phantom_load`](Node::phantom_load)**. It is therefore **always alive** — there is
/// no operating point to solve, no [`resolve_phantom`](Node::resolve_phantom), no threshold to cross.
/// A +48 V supply facing this output simply finds nothing to power: with no phantom load declared,
/// the compile-time solve skips it (the "48 V into a passive source is harmless" no-op), so the mic
/// sounds identical whether phantom is on or off.
///
/// Because nothing biases the line, the output sits at a **0 V common-mode pedestal** — the audio is
/// purely differential, floating symmetrically about ground:
///
/// ```text
///   V+ = +s/2,  V− = −s/2    ⇒    common-mode = 0,  differential = s
/// ```
///
/// This is the passive counterpart to the condenser's DC-pedestal output: the *audio* recovered at a
/// balanced receiver's difference is the same species of signal `s`, only here there is no phantom DC
/// riding underneath it to reject. No inputs; one balanced output.
///
/// # Less sensitive than a condenser
/// A moving coil is a far less efficient transducer than a charged capsule with an active head amp,
/// so a dynamic mic delivers a **weaker signal for the same sound** — typically ~10–15 dB below a
/// studio condenser. That is modelled directly as a lower default [`LEVEL`](Self::LEVEL) (the catalog
/// builds it at ~2 mV vs the condenser's ~10 mV): same knob, same units, just a quieter capsule, so
/// it leans harder on the preamp's gain.
///
/// # The capsule tone (a declared boundary stand-in)
/// Identical in spirit to the condenser (PROJECT_PLAN §2): acoustics are out of scope, so the capsule
/// emits a **deterministic internal sine** as the differential audio `s` — a stand-in for whatever
/// the boundary would deliver — shaped by two smoothed control params, [`LEVEL`](Self::LEVEL) (the
/// differential amplitude in volts, mic-level by default) and [`FREQ`](Self::FREQ) (Hz). An `f64`
/// phase accumulator stepped at the analog rate makes it reproducible with no ambient entropy
/// (structural determinism, like [`SynthVoice`](super::SynthVoice)). The deferred **"air link" story**
/// replaces this with a real acoustic path; until then the sine is the seam.
///
/// Unprepared (no analog rate yet — `compile` always prepares before `process`, so a compiled
/// schedule never hits this), the phase step is 0 and the tone is silent (both legs at 0 V).
pub struct DynamicMic {
    /// Analog rate (Hz), baked at [`prepare`](Node::prepare); `0.0` until prepared (tone silent).
    rate_hz: f64,
    /// Oscillator phase in `[0, 1)`. Free-runs across blocks.
    phase: f64,
    /// The [`LEVEL`](Self::LEVEL) default (differential amplitude, volts) — the constructed level and
    /// the fallback when run outside a schedule.
    default_level: f32,
    /// The declared control params: [`LEVEL`](Self::LEVEL), [`FREQ`](Self::FREQ).
    params: [ParamDecl; 2],
    outputs: [OutputPort; 1],
}

impl DynamicMic {
    /// The capsule tone's differential amplitude (volts) — a smoothed level knob. Uncontrolled, it
    /// holds the constructed level.
    pub const LEVEL: ParamId = ParamId(0);
    /// The capsule tone's frequency (Hz) — smoothed. Defaults to
    /// [`FREQ_DEFAULT_HZ`](Self::FREQ_DEFAULT_HZ).
    pub const FREQ: ParamId = ParamId(1);

    /// The default capsule frequency: 1 kHz — a hand-calc-friendly, plainly audible reference tone.
    pub const FREQ_DEFAULT_HZ: f32 = 1_000.0;
    /// Level knob ceiling (volts): the capsule stays a small-signal source, so ~100 mV is a generous
    /// top; the ~2 mV default is the typical (quieter) dynamic-mic level.
    const LEVEL_MAX_VOLTS: f32 = 0.1;
    /// Frequency knob bounds (Hz): the audible band.
    const FREQ_MIN_HZ: f32 = 20.0;
    const FREQ_MAX_HZ: f32 = 20_000.0;
    /// De-zipper glide for the level/frequency knobs — short enough to feel instant, matching the
    /// condenser's smoothing.
    const SMOOTH_MS: f32 = 5.0;

    /// A dynamic mic emitting a capsule sine of differential amplitude `signal` (the
    /// [`LEVEL`](Self::LEVEL) default, ~2 mV is a typical dynamic level) at
    /// [`FREQ_DEFAULT_HZ`](Self::FREQ_DEFAULT_HZ), from balanced output impedance `z_out`. Being
    /// passive, it needs no power: it is alive the moment it is prepared.
    #[must_use]
    pub fn new(signal: Volts, z_out: Ohms) -> Self {
        let default_level = signal.get();
        Self {
            rate_hz: 0.0,
            phase: 0.0,
            default_level,
            params: [
                ParamDecl {
                    id: Self::LEVEL,
                    default: default_level,
                    min: 0.0,
                    max: Self::LEVEL_MAX_VOLTS,
                    smooth_ms: Self::SMOOTH_MS,
                },
                ParamDecl {
                    id: Self::FREQ,
                    default: Self::FREQ_DEFAULT_HZ,
                    min: Self::FREQ_MIN_HZ,
                    max: Self::FREQ_MAX_HZ,
                    smooth_ms: Self::SMOOTH_MS,
                },
            ],
            outputs: [OutputZ::balanced(z_out).into()],
        }
    }
}

impl Node for DynamicMic {
    fn inputs(&self) -> &[InputPort] {
        &[]
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn params(&self) -> &[ParamDecl] {
        &self.params
    }

    fn prepare(&mut self, rate: AnalogRate) {
        // Bake the analog rate for the phase step; the phase itself is left untouched so it keeps
        // free-running across a recompile (matching `CondenserMic`/`SynthVoice`).
        self.rate_hz = rate.as_hz();
    }

    // Deliberately no `phantom_load` / `resolve_phantom`: a passive coil draws no DC, so it declares
    // no operating point and the compile-time phantom solve has nothing to resolve against it.

    fn process(&mut self, params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
        // A passive mic is always alive: the audio is purely differential about a 0 V common-mode, so
        // the hot loop just splits the capsule sine across the two legs. `inv_rate` is 0 when
        // unprepared, freezing the phase (tone silent) — the only "off" state this node has.
        let inv_rate = if self.rate_hz > 0.0 {
            1.0 / self.rate_hz
        } else {
            0.0
        };
        let (hot, cold) = outputs.split_at_mut(1);
        let hot = hot[0].voltage_mut().as_mut_slice();
        let cold = cold[0].voltage_mut().as_mut_slice();
        for (i, (h, c)) in hot.iter_mut().zip(cold.iter_mut()).enumerate() {
            let level = params.value_at_or(Self::LEVEL, i, self.default_level);
            let freq = params.value_at_or(Self::FREQ, i, Self::FREQ_DEFAULT_HZ);
            // Differential audio: the capsule sine (f64 phase → f32 emission), floating about 0 V.
            let s = level * (std::f64::consts::TAU * self.phase).sin() as f32;
            let half = 0.5 * s;
            *h = half;
            *c = -half;
            // Free-run the phase, wrapping in [0, 1) like `CondenserMic`/`SynthVoice`.
            self.phase += f64::from(freq) * inv_rate;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, VoltageBuffer};
    use crate::test_util::process_voltage;
    use approx::assert_relative_eq;
    use std::f64::consts::TAU;

    const RATE_HZ: f64 = 384_000.0;

    fn rate() -> AnalogRate {
        AnalogRate::new(RATE_HZ)
    }

    /// A prepared mic with level `level` (default 1 kHz), processed for `n` samples outside a
    /// schedule; returns the `[hot, cold]` leg buffers.
    fn run_mic(level: f32, n: usize) -> [VoltageBuffer; 2] {
        let mut m = DynamicMic::new(Volts::new(level), Ohms::new(200.0));
        m.prepare(rate());
        let mut out = [
            VoltageBuffer::zeros(n, rate()),
            VoltageBuffer::zeros(n, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        out
    }

    #[test]
    fn declares_a_balanced_passive_output() {
        let m = DynamicMic::new(Volts::new(0.002), Ohms::new(200.0));
        assert!(m.inputs().is_empty());
        assert_eq!(m.outputs()[0].lane_count(), 2);
        // A passive source declares no DC appetite on any port — the defining difference from the
        // condenser.
        assert!(
            m.phantom_load(0).is_none(),
            "a dynamic mic draws no phantom"
        );
        assert!(m.phantom_load(1).is_none());
        // Two smoothed knobs: LEVEL (the constructed 0.002 V) and FREQ (default 1 kHz).
        let ps = m.params();
        assert_eq!(ps.len(), 2);
        assert_relative_eq!(ps[0].default, 0.002);
        assert_relative_eq!(ps[1].default, 1_000.0);
    }

    #[test]
    fn emits_the_capsule_tone_about_zero_with_no_power() {
        // No phantom is ever resolved — a passive mic is alive the moment it's prepared. Two oracles,
        // sample-wise over a full 256-sample window (well inside one 384-sample period):
        //   1. common-mode (V+ + V−)/2 = 0, exactly — there is no DC pedestal, the tone floats.
        //   2. differential V+ − V− = level·sin(2π·f·k/rate), sample k — the capsule sine, with
        //      amplitude = the LEVEL param. Phase starts at 0 (sample 0's differential is 0).
        let (level, f) = (0.002_f32, 1_000.0_f64);
        let [hot, cold] = run_mic(level, 256);
        for k in 0..256 {
            let vp = hot.get(k).get();
            let vn = cold.get(k).get();
            assert_relative_eq!((vp + vn) / 2.0, 0.0, epsilon = 1e-9); // no pedestal, ever
            let want = f64::from(level) * (TAU * f * k as f64 / RATE_HZ).sin();
            assert_relative_eq!(f64::from(vp - vn), want, epsilon = 1e-7);
            // The legs are exact mirror images about 0 V.
            assert_relative_eq!(vp, -vn, epsilon = 1e-9);
        }
    }

    #[test]
    fn phase_continues_across_blocks() {
        // The phase accumulator persists: block 2's first sample continues block 1's sequence, no
        // per-block reset. Two 256-sample blocks on one mic must tile a single 512-sample reference
        // run of an identical mic — bit-for-bit (identical float ops in identical order).
        let level = 0.002_f32;
        let [reference, _] = run_mic(level, 512);

        let mut m = DynamicMic::new(Volts::new(level), Ohms::new(200.0));
        m.prepare(rate());
        let mut b1 = [
            VoltageBuffer::zeros(256, rate()),
            VoltageBuffer::zeros(256, rate()),
        ];
        process_voltage(&mut m, &[], &mut b1);
        let mut b2 = [
            VoltageBuffer::zeros(256, rate()),
            VoltageBuffer::zeros(256, rate()),
        ];
        process_voltage(&mut m, &[], &mut b2);

        for k in 0..256 {
            assert_eq!(b1[0].get(k).get(), reference.get(k).get());
            // The continuation: block 2 picks up exactly where the reference is at sample 256.
            assert_eq!(b2[0].get(k).get(), reference.get(k + 256).get());
        }
    }

    #[test]
    fn unprepared_mic_is_silent() {
        // No `prepare` ⇒ no analog rate ⇒ the phase step is 0: both legs sit at 0 V (the sine of a
        // frozen phase-0 is 0). (compile always prepares, so a real schedule never runs this path.)
        let mut m = DynamicMic::new(Volts::new(0.002), Ohms::new(200.0));
        let mut out = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        assert!(out[0].as_slice().iter().all(|&v| v == 0.0));
        assert!(out[1].as_slice().iter().all(|&v| v == 0.0));
    }
}
