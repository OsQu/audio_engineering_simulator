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
//! one-pole — the exact, complete model while there's at most one reactive element on an edge.
//! It's deliberately a struct behind a constructor and a single `process` entry,
//! **not** a contract: a reactive source later makes an edge a 2nd-order transfer function
//! depending on both endpoints, which generalizes the representation without touching callers.

mod events;
mod hum;
mod swap;
mod topo;

pub use events::{EventInputId, EventQueue};
pub use swap::ScheduleSlot;

use hum::HumGen;

use crate::electrical::{InputZ, Ohms, OnePole, fan_out_gains};
use crate::graph::{Edge, Graph, NodeId};
use crate::node::{Lifted, Node};
use crate::noise::NoiseDensity;
use crate::param::{ParamHandle, ParamId, ParamQueue, Params, Smoother, smooth_samples};
use crate::port::{DigitalFace, EventFace};
use crate::rng::Rng;
use crate::signal::{
    AnalogRate, ClockDomainId, Domain, EventBuffer, Lane, SampleBuffer, TimedEvent, VoltageBuffer,
};
use core::fmt;

/// Salt mixed into the compile seed to derive the **edge** pickup root, keeping it independent of
/// the per-node noise streams so adding cable pickup never perturbs a node's noise realization.
const EDGE_SEED_SALT: u64 = 0xED9E_5EED_ED9E_5EED;

/// Why a [`Graph`] could not be compiled. All structural — caught here, never on the hot path.
///
/// (`PartialEq` but not `Eq`: some variants carry `f64` rates, so equality is structural-with-floats
/// — fine for the exact, non-NaN values these errors report.)
#[derive(Debug, Clone, PartialEq)]
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
    /// match conductor counts, or insert an adapter device (not yet modeled).
    ConductorMismatch {
        from_node: usize,
        from_port: usize,
        to_node: usize,
        to_port: usize,
    },
    /// An edge connects two ports of different carrier domains (e.g. analog into digital). No
    /// physics bridges domains on a wire — only a converter node does, internally — so the edge
    /// is rejected rather than guessed.
    DomainMismatch {
        from_node: usize,
        from_port: usize,
        to_node: usize,
        to_port: usize,
    },
    /// A converter's digital sample rate does not integer-divide the analog rate, so its
    /// decimation factor `M` isn't a whole number. An integer ratio is required; arbitrary ratios
    /// would need a fractional resampler (not yet modeled).
    RateIndivisible {
        node: usize,
        analog_hz: f64,
        digital_hz: f64,
    },
    /// The block length is not a multiple of a converter's decimation factor `M`, so a block
    /// would not hold a whole number of digital samples.
    BlockLenIndivisible {
        node: usize,
        block_len: usize,
        factor: usize,
    },
    /// A digital edge connects two different sample rates — a clock crossing that needs a
    /// sample-rate converter, not yet modeled.
    ClockCrossingUnsupported { from_node: usize, to_node: usize },
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
            Self::DomainMismatch {
                from_node,
                from_port,
                to_node,
                to_port,
            } => write!(
                f,
                "node {from_node} output port {from_port} and node {to_node} input port \
                 {to_port} are different carrier domains (analog vs. digital) — only a converter \
                 bridges domains"
            ),
            Self::RateIndivisible {
                node,
                analog_hz,
                digital_hz,
            } => write!(
                f,
                "node {node}'s digital rate {digital_hz} Hz does not integer-divide the analog \
                 rate {analog_hz} Hz"
            ),
            Self::BlockLenIndivisible {
                node,
                block_len,
                factor,
            } => write!(
                f,
                "block length {block_len} is not a multiple of node {node}'s decimation factor \
                 {factor}"
            ),
            Self::ClockCrossingUnsupported { from_node, to_node } => write!(
                f,
                "digital edge from node {from_node} to node {to_node} crosses sample rates; \
                 sample-rate conversion is not yet modeled"
            ),
            Self::Cycle => write!(f, "the graph has a cycle"),
        }
    }
}

impl std::error::Error for CompileError {}

