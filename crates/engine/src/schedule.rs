//! Compiling a [`Graph`] into a runnable [`Schedule`], and running it.
//!
//! [`compile`] is the one fallible, allocating gate: it validates the graph, topologically
//! orders it, allocates every buffer from a pool, and bakes each connection's local solve
//! into an [`EdgeTransform`]. [`Schedule::process`] is then the hot path — it only reads and
//! writes those pre-allocated buffers, never allocating, panicking, or blocking.
//!
//! ## Buffers: two pools, no aliasing, no `unsafe`
//! Voltage lives in two pools — one buffer per node **output conductor**, one per node **input
//! conductor** (an unbalanced port owns one, a balanced port two: V+ then V−). A node writes its
//! open-circuit output into the output pool; an [`EdgeTransform`] reads an output buffer, applies
//! the connection's gain and cable rolloff, and writes the result into an input buffer; the
//! consuming node reads the input pool. A balanced edge runs one transform per conductor. Because
//! a node's conductors occupy a *contiguous range* of their pool, a step borrows `&input_pool[..]`
//! and `&mut output_pool[..]` — two different `Vec`s — so the disjointness the borrow checker
//! needs is structural, and the whole loop is safe and allocation-free.
//!
//! ## The connection seam
//! An [`EdgeTransform`] is *currently* a constant resistive gain plus an optional cable
//! one-pole — the exact, complete model while there's at most one reactive element on an edge
//! (Story 1.2). It's deliberately a struct behind a constructor and a single `process` entry,
//! **not** a contract: a reactive source later makes an edge a 2nd-order transfer function
//! depending on both endpoints, which generalizes the representation without touching callers.

mod swap;
mod topo;

pub use swap::ScheduleSlot;

use crate::electrical::{InputZ, Ohms, OnePole, fan_out_gains};
use crate::graph::{Edge, Graph};
use crate::node::{Lifted, Node};
use crate::noise::NoiseDensity;
use crate::rng::Rng;
use crate::signal::{AnalogRate, VoltageBuffer};
use core::fmt;

/// Salt mixed into the compile seed to derive the **edge** pickup root, keeping it independent of
/// the per-node noise streams so adding cable pickup never perturbs a node's noise realization.
const EDGE_SEED_SALT: u64 = 0xED9E_5EED_ED9E_5EED;

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
    /// An output port and the input port it feeds declare different conductor counts (e.g. a
    /// balanced output into an unbalanced input). Cross-type connections aren't modeled yet —
    /// match conductor counts, or insert an adapter device when those arrive (Epic 5).
    ConductorMismatch {
        from_node: usize,
        from_port: usize,
        to_node: usize,
        to_port: usize,
    },
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
            Self::ConductorMismatch {
                from_node,
                from_port,
                to_node,
                to_port,
            } => write!(
                f,
                "node {from_node} output port {from_port} and node {to_node} input port \
                 {to_port} have different conductor counts (balanced vs. unbalanced)"
            ),
            Self::Cycle => write!(f, "the graph has a cycle"),
        }
    }
}

impl std::error::Error for CompileError {}

