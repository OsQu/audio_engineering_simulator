//! A condenser microphone: a phantom-powered balanced source with a deterministic capsule tone.

use super::Node;
use crate::electrical::{Ohms, OutputZ, PhantomLoad};
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{InputPort, OutputPort};
use crate::signal::{AnalogRate, Lane, Volts};

/// A condenser microphone — a balanced source that needs **+48 V phantom power** to operate.
///
/// The mic **declares** its DC appetite on its output port (a [`phantom_load`](Node::phantom_load):
/// [`Z_DC_OHMS`](Self::Z_DC_OHMS), [`V_MIN_VOLTS`](Self::V_MIN_VOLTS)) and receives its power from
/// the patch: `compile` solves the DC
/// operating point against whatever engaged supply faces this output (the §17 superposition split)
/// and delivers the terminal volts via [`resolve_phantom`](Node::resolve_phantom). The pedestal on
/// the wire is therefore **earned from the bias network** — sag from the feed resistors and cable R
/// is already in the number — never self-asserted. The mic emits it as common-mode DC with its
/// audio differentially on top:
///
/// ```text
///   V+ = V_dc + s/2,  V− = V_dc − s/2    ⇒    common-mode = V_dc,  differential = s
/// ```
///
/// So the pedestal is genuinely present on the line yet **cancels at a balanced receiver's
/// difference** (which returns just the audio `s`) — exactly how a real balanced input separates
/// phantom from signal, emerging from the same common-mode rejection as hum and pickup. Whether the
/// mic runs is a **threshold in its own electronics** — terminal volts ≥
/// [`V_MIN_VOLTS`](Self::V_MIN_VOLTS), the same
/// species of device-internal physics as rail clipping, not a flag: unfed (or fed through enough
/// cable resistance to sag below the minimum) both legs sit dead at 0 V. No inputs; one balanced
/// output.
///
/// # The capsule tone (a declared boundary stand-in)
/// Acoustics are out of scope (PROJECT_PLAN §2), so the capsule does not transduce a real sound
/// field: it emits a **deterministic internal sine** as the differential audio `s`, a
/// stand-in for whatever the boundary would deliver. Two smoothed control params shape it —
/// [`LEVEL`](Self::LEVEL) (the differential amplitude in volts, mic-level by default) and
/// [`FREQ`](Self::FREQ) (Hz) — and an `f64` phase accumulator stepped at the analog rate makes it
/// reproducible with no ambient entropy (structural determinism, like [`SynthVoice`](super::SynthVoice)).
/// The deferred **"air link" story** replaces this with a real acoustic path (a vibrating source over
/// air, with a pressure↔volts transduction seam at each end); until then the sine is the seam.
///
/// The oscillator **free-runs regardless of power**: the phase accumulator always advances, so the
/// tone's phase (and determinism) never depends on the power state — a dead mic simply gates the
/// differential to 0 while its phase keeps ticking, and it resumes in phase when re-powered.
/// Unprepared (no analog rate yet — `compile` always prepares before `process`, so a compiled
/// schedule never hits this), the phase step is 0: the tone is silent while the pedestal logic is
/// unchanged (a powered-but-unprepared mic sits at a flat common-mode pedestal, no wiggle).
pub struct CondenserMic {
    /// Analog rate (Hz), baked at [`prepare`](Node::prepare); `0.0` until prepared (tone silent).
    rate_hz: f64,
    /// Oscillator phase in `[0, 1)`. Free-runs across blocks and power states.
    phase: f64,
    /// The [`LEVEL`](Self::LEVEL) default (differential amplitude, volts) — the constructed level and
    /// the fallback when run outside a schedule.
    default_level: f32,
    /// The compile-resolved DC operating point at the mic's terminals, delivered via
    /// [`resolve_phantom`](Node::resolve_phantom). Starts at 0 V (dead) and is set fresh by every
    /// compile, so the powered state is always the current patch's truth.
    pedestal: f32,
    /// The declared control params: [`LEVEL`](Self::LEVEL), [`FREQ`](Self::FREQ).
    params: [ParamDecl; 2],
    outputs: [OutputPort; 1],
}

impl CondenserMic {
    /// The capsule tone's differential amplitude (volts) — a smoothed level knob. Uncontrolled, it
    /// holds the constructed level.
    pub const LEVEL: ParamId = ParamId(0);
    /// The capsule tone's frequency (Hz) — smoothed. Defaults to
    /// [`FREQ_DEFAULT_HZ`](Self::FREQ_DEFAULT_HZ).
    pub const FREQ: ParamId = ParamId(1);