/// A connection's baked local solve: scale by a constant gain, then (if cabled) the one-pole
/// treble rolloff, then (if the cable couples it) add interference — broadband pickup and/or
/// ground-loop hum. See the module docs on why this is a struct, not a fixed contract.
struct EdgeTransform {
    gain: f32,
    lowpass: Option<OnePole>,
    /// Per-conductor interference stream + per-sample σ. Every conductor of one edge holds an
    /// **identically-seeded** clone, so they draw the *same* sequence: the pickup is common-mode
    /// (equal on V+ and V−) and cancels at a balanced receiver, while passing on an unbalanced one.
    pickup: Option<(Rng, f32)>,
    /// Ground-loop hum, identical on every conductor of the edge — common-mode, like pickup.
    hum: Option<HumGen>,
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
        if let Some(hum) = &mut self.hum {
            for d in dst.iter_mut() {
                *d += hum.step();
            }
        }
    }
}

/// A connection's baked behavior, by domain. Analog edges carry the electrical solve; digital
/// edges are a same-clock-domain sample copy. (A digital edge across *different* clock domains is
/// a resample — not yet modeled; `compile` rejects it.)
enum EdgeKind {
    /// Analog: scale by the divider gain, optional cable rolloff, optional coupled interference.
    Analog(EdgeTransform),
    /// Digital audio: copy samples src → dst within one clock domain.
    DigitalRoute,
    /// Control events: copy the sparse event list src → dst (no electrical solve, no clock).
    EventRoute,
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
    /// Run a connection: read output-pool lane `src`, write input-pool lane `dst`.
    Edge {
        src: usize,
        dst: usize,
        kind: EdgeKind,
    },
}

/// An **open** event input — an event-domain input port with no incoming edge, so the external
/// [`EventQueue`] may feed it. Records its `(node, port)` identity (for
/// [`Schedule::event_input`] resolution) and its `lane` in `input_pool` (the delivery target).
struct EventInputSlot {
    node: usize,
    port: usize,
    lane: usize,
}

/// A compiled, runnable patch: nodes, the buffer pools, and the ordered step list.
///
/// Produced by [`compile`]; driven by [`process`](Self::process) one block at a time. Owns its
/// nodes (with their state) and every buffer, so running it touches no shared state and
/// allocates nothing.
pub struct Schedule {
    nodes: Vec<Box<dyn Node>>,
    input_pool: Vec<Lane>,
    output_pool: Vec<Lane>,
    steps: Vec<Step>,
    /// Index into `output_pool` of the designated output tap.
    out_buf: usize,
    block_len: usize,
    /// Event-domain input ports the host can feed (those without an incoming edge), for resolving
    /// [`event_input`](Self::event_input) and clearing each block.
    event_inputs: Vec<EventInputSlot>,
    /// Absolute sample position of the next block's first sample — the clock external events are
    /// timestamped against. Starts at 0 and advances by `block_len` each processed block.
    sample_pos: u64,
    /// One de-zipper [`Smoother`] per declared control param, flat and contiguous by node;
    /// node `n`'s params are `param_store[param_base[n] .. param_base[n] + param_count[n]]`.
    param_store: Vec<Smoother>,
    /// Start index of each node's param run in `param_store`.
    param_base: Vec<usize>,
    /// Number of params each node declared (its run length in `param_store`).
    param_count: Vec<usize>,
}

impl Schedule {
    /// The fixed block length every buffer is sized to. [`process`](Self::process) fills this
    /// many samples.
    pub fn block_len(&self) -> usize {
        self.block_len
    }

    /// The schedule's total signal-path group delay in **analog-rate samples** — the sum of every
    /// node's [`group_delay_samples`](crate::Node::group_delay_samples) (the converter FIRs; most
    /// nodes add 0). Exact for a **linear chain** (the path to the output passes through every node),
    /// which the real-time patch is; for a branchy graph it over-counts delay off the output path, so
    /// treat it as the chain-latency estimate it is. Off the hot path — read once after `compile`.
    #[must_use]
    pub fn group_delay_samples(&self) -> f64 {
        self.nodes.iter().map(|n| n.group_delay_samples()).sum()
    }

