//! A condenser microphone: a phantom-powered balanced source.

use super::Node;
use crate::electrical::{Ohms, OutputZ};
use crate::param::Params;
use crate::port::{InputPort, OutputPort};
use crate::signal::{Lane, Volts};

/// The standard phantom supply voltage.
const PHANTOM_VOLTS: f32 = 48.0;

/// A condenser microphone — a balanced source that needs **+48 V phantom power** to operate.
///
/// Phantom rides the balanced pair as **common-mode DC**: the same +48 V on both conductors,
/// supplied in reality by the preamp *upstream*. Against the pull-based DAG that direction would
/// run backwards, so (per the Story 1.5 design notes — an informed approximation, §5.3) the mic
/// emits the phantom directly when powered, with its audio differentially:
///
/// ```text
///   V+ = 48 + s/2,  V− = 48 − s/2    ⇒    common-mode = 48 V,  differential = s
/// ```
///
/// So the +48 V is genuinely present on the line yet **cancels at a balanced receiver's
/// difference** (which returns just the audio `s`) — exactly how a real balanced input separates
/// phantom from signal, emerging from the same common-mode rejection as hum and pickup. Unpowered,
/// the mic is dead: it draws no phantom and produces nothing. The current draw and any voltage sag
/// are deferred (§5.3). No inputs; one balanced output.
pub struct CondenserMic {
    /// The differential audio level the capsule produces (a constant test level for now; the
    /// event-driven oscillator arrives in Story 1.7).
    signal: f32,
    /// Whether +48 V phantom is supplied — the mic is silent without it.
    powered: bool,
    outputs: [OutputPort; 1],
}

impl CondenserMic {
    /// A **powered** condenser mic emitting differential `signal` from balanced output impedance
    /// `z_out`. Use [`unpowered`](Self::unpowered) to model phantom being absent.
    #[must_use]
    pub fn new(signal: Volts, z_out: Ohms) -> Self {
        Self {
            signal: signal.get(),
            powered: true,
            outputs: [OutputZ::balanced(z_out).into()],
        }
    }

    /// The same mic with phantom power **removed** — it draws no +48 V and produces silence.
    #[must_use]
    pub fn unpowered(mut self) -> Self {
        self.powered = false;
        self
    }
}

impl Node for CondenserMic {
    fn inputs(&self) -> &[InputPort] {
        &[]
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
        // Powered: +48 V common-mode pedestal with the audio differentially on top. Unpowered: dead.
        let (cm, half) = if self.powered {
            (PHANTOM_VOLTS, self.signal * 0.5)
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
    fn declares_a_balanced_output_no_inputs() {
        let m = CondenserMic::new(Volts::new(0.01), Ohms::new(150.0));
        assert!(m.inputs().is_empty());
        assert_eq!(m.outputs()[0].lane_count(), 2);
    }

    #[test]
    fn powered_puts_phantom_common_mode_and_signal_differential() {
        // signal = 0.02 V ⇒ V+ = 48 + 0.01, V− = 48 − 0.01.
        //   common-mode (V+ + V−)/2 = 48 V exactly; differential V+ − V− = 0.02 V = the signal.
        let mut m = CondenserMic::new(Volts::new(0.02), Ohms::new(150.0));
        let mut out = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        let vp = out[0].get(0).get();
        let vn = out[1].get(0).get();
        assert_relative_eq!((vp + vn) / 2.0, 48.0, epsilon = 1e-5); // phantom present common-mode
        assert_relative_eq!(vp - vn, 0.02, epsilon = 1e-5); // signal differential, no 48 V
    }

    #[test]
    fn unpowered_is_silent() {
        // No phantom ⇒ no +48 V and no signal: both conductors sit at 0.
        let mut m = CondenserMic::new(Volts::new(0.02), Ohms::new(150.0)).unpowered();
        let mut out = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut m, &[], &mut out);
        assert!(out[0].as_slice().iter().all(|&v| v == 0.0));
        assert!(out[1].as_slice().iter().all(|&v| v == 0.0));
    }
}