    /// The mic's constant-resistance DC load: 12.7 kΩ — the ~3 mA class. Hand math: fed by P48
    /// (48 V behind 3.4 kΩ common-mode), the operating point is `48·12 700/16 100 = 37.86 V`,
    /// drawing `37.86 / 12 700 ≈ 3.0 mA`.
    pub const Z_DC_OHMS: f32 = 12_700.0;
    /// The minimum terminal voltage the mic's electronics run at. Below it the impedance
    /// converter starves and the mic is dead — 35 V puts the healthy no-cable operating point
    /// (37.86 V) comfortably above and lets realistic series resistance kill it.
    pub const V_MIN_VOLTS: f32 = 35.0;

    /// The default capsule frequency: 1 kHz — a hand-calc-friendly, plainly audible reference tone.
    pub const FREQ_DEFAULT_HZ: f32 = 1_000.0;
    /// Level knob ceiling (volts): the capsule stays a small-signal source, so ~100 mV is a generous
    /// top; the ~10 mV default is the typical mic level.
    const LEVEL_MAX_VOLTS: f32 = 0.1;
    /// Frequency knob bounds (Hz): the audible band.
    const FREQ_MIN_HZ: f32 = 20.0;
    const FREQ_MAX_HZ: f32 = 20_000.0;
    /// De-zipper glide for the level/frequency knobs — short enough to feel instant, matching the
    /// preamp's switch/gain smoothing.
    const SMOOTH_MS: f32 = 5.0;

