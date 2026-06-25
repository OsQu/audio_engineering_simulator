//! Nodes: black-box processing elements that transform voltage between electrical terminals.
//!
//! A [`Node`] is the engine's unit of processing and scheduling — the thing the graph wires
//! together and the schedule sorts and runs. It presents **real electrical faces**
//! (PROJECT_PLAN §5.3): each analog input has an input impedance ([`InputZ`](crate::InputZ)),
//! each analog output an output impedance ([`OutputZ`](crate::OutputZ) — the `Zout` of a
//! [`Thevenin`](crate::Thevenin) source), wrapped in a domain-tagged [`InputPort`] / [`OutputPort`].
//! We model
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

mod ad;
mod balanced;
mod compressor;
mod condenser;
mod da;
mod dc_blocker;
mod eq;
mod gain;
mod lifted;
mod source;
mod speaker;
mod sum;
mod synth;

pub use ad::AdConverter;
pub use balanced::{BalancedDriver, BalancedReceiver};
pub use compressor::Compressor;
pub use condenser::CondenserMic;
pub use da::DaConverter;
pub use dc_blocker::DcBlocker;
pub use eq::{EqBand, ThreeBandEq};
pub use gain::GainStage;
pub(crate) use lifted::Lifted;
pub use source::TestSource;
pub use speaker::Speaker;
pub use sum::PassiveSum;
pub use synth::SynthVoice;

use crate::param::{ParamDecl, Params};
use crate::port::{InputPort, OutputPort};
use crate::rng::Rng;
use crate::signal::{AnalogRate, Lane};

/// A black-box processing element: fixed electrical faces plus a per-block voltage transform.
///
/// # Hot-path contract
/// [`process`](Self::process) is on the audio path: it must **not allocate, panic, or
/// block**. All fallible setup (sizing, validation) happens earlier, at graph construction
/// and compile. The schedule owns every buffer and hands `process` the node's own input and
/// output blocks as already-sized slices — the node only reads inputs and writes outputs.
///
/// # Ports, lanes and conductors
/// [`inputs`](Self::inputs) / [`outputs`](Self::outputs) declare the node's **ports** (their
/// faces); the [`process`](Self::process) slices are the **lanes** buffering them — one
/// [`Lane`] per conductor (analog) or channel (digital), in port-then-lane order. An unbalanced
/// analog port owns one lane, a **balanced** one two (V+ then V−); a node maps ports to lanes
/// from its faces' [`lane_count`](crate::InputPort::lane_count) (e.g. a balanced input's two
/// lanes are `inputs[0]` = V+, `inputs[1]` = V−). An analog node reads each input lane as
/// [`voltage()`](Lane::voltage) and writes each output lane via
/// [`voltage_mut()`](Lane::voltage_mut); `compile` guarantees a lane's domain matches the port's,
/// so the typed accessor's other arm is unreachable. Each input carries the already
/// loaded-and-filtered signal; the node writes each output's **open-circuit** value. Every block
/// is the same length within a domain, fixed at compile; an unconnected lane reads silence.
pub trait Node {
    /// The node's input ports, in declaration order. Its length is the node's input-port count
    /// and must stay constant for the node's lifetime.
    fn inputs(&self) -> &[InputPort];

    /// The node's output ports, in declaration order. Its length is the node's output-port
    /// count and must stay constant for the node's lifetime.
    fn outputs(&self) -> &[OutputPort];

    /// The node's smoothed control parameters, in id order — the knobs/faders the host can drive.
    /// Each [`ParamDecl`] names an id, its initial value, range, and de-zipper time; `compile`
    /// builds one smoother per declaration and hands their current values to
    /// [`process`](Self::process) via [`Params`]. Default: no params. A node names its params with
    /// `const`s whose [`ParamId`](crate::ParamId) equals the declaration's position here.
    fn params(&self) -> &[ParamDecl] {
        &[]
    }

    /// Transform a block: read the `inputs` lanes and current `params`, write each output lane's
    /// **open-circuit** value into `outputs`. Hot path — no allocation, no panic. The slice
    /// lengths are the nodes' total lane counts (sum of each port's
    /// [`lane_count`](crate::InputPort::lane_count)); a node reads a control param at sample `i`
    /// with [`params.value_at_or`](Params::value_at_or), passing its own field as the fallback.
    fn process(&mut self, params: &Params, inputs: &[Lane], outputs: &mut [Lane]);

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

    /// The latency this node adds to the signal, in **analog-rate samples** — its filters' group
    /// delay. Default 0: memoryless or negligible-delay nodes (gain, sum, the voice, the speaker)
    /// add none. The linear-phase converter FIRs override it (a decimator/interpolator's symmetric
    /// kernel has a constant `(taps − 1) / 2` group delay). Off the hot path — read once after
    /// `compile` to report end-to-end latency (see [`Schedule::group_delay_samples`]).
    ///
    /// [`Schedule::group_delay_samples`]: crate::Schedule::group_delay_samples
    fn group_delay_samples(&self) -> f64 {
        0.0
    }

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
    use crate::electrical::{InputZ, Ohms, OutputZ};
    use crate::signal::{AnalogRate, VoltageBuffer, Volts};
    use crate::test_util::process_voltage;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A minimal node exercising the trait shape: one input, one output, doubles the signal.
    /// Confirms the declared port counts line up with the lanes `process` receives.
    struct Doubler {
        inputs: [InputPort; 1],
        outputs: [OutputPort; 1],
    }

    impl Node for Doubler {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }

        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }

        fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
            for (out, &v) in outputs[0]
                .voltage_mut()
                .as_mut_slice()
                .iter_mut()
                .zip(inputs[0].voltage().as_slice())
            {
                *out = v * 2.0;
            }
        }
    }

    #[test]
    fn port_declarations_match_the_process_slices() {
        let mut node = Doubler {
            inputs: [InputZ::new(Ohms::new(10_000.0)).into()],
            outputs: [OutputZ::new(Ohms::new(150.0)).into()],
        };
        assert_eq!(node.inputs().len(), 1);
        assert_eq!(node.outputs().len(), 1);

        let mut input = [VoltageBuffer::zeros(4, rate())];
        input[0].fill(Volts::new(0.5));
        let mut output = [VoltageBuffer::zeros(4, rate())];
        process_voltage(&mut node, &input, &mut output);

        assert!(output[0].as_slice().iter().all(|&v| (v - 1.0).abs() < 1e-6));
    }
}
