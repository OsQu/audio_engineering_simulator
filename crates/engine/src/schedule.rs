//! Compiling a [`Graph`] into a runnable [`Schedule`], and running it.
//!
//! [`compile`] is the one fallible, allocating gate: it validates the graph, topologically
//! orders it, allocates every buffer from a pool, and bakes each connection's local solve
//! into an [`EdgeTransform`]. [`Schedule::process`] is then the hot path — it only reads and
//! writes those pre-allocated buffers, never allocating, panicking, or blocking.
//!
//! ## Buffers: two pools, no aliasing, no `unsafe`
//! Voltage lives in two pools — one buffer per node **output** port, one per node **input**
//! port. A node writes its open-circuit output into the output pool; an [`EdgeTransform`]
//! reads an output buffer, applies the connection's gain and cable rolloff, and writes the
//! result into an input buffer; the consuming node reads the input pool. Because a node's
//! ports occupy a *contiguous range* of their pool, a step borrows `&input_pool[..]` and
//! `&mut output_pool[..]` — two different `Vec`s — so the disjointness the borrow checker
//! needs is structural, and the whole loop is safe and allocation-free.
//!
//! ## The connection seam
//! An [`EdgeTransform`] is *currently* a constant resistive gain plus an optional cable
//! one-pole — the exact, complete model while there's at most one reactive element on an edge
//! (Story 1.2). It's deliberately a struct behind a constructor and a single `process` entry,
//! **not** a contract: a reactive source later makes an edge a 2nd-order transfer function
//! depending on both endpoints, which generalizes the representation without touching callers.

mod topo;

use crate::electrical::{InputZ, Ohms, OnePole, fan_out_gains};
use crate::graph::Graph;
use crate::node::Node;
use crate::signal::{AnalogRate, VoltageBuffer};
use core::fmt;

/// Why a [`Graph`] could not be compiled. All structural — caught here, never on the hot path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// No output tap was designated (see [`Graph::set_output`](crate::Graph::set_output)).
    NoOutput,
    /// A referenced node index does not exist in the graph.
    NodeOutOfRange { node: usize },
    /// An output port index is past the node's output count.
    OutputPortOutOfRange { node: usize, port: usize },
    /// An input port index is past the node's input count.
    InputPortOutOfRange { node: usize, port: usize },
    /// Two connections target the same input port — fan-in is modeled by a node with several
    /// input ports, not by two edges into one.
    InputAlreadyConnected { node: usize, port: usize },
    /// The graph has a cycle; the local-solve engine has no feedback paths to resolve.
    Cycle,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutput => write!(f, "no output tap was set on the graph"),
            Self::NodeOutOfRange { node } => write!(f, "node {node} does not exist"),
            Self::OutputPortOutOfRange { node, port } => {
                write!(f, "node {node} has no output port {port}")
            }
            Self::InputPortOutOfRange { node, port } => {
                write!(f, "node {node} has no input port {port}")
            }
            Self::InputAlreadyConnected { node, port } => {
                write!(f, "node {node} input port {port} is already connected")
            }
            Self::Cycle => write!(f, "the graph has a cycle"),
        }
    }
}

impl std::error::Error for CompileError {}

/// A connection's baked local solve: scale by a constant gain, then (if cabled) the one-pole
/// treble rolloff. See the module docs on why this is a struct, not a fixed contract.
struct EdgeTransform {
    gain: f32,
    lowpass: Option<OnePole>,
}

impl EdgeTransform {
    /// Map a source's open-circuit block (`src`) to a receiver's input block (`dst`). Hot
    /// path: no allocation, no panic. `dst` is fully overwritten (each input has ≤1 edge).
    fn process(&mut self, src: &[f32], dst: &mut [f32]) {
        for (d, &s) in dst.iter_mut().zip(src) {
            *d = s * self.gain;
        }
        if let Some(lp) = &mut self.lowpass {
            lp.process_slice(dst);
        }
    }
}

/// One instruction in the schedule, in execution order.
enum Step {
    /// Run a node: read its contiguous input-pool range, write its output-pool range.
    Node {
        node: usize,
        in_start: usize,
        in_len: usize,
        out_start: usize,
        out_len: usize,
    },
    /// Run a connection: read output-pool buffer `src`, write input-pool buffer `dst`.
    Edge {
        src: usize,
        dst: usize,
        transform: EdgeTransform,
    },
}

