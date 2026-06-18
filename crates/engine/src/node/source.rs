//! A test signal source.

use super::Node;
use crate::electrical::{InputZ, Ohms, OutputZ};
use crate::signal::{VoltageBuffer, Volts};

/// A source that emits a constant DC level from a real Thévenin output.
///
/// The simplest thing that injects a known voltage so chains have something to carry and hand
/// calculations stay trivial: the open-circuit output is a flat `level`, and the output
/// impedance makes the source a real electrical face that downstream loading divides against.
/// Steady-state output is pure DC, so the resistive dividers (not the cable rolloff, which
/// passes DC) are what an end-to-end test asserts. AC sources — oscillators driven by events —
/// arrive in Story 1.7; until then this is the canonical source.
///
/// No inputs; one output.
pub struct TestSource {
    level: Volts,
    outputs: [OutputZ; 1],
}

impl TestSource {
    /// A source emitting `level` from an output of impedance `z_out`.
    #[must_use]
    pub fn new(level: Volts, z_out: Ohms) -> Self {
        Self {
            level,
            outputs: [OutputZ::new(z_out)],
        }
    }
}

impl Node for TestSource {
    fn inputs(&self) -> &[InputZ] {
        &[]
    }

    fn outputs(&self) -> &[OutputZ] {
        &self.outputs
    }

    fn process(&mut self, _inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]) {
        outputs[0].fill(self.level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::AnalogRate;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn declares_one_output_no_inputs() {
        let src = TestSource::new(Volts::new(1.0), Ohms::new(150.0));
        assert!(src.inputs().is_empty());
        assert_eq!(src.outputs(), &[OutputZ::new(Ohms::new(150.0))]);
    }

    #[test]
    fn emits_a_constant_level() {
        let mut src = TestSource::new(Volts::new(0.775), Ohms::new(150.0));
        let mut out = [VoltageBuffer::zeros(8, rate())];
        src.process(&[], &mut out);
        assert!(out[0].as_slice().iter().all(|&v| v == 0.775));
    }
}
