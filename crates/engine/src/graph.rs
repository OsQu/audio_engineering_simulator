//! The patch graph: nodes (processing elements) wired by connections (edges).
//!
//! A construct-in-code description of a studio — nodes and the cables between them — built up
//! with [`Graph::add`] / [`Graph::connect`], then handed to `compile` (Task 1.3.5), which
//! consumes it and validates it into a runnable schedule. The graph is the *what*; the
//! schedule is the *how and in what order*.
//!
//! Connections are recorded as given; **all validation happens at compile** (the doctrine:
//! validation and error reporting live in graph construction and `compile`, never the hot
//! path). The engine solves connections **locally** (no global nodal solve), so the graph is
//! a DAG — a cycle is a wiring mistake the compiler rejects rather than a feedback path to
//! solve.

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
        let id = NodeId(self.nodes.len());
        self.nodes.push(Box::new(node));
        id
    }

    /// Connect `from`'s output port `out_port` to `to`'s input port `in_port` with an ideal
    /// wire (no cable). Recorded as-is; validated at compile.
    pub fn connect(&mut self, from: NodeId, out_port: usize, to: NodeId, in_port: usize) {
        self.push_edge(from, out_port, to, in_port, None);
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
        self.push_edge(from, out_port, to, in_port, Some(cable));
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
    ) {
        self.edges.push(Edge {
            from_node: from,
            from_port,
            to_node: to,
            to_port,
            cable,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::{InputZ, Ohms, OutputZ};
    use crate::signal::VoltageBuffer;

    /// A no-op node with configurable port counts, for wiring tests.
    struct Stub {
        inputs: Vec<InputZ>,
        outputs: Vec<OutputZ>,
    }

    impl Stub {
        fn new(n_in: usize, n_out: usize) -> Self {
            Self {
                inputs: vec![InputZ::new(Ohms::new(10_000.0)); n_in],
                outputs: vec![OutputZ::new(Ohms::new(150.0)); n_out],
            }
        }
    }

    impl Node for Stub {
        fn inputs(&self) -> &[InputZ] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputZ] {
            &self.outputs
        }
        fn process(&mut self, _inputs: &[VoltageBuffer], _outputs: &mut [VoltageBuffer]) {}
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
        g.connect(src, 0, sink, 0);
        g.set_output(sink, 0);

        assert_eq!(g.connection_count(), 1);
        assert_eq!(g.output_tap(), Some((sink, 0)));
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