/// A compiled, runnable patch: nodes, the buffer pools, and the ordered step list.
///
/// Produced by [`compile`]; driven by [`process`](Self::process) one block at a time. Owns its
/// nodes (with their state) and every buffer, so running it touches no shared state and
/// allocates nothing.
pub struct Schedule {
    nodes: Vec<Box<dyn Node>>,
    input_pool: Vec<VoltageBuffer>,
    output_pool: Vec<VoltageBuffer>,
    steps: Vec<Step>,
    /// Index into `output_pool` of the designated output tap.
    out_buf: usize,
    block_len: usize,
}

impl Schedule {
    /// The fixed block length every buffer is sized to. [`process`](Self::process) fills this
    /// many samples.
    pub fn block_len(&self) -> usize {
        self.block_len
    }

    /// Process one block: run every step in order, then copy the designated output tap into
    /// `out`. Hot path — zero allocation, no panic, no locks.
    ///
    /// Writes `min(out.len(), block_len)` samples; size `out` to [`block_len`](Self::block_len).
    pub fn process(&mut self, out: &mut VoltageBuffer) {
        let Self {
            nodes,
            input_pool,
            output_pool,
            steps,
            out_buf,
            ..
        } = self;

        for step in steps.iter_mut() {
            match step {
                Step::Node {
                    node,
                    in_start,
                    in_len,
                    out_start,
                    out_len,
                } => {
                    let ins = &input_pool[*in_start..*in_start + *in_len];
                    let outs = &mut output_pool[*out_start..*out_start + *out_len];
                    nodes[*node].process(ins, outs);
                }
                Step::Edge {
                    src,
                    dst,
                    transform,
                } => {
                    // Different pools ⇒ disjoint borrows, no aliasing.
                    let source = output_pool[*src].as_slice();
                    let dest = input_pool[*dst].as_mut_slice();
                    transform.process(source, dest);
                }
            }
        }

        let tapped = output_pool[*out_buf].as_slice();
        let dst = out.as_mut_slice();
        let n = dst.len().min(tapped.len());
        dst[..n].copy_from_slice(&tapped[..n]);
    }
}