/// A connection's baked local solve: scale by a constant gain, then (if cabled) the one-pole
/// treble rolloff, then (if the cable picks up) add interference. See the module docs on why this
/// is a struct, not a fixed contract.
struct EdgeTransform {
    gain: f32,
    lowpass: Option<OnePole>,
    /// Per-conductor interference stream + per-sample σ. Every conductor of one edge holds an
    /// **identically-seeded** clone, so they draw the *same* sequence: the pickup is common-mode
    /// (equal on V+ and V−) and cancels at a balanced receiver, while passing on an unbalanced one.
    pickup: Option<(Rng, f32)>,
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
        if let Some((rng, sigma)) = &mut self.pickup {
            // Coupled onto the wire after the divider; broadband, so one Gaussian draw per sample.
            for d in dst.iter_mut() {
                *d += rng.next_gaussian() * *sigma;
            }
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
/// `seed` makes the run reproducible: it builds a root [`Rng`] and splits an independent child
/// stream into each node (via [`Node::seed`]). The same seed reproduces the same run —
/// recompiling or hot-swapping with the same seed gives identical output.
///
/// # Errors
/// Returns [`CompileError`] if no output is set, a port/node reference is out of range, an
/// input port is connected twice, or the graph has a cycle.
pub fn compile(
    graph: Graph,
    block_len: usize,
    rate: AnalogRate,
    seed: u64,
) -> Result<Schedule, CompileError> {
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

    // --- 2. Validate every edge's node and port indices up front: both the conductor inference
    //        below and the buffer pool index nodes by them. (Lifting preserves port counts, so a
    //        check here stays valid afterward.) ---
    for e in &edges {
        check_node(&nodes, e.from_node.0)?;
        check_node(&nodes, e.to_node.0)?;
        check_output_port(&nodes, e.from_node.0, e.from_port)?;
        check_input_port(&nodes, e.to_node.0, e.to_port)?;
    }

    // --- 3. Infer each per-conductor node's conductor multiplicity from the wiring, then lift the
    //        ones with >1 conductor — wrap in `Lifted`, one independent lane per leg. After this
    //        every node's faces report its true conductor count and the rest of compile is
    //        conductor-agnostic. A genuine balanced↔unbalanced clash surfaces here as a mismatch. ---
    let per_c: Vec<bool> = nodes.iter().map(|n| n.per_conductor()).collect();
    let multiplicity = infer_conductors(&nodes, &edges, &per_c)?;
    let mut nodes: Vec<Box<dyn Node>> = nodes
        .into_iter()
        .enumerate()
        .map(|(i, n)| {
            if per_c[i] && multiplicity[i] > 1 {
                Box::new(Lifted::new(n, multiplicity[i])) as Box<dyn Node>
            } else {
                n
            }
        })
        .collect();

    // --- 4. Buffer pools: one buffer per **conductor** (an unbalanced port owns one, a balanced
    //        port two), contiguous by node so each node's conductors form a single sliceable
    //        range. `*_port_base[n][p]` is the pool index of port p's first conductor; `*_count`
    //        is the node's total conductor (buffer) count for the Step::Node slice. ---
    let mut out_offset = vec![0usize; node_count];
    let mut in_offset = vec![0usize; node_count];
    let mut out_count = vec![0usize; node_count];
    let mut in_count = vec![0usize; node_count];
    let mut out_port_base: Vec<Vec<usize>> = Vec::with_capacity(node_count);
    let mut in_port_base: Vec<Vec<usize>> = Vec::with_capacity(node_count);
    let mut output_pool = Vec::new();
    let mut input_pool = Vec::new();
    for n in 0..node_count {
        out_offset[n] = output_pool.len();
        let mut obases = Vec::with_capacity(nodes[n].outputs().len());
        for face in nodes[n].outputs() {
            obases.push(output_pool.len());
            for _ in 0..face.conductors() {
                output_pool.push(VoltageBuffer::zeros(block_len, rate));
            }
        }
        out_count[n] = output_pool.len() - out_offset[n];
        out_port_base.push(obases);

        in_offset[n] = input_pool.len();
        let mut ibases = Vec::with_capacity(nodes[n].inputs().len());
        for face in nodes[n].inputs() {
            ibases.push(input_pool.len());
            for _ in 0..face.conductors() {
                input_pool.push(VoltageBuffer::zeros(block_len, rate));
            }
        }
        in_count[n] = input_pool.len() - in_offset[n];
        in_port_base.push(ibases);
    }

    // --- 5. Reject a second edge into any input port (fan-in is a multi-input node, not two edges
    //        into one), marking the port's first conductor. Port ranges were checked in step 2 and
    //        conductor counts reconciled in step 3. ---
    let mut input_taken = vec![false; input_pool.len()];
    for e in &edges {
        let dst = in_port_base[e.to_node.0][e.to_port];
        if input_taken[dst] {
            return Err(CompileError::InputAlreadyConnected {
                node: e.to_node.0,
                port: e.to_port,
            });
        }
        input_taken[dst] = true;
    }

    // --- 6. Topological order (rejects cycles). ---
    let deps: Vec<(usize, usize)> = edges.iter().map(|e| (e.from_node.0, e.to_node.0)).collect();
    let order = topo::topo_sort(node_count, &deps).ok_or(CompileError::Cycle)?;

    // --- 7. Bake each edge's local solve. Edges sharing an output port are one fan-out node:
    //        solve them together so the parallel loading is right. A balanced edge bakes **one
    //        transform per conductor** — the same differential divider gain on each, but an
    //        independent cable one-pole (each wire has its own filter state). ---
    let mut edge_transform: Vec<Option<Vec<EdgeTransform>>> =
        (0..edges.len()).map(|_| None).collect();
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
            // One transform per conductor: same (differential) gain, an independent one-pole.
            let conductors = load.conductors();
            let mut transforms = Vec::with_capacity(conductors);
            for _ in 0..conductors {
                transforms.push(EdgeTransform {
                    gain: gains[k],
                    lowpass: e.cable.map(|c| c.lowpass(z_out, load, rate)),
                    pickup: None, // installed below, after the gains are baked
                });
            }
            edge_transform[ei] = Some(transforms);
        }
        i = j;
    }

    // --- 7b. Seed each edge's interference pickup. Split a stream per edge in **edge-index
    //         order** from a root salted off the compile seed (kept separate from node seeding so
    //         a node's stream is unchanged whether or not edges pick up). Every conductor of an
    //         edge gets an identical clone, so the pickup is common-mode. Edges without pickup
    //         still consume their split, so each edge's stream is stable regardless of neighbours.
    let mut edge_root = Rng::from_seed(seed ^ EDGE_SEED_SALT);
    for (ei, e) in edges.iter().enumerate() {
        let stream = edge_root.split();
        let density = e.cable.map_or(NoiseDensity::ZERO, |c| c.pickup());
        if density != NoiseDensity::ZERO {
            let sigma = density.per_sample_sigma(rate);
            if let Some(transforms) = &mut edge_transform[ei] {
                for t in transforms {
                    t.pickup = Some((stream.clone(), sigma));
                }
            }
        }
    }

