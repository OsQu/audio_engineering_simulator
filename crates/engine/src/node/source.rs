//! A test signal source.

use super::Node;
use crate::electrical::{Ohms, OutputZ};
use crate::param::Params;
use crate::port::{InputPort, OutputPort};
use crate::signal::{Lane, Volts};

/// A source that emits a constant DC level from a real Thévenin output.
///
/// The simplest thing that injects a known voltage so chains have something to carry and hand
/// calculations stay trivial: the open-circuit output is a flat `level`, and the output
/// impedance makes the source a real electrical face that downstream loading divides against.
/// Steady-state output is pure DC, so the resistive dividers (not the cable rolloff, which
/// passes DC) are what an end-to-end test asserts. This is the simple constant-level test source;
/// an AC source driven by events is the [`SynthVoice`](crate::SynthVoice).
///
/// No inputs; one output.
pub struct TestSource {
    level: Volts,
    outputs: [OutputPort; 1],
}

impl TestSource {
    /// A source emitting `level` from an output of impedance `z_out`.
    #[must_use]
    pub fn new(level: Volts, z_out: Ohms) -> Self {
        Self {
            level,
            outputs: [OutputZ::new(z_out).into()],
        }
    }
}

impl Node for TestSource {
    fn inputs(&self) -> &[InputPort] {
        &[]
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
        outputs[0].voltage_mut().fill(self.level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, VoltageBuffer};
    use crate::test_util::process_voltage;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn declares_one_output_no_inputs() {
        let src = TestSource::new(Volts::new(1.0), Ohms::new(150.0));
        assert!(src.inputs().is_empty());
        assert_eq!(
            src.outputs(),
            &[OutputPort::Analog(OutputZ::new(Ohms::new(150.0)))]
        );
    }

    #[test]
    fn emits_a_constant_level() {
        let mut src = TestSource::new(Volts::new(0.775), Ohms::new(150.0));
        let mut out = [VoltageBuffer::zeros(8, rate())];
        process_voltage(&mut src, &[], &mut out);
        assert!(out[0].as_slice().iter().all(|&v| v == 0.775));
    }
}