    /// Resolve an **open** event input port — an event-domain input with no incoming edge — to a
    /// handle the host can push external events to (see [`EventQueue`]). Returns `None` if the port
    /// isn't an open event input (it's analog/digital, out of range, or fed by an edge — an
    /// edge-driven event input is filled by the graph, not the host). Off the hot path.
    #[must_use]
    pub fn event_input(&self, node: NodeId, port: usize) -> Option<EventInputId> {
        self.event_inputs
            .iter()
            .find(|s| s.node == node.0 && s.port == port)
            .map(|s| EventInputId(s.lane))
    }

    /// Resolve a node's declared control parameter to a handle the host can drive (see
    /// [`ParamQueue`]). Returns `None` if the node or param id is out of range. Off the hot path.
    #[must_use]
    pub fn param(&self, node: NodeId, id: ParamId) -> Option<ParamHandle> {
        let base = *self.param_base.get(node.0)?;
        if (id.0 as usize) < self.param_count[node.0] {
            Some(ParamHandle(base + id.0 as usize))
        } else {
            None
        }
    }

    /// Process one block with no external input — the convenience for offline renders and chains
    /// driven entirely from within the graph. See [`process_io`](Self::process_io).
    ///
    /// Writes `min(out.len(), block_len)` samples; size `out` to [`block_len`](Self::block_len).
    pub fn process(&mut self, out: &mut VoltageBuffer) {
        // Zero-capacity queues allocate nothing and drain to nothing — the hot path is identical to
        // the full path with no pending input.
        let mut no_params = ParamQueue::with_capacity(0);
        let mut no_events = EventQueue::with_capacity(0);
        self.process_io(out, &mut no_params, &mut no_events);
    }

    /// Process one block delivering only `events` (no param changes). See [`process_io`](Self::process_io).
    pub fn process_with_events(&mut self, out: &mut VoltageBuffer, events: &mut EventQueue) {
        let mut no_params = ParamQueue::with_capacity(0);
        self.process_io(out, &mut no_params, events);
    }

    /// Process one block applying only `params` (no events). See [`process_io`](Self::process_io).
    pub fn process_with_params(&mut self, out: &mut VoltageBuffer, params: &mut ParamQueue) {
        let mut no_events = EventQueue::with_capacity(0);
        self.process_io(out, params, &mut no_events);
    }