    // --- 8. Steps in topo order: each node, then its outgoing edges (so a downstream node's
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
            in_len: in_count[node],
            out_start: out_offset[node],
            out_len: out_count[node],
        });
        for &ei in &edges_from[node] {
            let e = &edges[ei];
            let transforms = edge_transform[ei]
                .take()
                .expect("each edge is baked once and emitted once");
            // Map conductor k of the source port to conductor k of the destination port.
            let src_base = out_port_base[e.from_node.0][e.from_port];
            let dst_base = in_port_base[e.to_node.0][e.to_port];
            for (k, transform) in transforms.into_iter().enumerate() {
                steps.push(Step::Edge {
                    src: src_base + k,
                    dst: dst_base + k,
                    transform,
                });
            }
        }
    }

    // --- 9. Prepare each node off the hot path: hand it the analog `rate` (so filter nodes bake
    //        their coefficients), then seed its stochastic state from an independent child
    //        stream. Split in node index order so a node's stream is stable regardless of topo
    //        order or which other nodes are noisy; rate-free / deterministic nodes use the
    //        default no-ops and ignore both. ---
    let mut root = Rng::from_seed(seed);
    for node in &mut nodes {
        node.prepare(rate);
        node.seed(root.split());
    }

    // The tap is the output port's first conductor (its hot leg for a balanced port).
    let out_buf = out_port_base[out_node.0][out_port];
    Ok(Schedule {
        nodes,
        input_pool,
        output_pool,
        steps,
        out_buf,
        block_len,
    })
}

/// Infer each per-conductor node's conductor multiplicity from the wiring.
///
/// A balanced line is two wires, and an inline per-conductor processor inherits its conductor
/// count from what it's wired to — anchored by the fixed faces of sources, the balanced driver,
/// and the receiver. This propagates those counts along edges to a fixpoint: a `Some` count on
/// one end of an edge fixes a still-unknown per-conductor node on the other. Two *known* counts
/// that disagree (e.g. a balanced output into an unbalanced input) are a [`ConductorMismatch`].
/// Per-conductor nodes left unconstrained (isolated, or in an all-unbalanced subgraph) default to
/// one conductor — i.e. they stay plain unbalanced nodes and are never lifted.
///
/// Returns one multiplicity per node (meaningful for per-conductor nodes; 1 otherwise).
///
/// [`ConductorMismatch`]: CompileError::ConductorMismatch
fn infer_conductors(
    nodes: &[Box<dyn Node>],
    edges: &[Edge],
    per_c: &[bool],
) -> Result<Vec<usize>, CompileError> {
    // `m[i]` is a per-conductor node's resolved count (`None` until inferred); fixed nodes read
    // their count straight off the face, so their `m` entry is unused.
    let mut m: Vec<Option<usize>> = vec![None; nodes.len()];

    // A port's conductor count if currently known: the face for a fixed node, `m` for a
    // per-conductor one (all its ports share the single multiplicity).
    let port_cond = |node: usize, is_output: bool, port: usize, m: &[Option<usize>]| {
        if per_c[node] {
            m[node]
        } else if is_output {
            Some(nodes[node].outputs()[port].conductors())
        } else {
            Some(nodes[node].inputs()[port].conductors())
        }
    };

    loop {
        let mut changed = false;
        for e in edges {
            let from = port_cond(e.from_node.0, true, e.from_port, &m);
            let to = port_cond(e.to_node.0, false, e.to_port, &m);
            match (from, to) {
                (Some(a), Some(b)) if a != b => {
                    return Err(CompileError::ConductorMismatch {
                        from_node: e.from_node.0,
                        from_port: e.from_port,
                        to_node: e.to_node.0,
                        to_port: e.to_port,
                    });
                }
                (Some(a), None) => {
                    m[e.to_node.0] = Some(a);
                    changed = true;
                }
                (None, Some(b)) => {
                    m[e.from_node.0] = Some(b);
                    changed = true;
                }
                _ => {}
            }
        }
        if !changed {
            break;
        }
    }

    Ok(m.into_iter().map(|x| x.unwrap_or(1)).collect())
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

        let mut sched = compile(g, 8, rate(), 0).expect("valid chain");
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

        let mut sched = compile(g, 4, rate(), 0).expect("valid fan-out chain");
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
        assert_eq!(compile(g, 8, rate(), 0).err(), Some(CompileError::NoOutput));
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
        assert_eq!(compile(g, 8, rate(), 0).err(), Some(CompileError::Cycle));
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
            compile(g, 8, rate(), 0).err(),
            Some(CompileError::InputAlreadyConnected { node: 2, port: 0 })
        );
    }

    #[test]
    fn rejects_output_port_out_of_range() {
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        g.set_output(src, 5); // a source has only output port 0
        assert_eq!(
            compile(g, 8, rate(), 0).err(),
            Some(CompileError::OutputPortOutOfRange { node: 0, port: 5 })
        );
    }

    #[test]
    fn rejects_unknown_node() {
        let mut g = Graph::new();
        g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        g.set_output(NodeId(9), 0); // no such node
        assert_eq!(
            compile(g, 8, rate(), 0).err(),
            Some(CompileError::NodeOutOfRange { node: 9 })
        );
    }
}