/// Compile a [`Graph`] into a runnable [`Schedule`] for blocks of `block_len` samples at
/// `rate`. Consumes the graph (its nodes move into the schedule).
///
/// This is the project's one fallible, allocating gate: it validates wiring, orders the DAG,
/// allocates the buffer pools, and bakes each connection's local solve. Everything that could
/// fail happens here, so [`Schedule::process`] can be total.
///
/// # Errors
/// Returns [`CompileError`] if no output is set, a port/node reference is out of range, an
/// input port is connected twice, or the graph has a cycle.
pub fn compile(graph: Graph, block_len: usize, rate: AnalogRate) -> Result<Schedule, CompileError> {
    let Graph {
        nodes,
        edges,
        output,
    } = graph;
    let node_count = nodes.len();

    // --- 1. The output tap must exist and reference a real output port. ---
    let (out_node, out_port) = output.ok_or(CompileError::NoOutput)?;
    check_node(&nodes, out_node.0)?;
    check_output_port(&nodes, out_node.0, out_port)?;

    // --- 2. Buffer pools: one buffer per output port and per input port, contiguous by node
    //        so each node's ports form a single sliceable range. ---
    let mut out_offset = vec![0usize; node_count];
    let mut in_offset = vec![0usize; node_count];
    let mut output_pool = Vec::new();
    let mut input_pool = Vec::new();
    for n in 0..node_count {
        out_offset[n] = output_pool.len();
        for _ in 0..nodes[n].outputs().len() {
            output_pool.push(VoltageBuffer::zeros(block_len, rate));
        }
        in_offset[n] = input_pool.len();
        for _ in 0..nodes[n].inputs().len() {
            input_pool.push(VoltageBuffer::zeros(block_len, rate));
        }
    }

    // --- 3. Validate every edge's ports, and reject a second edge into any input. ---
    let mut input_taken = vec![false; input_pool.len()];
    for e in &edges {
        check_node(&nodes, e.from_node.0)?;
        check_node(&nodes, e.to_node.0)?;
        check_output_port(&nodes, e.from_node.0, e.from_port)?;
        check_input_port(&nodes, e.to_node.0, e.to_port)?;
        let dst = in_offset[e.to_node.0] + e.to_port;
        if input_taken[dst] {
            return Err(CompileError::InputAlreadyConnected {
                node: e.to_node.0,
                port: e.to_port,
            });
        }
        input_taken[dst] = true;
    }

    // --- 4. Topological order (rejects cycles). ---
    let deps: Vec<(usize, usize)> = edges.iter().map(|e| (e.from_node.0, e.to_node.0)).collect();
    let order = topo::topo_sort(node_count, &deps).ok_or(CompileError::Cycle)?;

    // --- 5. Bake each edge's local solve. Edges sharing an output port are one fan-out node:
    //        solve them together so the parallel loading is right. ---
    let mut edge_transform: Vec<Option<EdgeTransform>> = (0..edges.len()).map(|_| None).collect();
    let mut by_port: Vec<usize> = (0..edges.len()).collect();
    by_port.sort_by_key(|&ei| (edges[ei].from_node.0, edges[ei].from_port));
    let mut i = 0;
    while i < by_port.len() {
        let key = (edges[by_port[i]].from_node.0, edges[by_port[i]].from_port);
        let mut j = i;
        while j < by_port.len()
            && (edges[by_port[j]].from_node.0, edges[by_port[j]].from_port) == key
        {
            j += 1;
        }
        let (from_node, from_port) = key;
        let z_out: Ohms = nodes[from_node].outputs()[from_port].z_out();
        let group = &by_port[i..j];
        let branches: Vec<(Ohms, InputZ)> = group
            .iter()
            .map(|&ei| {
                let e = &edges[ei];
                let r = e.cable.map_or(Ohms::ZERO, |c| c.r());
                (r, nodes[e.to_node.0].inputs()[e.to_port])
            })
            .collect();
        let gains = fan_out_gains(z_out, &branches);
        for (k, &ei) in group.iter().enumerate() {
            let e = &edges[ei];
            let load = nodes[e.to_node.0].inputs()[e.to_port];
            edge_transform[ei] = Some(EdgeTransform {
                gain: gains[k],
                lowpass: e.cable.map(|c| c.lowpass(z_out, load, rate)),
            });
        }
        i = j;
    }

    // --- 6. Steps in topo order: each node, then its outgoing edges (so a downstream node's
    //        inputs are filled before it runs). ---
    let mut edges_from: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    for (ei, e) in edges.iter().enumerate() {
        edges_from[e.from_node.0].push(ei);
    }
    let mut steps = Vec::with_capacity(node_count + edges.len());
    for &node in &order {
        steps.push(Step::Node {
            node,
            in_start: in_offset[node],
            in_len: nodes[node].inputs().len(),
            out_start: out_offset[node],
            out_len: nodes[node].outputs().len(),
        });
        for &ei in &edges_from[node] {
            let e = &edges[ei];
            let transform = edge_transform[ei]
                .take()
                .expect("each edge is baked once and emitted once");
            steps.push(Step::Edge {
                src: out_offset[e.from_node.0] + e.from_port,
                dst: in_offset[e.to_node.0] + e.to_port,
                transform,
            });
        }
    }

    let out_buf = out_offset[out_node.0] + out_port;
    Ok(Schedule {
        nodes,
        input_pool,
        output_pool,
        steps,
        out_buf,
        block_len,
    })
}

fn check_node(nodes: &[Box<dyn Node>], node: usize) -> Result<(), CompileError> {
    if node < nodes.len() {
        Ok(())
    } else {
        Err(CompileError::NodeOutOfRange { node })
    }
}

fn check_output_port(
    nodes: &[Box<dyn Node>],
    node: usize,
    port: usize,
) -> Result<(), CompileError> {
    if port < nodes[node].outputs().len() {
        Ok(())
    } else {
        Err(CompileError::OutputPortOutOfRange { node, port })
    }
}

