//! The two-conductor (V+, V−) balanced-line interface: a driver and a receiver.
//!
//! A balanced line carries the audio **differentially** across two conductors. The driver puts
//! the signal on as `V+ = +s/2`, `V− = −s/2` (so the differential `V+ − V−` equals the input —
//! unity, no level change), and the receiver recovers it as `V+ − V−`. Interference that couples
//! equally onto both conductors (cable pickup, hum, phantom) is **common-mode** and cancels at that
//! subtraction: common-mode rejection *emerges* from the difference, it is not a flag. The
//! conductors live as two adjacent lanes in the schedule pool ("buffer = conductor"); a node
//! reads/writes them in port-then-conductor order.

use super::Node;
use crate::electrical::{InputZ, Ohms, OutputZ};
use crate::param::Params;
use crate::port::{InputPort, OutputPort};
use crate::signal::Lane;

/// Drives a single-ended input onto a **balanced** pair: `V+ = +in/2`, `V− = −in/2`.
///
/// The differential output `V+ − V−` equals the input (unity gain), and the common-mode it
/// produces is zero — a clean differential drive. One **unbalanced** input; one **balanced**
/// (two-conductor) output.
pub struct BalancedDriver {
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl BalancedDriver {
    /// A driver presenting unbalanced input impedance `z_in` and driving a balanced output of
    /// **differential** output impedance `z_out`.
    #[must_use]
    pub fn new(z_in: InputZ, z_out: Ohms) -> Self {
        Self {
            inputs: [z_in.into()],
            outputs: [OutputZ::balanced(z_out).into()],
        }
    }
}

impl Node for BalancedDriver {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // One balanced output port = two conductor lanes: [0] = V+, [1] = V−.
        let (hot, cold) = outputs.split_at_mut(1);
        let vp = hot[0].voltage_mut().as_mut_slice();
        let vn = cold[0].voltage_mut().as_mut_slice();
        for ((p, n), &x) in vp
            .iter_mut()
            .zip(vn.iter_mut())
            .zip(inputs[0].voltage().as_slice())
        {
            let half = x * 0.5;
            *p = half;
            *n = -half;
        }
    }
}

/// Recovers a **balanced** pair to single-ended by taking the difference: `out = V+ − V−`.
///
/// This subtraction is the whole point of a balanced line: a differential signal (`V+ = +s/2`,
/// `V− = −s/2`) comes back as `s`, while anything common to both conductors — interference that
/// coupled equally onto the cable — cancels. One **balanced** (two-conductor) input; one
/// **unbalanced** output.
pub struct BalancedReceiver {
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl BalancedReceiver {
    /// A receiver presenting **differential** balanced input impedance `z_in` and driving a
    /// single-ended output of impedance `z_out`.
    #[must_use]
    pub fn new(z_in: Ohms, z_out: Ohms) -> Self {
        Self {
            inputs: [InputZ::balanced(z_in).into()],
            outputs: [OutputZ::new(z_out).into()],
        }
    }
}

impl Node for BalancedReceiver {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // One balanced input port = two conductor lanes: [0] = V+, [1] = V−.
        let out = outputs[0].voltage_mut().as_mut_slice();
        let vp = inputs[0].voltage().as_slice();
        let vn = inputs[1].voltage().as_slice();
        for (o, (&p, &n)) in out.iter_mut().zip(vp.iter().zip(vn)) {
            *o = p - n;
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

    #[test]
    fn driver_declares_unbalanced_in_balanced_out() {
        let d = BalancedDriver::new(InputZ::new(Ohms::new(10_000.0)), Ohms::new(200.0));
        assert_eq!(d.inputs()[0].lane_count(), 1);
        assert_eq!(d.outputs()[0].lane_count(), 2);
    }

    #[test]
    fn receiver_declares_balanced_in_unbalanced_out() {
        let r = BalancedReceiver::new(Ohms::new(20_000.0), Ohms::new(150.0));
        assert_eq!(r.inputs()[0].lane_count(), 2);
        assert_eq!(r.outputs()[0].lane_count(), 1);
    }

    #[test]
    fn driver_splits_into_antiphase_halves() {
        // A 2 V input becomes V+ = +1 V, V− = −1 V (differential 2 V, common-mode 0).
        let mut d = BalancedDriver::new(InputZ::new(Ohms::new(1e9)), Ohms::new(1.0));
        let mut input = [VoltageBuffer::zeros(4, rate())];
        input[0].fill(Volts::new(2.0));
        let mut out = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut d, &input, &mut out);
        assert!(out[0].as_slice().iter().all(|&v| (v - 1.0).abs() < 1e-6));
        assert!(out[1].as_slice().iter().all(|&v| (v + 1.0).abs() < 1e-6));
    }

    #[test]
    fn receiver_takes_the_difference() {
        // V+ = 6, V− = 4 → out = 2 (the 5 V common-mode pedestal is rejected).
        let mut r = BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0));
        let mut ins = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        ins[0].fill(Volts::new(6.0));
        ins[1].fill(Volts::new(4.0));
        let mut out = [VoltageBuffer::zeros(4, rate())];
        process_voltage(&mut r, &ins, &mut out);
        assert!(out[0].as_slice().iter().all(|&v| (v - 2.0).abs() < 1e-6));
    }

    #[test]
    fn driver_then_receiver_is_unity() {
        // Drive 2 V differential, recover it: V± = ±1 → V+ − V− = 2.
        let mut d = BalancedDriver::new(InputZ::new(Ohms::new(1e9)), Ohms::new(1.0));
        let mut r = BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0));
        let mut input = [VoltageBuffer::zeros(4, rate())];
        input[0].fill(Volts::new(2.0));
        let mut pair = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        process_voltage(&mut d, &input, &mut pair);
        let mut out = [VoltageBuffer::zeros(4, rate())];
        process_voltage(&mut r, &pair, &mut out);
        assert_relative_eq!(out[0].get(0).get(), 2.0, epsilon = 1e-6);
    }
}