    /// A condenser mic emitting a capsule sine of differential amplitude `signal` (the
    /// [`LEVEL`](Self::LEVEL) default, ~10 mV is a typical mic level) at
    /// [`FREQ_DEFAULT_HZ`](Self::FREQ_DEFAULT_HZ), from balanced output impedance `z_out`. It powers
    /// up only when a compile resolves ≥ [`V_MIN_VOLTS`](Self::V_MIN_VOLTS) at its terminals — freshly
    /// built (or facing no engaged supply) it is dead.
    #[must_use]
    pub fn new(signal: Volts, z_out: Ohms) -> Self {
        let default_level = signal.get();
        Self {
            rate_hz: 0.0,
            phase: 0.0,
            default_level,
            pedestal: 0.0,
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

impl Node for CondenserMic {
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
        // free-running across a recompile (matching `SynthVoice`).
        self.rate_hz = rate.as_hz();
    }

    fn phantom_load(&self, port: usize) -> Option<PhantomLoad> {
        (port == 0)
            .then(|| PhantomLoad::new(Ohms::new(Self::Z_DC_OHMS), Volts::new(Self::V_MIN_VOLTS)))
    }

    fn resolve_phantom(&mut self, port: usize, volts: Volts) {
        if port == 0 {
            self.pedestal = volts.get();
        }
    }

    fn process(&mut self, params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
        // Whether the mic runs is a per-block threshold in its own electronics — decided by the
        // compile-time solve, never a flag. `gate` folds it into the per-sample differential so the
        // hot loop stays branch-light: alive ⇒ pedestal common-mode with the tone on top; dead ⇒
        // both legs pinned to 0 (no pedestal, no tone). `inv_rate` is 0 when unprepared, freezing
        // the phase (tone silent) while the pedestal still rides.
        let alive = self.pedestal >= Self::V_MIN_VOLTS;
        let cm = if alive { self.pedestal } else { 0.0 };
        let gate = if alive { 1.0 } else { 0.0 };
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
            // Differential audio: the capsule sine (f64 phase → f32 emission), gated by power.
            let s = gate * level * (std::f64::consts::TAU * self.phase).sin() as f32;
            let half = 0.5 * s;
            *h = cm + half;
            *c = cm - half;
            // Free-run the phase (advances even when dead), wrapping in [0, 1) like `SynthVoice`.
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

    /// A prepared mic with its pedestal resolved to `ped` and level `level` (default 1 kHz),
    /// processed for `n` samples outside a schedule; returns the `[hot, cold]` leg buffers.
    fn run_mic(level: f32, ped: f32, n: usize) -> [VoltageBuffer; 2] {
        let mut m = CondenserMic::new(Volts::new(level), Ohms::new(150.0));
        m.prepare(rate());
        m.resolve_phantom(0, Volts::new(ped));
        let mut out = [
            VoltageBuffer::zeros(n, rate()),
            VoltageBuffer::zeros(n, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        out
    }

    #[test]
    fn declares_a_balanced_output_with_its_dc_load() {
        let m = CondenserMic::new(Volts::new(0.01), Ohms::new(150.0));
        assert!(m.inputs().is_empty());
        assert_eq!(m.outputs()[0].lane_count(), 2);
        // The DC appetite is a port declaration, not a signal label.
        let load = m.phantom_load(0).expect("output 0 declares its DC load");
        assert_relative_eq!(load.z_dc().get(), 12_700.0);
        assert_relative_eq!(load.v_min().get(), 35.0);
        assert!(m.phantom_load(1).is_none(), "only output 0 draws phantom");
        // Two smoothed knobs: LEVEL (the constructed 0.01 V) and FREQ (default 1 kHz).
        let ps = m.params();
        assert_eq!(ps.len(), 2);
        assert_relative_eq!(ps[0].default, 0.01);
        assert_relative_eq!(ps[1].default, 1_000.0);
    }

    #[test]
    fn powered_mic_rides_the_capsule_tone_on_the_pedestal() {
        // Deliver the §17 hand-calc operating point 48·12 700/16 100 = 37.86 V (≥ 35 V minimum),
        // level 0.01 V, default 1 kHz, at 384 kHz. Two oracles, sample-wise over a full 256-sample
        // window (well inside one 384-sample period):
        //   1. common-mode (V+ + V−)/2 = the resolved pedestal, exactly — the tone is *purely*
        //      differential, so nothing of it leaks onto the common-mode axis.
        //   2. differential V+ − V− = level·sin(2π·f·k/rate), sample k — the capsule sine, with
        //      amplitude = the LEVEL param. Phase starts at 0 (sample 0's differential is 0).
        let (level, ped, f) = (0.01_f32, 37.86_f32, 1_000.0_f64);
        let [hot, cold] = run_mic(level, ped, 256);
        for k in 0..256 {
            let vp = hot.get(k).get();
            let vn = cold.get(k).get();
            assert_relative_eq!((vp + vn) / 2.0, ped, epsilon = 1e-3); // pedestal, every sample
            let want = f64::from(level) * (TAU * f * k as f64 / RATE_HZ).sin();
            // f32 note: the differential is recovered from two ~37.86 V legs, so it carries a few
            // ULP of cancellation noise at that magnitude — µV-scale, far under this tolerance.
            assert_relative_eq!(f64::from(vp - vn), want, epsilon = 1e-4);
        }
    }

    #[test]
    fn phase_continues_across_blocks() {
        // The phase accumulator persists: block 2's first sample continues block 1's sequence, no
        // per-block reset. Two 256-sample blocks on one mic must tile a single 512-sample reference
        // run of an identical mic — bit-for-bit (identical float ops in identical order).
        let (level, ped) = (0.01_f32, 37.86_f32);
        let [reference, _] = run_mic(level, ped, 512);

        let mut m = CondenserMic::new(Volts::new(level), Ohms::new(150.0));
        m.prepare(rate());
        m.resolve_phantom(0, Volts::new(ped));
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
    fn below_minimum_volts_is_dead_even_while_the_oscillator_runs() {
        // A sagged operating point below the 35 V minimum (e.g. 48·12 700/17 600 = 34.64 V through
        // 1.5 kΩ of cable) starves the mic's electronics: both legs sit at exactly 0 V — no
        // pedestal, no tone. The mic is prepared, so the oscillator *is* free-running; the power
        // gate still zeroes the differential. Bit-exact 0 (true silence, the house == exception).
        let [hot, cold] = run_mic(0.02, 34.64, 64);
        assert!(hot.as_slice().iter().all(|&v| v == 0.0));
        assert!(cold.as_slice().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn unresolved_mic_is_dead() {
        // Freshly built, nothing resolved (or an explicit 0 V from a supply-less compile): silent.
        let mut m = CondenserMic::new(Volts::new(0.02), Ohms::new(150.0));
        m.prepare(rate());
        let mut out = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        assert!(out[0].as_slice().iter().all(|&v| v == 0.0));
        assert!(out[1].as_slice().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn unprepared_mic_holds_a_flat_pedestal_with_no_tone() {
        // No `prepare` ⇒ no analog rate ⇒ the phase step is 0: a powered mic sits at a flat
        // common-mode pedestal with the tone silent, the pedestal logic otherwise unchanged.
        // (compile always prepares, so a real schedule never runs this path.)
        let mut m = CondenserMic::new(Volts::new(0.02), Ohms::new(150.0));
        m.resolve_phantom(0, Volts::new(37.86));
        let mut out = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        for k in 0..4 {
            assert_relative_eq!(out[0].get(k).get(), 37.86, epsilon = 1e-4); // hot = pedestal
            assert_relative_eq!(out[1].get(k).get(), 37.86, epsilon = 1e-4); // cold = pedestal
        }
    }
}