    /// Process one block: apply pending control-param targets and deliver due events, run every
    /// step, advance the param de-zippers, and copy the designated output tap into `out`. Hot path
    /// — zero allocation, no panic, no locks.
    ///
    /// `params` are applied latest-wins as new glide targets; each node then reads its
    /// **within-block-ramped** values during `process`. `events` due in
    /// `[sample_pos, sample_pos + block_len)` land at their block-relative offsets (a late event
    /// clamps to offset 0); later events stay queued. `sample_pos` then advances one block.
    ///
    /// Writes `min(out.len(), block_len)` samples; size `out` to [`block_len`](Self::block_len).
    pub fn process_io(
        &mut self,
        out: &mut VoltageBuffer,
        params: &mut ParamQueue,
        events: &mut EventQueue,
    ) {
        let blk = self.block_len;
        let block_len = blk as u64;
        let pos = self.sample_pos;
        let end = pos + block_len;

        // Apply pending param targets (latest-wins) — each re-aims its smoother's glide. The handle
        // comes from the external queue (the cross-thread seam), so index defensively: a stale or
        // foreign handle is skipped, never a panic — `process` must be total on the audio thread.
        for (handle, target) in params.drain() {
            if let Some(smoother) = self.param_store.get_mut(handle.0) {
                smoother.set_target(target);
            }
        }

        // Refill the open event inputs for this block: clear last block's events (no edge writes
        // them), then deliver the due ones at their block-relative offsets. Disjoint field borrows.
        for slot in &self.event_inputs {
            self.input_pool[slot.lane].events_mut().clear();
        }
        for e in events.drain_due(end) {
            let offset = e.when.saturating_sub(pos).min(block_len.saturating_sub(1)) as u32;
            // `e.target` is host-supplied: guard both bounds *and* variant, so a foreign id that is
            // in range but points at a non-event lane skips rather than hitting `events_mut`'s
            // `unreachable!`. Totality over the cross-thread seam, same as the param drain above.
            if let Some(Lane::Events(buf)) = self.input_pool.get_mut(e.target.0) {
                buf.push(TimedEvent {
                    offset,
                    message: e.message,
                });
            }
        }
        self.sample_pos = end;

        let Self {
            nodes,
            input_pool,
            output_pool,
            steps,
            out_buf,
            param_store,
            param_base,
            param_count,
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
                    // Hand the node a view of its own smoothers' current (ramped) values.
                    let base = param_base[*node];
                    let params = Params::new(&param_store[base..base + param_count[*node]]);
                    nodes[*node].process(&params, ins, outs);
                }
                Step::Edge { src, dst, kind } => {
                    // Different pools ⇒ disjoint borrows, no aliasing.
                    match kind {
                        EdgeKind::Analog(transform) => {
                            let source = output_pool[*src].voltage().as_slice();
                            let dest = input_pool[*dst].voltage_mut().as_mut_slice();
                            transform.process(source, dest);
                        }
                        EdgeKind::DigitalRoute => {
                            // Same clock domain ⇒ equal length (validated at compile): a copy.
                            let source = output_pool[*src].sample().as_slice();
                            let dest = input_pool[*dst].sample_mut().as_mut_slice();
                            dest.copy_from_slice(source);
                        }
                        EdgeKind::EventRoute => {
                            // Copy the sparse event list (bounded, alloc-free; drops any excess
                            // past the destination lane's capacity).
                            let (src_pool, dst_pool) = (&output_pool[*src], &mut input_pool[*dst]);
                            dst_pool.events_mut().copy_from(src_pool.events());
                        }
                    }
                }
            }
        }

        // Advance every de-zipper one block: this block's nodes read the block-start values, so
        // the glide steps forward only now, off the per-sample path.
        for smoother in param_store.iter_mut() {
            smoother.advance(blk);
        }

        let tapped = output_pool[*out_buf].voltage().as_slice();
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
        // Carriers must match on a wire — only a converter node bridges domains, internally.
        if nodes[e.from_node.0].outputs()[e.from_port].domain()
            != nodes[e.to_node.0].inputs()[e.to_port].domain()
        {
            return Err(CompileError::DomainMismatch {
                from_node: e.from_node.0,
                from_port: e.from_port,
                to_node: e.to_node.0,
                to_port: e.to_port,
            });
        }
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
            for _ in 0..face.lane_count() {
                output_pool.push(alloc_lane(
                    face.digital(),
                    face.events(),
                    block_len,
                    rate,
                    n,
                )?);
            }
        }
        out_count[n] = output_pool.len() - out_offset[n];
        out_port_base.push(obases);

        in_offset[n] = input_pool.len();
        let mut ibases = Vec::with_capacity(nodes[n].inputs().len());
        for face in nodes[n].inputs() {
            ibases.push(input_pool.len());
            for _ in 0..face.lane_count() {
                input_pool.push(alloc_lane(
                    face.digital(),
                    face.events(),
                    block_len,
                    rate,
                    n,
                )?);
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

    // --- 5b. Collect the **open** event inputs: event-domain input ports with no incoming edge,
    //         which the external queue may feed. (An edge-driven event input is filled by an
    //         EventRoute, so it's excluded.) Event ports own one lane, so the port's lane is its
    //         first conductor. ---
    let mut event_inputs = Vec::new();
    for n in 0..node_count {
        for (p, face) in nodes[n].inputs().iter().enumerate() {
            if face.domain() == Domain::Events {
                let lane = in_port_base[n][p];
                if !input_taken[lane] {
                    event_inputs.push(EventInputSlot {
                        node: n,
                        port: p,
                        lane,
                    });
                }
            }
        }
    }

    // --- 6. Topological order (rejects cycles). ---
    let deps: Vec<(usize, usize)> = edges.iter().map(|e| (e.from_node.0, e.to_node.0)).collect();
    let order = topo::topo_sort(node_count, &deps).ok_or(CompileError::Cycle)?;

    // --- 7. Bake each edge's local solve. Edges sharing an output port are one fan-out node:
    //        solve them together so the parallel loading is right. A balanced edge bakes **one
    //        transform per conductor** — the same differential divider gain on each, but an
    //        independent cable one-pole (each wire has its own filter state). ---
    let mut edge_kinds: Vec<Option<Vec<EdgeKind>>> = (0..edges.len()).map(|_| None).collect();
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
        let group = &by_port[i..j];
        let out_face = nodes[from_node].outputs()[from_port];
        match out_face.domain() {
            Domain::Analog => {
                // Fan-out: solve the parallel loading across the whole group so the divider gains
                // are right, then one transform per conductor (same gain, independent one-pole).
                let z_out: Ohms = out_face.analog().expect("analog output face").z_out();
                let branches: Vec<(Ohms, InputZ)> = group
                    .iter()
                    .map(|&ei| {
                        let e = &edges[ei];
                        let r = e.cable.map_or(Ohms::ZERO, |c| c.r());
                        let load = nodes[e.to_node.0].inputs()[e.to_port]
                            .analog()
                            .expect("analog input face");
                        (r, load)
                    })
                    .collect();
                let gains = fan_out_gains(z_out, &branches);
                for (k, &ei) in group.iter().enumerate() {
                    let e = &edges[ei];
                    let load = nodes[e.to_node.0].inputs()[e.to_port]
                        .analog()
                        .expect("analog input face");
                    let conductors = load.conductors();
                    let mut kinds = Vec::with_capacity(conductors);
                    for _ in 0..conductors {
                        kinds.push(EdgeKind::Analog(EdgeTransform {
                            gain: gains[k],
                            lowpass: e.cable.map(|c| c.lowpass(z_out, load, rate)),
                            pickup: None, // pickup/hum are installed below, after the gains are baked
                            hum: None,
                        }));
                    }
                    edge_kinds[ei] = Some(kinds);
                }
            }
            Domain::DigitalAudio => {
                // No electrical solve on a digital wire: each channel is a same-domain sample copy.
                let src_rate = out_face
                    .digital()
                    .expect("digital output face")
                    .format()
                    .rate();
                for &ei in group {
                    let e = &edges[ei];
                    let dst = nodes[e.to_node.0].inputs()[e.to_port]
                        .digital()
                        .expect("digital input face")
                        .format();
                    // Same sample rate ⇒ equal lane length (the copy is total). A cross-rate edge
                    // is a sample-rate conversion, not yet modeled.
                    if dst.rate() != src_rate {
                        return Err(CompileError::ClockCrossingUnsupported {
                            from_node,
                            to_node: e.to_node.0,
                        });
                    }
                    let kinds = (0..dst.channels() as usize)
                        .map(|_| EdgeKind::DigitalRoute)
                        .collect();
                    edge_kinds[ei] = Some(kinds);
                }
            }
            Domain::Events => {
                // No electrical solve and no clock on an event wire: each edge is a sparse
                // event-list copy. An event port owns exactly one lane, so one route per edge.
                for &ei in group {
                    edge_kinds[ei] = Some(vec![EdgeKind::EventRoute]);
                }
            }
        }
        i = j;
    }

    // --- 7b. Seed each edge's coupled interference — broadband pickup and/or ground-loop hum.
    //         Split a stream per edge in **edge-index order** from a root salted off the compile
    //         seed (kept separate from node seeding so a node's stream is unchanged whether or not
    //         edges couple anything). Every conductor of an edge gets the *identical* pickup clone
    //         and hum generator, so both are common-mode. Edges with nothing still consume their
    //         split, so each edge's stream is stable regardless of its neighbours.
    let mut edge_root = Rng::from_seed(seed ^ EDGE_SEED_SALT);
    for (ei, e) in edges.iter().enumerate() {
        let mut stream = edge_root.split();
        let Some(cable) = e.cable else { continue };

        // Hum: a deterministic 50/60 Hz tone whose initial phase is seeded from the edge stream.
        let hum = cable
            .hum()
            .map(|(freq, amp)| HumGen::new(freq, amp, rate, &mut stream));
        // Pickup: broadband Gaussian; the same (post-phase) stream clone on every conductor.
        let pickup_sigma = {
            let d = cable.pickup();
            (d != NoiseDensity::ZERO).then(|| d.per_sample_sigma(rate))
        };

        if hum.is_none() && pickup_sigma.is_none() {
            continue;
        }
        if let Some(kinds) = &mut edge_kinds[ei] {
            for kind in kinds {
                // Interference only couples onto analog wires (a cabled edge is always analog).
                if let EdgeKind::Analog(t) = kind {
                    if let Some(sigma) = pickup_sigma {
                        t.pickup = Some((stream.clone(), sigma));
                    }
                    t.hum = hum;
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
            let kinds = edge_kinds[ei]
                .take()
                .expect("each edge is baked once and emitted once");
            // Map lane k of the source port to lane k of the destination port.
            let src_base = out_port_base[e.from_node.0][e.from_port];
            let dst_base = in_port_base[e.to_node.0][e.to_port];
            for (k, kind) in kinds.into_iter().enumerate() {
                steps.push(Step::Edge {
                    src: src_base + k,
                    dst: dst_base + k,
                    kind,
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

    // --- 10. Build the control-param smoother store: one de-zipper smoother per declared param,
    //         contiguous by node, each starting at its declared default and gliding over its
    //         `smooth_ms` (converted to samples at the analog rate). A node's id `p` resolves to
    //         `param_base[node] + p` (params declared in id order). ---
    let mut param_store = Vec::new();
    let mut param_base = vec![0usize; node_count];
    let mut param_count = vec![0usize; node_count];
    for (n, node) in nodes.iter().enumerate() {
        param_base[n] = param_store.len();
        for decl in node.params() {
            param_store.push(Smoother::new(
                decl.default,
                decl.min,
                decl.max,
                smooth_samples(decl.smooth_ms, rate.as_hz()),
            ));
        }
        param_count[n] = param_store.len() - param_base[n];
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
        event_inputs,
        sample_pos: 0,
        param_store,
        param_base,
        param_count,
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
            Some(nodes[node].outputs()[port].lane_count())
        } else {
            Some(nodes[node].inputs()[port].lane_count())
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

/// Allocate one pool lane for a port, by domain: an analog lane spans the full `block_len`; a
/// digital lane spans `block_len / M`, where `M = analog / digital` rate; an event lane is sparse,
/// pre-allocated to the face's capacity. Validates the integer-divide and block-length constraints.
/// `node` names the owner for errors.
///
/// Every digital lane belongs to one converter clock domain; the [`ClockDomainId`] is a placeholder
/// until multiple clock domains are modeled.
fn alloc_lane(
    digital: Option<DigitalFace>,
    event: Option<EventFace>,
    block_len: usize,
    rate: AnalogRate,
    node: usize,
) -> Result<Lane, CompileError> {
    if let Some(face) = event {
        // Sparse carrier: a bounded list, sized by capacity rather than block length.
        return Ok(Lane::Events(EventBuffer::with_capacity(face.capacity())));
    }
    let Some(face) = digital else {
        return Ok(Lane::Voltage(VoltageBuffer::zeros(block_len, rate)));
    };
    let fmt = face.format();
    let ratio = rate.as_hz() / fmt.rate().as_hz();
    if ratio < 1.0 || ratio.fract() != 0.0 {
        return Err(CompileError::RateIndivisible {
            node,
            analog_hz: rate.as_hz(),
            digital_hz: fmt.rate().as_hz(),
        });
    }
    let m = ratio as usize;
    if !block_len.is_multiple_of(m) {
        return Err(CompileError::BlockLenIndivisible {
            node,
            block_len,
            factor: m,
        });
    }
    Ok(Lane::Sample(SampleBuffer::zeros(
        block_len / m,
        fmt.rate(),
        fmt.bits(),
        ClockDomainId(0),
    )))
}

#[cfg(test)]
mod tests;
