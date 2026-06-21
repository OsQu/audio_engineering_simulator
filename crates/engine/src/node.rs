//! Nodes: black-box processing elements that transform voltage between electrical terminals.
//!
//! A [`Node`] is the engine's unit of processing and scheduling — the thing the graph wires
//! together and the schedule sorts and runs. It presents **real electrical faces**
//! (PROJECT_PLAN §5.3): each input has an input impedance ([`InputZ`]), each output an output
//! impedance ([`OutputZ`] — the `Zout` of a [`Thevenin`](crate::Thevenin) source). We model
//! the *observable I/O*, not the circuitry inside — the transform from input voltages to
//! output (open-circuit) voltages.
//!
//! **Node vs. device.** A *node* is one schedulable processing element. A physical *device*
//! (a chassis — a mixer, an audio interface) may map to **several** nodes when its signal
//! path leaves and re-enters the box (an insert, a routed interface): those are distinct
//! stages of the path, scheduled separately. Today every node is a whole simple device
//! (the single-stage case); the node ⇄ logical-device grouping arrives with the first
//! multi-stage device (see `IMPLEMENTATION_PLAN.md`, Story 1.3 design notes).
//!
//! The fixed electrical faces are declared up front ([`Node::inputs`] / [`Node::outputs`]);
//! the dynamic per-block signal flows through [`Node::process`]. The voltage *between* nodes
//! — the loading divider and cable rolloff — is owned by the connection, not the node, and
//! applied by the schedule, so a node's output is always its **open-circuit** `v_src` (what
//! it would produce into an infinite load).

mod balanced;
mod dc_blocker;
mod gain;
mod lifted;
mod source;
mod sum;

pub use balanced::{BalancedDriver, BalancedReceiver};
pub use dc_blocker::DcBlocker;
pub use gain::GainStage;
pub(crate) use lifted::Lifted;
pub use source::TestSource;
pub use sum::PassiveSum;

use crate::electrical::{InputZ, OutputZ};
use crate::rng::Rng;
use crate::signal::{AnalogRate, VoltageBuffer};

/// A black-box processing element: fixed electrical faces plus a per-block voltage transform.
///
/// # Hot-path contract
/// [`process`](Self::process) is on the audio path: it must **not allocate, panic, or
/// block**. All fallible setup (sizing, validation) happens earlier, at graph construction
/// and compile. The schedule owns every buffer and hands `process` the node's own input and
/// output blocks as already-sized slices — the node only reads inputs and writes outputs.
///
/// # Buffers and conductors
/// `inputs` and `outputs` are the node's ports' **conductors**, in port-then-conductor order: an
/// unbalanced port owns one buffer, a **balanced** port two (V+ then V−). For an all-unbalanced
/// node — every node before Story 1.5 — conductor index equals port index and `inputs[i]` is just
/// port `i`'s arriving voltage. A node with a balanced port maps ports to conductor buffers itself
/// from its declared faces' [`conductors`](crate::InputZ::conductors) (e.g. a balanced input's two
/// buffers are `inputs[0]` = V+, `inputs[1]` = V−). Each input carries the already
/// loaded-and-filtered voltage; the node writes each output conductor's **open-circuit** voltage.
/// Every block is the same length, fixed at compile. A conductor with nothing connected reads
/// silence.
pub trait Node {
    /// The input impedance of each input port, in declaration order. Its length is the node's
    /// input-port count and must stay constant for the node's lifetime.
    fn inputs(&self) -> &[InputZ];

    /// The output impedance ([`OutputZ`]) of each output port, in declaration order. Its
    /// length is the node's output-port count and must stay constant for the node's lifetime.
    fn outputs(&self) -> &[OutputZ];

