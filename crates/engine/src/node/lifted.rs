//! `Lifted`: runs a single-conductor node independently on each conductor of a balanced port.

use super::Node;
use crate::electrical::{InputZ, OutputZ};
use crate::rng::Rng;
use crate::signal::{AnalogRate, VoltageBuffer};

/// Wraps a per-conductor node as `conductors` independent lanes — the **per-conductor lift**.
///
/// A balanced line is two ordinary wires, so an inline processor (a DC blocker, a gain) is just
/// that processor applied to each leg independently — its own state, identical coefficients.
/// `compile` infers a [per-conductor](Node::per_conductor) node's conductor count from the wiring
/// (Story 1.5 detour) and, when it is >1, wraps it here: one lane per conductor (the original plus
/// [`replicate`](Node::replicate)d copies), with the faces widened to that conductor count.
/// Because both legs run the *identical* transform, whatever is common to them stays common and
/// cancels at the receiver — common-mode rejection emerges, with no "balanced" variant of the
/// node. (Per-leg *asymmetry* would be the finite-CMRR case, deferred.)
///
/// The inner node has one input and one output port (or none and one); the lift maps conductor
/// `k` to lane `k`. Internal — only `compile` constructs it.
pub(crate) struct Lifted {
    lanes: Vec<Box<dyn Node>>,
    inputs: Vec<InputZ>,
    outputs: Vec<OutputZ>,
    has_input: bool,
}

impl Lifted {
    /// Lift `inner` across `conductors` lanes: `inner` is one lane, the rest are
    /// [`replicate`](Node::replicate)d. The faces are `inner`'s, widened to `conductors`.
    ///
    /// # Panics
    /// Panics unless `conductors >= 1` and `inner` has one input and one output port (or none and
    /// one) — both guaranteed by `compile`, never reached on the hot path.
    pub(crate) fn new(inner: Box<dyn Node>, conductors: usize) -> Self {
        assert!(conductors >= 1, "a lift needs at least one conductor");
        assert!(
            inner.inputs().len() <= 1 && inner.outputs().len() == 1,
            "the per-conductor lift supports only 1-in/1-out (or 0-in/1-out) nodes"
        );
        let has_input = inner.inputs().len() == 1;
        let inputs = inner
            .inputs()
            .iter()
            .map(|f| InputZ::with_conductors(f.z_in(), conductors))
            .collect();
        let outputs = inner
            .outputs()
            .iter()
            .map(|f| OutputZ::with_conductors(f.z_out(), conductors))
            .collect();
        // One lane per conductor: replicas for the extra legs, then the original.
        let mut lanes: Vec<Box<dyn Node>> = (1..conductors).map(|_| inner.replicate()).collect();
        lanes.push(inner);
        Self {
            lanes,
            inputs,
            outputs,
            has_input,
        }
    }
}

impl Node for Lifted {
    fn inputs(&self) -> &[InputZ] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputZ] {
        &self.outputs
    }

    fn process(&mut self, inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]) {
        // Conductor k ↔ lane k. Each lane is a single-conductor node, so it gets a one-element
        // input/output slice — the same disjoint-pool borrows the schedule already relies on.
        let has_input = self.has_input;
        for (k, lane) in self.lanes.iter_mut().enumerate() {
            let out = &mut outputs[k..=k];
            if has_input {
                lane.process(&inputs[k..=k], out);
            } else {
                lane.process(&[], out);
            }
        }
    }

    fn prepare(&mut self, rate: AnalogRate) {
        for lane in &mut self.lanes {
            lane.prepare(rate);
        }
    }

    fn seed(&mut self, mut rng: Rng) {
        // Each leg gets an independent stream — per-leg noise is genuinely uncorrelated.
        for lane in &mut self.lanes {
            lane.seed(rng.split());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Ohms;
    use crate::signal::{AnalogRate, Volts};

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A 1-in/1-out node with internal state: a running sum of its input. State makes lane
    /// independence observable — if two lanes shared `acc`, conductor 1 would see conductor 0's
    /// running total. Per-conductor, so it can be lifted.
    struct Accum {
        acc: f32,
        inputs: [InputZ; 1],
        outputs: [OutputZ; 1],
    }

    impl Accum {
        fn new() -> Self {
            Self {
                acc: 0.0,
                inputs: [InputZ::new(Ohms::new(10_000.0))],
                outputs: [OutputZ::new(Ohms::new(150.0))],
            }
        }
    }

    impl Node for Accum {
        fn inputs(&self) -> &[InputZ] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputZ] {
            &self.outputs
        }
        fn process(&mut self, inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]) {
            for (o, &v) in outputs[0]
                .as_mut_slice()
                .iter_mut()
                .zip(inputs[0].as_slice())
            {
                self.acc += v;
                *o = self.acc;
            }
        }
        fn per_conductor(&self) -> bool {
            true
        }
        fn replicate(&self) -> Box<dyn Node> {
            Box::new(Accum::new())
        }
    }

    #[test]
    fn widens_the_faces_to_the_conductor_count() {
        let lifted = Lifted::new(Box::new(Accum::new()), 2);
        assert_eq!(lifted.inputs().len(), 1);
        assert_eq!(lifted.inputs()[0].conductors(), 2);
        assert_eq!(lifted.outputs()[0].conductors(), 2);
    }

    #[test]
    fn each_lane_has_independent_state() {
        // Two conductors: leg 0 carries a constant 1 V, leg 1 carries silence. The accumulator's
        // running sum on leg 0 ramps 1, 2, 3, 4; leg 1 stays 0 — proving the lanes don't share
        // state and conductor k maps to lane k.
        let mut lifted = Lifted::new(Box::new(Accum::new()), 2);
        let mut ins = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        ins[0].fill(Volts::new(1.0));
        let mut outs = [
            VoltageBuffer::zeros(4, rate()),
            VoltageBuffer::zeros(4, rate()),
        ];
        lifted.process(&ins, &mut outs);
        assert_eq!(
            outs[0].as_slice(),
            &[1.0, 2.0, 3.0, 4.0],
            "leg 0 should accumulate its own input"
        );
        assert!(
            outs[1].as_slice().iter().all(|&v| v == 0.0),
            "leg 1 must be untouched by leg 0's state"
        );
    }
}
