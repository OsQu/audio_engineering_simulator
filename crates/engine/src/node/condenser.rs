//! A condenser microphone: a phantom-powered balanced source.

use super::Node;
use crate::electrical::{Ohms, OutputZ, PhantomLoad};
use crate::param::Params;
use crate::port::{InputPort, OutputPort};
use crate::signal::{Lane, Volts};

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
pub struct CondenserMic {
    /// The differential audio level the capsule produces (a constant test level).
    signal: f32,
    /// The compile-resolved DC operating point at the mic's terminals, delivered via
    /// [`resolve_phantom`](Node::resolve_phantom). Starts at 0 V (dead) and is set fresh by every
    /// compile, so the powered state is always the current patch's truth.
    pedestal: f32,
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

    /// A condenser mic emitting differential `signal` from balanced output impedance `z_out`.
    /// It powers up only when a compile resolves ≥ [`V_MIN_VOLTS`](Self::V_MIN_VOLTS) at its
    /// terminals — freshly built (or facing no engaged supply) it is dead.
    #[must_use]
    pub fn new(signal: Volts, z_out: Ohms) -> Self {
        Self {
            signal: signal.get(),
            pedestal: 0.0,
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

    fn phantom_load(&self, port: usize) -> Option<PhantomLoad> {
        (port == 0)
            .then(|| PhantomLoad::new(Ohms::new(Self::Z_DC_OHMS), Volts::new(Self::V_MIN_VOLTS)))
    }

    fn resolve_phantom(&mut self, port: usize, volts: Volts) {
        if port == 0 {
            self.pedestal = volts.get();
        }
    }

    fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
        // Runs iff the resolved terminal volts clear the mic's minimum — a threshold in the mic's
        // own electronics, decided by the compile-time solve, never a flag. Below it: dead (0 V on
        // both legs, no pedestal, no signal).
        let (cm, half) = if self.pedestal >= Self::V_MIN_VOLTS {
            (self.pedestal, self.signal * 0.5)
        } else {
            (0.0, 0.0)
        };
        let (hot, cold) = outputs.split_at_mut(1);
        hot[0].voltage_mut().fill(Volts::new(cm + half));
        cold[0].voltage_mut().fill(Volts::new(cm - half));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, VoltageBuffer};
    use crate::test_util::process_voltage;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
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
    }

    #[test]
    fn resolved_mic_puts_pedestal_common_mode_and_signal_differential() {
        // Deliver the §17 hand-calc operating point, 48·12 700/16 100 = 37.86 V (≥ 35 V minimum):
        //   signal = 0.02 V ⇒ V+ = 37.86 + 0.01, V− = 37.86 − 0.01
        //   common-mode (V+ + V−)/2 = 37.86 V exactly; differential V+ − V− = 0.02 V = the signal.
        let mut m = CondenserMic::new(Volts::new(0.02), Ohms::new(150.0));
        m.resolve_phantom(0, Volts::new(37.86));
        let mut out = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        let vp = out[0].get(0).get();
        let vn = out[1].get(0).get();
        assert_relative_eq!((vp + vn) / 2.0, 37.86, epsilon = 1e-4); // the resolved pedestal
        assert_relative_eq!(vp - vn, 0.02, epsilon = 1e-5); // signal differential, no DC in it
    }

    #[test]
    fn below_minimum_volts_is_dead() {
        // A sagged operating point below the 35 V minimum (e.g. 48·12 700/17 600 = 34.64 V through
        // 1.5 kΩ of cable) starves the mic's electronics: no pedestal, no signal — dead, not just
        // quieter. The threshold lives in the mic, not in any flag.
        let mut m = CondenserMic::new(Volts::new(0.02), Ohms::new(150.0));
        m.resolve_phantom(0, Volts::new(34.64));
        let mut out = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        assert!(out[0].as_slice().iter().all(|&v| v == 0.0));
        assert!(out[1].as_slice().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn unresolved_mic_is_dead() {
        // Freshly built, nothing resolved (or an explicit 0 V from a supply-less compile): silent.
        let mut m = CondenserMic::new(Volts::new(0.02), Ohms::new(150.0));
        let mut out = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        assert!(out[0].as_slice().iter().all(|&v| v == 0.0));
        assert!(out[1].as_slice().iter().all(|&v| v == 0.0));
    }
}