/// Story 1.4.1 — device noise floors emerging from the voltage math, on real compiled chains.
///
/// "Tests are the oracle" (§3.5): you can't hear a µV noise floor, so each assert is a number
/// computed by hand, with the calc in a comment. RMS converges to the true `σ` only in the
/// limit, so the tolerances are the finite-sample sampling error (`~1/√(2N)`), not slop.
#[cfg(test)]
mod noise_phenomena {
    use super::*;
    use crate::electrical::Ohms;
    use crate::node::{GainStage, TestSource};
    use crate::noise::NoiseDensity;
    use crate::signal::Volts;
    use crate::test_util::rms;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A near-ideal unity buffer: huge `Zin`, tiny `Zout`, so every edge divider is ~1 and the
    /// only thing the stage does to the signal is add its own input-referred noise floor.
    /// That keeps the hand calc clean — no gain or loss bookkeeping muddying the noise power.
    fn noisy_buffer(density: NoiseDensity) -> GainStage {
        GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        )
        .with_noise(density)
    }

    /// Run a silent source through a chain of unity noisy buffers; return the tapped output.
    fn run_silence(densities: &[NoiseDensity], len: usize, seed: u64) -> Vec<f32> {
        let mut g = Graph::new();
        let mut tail = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
        for &d in densities {
            let b = g.add(noisy_buffer(d));
            g.connect(tail, 0, b, 0);
            tail = b;
        }
        g.set_output(tail, 0);
        let mut sched = compile(g, len, rate(), seed).expect("valid noise chain");
        let mut out = VoltageBuffer::zeros(len, rate());
        sched.process(&mut out);
        out.as_slice().to_vec()
    }

    #[test]
    fn device_noise_floor_matches_density() {
        // One unity buffer, silent input. With the noise referred to the input and unity gain,
        // the output RMS is exactly the per-sample σ on the wire:
        //   σ = D·√(fs/2) = 10e-9 · √(384000/2) = 10e-9 · 438.178 = 4.3818 µV.
        let d = NoiseDensity::new(10e-9);
        let sigma = d.per_sample_sigma(rate());
        let out = run_silence(&[d], 200_000, 0x0A11_CE00);
        // 200k Gaussian samples ⇒ RMS converges to σ to ~0.16% (1/√(2N)); 2% is comfortable.
        assert_relative_eq!(rms(&out), sigma, max_relative = 0.02);
    }

    #[test]
    fn noise_adds_in_quadrature_down_the_chain() {
        // Two identical unity noise stages, same compile seed ⇒ stage 1's noise stream is the
        // *same realization* in both graphs (split is by node index), so the second stage only
        // adds uncorrelated power:  σ_total = √(σ1² + σ2²). Two equal stages ⇒ √2·σ, i.e. the
        // floor rises +3.01 dB and a fixed signal's SNR drops the same 3.01 dB. (The classic
        // "the first preamp sets your SNR; every later stage can only add noise" lesson.)
        let d = NoiseDensity::new(10e-9);
        let sigma = d.per_sample_sigma(rate());

        let one = run_silence(&[d], 200_000, 7);
        let two = run_silence(&[d, d], 200_000, 7);
        let n1 = rms(&one);
        let n2 = rms(&two);

        // Stage 1 alone is the device floor; the chain is strictly noisier (monotonic).
        assert_relative_eq!(n1, sigma, max_relative = 0.02);
        assert!(
            n2 > n1,
            "the chain must be noisier than one stage: {n2} vs {n1}"
        );

        // Quadrature sum of two equal stages: √(σ² + σ²) = √2·σ.
        assert_relative_eq!(n2, core::f32::consts::SQRT_2 * sigma, max_relative = 0.02);

        // SNR cost of the second stage, signal held fixed: 20·log10(n2/n1) = 3.01 dB.
        let snr_loss_db = 20.0 * (n2 / n1).log10();
        assert_relative_eq!(snr_loss_db, 3.0103, epsilon = 0.1);
    }
}

