//! The patch graph: nodes (processing elements) wired by connections (edges).
//!
//! A construct-in-code description of a studio — nodes and the cables between them — built up
//! with [`Graph::add`] / [`Graph::connect_ideal`] / [`Graph::connect_cabled`], then handed to
//! `compile`, which consumes it and validates it into a runnable schedule. The graph is the *what*;
//! the schedule is the *how and in what order*.
//!
//! Connections are recorded as given; **all validation happens at compile** (the doctrine:
//! validation and error reporting live in graph construction and `compile`, never the hot
//! path). The engine solves connections **locally** (no global nodal solve), so the *scheduled*
//! graph is a DAG — a same-block cycle is a wiring mistake the compiler rejects rather than a
//! feedback path to solve. The one escape hatch is [`Graph::connect_delayed`]: a **delayed edge**
//! carries one block of latency, is cut from the topological sort, and so may close a loop (a
//! round-trip through a latent device — an interface ↔ DAW monitoring loop) without a same-block
//! feedback solve. The invariant holds: the schedule stays acyclic; only bounded latency is added.

use crate::electrical::Cable;
use crate::node::Node;

/// A handle to a node in a [`Graph`], returned by [`Graph::add`].
///
/// Opaque on purpose: it indexes the graph that produced it and means nothing to another
/// graph. `Copy`, so it's cheap to pass around while wiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) usize);

/// A connection from one node's output port to another's input port, optionally through a
/// [`Cable`] (series R + shunt C). No cable means an ideal wire (no series resistance, no
/// rolloff) — useful for isolating loading from cable effects in tests.
///
/// Fields are read by `compile`, which turns each edge into a baked local solve.
pub(crate) struct Edge {
    pub(crate) from_node: NodeId,
    pub(crate) from_port: usize,
    pub(crate) to_node: NodeId,
    pub(crate) to_port: usize,
    pub(crate) cable: Option<Cable>,
    /// A **delayed** edge carries one block of latency: it is cut from the topological sort (so a loop
    /// containing it is *not* a scheduling cycle — the schedule stays a strict DAG) and its copy runs
    /// **before** the per-block step loop, reading the producer's *persistent* output buffer, which at
    /// that moment still holds **last block's** value. This is how a round-trip through a latent device
    /// (an interface ↔ DAW loop) is expressed without a same-block feedback solve — see `compile`.
    pub(crate) delayed: bool,
}

/// A patch: a set of nodes and the connections between them, plus the designated output.
#[derive(Default)]
pub struct Graph {
    pub(crate) nodes: Vec<Box<dyn Node>>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) output: Option<(NodeId, usize)>,
}

