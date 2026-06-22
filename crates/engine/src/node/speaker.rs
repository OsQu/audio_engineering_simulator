//! The speaker: the chain's analog terminus.

use super::Node;
use crate::electrical::{InputZ, Ohms, OutputZ};
use crate::param::Params;
use crate::port::{InputPort, OutputPort};
use crate::signal::Lane;

/// A speaker — the **terminus** of the simulated signal chain: `out = sensitivity · in`.
///
/// This is where the simulation stops. We deliberately do **not** model acoustics (no cone
/// physics, no V→SPL, no air or room — PROJECT_PLAN §5.5): the speaker stays in the voltage
/// domain, applying a single **sensitivity** gain to the drive voltage it's fed, and its output
/// is the voltage the harness *taps and captures* as "what we hear" (the implicit
/// analog→digital capture lives off-engine, in the render harness — see `IMPLEMENTATION_PLAN.md`
/// Story 2.1). The output port is therefore a benign terminus fiction: nothing in the graph
/// loads it, so its `OutputZ` is nominal.
///
/// Flat this story — a frequency-response curve is cosmetic and deliberately deferred (it would
/// reuse [`OnePole`](crate::OnePole) or, later, a biquad). A speaker's real low-impedance load
/// and power transfer are out of scope (we model neither current draw nor amplifier power), so it
/// presents a high, **bridging** `InputZ` and does not load its source.
///
/// One input; one output.
pub struct Speaker {
    sensitivity: f32,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl Speaker {
    /// A nominal output impedance for the terminus tap. Nothing downstream loads the speaker, so
    /// this value never enters a divider solve — it exists only because every output is a
    /// [`Thevenin`](crate::Thevenin) source with a real `Zout`.
    const Z_OUT: f32 = 100.0;

    /// A speaker with voltage `sensitivity`, presenting the bridging input impedance `z_in`.
    ///
    /// `sensitivity` is a plain voltage gain (not a smoothed control param) — flat and fixed for
    /// this story. Use `1.0` for a unity monitoring tap; the render level is then governed by the
    /// upstream chain and the capture's monitor reference.
    #[must_use]
    pub fn new(sensitivity: f32, z_in: InputZ) -> Self {
        Self {
            sensitivity,
            inputs: [z_in.into()],
            outputs: [OutputZ::new(Ohms::new(Self::Z_OUT)).into()],
        }
    }
}

impl Node for Speaker {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let sensitivity = self.sensitivity;
        let src = inputs[0].voltage().as_slice();
        let out = outputs[0].voltage_mut().as_mut_slice();
        for (o, &v) in out.iter_mut().zip(src) {
            *o = v * sensitivity;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, VoltageBuffer, Volts};
    use crate::test_util::process_voltage;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn speaker(sensitivity: f32) -> Speaker {
        Speaker::new(sensitivity, InputZ::new(Ohms::new(10_000.0)))
    }

    #[test]
    fn declares_faces() {
        let s = speaker(1.0);
        assert_eq!(
            s.inputs(),
            &[InputPort::Analog(InputZ::new(Ohms::new(10_000.0)))]
        );
        assert_eq!(
            s.outputs(),
            &[OutputPort::Analog(OutputZ::new(Ohms::new(Speaker::Z_OUT)))]
        );
    }

    #[test]
    fn applies_sensitivity_gain() {
        // A unity tap passes the drive voltage straight through.
        let mut s = speaker(1.0);
        let mut input = [VoltageBuffer::zeros(4, rate())];
        input[0].fill(Volts::new(0.75));
        let mut out = [VoltageBuffer::zeros(4, rate())];
        process_voltage(&mut s, &input, &mut out);
        assert!(out[0].as_slice().iter().all(|&v| (v - 0.75).abs() < 1e-6));
    }

    #[test]
    fn scales_by_sensitivity() {
        // sensitivity 0.5 halves the drive voltage; flat, no rail.
        let mut s = speaker(0.5);
        let mut input = [VoltageBuffer::zeros(2, rate())];
        input[0].set(0, Volts::new(2.0));
        input[0].set(1, Volts::new(-2.0));
        let mut out = [VoltageBuffer::zeros(2, rate())];
        process_voltage(&mut s, &input, &mut out);
        assert_relative_eq!(out[0].get(0).get(), 1.0, epsilon = 1e-6);
        assert_relative_eq!(out[0].get(1).get(), -1.0, epsilon = 1e-6);
    }
}