/// Story 1.4.2 — a DC offset riding the AC, removed by a DC-blocking high-pass, on a compiled
/// patch. "Tests are the oracle" (§3.5): the numbers are hand-computed, with the calc inline.
#[cfg(test)]
mod dc_phenomena {
    use super::*;
    use crate::electrical::{Farads, Ohms};
    use crate::node::DcBlocker;
    use crate::signal::Volts;
    use crate::test_util::{SineSource, rms};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn dc_blocker_strips_the_offset_and_passes_the_audio() {
        // A 1 kHz, 1 V sine riding on a 2 V DC pedestal → a DC blocker → tap.
        //   source:   2.0 + 1.0·sin(2π·1000·t), from a near-ideal 1 Ω output
        //   blocker:  c = 31.831 nF, r = 1 MΩ ⇒ f_c = 1/(2π·1e6·31.831e-9) = 5.00 Hz
        //   edge:     1 Ω into 1 MΩ ⇒ divider 1e6/(1+1e6) = 0.999999 ≈ unity (loading isolated)
        // 1 kHz sits 200× above the 5 Hz corner → the AC passes ~untouched; DC (0 Hz) is a zero
        // of the high-pass → fully blocked. So after settling the output is a 1 V sine on 0 V.
        let mut g = Graph::new();
        let src = g.add(SineSource::new(
            1_000.0,
            Volts::new(1.0),
            Volts::new(2.0),
            Ohms::new(1.0),
        ));
        let blk = g.add(DcBlocker::new(
            Farads::new(31.831e-9),
            Ohms::new(1_000_000.0),
            Ohms::new(150.0),
        ));
        g.connect(src, 0, blk, 0);
        g.set_output(blk, 0);

        // One long block: 200k samples ≫ the settling time (τ = RC ≈ 12.2k samples), so the
        // second half is fully steady. Drop the first half as the high-pass transient.
        let len = 200_000;
        let mut sched = compile(g, len, rate(), 0).expect("valid DC-block chain");
        let mut out = VoltageBuffer::zeros(len, rate());
        sched.process(&mut out);
        let tail = &out.as_slice()[len / 2..];

        // DC removed: the 2 V pedestal is gone — the steady tail averages to ≈ 0.
        let mean: f64 = tail.iter().map(|&v| f64::from(v)).sum::<f64>() / tail.len() as f64;
        assert!(
            mean.abs() < 5e-3,
            "DC offset should be blocked, mean = {mean}"
        );

        // AC preserved: a 1 V sine through the unity divider has RMS amp/√2 = 0.7071, and the
        // 5 Hz corner takes nothing off a 1 kHz tone.
        assert_relative_eq!(rms(tail), 0.707_106_77, max_relative = 2e-2);
    }
}

/// Story 1.4.3 — headroom & clipping at the rail voltage, and the harmonic distortion that
/// emerges, on compiled patches. "Tests are the oracle" (§3.5): you can't hear a clip onset or
/// count harmonics by ear, so each number is hand-computed with the calc inline.
#[cfg(test)]
mod clipping_phenomena {
    use super::*;
    use crate::electrical::Ohms;
    use crate::level::headroom_db;
    use crate::node::GainStage;
    use crate::signal::Volts;
    use crate::test_util::{SineSource, tone_amplitude};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// `SineSource(amp, freq) → GainStage(gain, rail)`, tapped at the stage output. The source
    /// drives from 1 Ω into the stage's 1 MΩ input, so the loading divider is ~unity (`DIVIDER`)
    /// and the only thing shaping the signal is the gain and its rail clip. `len` is a whole
    /// number of cycles of `freq` so [`tone_amplitude`] reads harmonics exactly.
    fn run_tone(amp: Volts, freq: f64, gain: f32, rail: Volts, len: usize) -> Vec<f32> {
        let mut g = Graph::new();
        let src = g.add(SineSource::new(freq, amp, Volts::new(0.0), Ohms::new(1.0)));
        let stage = g.add(GainStage::new(
            gain,
            rail,
            InputZ::new(Ohms::new(1_000_000.0)),
            Ohms::new(150.0),
        ));
        g.connect(src, 0, stage, 0);
        g.set_output(stage, 0);
        let mut sched = compile(g, len, rate(), 0).expect("valid clip chain");
        let mut out = VoltageBuffer::zeros(len, rate());
        sched.process(&mut out);
        out.as_slice().to_vec()
    }