    /// Transform a block: read `inputs`, write each output port's **open-circuit** voltage
    /// into `outputs`. Hot path — no allocation, no panic. `inputs.len()` equals
    /// [`inputs`](Self::inputs)`.len()` and likewise for `outputs`.
    fn process(&mut self, inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]);

    /// Seed this node's stochastic state from `rng`, an independent per-node stream.
    ///
    /// Called once by [`compile`](crate::compile) before any [`process`](Self::process), so a
    /// run is reproducible: the same compile `seed` gives every node the same stream every
    /// time. Most nodes are deterministic and use the default no-op; a node with a noise floor
    /// (or any randomness) keeps the `rng` and draws from it on the hot path. Off the hot path.
    fn seed(&mut self, _rng: Rng) {}

    /// Prepare rate-dependent state for the analog `rate` the schedule will run at.
    ///
    /// Called once by [`compile`](crate::compile) before any [`process`](Self::process). This
    /// is where a node bakes any coefficient that depends on the sample period — a filter pole,
    /// an oscillator increment, an anti-alias kernel — so the expensive setup (an `exp`, a
    /// kernel design) is paid here, not on the hot path. Rate-free nodes use the default no-op.
    ///
    /// The companion to [`seed`](Self::seed): `seed` hands a node its randomness, `prepare`
    /// hands it the clock. It exists because nodes own their state across compiles, so — unlike
    /// the connection's cable filter, which `compile` builds directly — a stateful filter *node*
    /// needs the rate delivered to it. Off the hot path.
    fn prepare(&mut self, _rate: AnalogRate) {}

    /// Whether this is a **per-conductor** processor the compiler may replicate across the
    /// conductors of a balanced connection — one independent instance per leg, identical
    /// coefficients (see `IMPLEMENTATION_PLAN.md`, Story 1.5 detour).
    ///
    /// A balanced line is two ordinary wires; an inline processor (a DC blocker, a gain) acts on
    /// each leg independently, and that per-leg *symmetry* is what makes common-mode cancel at the
    /// receiver. So such a node is written **once** for a single conductor and declares itself
    /// per-conductor here; `compile` infers its conductor count from the wiring and lifts it. The
    /// default is `false`: sources, the balanced driver/receiver, and conductor-mixing nodes own
    /// their layout and are never lifted. A per-conductor node must have one input and one output
    /// port (or none and one) and must implement [`replicate`](Self::replicate).
    fn per_conductor(&self) -> bool {
        false
    }

    /// Mint a fresh, independent instance of this node for one extra conductor lane — same
    /// construction parameters, **zeroed** state. Called by `compile` only when
    /// [`per_conductor`](Self::per_conductor) is `true` and the node is lifted onto a balanced
    /// connection; each lane is later [`prepare`](Self::prepare)d and [`seed`](Self::seed)ed on
    /// its own. The default is unreachable — overriding it is part of opting into the lift.
    fn replicate(&self) -> Box<dyn Node> {
        unreachable!("replicate() is only called on per-conductor nodes, which must override it")
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

    /// A minimal node exercising the trait shape: one input, one output, doubles the signal.
    /// Confirms the declared port counts line up with the slices `process` receives.
    struct Doubler {
        inputs: [InputZ; 1],
        outputs: [OutputZ; 1],
    }

    impl Node for Doubler {
        fn inputs(&self) -> &[InputZ] {
            &self.inputs
        }

        fn outputs(&self) -> &[OutputZ] {
            &self.outputs
        }

        fn process(&mut self, inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]) {
            for (out, &v) in outputs[0]
                .as_mut_slice()
                .iter_mut()
                .zip(inputs[0].as_slice())
            {
                *out = v * 2.0;
            }
        }
    }

    #[test]
    fn port_declarations_match_the_process_slices() {
        let mut node = Doubler {
            inputs: [InputZ::new(Ohms::new(10_000.0))],
            outputs: [OutputZ::new(Ohms::new(150.0))],
        };
        assert_eq!(node.inputs().len(), 1);
        assert_eq!(node.outputs().len(), 1);

        let mut input = [VoltageBuffer::zeros(4, rate())];
        input[0].fill(Volts::new(0.5));
        let mut output = [VoltageBuffer::zeros(4, rate())];
        node.process(&input, &mut output);

        assert!(output[0].as_slice().iter().all(|&v| (v - 1.0).abs() < 1e-6));
    }
}
