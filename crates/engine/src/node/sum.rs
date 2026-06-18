//! A passive summing node.

use super::Node;
use crate::electrical::{InputZ, Ohms, OutputZ};
use crate::signal::VoltageBuffer;

/// A passive summing node: its open-circuit output is the **sum** of its input voltages.
///
/// Each input presents a real `InputZ` and the node drives from a real output impedance, so
/// the *loading* loss is honest — it emerges from the connection dividers (a low `InputZ`
/// loads its source down; the output impedance is divided against the next stage). What's
/// deliberately **simplified** is the summing law itself: a real passive resistive mixer also
/// attenuates (roughly `1/N`) and couples its sources to one another through the shared bus.
/// We model a unity sum and leave that make-up/attenuation to a downstream
/// [`GainStage`](super::GainStage); the inter-source bus coupling is not modeled. This is the
/// "correct-enough, never false" line (PROJECT_PLAN §3): the loading is real, the
/// mixing-resistor network is abstracted.
///
/// `n` inputs; one output.
pub struct PassiveSum {
    inputs: Vec<InputZ>,
    outputs: [OutputZ; 1],
}

impl PassiveSum {
    /// A summing node with one input per entry of `inputs`, driving from `z_out`.
    ///
    /// # Panics
    /// Panics if `inputs` is empty — a sum needs at least one input. Checked at construction,
    /// never on the hot path.
    #[must_use]
    pub fn new(inputs: Vec<InputZ>, z_out: Ohms) -> Self {
        assert!(!inputs.is_empty(), "PassiveSum needs at least one input");
        Self {
            inputs,
            outputs: [OutputZ::new(z_out)],
        }
    }
}

impl Node for PassiveSum {
    fn inputs(&self) -> &[InputZ] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputZ] {
        &self.outputs
    }

    fn process(&mut self, inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]) {
        let out = outputs[0].as_mut_slice();
        out.fill(0.0);
        for input in inputs {
            for (o, &v) in out.iter_mut().zip(input.as_slice()) {
                *o += v;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, Volts};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn sum(n: usize) -> PassiveSum {
        PassiveSum::new(vec![InputZ::new(Ohms::new(10_000.0)); n], Ohms::new(150.0))
    }

    #[test]
    fn declares_n_inputs_one_output() {
        let s = sum(3);
        assert_eq!(s.inputs().len(), 3);
        assert_eq!(s.outputs(), &[OutputZ::new(Ohms::new(150.0))]);
    }

    #[test]
    fn sums_its_inputs() {
        // Open-circuit output = 0.3 + 0.4 = 0.7 V (unity sum).
        let mut s = sum(2);
        let mut ins = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        ins[0].fill(Volts::new(0.3));
        ins[1].fill(Volts::new(0.4));
        let mut out = [VoltageBuffer::zeros(4, rate())];
        s.process(&ins, &mut out);
        assert!(out[0].as_slice().iter().all(|&v| (v - 0.7).abs() < 1e-6));
    }

    #[test]
    fn opposite_signals_cancel() {
        // +0.5 and −0.5 sum to silence — the difference falls out of the same add.
        let mut s = sum(2);
        let mut ins = [
            VoltageBuffer::zeros(2, rate()),
            VoltageBuffer::zeros(2, rate()),
        ];
        ins[0].fill(Volts::new(0.5));
        ins[1].fill(Volts::new(-0.5));
        let mut out = [VoltageBuffer::zeros(2, rate())];
        s.process(&ins, &mut out);
        assert_relative_eq!(out[0].get(0).get(), 0.0, epsilon = 1e-6);
    }

    #[test]
    #[should_panic(expected = "at least one input")]
    fn rejects_empty() {
        let _ = PassiveSum::new(vec![], Ohms::new(150.0));
    }
}