    /// 1 Ω source into a 1 MΩ input: 1e6/(1+1e6) = 0.999999, i.e. ~unity loading.
    const DIVIDER: f32 = 0.999_999;
    /// 200 whole cycles of a 1 kHz tone at 384 kHz (384 samples/cycle) — also whole cycles of
    /// 2, 3, 4, 5 kHz, so the harmonic bins stay orthogonal.
    const LEN: usize = 384 * 200;

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |m, &v| m.max(v.abs()))
    }

    #[test]
    fn output_clips_to_the_rail_past_clip_onset() {
        // Stage: ×5 gain into a 10 V rail. The stage clips when its output wants to exceed the
        // rail, i.e. at source amplitude  amp_onset = rail / (DIVIDER · gain) = 10 / (·5) = 2.0 V.
        let onset = 10.0 / (DIVIDER * 5.0);
        assert_relative_eq!(onset, 2.0, epsilon = 1e-4);

        // Below onset (1.8 V): wanted peak = 1.8 · 0.999999 · 5 = 9.0 V < 10 V rail → clean,
        // unclipped, peak sits at the wanted 9.0 V.
        let clean = run_tone(Volts::new(1.8), 1_000.0, 5.0, Volts::new(10.0), LEN);
        assert_relative_eq!(peak(&clean), 1.8 * DIVIDER * 5.0, max_relative = 1e-2);
        assert!(peak(&clean) < 10.0, "below onset must not clip");

        // Above onset (3.0 V): wanted peak = 3.0 · ~1 · 5 = 15 V > 10 V → the output flat-tops
        // at exactly the ±10 V rail. Clipping emergent from the rail in volts, not a flag.
        let clipped = run_tone(Volts::new(3.0), 1_000.0, 5.0, Volts::new(10.0), LEN);
        assert_relative_eq!(peak(&clipped), 10.0, max_relative = 1e-3);
    }

    #[test]
    fn a_clean_signal_below_the_rail_is_undistorted() {
        // ×2 into a 10 V rail, 1 V source → ~2 V peak, far under the rail: a pure sine.
        let out = run_tone(Volts::new(1.0), 1_000.0, 2.0, Volts::new(10.0), LEN);
        let p = peak(&out);

        // The fundamental carries the whole signal; the 3rd harmonic is negligible (no clip).
        let fund = tone_amplitude(&out, 1_000.0, rate());
        let third = tone_amplitude(&out, 3_000.0, rate());
        assert_relative_eq!(fund, 2.0 * DIVIDER, max_relative = 1e-3);
        assert!(third / fund < 0.01, "an unclipped sine has no harmonics");

        // Headroom: a ~2 V peak under a 10 V rail = 20·log10(10/2) = 13.98 dB of room left.
        assert_relative_eq!(
            headroom_db(Volts::new(p), Volts::new(10.0)),
            13.979,
            epsilon = 5e-2
        );
    }

    #[test]
    fn hard_clipping_generates_odd_harmonics() {
        // Overdrive ×100 into a 1 V rail: the sine is clamped almost the instant it leaves zero,
        // so the output is essentially a ±1 V square wave. A square wave of amplitude R has the
        // Fourier series (4R/π)·(sin ωt + ⅓ sin 3ωt + ⅕ sin 5ωt + …): only ODD harmonics, each
        // falling as 1/n. Symmetric clipping ⇒ no even harmonics. (This is *why* clipping sounds
        // harsh — it injects a stack of odd overtones.)
        let out = run_tone(Volts::new(1.0), 1_000.0, 100.0, Volts::new(1.0), LEN);
        let fund = tone_amplitude(&out, 1_000.0, rate());
        let second = tone_amplitude(&out, 2_000.0, rate());
        let third = tone_amplitude(&out, 3_000.0, rate());
        let fifth = tone_amplitude(&out, 5_000.0, rate());

        // Fundamental of a ±1 V square wave: 4·R/π = 1.2732 V.
        assert_relative_eq!(fund, 4.0 / core::f32::consts::PI, max_relative = 2e-2);
        // Odd harmonics fall as 1/n: 3rd/1st = 1/3, 5th/1st = 1/5.
        assert_relative_eq!(third / fund, 1.0 / 3.0, max_relative = 3e-2);
        assert_relative_eq!(fifth / fund, 1.0 / 5.0, max_relative = 3e-2);
        // Symmetric clip ⇒ the even harmonics are absent.
        assert!(
            second / fund < 0.02,
            "symmetric clipping has no even harmonics"
        );
    }
}