impl Graph {
    /// An empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node, returning its [`NodeId`]. The node moves into the graph (and later into
    /// the schedule), carrying its internal state.
    pub fn add<N: Node + 'static>(&mut self, node: N) -> NodeId {
        self.add_boxed(Box::new(node))
    }

    /// Add an already-boxed node, returning its [`NodeId`]. The trait-object form of
    /// [`add`](Self::add): for callers that build a node behind `Box<dyn Node>` — a type-id → node
    /// factory like the UI device catalog — and so can't name the concrete type at the call site.
    /// [`add`](Self::add) delegates here, so both share one push path.
    pub fn add_boxed(&mut self, node: Box<dyn Node>) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    /// Connect `from`'s output port `out_port` to `to`'s input port `in_port` with an ideal
    /// wire (no cable). Recorded as-is; validated at compile.
    pub fn connect_ideal(&mut self, from: NodeId, out_port: usize, to: NodeId, in_port: usize) {
        self.push_edge(from, out_port, to, in_port, None, false);
    }

    /// Connect through a [`Cable`] — its series resistance joins the loading divider and its
    /// shunt capacitance becomes the treble rolloff on this edge.
    pub fn connect_cabled(
        &mut self,
        from: NodeId,
        out_port: usize,
        to: NodeId,
        in_port: usize,
        cable: Cable,
    ) {
        self.push_edge(from, out_port, to, in_port, Some(cable), false);
    }

    /// Connect with **one block of latency** (an ideal wire otherwise). The edge is cut from the
    /// schedule's topological sort — so it can close a loop without forming a cycle — and delivers the
    /// producer's *previous*-block output. This is the round-trip-latency seam: a device that is a
    /// latency source (a computer/DAW, whose playback trails its input by a buffer) wires its output
    /// through this, letting the classic interface → DAW → interface monitoring loop build. The
    /// electrical model is an ideal digital copy (round-trip latency, not analog loading).
    pub fn connect_delayed(&mut self, from: NodeId, out_port: usize, to: NodeId, in_port: usize) {
        self.push_edge(from, out_port, to, in_port, None, true);
    }

    /// Designate the graph's output tap: output port `out_port` of `node` is what the
    /// schedule writes out. A meter/sink is a bridging (high-Z) load, so the tapped voltage
    /// is the node's open-circuit output.
    pub fn set_output(&mut self, node: NodeId, out_port: usize) {
        self.output = Some((node, out_port));
    }

    /// Number of nodes in the graph.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of connections recorded so far.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.edges.len()
    }

    /// The designated output tap, if one has been set.
    #[must_use]
    pub fn output_tap(&self) -> Option<(NodeId, usize)> {
        self.output
    }

    fn push_edge(
        &mut self,
        from: NodeId,
        from_port: usize,
        to: NodeId,
        to_port: usize,
        cable: Option<Cable>,
        delayed: bool,
    ) {
        self.edges.push(Edge {
            from_node: from,
            from_port,
            to_node: to,
            to_port,
            cable,
            delayed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::{InputZ, Ohms, OutputZ};
    use crate::param::Params;
    use crate::port::{InputPort, OutputPort};
    use crate::signal::Lane;

    /// A no-op node with configurable port counts, for wiring tests.
    struct Stub {
        inputs: Vec<InputPort>,
        outputs: Vec<OutputPort>,
    }

    impl Stub {
        fn new(n_in: usize, n_out: usize) -> Self {
            Self {
                inputs: (0..n_in)
                    .map(|_| InputZ::new(Ohms::new(10_000.0)).into())
                    .collect(),
                outputs: (0..n_out)
                    .map(|_| OutputZ::new(Ohms::new(150.0)).into())
                    .collect(),
            }
        }
    }

    impl Node for Stub {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], _outputs: &mut [Lane]) {}
    }

    #[test]
    fn add_returns_sequential_ids() {
        let mut g = Graph::new();
        let a = g.add(Stub::new(0, 1));
        let b = g.add(Stub::new(1, 1));
        assert_eq!(a, NodeId(0));
        assert_eq!(b, NodeId(1));
        assert_eq!(g.node_count(), 2);
    }

    #[test]
    fn connections_and_output_are_recorded() {
        let mut g = Graph::new();
        let src = g.add(Stub::new(0, 1));
        let sink = g.add(Stub::new(1, 1));
        g.connect_ideal(src, 0, sink, 0);
        g.set_output(sink, 0);

        assert_eq!(g.connection_count(), 1);
        assert_eq!(g.output_tap(), Some((sink, 0)));
    }

    #[test]
    fn add_boxed_assigns_the_same_sequential_ids_as_add() {
        // The catalog builds nodes behind `Box<dyn Node>`; `add_boxed` must slot them in exactly
        // like `add` does (same id sequence, same count).
        let mut g = Graph::new();
        let a = g.add(Stub::new(0, 1));
        let b = g.add_boxed(Box::new(Stub::new(1, 1)));
        assert_eq!(a, NodeId(0));
        assert_eq!(b, NodeId(1));
        assert_eq!(g.node_count(), 2);
    }

    #[test]
    fn holds_heterogeneous_nodes_behind_one_trait_object_type() {
        // Two different concrete types stored in the same Vec<Box<dyn Node>>.
        let mut g = Graph::new();
        g.add(Stub::new(0, 1));
        g.add(Stub::new(2, 1));
        assert_eq!(g.node_count(), 2);
    }
}
