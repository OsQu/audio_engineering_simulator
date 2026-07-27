//! A condenser microphone: a phantom-powered balanced acoustic→analog transducer.

use super::Node;
use crate::electrical::{Ohms, OutputZ, PhantomLoad};
use crate::param::Params;
use crate::port::{AcousticFace, InputPort, OutputPort};
use crate::signal::{Lane, Volts};

/// A condenser microphone — a balanced **acoustic→analog transducer** that needs **+48 V phantom
/// power** to operate.
///
/// It takes sound pressure at its capsule (one [`Acoustic`](crate::Domain::Acoustic) input) and
/// converts it to a balanced mic-level voltage: the differential audio is
/// `s = pressure · sensitivity`, where the sensitivity (set at [`new`](Self::new)) is in volts per
/// pascal.
/// A microphone is the *only* place the acoustic domain becomes the analog domain — the input-side
/// bookend to the speaker, and the acoustic peer of an AD converter (here the conversion is a plain
/// per-sample scale, no rate change).
///
/// The mic **declares** its DC appetite on its output port (a [`phantom_load`](Node::phantom_load):
/// [`Z_DC_OHMS`](Self::Z_DC_OHMS), [`V_MIN_VOLTS`](Self::V_MIN_VOLTS)) and receives its power from
/// the patch: `compile` solves the DC operating point against whatever engaged supply faces this
/// output (the §17 superposition split) and delivers the terminal volts via
/// [`resolve_phantom`](Node::resolve_phantom). The pedestal on the wire is therefore **earned from
/// the bias network** — sag from the feed resistors and cable R is already in the number — never
/// self-asserted. The mic emits it as common-mode DC with the transduced audio differentially on
/// top:
///
/// ```text
///   V+ = V_dc + s/2,  V− = V_dc − s/2    ⇒    common-mode = V_dc,  differential = s
/// ```
///
/// So the pedestal is genuinely present on the line yet **cancels at a balanced receiver's
/// difference** (which returns just the audio `s`) — exactly how a real balanced input separates
/// phantom from signal, emerging from the same common-mode rejection as hum and pickup. Whether the
/// mic runs is a **threshold in its own electronics** — terminal volts ≥
/// [`V_MIN_VOLTS`](Self::V_MIN_VOLTS), the same species of device-internal physics as rail clipping,
/// not a flag: unfed (or fed through enough cable resistance to sag below the minimum) both legs sit
/// dead at 0 V, regardless of the sound reaching the capsule.
///
/// # Sensitivity
/// A studio condenser is an efficient transducer: a typical large-diaphragm sensitivity is ~10 mV/Pa
/// (the calibration anchor is **1 Pa = 94 dB SPL**, so 10 mV/Pa delivers 10 mV of differential audio
/// from a 94 dB SPL source). It is a fixed property of the mic, not a knob — the mic has no control
/// params. An **unpatched** mic (no acoustic source at its capsule) is silent but for its pedestal:
/// there is no sound, so there is no audio.
///
/// One acoustic input; one balanced analog output.
pub struct CondenserMic {
    /// Transducer sensitivity, volts per pascal (`s = pressure · sensitivity`).
    sensitivity: f32,
    /// The compile-resolved DC operating point at the mic's terminals, delivered via
    /// [`resolve_phantom`](Node::resolve_phantom). Starts at 0 V (dead) and is set fresh by every
    /// compile, so the powered state is always the current patch's truth.
    pedestal: f32,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl CondenserMic {
    /// The mic's constant-resistance DC load: 12.7 kΩ — the ~3 mA class. Hand math: fed by P48
    /// (48 V behind 3.4 kΩ common-mode), the operating point is `48·12 700/16 100 = 37.86 V`,
    /// drawing `37.86 / 12 700 ≈ 3.0 mA`.
    pub const Z_DC_OHMS: f32 = 12_700.0;
    /// The minimum terminal voltage the mic's electronics run at. Below it the impedance
    /// converter starves and the mic is dead — 35 V puts the healthy no-cable operating point
    /// (37.86 V) comfortably above and lets realistic series resistance kill it.
    pub const V_MIN_VOLTS: f32 = 35.0;

    /// A condenser mic of `sensitivity` volts per pascal (~0.01 = 10 mV/Pa is a typical studio
    /// value), driving from balanced output impedance `z_out`. It powers up only when a compile
    /// resolves ≥ [`V_MIN_VOLTS`](Self::V_MIN_VOLTS) at its terminals — freshly built (or facing no
    /// engaged supply) it is dead.
    #[must_use]
    pub fn new(sensitivity: f32, z_out: Ohms) -> Self {
        Self {
            sensitivity,
            pedestal: 0.0,
            inputs: [AcousticFace::new().into()],
            outputs: [OutputZ::balanced(z_out).into()],
        }
    }
}

impl Node for CondenserMic {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
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

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // Whether the mic runs is a per-block threshold in its own electronics — decided by the
        // compile-time solve, never a flag. `gate` folds it into the per-sample differential so the
        // hot loop stays branch-light: alive ⇒ pedestal common-mode with the transduced audio on
        // top; dead ⇒ both legs pinned to 0 (no pedestal, no audio). The audio is the capsule
        // pressure scaled by sensitivity — a microphone is the acoustic→analog converter.
        let alive = self.pedestal >= Self::V_MIN_VOLTS;
        let cm = if alive { self.pedestal } else { 0.0 };
        let gate = if alive { self.sensitivity } else { 0.0 };
        let pressure = inputs[0].pressure().as_slice();
        let (hot, cold) = outputs.split_at_mut(1);
        let hot = hot[0].voltage_mut().as_mut_slice();
        let cold = cold[0].voltage_mut().as_mut_slice();
        for ((h, c), &p) in hot.iter_mut().zip(cold.iter_mut()).zip(pressure) {
            // Differential audio = pressure · sensitivity, gated by power; floats about the pedestal.
            let half = 0.5 * gate * p;
            *h = cm + half;
            *c = cm - half;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::spl_to_pascals;
    use crate::signal::{AnalogRate, Domain, Pascals, PressureBuffer, VoltageBuffer};
    use approx::assert_relative_eq;

    const RATE_HZ: f64 = 384_000.0;

    fn rate() -> AnalogRate {
        AnalogRate::new(RATE_HZ)
    }

    /// A mic of `sensitivity` (V/Pa) with its pedestal resolved to `ped`, fed a constant capsule
    /// pressure `p` for `n` samples; returns the `[hot, cold]` leg buffers.
    fn run_mic(sensitivity: f32, ped: f32, p: Pascals, n: usize) -> [VoltageBuffer; 2] {
        let mut m = CondenserMic::new(sensitivity, Ohms::new(150.0));
        m.resolve_phantom(0, Volts::new(ped));
        let in_lane = [Lane::Pressure(PressureBuffer::from_pascals(
            vec![p.get(); n],
            rate(),
        ))];
        let mut out = [
            Lane::Voltage(VoltageBuffer::zeros(n, rate())),
            Lane::Voltage(VoltageBuffer::zeros(n, rate())),
        ];
        m.process(&Params::EMPTY, &in_lane, &mut out);
        let mut it = out.into_iter().map(|l| match l {
            Lane::Voltage(b) => b,
            _ => unreachable!(),
        });
        [it.next().unwrap(), it.next().unwrap()]
    }

    #[test]
    fn declares_an_acoustic_input_a_balanced_output_and_its_dc_load() {
        let m = CondenserMic::new(0.01, Ohms::new(150.0));
        assert_eq!(m.inputs().len(), 1);
        assert_eq!(m.inputs()[0].domain(), Domain::Acoustic);
        assert_eq!(m.outputs()[0].lane_count(), 2); // balanced = two conductors
        // The DC appetite is a port declaration, not a signal label.
        let load = m.phantom_load(0).expect("output 0 declares its DC load");
        assert_relative_eq!(load.z_dc().get(), 12_700.0);
        assert_relative_eq!(load.v_min().get(), 35.0);
        assert!(m.phantom_load(1).is_none(), "only output 0 draws phantom");
        // No control params — sensitivity is a fixed transducer property, not a knob.
        assert!(m.params().is_empty());
    }

    #[test]
    fn powered_mic_transduces_pressure_on_the_pedestal() {
        // Deliver the §17 operating point 48·12 700/16 100 = 37.86 V (≥ 35 V minimum). At the 94 dB
        // SPL anchor (1 Pa) with 10 mV/Pa sensitivity, the differential is 1 Pa · 0.01 V/Pa = 10 mV,
        // riding as common-mode 37.86 V: hot = 37.86 + 0.005, cold = 37.86 − 0.005. Two oracles:
        //   1. common-mode (V+ + V−)/2 = the pedestal, exactly — the audio is purely differential.
        //   2. differential V+ − V− = 0.010 V — pressure · sensitivity, the transduced 94 dB source.
        let (sens, ped) = (0.01_f32, 37.86_f32);
        let [hot, cold] = run_mic(sens, ped, spl_to_pascals(94.0), 8);
        for k in 0..8 {
            let (vp, vn) = (hot.get(k).get(), cold.get(k).get());
            assert_relative_eq!((vp + vn) / 2.0, ped, epsilon = 1e-3); // pedestal
            // 94 dB SPL = 1.00237 Pa · 0.01 = 0.0100237 V differential. The tolerance is loose
            // because the differential is recovered from two ~37.86 V legs: at f32 that pedestal's
            // ULP (~4 µV) is the cancellation-noise floor on the difference — which is exactly why a
            // mic's transduction magnitude is asserted on the pedestal-free dynamic mic instead.
            assert_relative_eq!(vp - vn, 0.010_023_7, epsilon = 1e-4);
        }
    }

    #[test]
    fn below_minimum_volts_is_dead_regardless_of_the_sound() {
        // A sagged operating point below the 35 V minimum (e.g. 48·12 700/17 600 = 34.64 V through
        // 1.5 kΩ of cable) starves the mic's electronics: both legs sit at exactly 0 V — no
        // pedestal, no audio — even with a loud 120 dB SPL source at the capsule. Bit-exact 0.
        let [hot, cold] = run_mic(0.01, 34.64, spl_to_pascals(120.0), 8);
        assert!(hot.as_slice().iter().all(|&v| v == 0.0));
        assert!(cold.as_slice().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn unresolved_mic_is_dead() {
        // Freshly built, nothing resolved (or an explicit 0 V from a supply-less compile): silent,
        // whatever the capsule hears.
        let [hot, cold] = run_mic(0.01, 0.0, spl_to_pascals(94.0), 4);
        assert!(hot.as_slice().iter().all(|&v| v == 0.0));
        assert!(cold.as_slice().iter().all(|&v| v == 0.0));
    }
}