/// Story 1.5.1 — two-conductor balanced lines: a differential signal survives the trip, and a
/// common-mode offset cancels at the receiver difference (`V+ − V−`). The rejection *emerges*
/// from the subtraction; it is not a flag. "Tests are the oracle" (§3.5) — numbers hand-computed.
#[cfg(test)]
mod balanced_phenomena {
    use super::*;
    use crate::electrical::{Farads, Ohms};
    use crate::node::{BalancedDriver, BalancedReceiver, DcBlocker, GainStage, TestSource};
    use crate::signal::Volts;
    use crate::test_util::{BalancedTestSource, SineSource, rms};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn balanced_chain_preserves_the_differential_signal() {
        // source(2 V, 1 Ω) → balanced driver → balanced receiver → tap. Every face is near-ideal
        // (1 Ω out into 1 GΩ in), so each divider ≈ 1:
        //   driver in ≈ 2 V → V+ = +1 V, V− = −1 V
        //   balanced edge ≈ unity per conductor → V+ ≈ +1, V− ≈ −1
        //   receiver out = V+ − V− ≈ 2 V  ← the differential survives unity end-to-end.
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(2.0), Ohms::new(1.0)));
        let drv = g.add(BalancedDriver::new(
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect(src, 0, drv, 0);
        g.connect(drv, 0, rcv, 0);
        g.set_output(rcv, 0);

        let mut sched = compile(g, 8, rate(), 0).expect("valid balanced chain");
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
        for &v in out.as_slice() {
            assert_relative_eq!(v, 2.0, epsilon = 1e-4);
        }
    }

    #[test]
    fn balanced_receiver_rejects_common_mode() {
        // A balanced source emits a 2 V differential signal on a common-mode pedestal `cm`:
        //   V+ = cm + 1, V− = cm − 1. The edge scales both conductors by the same ≈unity gain,
        //   so the receiver difference is (cm+1) − (cm−1) = 2 V, *independent of cm*. That equal
        //   scaling is why common-mode cancels — the headline of a balanced line.
        fn run(cm: f32) -> f32 {
            let mut g = Graph::new();
            let src = g.add(BalancedTestSource::new(
                Volts::new(2.0),
                Volts::new(cm),
                Ohms::new(1.0),
            ));
            let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
            g.connect(src, 0, rcv, 0);
            g.set_output(rcv, 0);
            let mut sched = compile(g, 8, rate(), 0).expect("valid balanced chain");
            let mut out = VoltageBuffer::zeros(8, rate());
            sched.process(&mut out);
            out.get(0).get()
        }

        // No common-mode and a large +100 V common-mode pedestal give the same 2 V differential.
        assert_relative_eq!(run(0.0), 2.0, epsilon = 1e-4);
        assert_relative_eq!(run(100.0), 2.0, epsilon = 1e-4);
        // Ideal rejection: the 100 V pedestal leaves no residue beyond float epsilon.
        assert_relative_eq!(run(100.0), run(0.0), epsilon = 1e-4);
    }

    #[test]
    fn rejects_conductor_count_mismatch() {
        // A balanced output (2 conductors) into an unbalanced input (1) is a conductor mismatch:
        // cross-type connections aren't modeled yet, so compile rejects it rather than guessing.
        let mut g = Graph::new();
        let drv = g.add(BalancedDriver::new(
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        let amp = g.add(GainStage::new(
            2.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        ));
        g.connect(drv, 0, amp, 0); // balanced out → unbalanced in
        g.set_output(amp, 0);
        assert_eq!(
            compile(g, 8, rate(), 0).err(),
            Some(CompileError::ConductorMismatch {
                from_node: 0,
                from_port: 0,
                to_node: 1,
                to_port: 0,
            })
        );
    }

    #[test]
    fn dc_blocker_composes_on_the_balanced_pair() {
        // The DC blocker is a per-conductor node, so the compiler lifts it across the pair: the
        // driver's 2-conductor output infers it to 2 and replicates it per leg. Before the lift
        // this very wiring would be a ConductorMismatch — now an ordinary processor just composes,
        // "balanced" never a label. (This is the mechanism phantom rides on in 1.5.3.)
        //
        //   source:  2 V DC + 1 V·sin(2π·10k)                    (single-ended)
        //   driver:  V+ = 1 + 0.5·sin, V− = −(1 + 0.5·sin)       (≈unity edge)
        //   per-leg DC block (1 kHz corner): strips the ±1 V DC on each leg → leaves ±0.5·sin
        //   receiver: V+ − V− = sin  → amp 1 V, RMS 0.7071, mean 0
        let mut g = Graph::new();
        let src = g.add(SineSource::new(
            10_000.0,
            Volts::new(1.0),
            Volts::new(2.0),
            Ohms::new(1.0),
        ));
        let drv = g.add(BalancedDriver::new(
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        let blk = g.add(DcBlocker::new(
            Farads::new(15.915e-9), // with r = 10 kΩ → f_c = 1 kHz, a decade below the 10 kHz tone
            Ohms::new(10_000.0),
            Ohms::new(150.0),
        ));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect(src, 0, drv, 0);
        g.connect(drv, 0, blk, 0);
        g.connect(blk, 0, rcv, 0);
        g.set_output(rcv, 0);

        let len = 40_000; // ≫ settling (τ = RC ≈ 61 samples)
        let mut sched =
            compile(g, len, rate(), 0).expect("balanced chain with a lifted DC blocker");
        let mut out = VoltageBuffer::zeros(len, rate());
        sched.process(&mut out);
        let tail = &out.as_slice()[len / 2..];

        // DC stripped on each leg → the recovered differential averages to ≈ 0.
        let mean: f64 = tail.iter().map(|&v| f64::from(v)).sum::<f64>() / tail.len() as f64;
        assert!(
            mean.abs() < 1e-2,
            "per-leg DC block should remove the offset, mean = {mean}"
        );
        // Differential audio survives the passband: RMS ≈ amp/√2.
        assert_relative_eq!(rms(tail), 0.707_106_77, max_relative = 2e-2);
    }
}

/// Story 1.5.2 — cable pickup: broadband interference (EMI) coupling onto the wire as a noise
/// voltage. On an unbalanced edge it lands on the signal at the µV scale the plan calls for (the
/// balanced *rejection* of it is the CMRR story, Story 1.5.4). "Tests are the oracle" (§3.5):
/// the floor is a hand-computed number, with the calc inline.
#[cfg(test)]
mod pickup_phenomena {
    use super::*;
    use crate::electrical::{Cable, Farads};
    use crate::node::{GainStage, TestSource};
    use crate::signal::Volts;
    use crate::test_util::rms;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A near-ideal unity buffer: huge `Zin`, tiny `Zout`, gain 1, no internal noise — so its
    /// output is exactly what arrived at its input (here, the pickup coupled onto the cable).
    fn unity_buffer() -> GainStage {
        GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        )
    }

    /// Silent source → a cable that picks up `density` → unity buffer → tap; return the output.
    fn run_pickup(density: NoiseDensity, len: usize, seed: u64) -> Vec<f32> {
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
        let buf = g.add(unity_buffer());
        g.connect_cabled(
            src,
            0,
            buf,
            0,
            Cable::new(Ohms::ZERO, Farads::ZERO).with_pickup(density),
        );
        g.set_output(buf, 0);
        let mut sched = compile(g, len, rate(), seed).expect("valid pickup chain");
        let mut out = VoltageBuffer::zeros(len, rate());
        sched.process(&mut out);
        out.as_slice().to_vec()
    }

    #[test]
    fn cable_pickup_floor_matches_density() {
        // Pickup couples onto the wire after the (≈unity) divider, so an unbalanced receiver sees
        // the full floor:  σ = D·√(fs/2) = 10e-9·√192000 = 4.3818 µV. (200k samples ⇒ RMS
        // converges to σ within ~0.16%; 2% is comfortable.)
        let d = NoiseDensity::new(10e-9);
        let sigma = d.per_sample_sigma(rate());
        let out = run_pickup(d, 200_000, 0xCAB1_E000);
        assert_relative_eq!(rms(&out), sigma, max_relative = 0.02);
    }

    #[test]
    fn no_pickup_is_silence() {
        // A cable with zero pickup density adds nothing — a silent source stays silent.
        let out = run_pickup(NoiseDensity::ZERO, 1_000, 1);
        assert!(out.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn pickup_is_reproducible() {
        // Same compile seed ⇒ identical pickup realization (determinism for tests/replays).
        let d = NoiseDensity::new(50e-9);
        assert_eq!(run_pickup(d, 1_000, 42), run_pickup(d, 1_000, 42));
    }
}

/// Story 1.5.4 — common-mode rejection: the same cable pickup that contaminates an unbalanced
/// line cancels at a balanced receiver's difference. **Ideal rejection only** — both conductors
/// carry the *identical* common-mode draw, so `V+ − V−` cancels it to **bit-exact zero** (infinite
/// CMRR). Finite CMRR is leg *asymmetry*, deferred (Story 1.5 design notes). "Tests are the oracle".
#[cfg(test)]
mod cmrr_phenomena {
    use super::*;
    use crate::electrical::{Cable, Farads};
    use crate::node::{BalancedDriver, BalancedReceiver, GainStage, TestSource};
    use crate::signal::Volts;
    use crate::test_util::rms;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// Pickup density used across the contrast: 50 nV/√Hz ⇒ σ = 50e-9·√192000 = 21.9 µV.
    fn pickup_cable() -> Cable {
        Cable::new(Ohms::ZERO, Farads::ZERO).with_pickup(NoiseDensity::new(50e-9))
    }

    /// Unbalanced: silent source → pickup cable → unity buffer → tap (the pickup passes straight
    /// through — no second conductor to subtract it against).
    fn run_unbalanced(len: usize, seed: u64) -> Vec<f32> {
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
        let buf = g.add(GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        g.connect_cabled(src, 0, buf, 0, pickup_cable());
        g.set_output(buf, 0);
        let mut sched = compile(g, len, rate(), seed).expect("unbalanced pickup chain");
        let mut out = VoltageBuffer::zeros(len, rate());
        sched.process(&mut out);
        out.as_slice().to_vec()
    }

    /// Balanced: silent source → driver → pickup cable → receiver → tap. The pickup couples
    /// common-mode (identical on both legs) and is rejected by the receiver difference.
    fn run_balanced(len: usize, seed: u64) -> Vec<f32> {
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
        let drv = g.add(BalancedDriver::new(
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect(src, 0, drv, 0);
        g.connect_cabled(drv, 0, rcv, 0, pickup_cable());
        g.set_output(rcv, 0);
        let mut sched = compile(g, len, rate(), seed).expect("balanced pickup chain");
        let mut out = VoltageBuffer::zeros(len, rate());
        sched.process(&mut out);
        out.as_slice().to_vec()
    }

    #[test]
    fn unbalanced_passes_interference_while_balanced_rejects_it() {
        let sigma = NoiseDensity::new(50e-9).per_sample_sigma(rate());
        let unbal = run_unbalanced(200_000, 0xCAB1_E001);
        let bal = run_balanced(200_000, 0xCAB1_E001);

        // Unbalanced: the full µV pickup floor reaches the receiver (σ = 21.9 µV).
        assert_relative_eq!(rms(&unbal), sigma, max_relative = 0.02);
        // Balanced: the identical common-mode draw on V+ and V− cancels at the difference — exactly,
        // not just statistically. Ideal (infinite) CMRR, the headline of a balanced line.
        assert!(
            bal.iter().all(|&v| v == 0.0),
            "balanced should reject common-mode pickup to bit-exact zero, got rms {}",
            rms(&bal)
        );
    }

    #[test]
    fn balanced_recovers_the_signal_through_pickup() {
        // Not just zeroing everything: a 2 V DC differential signal driven through the same
        // picking-up cable comes back clean (≈2 V), with the common-mode pickup gone and no noise
        // left on top.
        //   driver: V+ = +1, V− = −1; edge adds identical pickup p to each → V+ = g+p, V− = −g+p
        //   receiver: (g+p) − (−g+p) = 2g ≈ 2 V — the pickup cancels, the signal survives.
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(2.0), Ohms::new(1.0)));
        let drv = g.add(BalancedDriver::new(
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect(src, 0, drv, 0);
        g.connect_cabled(drv, 0, rcv, 0, pickup_cable());
        g.set_output(rcv, 0);
        let mut sched = compile(g, 16, rate(), 5).expect("balanced signal+pickup chain");
        let mut out = VoltageBuffer::zeros(16, rate());
        sched.process(&mut out);
        for &v in out.as_slice() {
            assert_relative_eq!(v, 2.0, epsilon = 1e-4);
        }
    }
}
