//! A gain / preamp stage.

use super::Device;
use crate::electrical::{InputZ, Ohms, OutputZ};
use crate::signal::{VoltageBuffer, Volts};

/// A gain stage with a finite supply rail: `out = clamp(in · gain, ±rail)`.
///
/// Models a buffered active stage — a real `InputZ` it presents to its source and a real
/// output impedance it drives downstream, with a voltage gain in between. The **rail** is the
/// supply voltage the output can't swing past; beyond it the signal clips, hard, in volts.
/// That the rail and clamp live here (not as a flag) is the point: headroom and clipping
/// *emerge* from the physics. The clipping **phenomenon** is validated in Story 1.4; here the
/// stage simply enforces the rail, and end-to-end tests stay below it so the transform is linear.
///
/// One input; one output.
pub struct GainStage {
    gain: f32,
    rail: f32,
    inputs: [InputZ; 1],
    outputs: [OutputZ; 1],
}

impl GainStage {
    /// A stage with voltage gain `gain`, clipping at `±rail`, presenting `z_in` and driving
    /// from `z_out`.
    ///
    /// # Panics
    /// Panics unless `rail` is finite and `> 0` — a non-positive or non-finite rail is a setup
    /// bug (it would make the clamp degenerate). Checked here at construction, never on the hot
    /// path.
    #[must_use]
    pub fn new(gain: f32, rail: Volts, z_in: InputZ, z_out: Ohms) -> Self {
        let rail = rail.get();
        assert!(
            rail.is_finite() && rail > 0.0,
            "GainStage rail must be finite and > 0, got {rail}"
        );
        Self {
            gain,
            rail,
            inputs: [z_in],
            outputs: [OutputZ::new(z_out)],
        }
    }
}

impl Device for GainStage {
    fn inputs(&self) -> &[InputZ] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputZ] {
        &self.outputs
    }

    fn process(&mut self, inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]) {
        let (gain, rail) = (self.gain, self.rail);
        let src = inputs[0].as_slice();
        for (out, &v) in outputs[0].as_mut_slice().iter_mut().zip(src) {
            *out = (v * gain).clamp(-rail, rail);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::AnalogRate;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn stage(gain: f32, rail: f32) -> GainStage {
        GainStage::new(
            gain,
            Volts::new(rail),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
    }

    #[test]
    fn declares_faces() {
        let s = stage(2.0, 10.0);
        assert_eq!(s.inputs(), &[InputZ::new(Ohms::new(10_000.0))]);
        assert_eq!(s.outputs(), &[OutputZ::new(Ohms::new(150.0))]);
    }

    #[test]
    fn applies_gain_below_the_rail() {
        // 0.5 V × 4 = 2.0 V, well under a 10 V rail → linear.
        let mut s = stage(4.0, 10.0);
        let mut input = [VoltageBuffer::zeros(4, rate())];
        input[0].fill(Volts::new(0.5));
        let mut out = [VoltageBuffer::zeros(4, rate())];
        s.process(&input, &mut out);
        assert!(out[0].as_slice().iter().all(|&v| (v - 2.0).abs() < 1e-6));
    }

    #[test]
    fn clips_hard_at_the_rail() {
        // 0.5 V × 4 = 2.0 V wanted, but the rail is 1.5 V → clamps to +1.5 V; the negative
        // half clamps to −1.5 V. Symmetric hard clip in volts.
        let mut s = stage(4.0, 1.5);
        let mut input = [VoltageBuffer::zeros(2, rate())];
        input[0].set(0, Volts::new(0.5));
        input[0].set(1, Volts::new(-0.5));
        let mut out = [VoltageBuffer::zeros(2, rate())];
        s.process(&input, &mut out);
        assert_relative_eq!(out[0].get(0).get(), 1.5, epsilon = 1e-6);
        assert_relative_eq!(out[0].get(1).get(), -1.5, epsilon = 1e-6);
    }

    #[test]
    #[should_panic(expected = "rail must be finite and > 0")]
    fn rejects_nonpositive_rail() {
        let _ = stage(1.0, 0.0);
    }
}