fn check_input_port(nodes: &[Box<dyn Node>], node: usize, port: usize) -> Result<(), CompileError> {
    if port < nodes[node].inputs().len() {
        Ok(())
    } else {
        Err(CompileError::InputPortOutOfRange { node, port })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Ohms;
    use crate::graph::NodeId;
    use crate::node::{GainStage, PassiveSum, TestSource};
    use crate::signal::Volts;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn gain(g: f32) -> GainStage {
        GainStage::new(
            g,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
    }

    #[test]
    fn source_gain_sum_chain_matches_hand_calc() {
        // source(1.0 V, 100 Ω) → gain(×2) → sum(1 input). No cables (ideal wires), DC.
        //   edge s→g:  10000/(100+10000)  = 0.990099  → gain in  = 0.990099 V
        //   gain out (open-circuit):       0.990099 × 2 = 1.980198 V  (below the 10 V rail)
        //   edge g→sum: 10000/(150+10000) = 0.985222  → sum in   = 1.980198 × 0.985222 = 1.950931 V
        //   sum out (1 input, unity)      = 1.950931 V  ← the tapped output
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        let amp = g.add(gain(2.0));
        let sum = g.add(PassiveSum::new(
            vec![InputZ::new(Ohms::new(10_000.0))],
            Ohms::new(150.0),
        ));
        g.connect(src, 0, amp, 0);
        g.connect(amp, 0, sum, 0);
        g.set_output(sum, 0);

        let mut sched = compile(g, 8, rate()).expect("valid chain");
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
        for &v in out.as_slice() {
            assert_relative_eq!(v, 1.950931, epsilon = 1e-4);
        }
    }

    #[test]
    fn fan_out_then_sum_matches_hand_calc() {
        // source(1.0 V, 100 Ω) fans out to two ×2 gains, summed.
        //   fan-out: two 10 kΩ in parallel = 5 kΩ; node = 5000/5100 = 0.980392
        //     → each gain in = 0.980392 V; ×2 = 1.960784 V
        //   each edge gain→sum: 10000/(150+10000) = 0.985222 → 1.960784 × 0.985222 = 1.931807 V
        //   sum of the two inputs = 3.863614 V
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        let a = g.add(gain(2.0));
        let b = g.add(gain(2.0));
        let sum = g.add(PassiveSum::new(
            vec![
                InputZ::new(Ohms::new(10_000.0)),
                InputZ::new(Ohms::new(10_000.0)),
            ],
            Ohms::new(150.0),
        ));
        g.connect(src, 0, a, 0);
        g.connect(src, 0, b, 0);
        g.connect(a, 0, sum, 0);
        g.connect(b, 0, sum, 1);
        g.set_output(sum, 0);

        let mut sched = compile(g, 4, rate()).expect("valid fan-out chain");
        let mut out = VoltageBuffer::zeros(4, rate());
        sched.process(&mut out);
        for &v in out.as_slice() {
            assert_relative_eq!(v, 3.863614, epsilon = 1e-4);
        }
    }

    #[test]
    fn rejects_missing_output() {
        let mut g = Graph::new();
        g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        assert_eq!(compile(g, 8, rate()).err(), Some(CompileError::NoOutput));
    }

    #[test]
    fn rejects_a_cycle() {
        // a → b → a is a loop.
        let mut g = Graph::new();
        let a = g.add(gain(1.0));
        let b = g.add(gain(1.0));
        g.connect(a, 0, b, 0);
        g.connect(b, 0, a, 0);
        g.set_output(b, 0);
        assert_eq!(compile(g, 8, rate()).err(), Some(CompileError::Cycle));
    }

    #[test]
    fn rejects_double_connected_input() {
        let mut g = Graph::new();
        let s1 = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        let s2 = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        let sum = g.add(PassiveSum::new(
            vec![InputZ::new(Ohms::new(10_000.0))],
            Ohms::new(150.0),
        ));
        g.connect(s1, 0, sum, 0);
        g.connect(s2, 0, sum, 0); // same input port 0
        g.set_output(sum, 0);
        assert_eq!(
            compile(g, 8, rate()).err(),
            Some(CompileError::InputAlreadyConnected { node: 2, port: 0 })
        );
    }

    #[test]
    fn rejects_output_port_out_of_range() {
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        g.set_output(src, 5); // a source has only output port 0
        assert_eq!(
            compile(g, 8, rate()).err(),
            Some(CompileError::OutputPortOutOfRange { node: 0, port: 5 })
        );
    }

    #[test]
    fn rejects_unknown_node() {
        let mut g = Graph::new();
        g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        g.set_output(NodeId(9), 0); // no such node
        assert_eq!(
            compile(g, 8, rate()).err(),
            Some(CompileError::NodeOutOfRange { node: 9 })
        );
    }
}
