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

mod events;
mod swap;
mod topo;

pub use events::{EventInputId, EventQueue};
pub use swap::ScheduleSlot;

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
    /// match conductor counts, or insert an adapter device when those arrive (Epic 5).
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
    /// decimation factor `M` isn't a whole number. Story 1.6 requires an integer ratio; arbitrary
    /// ratios need the fractional resampler deferred to Epic 5.
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
    /// sample-rate converter (deferred to Epic 5), not yet modeled.
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
                 sample-rate conversion is not yet modeled (Epic 5)"
            ),
            Self::Cycle => write!(f, "the graph has a cycle"),
        }
    }
}

impl std::error::Error for CompileError {}

/// A deterministic 50/60 Hz ground-loop hum generator coupled onto an edge: `amp·sin(phase)` per
/// sample. `Copy` so every conductor of one edge holds an identical generator (same seeded phase,
/// same increment) — the hum is common-mode and cancels at a balanced receiver.
#[derive(Clone, Copy)]
struct HumGen {
    phase: f64,
    dphase: f64,
    amp: f32,
}

impl HumGen {
    /// Next hum sample, advancing the phase. Hot path: no allocation, no panic.
    #[inline]
    fn step(&mut self) -> f32 {
        let v = self.amp * self.phase.sin() as f32;
        self.phase += self.dphase;
        if self.phase >= core::f64::consts::TAU {
            self.phase -= core::f64::consts::TAU;
        }
        v
    }
}

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
/// a resample — deferred to Epic 5; `compile` rejects it for now.)
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
                    // is a sample-rate conversion, deferred to Epic 5.
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
        let hum = cable.hum().map(|(freq, amp)| {
            let phase = f64::from(stream.next_f32_unit()) * core::f64::consts::TAU;
            let dphase = core::f64::consts::TAU * freq * rate.seconds_per_sample();
            HumGen {
                phase,
                dphase,
                amp: amp.get(),
            }
        });
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
/// pre-allocated to the face's capacity. Validates the integer-divide and block-length constraints
/// (Story 1.6). `node` names the owner for errors.
///
/// In Story 1.6 every digital lane belongs to one converter clock domain; the [`ClockDomainId`] is
/// a placeholder until the emergent multi-domain model (Epic 5).
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

/// Story 1.5.5 — ground-loop hum: a 50/60 Hz common-mode tone coupled onto the cable. Audible on
/// an unbalanced line, rejected (bit-exact) on a balanced one — the "lift the ground" lesson. It
/// rides the same edge-injection seam as pickup, just a deterministic generator instead of noise.
#[cfg(test)]
mod hum_phenomena {
    use super::*;
    use crate::electrical::{Cable, Farads};
    use crate::node::{BalancedDriver, BalancedReceiver, GainStage, TestSource};
    use crate::signal::Volts;
    use crate::test_util::tone_amplitude;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    const HUM_HZ: f64 = 60.0; // US mains; 50 Hz in the EU — just the parameter
    const HUM_V: f32 = 0.1;
    const LEN: usize = 64_000; // 10 whole cycles of 60 Hz at 384 kHz (6400 samples/cycle)

    fn hum_cable() -> Cable {
        Cable::new(Ohms::ZERO, Farads::ZERO).with_hum(HUM_HZ, Volts::new(HUM_V))
    }

    #[test]
    fn unbalanced_carries_hum() {
        // Silent source → humming cable → unity buffer → tap: the 60 Hz tone reaches the output at
        // its full amplitude (≈0.1 V) — an unbalanced line has nothing to subtract it against.
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
        let buf = g.add(GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        g.connect_cabled(src, 0, buf, 0, hum_cable());
        g.set_output(buf, 0);
        let mut sched = compile(g, LEN, rate(), 9).expect("unbalanced hum chain");
        let mut out = VoltageBuffer::zeros(LEN, rate());
        sched.process(&mut out);
        assert_relative_eq!(
            tone_amplitude(out.as_slice(), HUM_HZ, rate()),
            HUM_V,
            max_relative = 1e-2
        );
    }

    #[test]
    fn balanced_rejects_hum() {
        // The same humming cable between a balanced driver and receiver: the identical 60 Hz
        // common-mode tone on both legs cancels at V+ − V− to bit-exact zero.
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
        let drv = g.add(BalancedDriver::new(
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect(src, 0, drv, 0);
        g.connect_cabled(drv, 0, rcv, 0, hum_cable());
        g.set_output(rcv, 0);
        let mut sched = compile(g, LEN, rate(), 9).expect("balanced hum chain");
        let mut out = VoltageBuffer::zeros(LEN, rate());
        sched.process(&mut out);
        assert!(
            out.as_slice().iter().all(|&v| v == 0.0),
            "balanced should reject common-mode hum to bit-exact zero"
        );
    }
}

/// Story 1.5.3 — phantom power: +48 V common-mode DC powering a condenser mic. The mic puts it on
/// the line common-mode (asserted at the node in `node::condenser`); here, end-to-end, a balanced
/// receiver recovers just the audio and rejects the 48 V, and an unpowered mic is silent. Phantom
/// rides the *same* common-mode rejection as pickup and hum — not a special case.
#[cfg(test)]
mod phantom_phenomena {
    use super::*;
    use crate::node::{BalancedReceiver, CondenserMic};
    use crate::signal::Volts;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn receiver_recovers_audio_and_rejects_phantom() {
        // Powered mic: V+ = 48 + 1, V− = 48 − 1 (a 2 V differential signal on the +48 V common-mode
        // pedestal). The balanced receiver returns V+ − V− ≈ 2 V — the audio — and the 48 V, being
        // common-mode, cancels. Phantom and signal share one wire pair, separated by the difference.
        let mut g = Graph::new();
        let mic = g.add(CondenserMic::new(Volts::new(2.0), Ohms::new(150.0)));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect(mic, 0, rcv, 0);
        g.set_output(rcv, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("phantom mic chain");
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
        for &v in out.as_slice() {
            assert_relative_eq!(v, 2.0, epsilon = 1e-3);
        }
    }

    #[test]
    fn unpowered_mic_yields_silence() {
        // No phantom ⇒ the mic produces nothing on either conductor ⇒ the receiver difference is 0.
        let mut g = Graph::new();
        let mic = g.add(CondenserMic::new(Volts::new(2.0), Ohms::new(150.0)).unpowered());
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect(mic, 0, rcv, 0);
        g.set_output(rcv, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("unpowered mic chain");
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
        assert!(out.as_slice().iter().all(|&v| v == 0.0));
    }
}

/// Story 1.6.1 — the **digital carrier seam**: the schedule pool carries `Lane::Sample` lanes
/// sized to `block_len / M`, a digital edge is a same-clock-domain copy, and `compile` rejects
/// cross-domain edges, non-integer rates, indivisible block lengths, and clock crossings. No
/// converter yet (the AD/DA arrive in 1.6.3/1.6.4) — these test nodes are pure digital
/// scaffolding to exercise the plumbing. Tests inspect the private pools (white-box).
#[cfg(test)]
mod digital_seam {
    use super::*;
    use crate::electrical::Ohms;
    use crate::node::TestSource;
    use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
    use crate::signal::{BitDepth, SampleRate, Volts};

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A mono digital format at `rate_hz`, 24-bit.
    fn fmt(rate_hz: f64) -> AudioFormat {
        AudioFormat::new(SampleRate::new(rate_hz), BitDepth::new(24), 1)
    }

    /// A digital source: no inputs, one digital output filled with a constant sample value.
    struct DigitalSource {
        level: f32,
        outputs: [OutputPort; 1],
    }
    impl DigitalSource {
        fn new(level: f32, format: AudioFormat) -> Self {
            Self {
                level,
                outputs: [DigitalFace::new(format).into()],
            }
        }
    }
    impl Node for DigitalSource {
        fn inputs(&self) -> &[InputPort] {
            &[]
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
            outputs[0].sample_mut().fill(self.level);
        }
    }

    /// A digital sink: one digital input, no outputs. A no-op — tests read its input lane.
    struct DigitalSink {
        inputs: [InputPort; 1],
    }
    impl DigitalSink {
        fn new(format: AudioFormat) -> Self {
            Self {
                inputs: [DigitalFace::new(format).into()],
            }
        }
    }
    impl Node for DigitalSink {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &[]
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], _outputs: &mut [Lane]) {}
    }

    #[test]
    fn digital_lanes_are_sized_by_the_decimation_factor() {
        // analog 384 kHz, digital 48 kHz ⇒ M = 8; a block of 16 analog samples ⇒ 2 digital samples.
        let mut g = Graph::new();
        let src = g.add(DigitalSource::new(0.5, fmt(48_000.0)));
        let sink = g.add(DigitalSink::new(fmt(48_000.0)));
        g.connect(src, 0, sink, 0);
        g.set_output(src, 0); // digital tap; this test inspects the pool, never calls process
        let sched = compile(g, 16, analog_rate(), 0).expect("valid digital chain");

        let sample_lanes: Vec<&Lane> = sched
            .output_pool
            .iter()
            .chain(sched.input_pool.iter())
            .filter(|l| matches!(l, Lane::Sample(_)))
            .collect();
        assert_eq!(
            sample_lanes.len(),
            2,
            "one source-output + one sink-input sample lane"
        );
        for lane in sample_lanes {
            assert_eq!(lane.domain(), Domain::DigitalAudio);
            assert_eq!(lane.len(), 2, "digital lane is block_len / M = 16 / 8");
        }
    }

    #[test]
    fn digital_route_copies_samples() {
        // A separate analog node provides the (voltage) output tap so `process` can run; the
        // digital source → sink component runs alongside, and its DigitalRoute copies the samples.
        let mut g = Graph::new();
        let atap = g.add(TestSource::new(Volts::new(1.0), Ohms::new(150.0)));
        g.set_output(atap, 0);
        let src = g.add(DigitalSource::new(0.5, fmt(48_000.0)));
        let sink = g.add(DigitalSink::new(fmt(48_000.0)));
        g.connect(src, 0, sink, 0);

        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid mixed chain");
        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process(&mut out);

        // The analog tap is unaffected by the digital component.
        assert!(out.as_slice().iter().all(|&v| (v - 1.0).abs() < 1e-3));
        // The sink's input sample lane received the source's 0.5 via the DigitalRoute copy.
        let sink_in = sched
            .input_pool
            .iter()
            .find(|l| matches!(l, Lane::Sample(_)))
            .expect("a digital input lane");
        assert!(sink_in.sample().as_slice().iter().all(|&s| s == 0.5));
    }

    #[test]
    fn rejects_domain_mismatch() {
        // An analog output into a digital input: no physics bridges domains on a wire.
        let mut g = Graph::new();
        let asrc = g.add(TestSource::new(Volts::new(1.0), Ohms::new(150.0)));
        let dsink = g.add(DigitalSink::new(fmt(48_000.0)));
        g.connect(asrc, 0, dsink, 0);
        g.set_output(asrc, 0);
        assert_eq!(
            compile(g, 16, analog_rate(), 0).err(),
            Some(CompileError::DomainMismatch {
                from_node: 0,
                from_port: 0,
                to_node: 1,
                to_port: 0,
            })
        );
    }

    #[test]
    fn rejects_non_integer_rate() {
        // 44.1 kHz does not integer-divide 384 kHz (384000 / 44100 = 8.707…).
        let mut g = Graph::new();
        let src = g.add(DigitalSource::new(0.5, fmt(44_100.0)));
        g.set_output(src, 0);
        let err = compile(g, 16, analog_rate(), 0).err().unwrap();
        assert!(
            matches!(err, CompileError::RateIndivisible { node: 0, .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_indivisible_block_len() {
        // 48 kHz ⇒ M = 8; a block of 10 isn't a multiple of 8.
        let mut g = Graph::new();
        let src = g.add(DigitalSource::new(0.5, fmt(48_000.0)));
        g.set_output(src, 0);
        assert_eq!(
            compile(g, 10, analog_rate(), 0).err(),
            Some(CompileError::BlockLenIndivisible {
                node: 0,
                block_len: 10,
                factor: 8,
            })
        );
    }

    #[test]
    fn rejects_clock_crossing() {
        // Both ends digital (domain matches) but at different rates ⇒ a resample, deferred.
        let mut g = Graph::new();
        let src = g.add(DigitalSource::new(0.5, fmt(48_000.0)));
        let sink = g.add(DigitalSink::new(fmt(96_000.0)));
        g.connect(src, 0, sink, 0);
        g.set_output(src, 0);
        assert_eq!(
            compile(g, 16, analog_rate(), 0).err(),
            Some(CompileError::ClockCrossingUnsupported {
                from_node: 0,
                to_node: 1,
            })
        );
    }
}

/// Story 1.6.5 — the converter **artifacts**, on real compiled chains through the carrier seam:
/// calibration (+4 dBu = −18 dBFS), aliasing fold-back from a weak anti-alias filter, the TPDF
/// quantization noise floor (RMS `Δ/2`, SNR ≈ `6.02·N − 3`), and the end-to-end capstone
/// `analog → AD → digital → DA → analog`. "Tests are the oracle" (§3.5): every number is a hand
/// calc, inline. Digital-domain assertions read the AD's output sample lane (white-box, as in
/// [`digital_seam`]); the capstone taps the DA's analog output through `process`.
#[cfg(test)]
mod converter_phenomena {
    use super::*;
    use crate::electrical::{InputZ, Ohms};
    use crate::level::{dbu_to_volts, sample_to_dbfs};
    use crate::node::{AdConverter, BalancedDriver, BalancedReceiver, DaConverter, TestSource};
    use crate::signal::{BitDepth, SampleRate, Volts};
    use crate::test_util::{SineSource, rms, tone_amplitude};
    use approx::assert_relative_eq;

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn digital_rate() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    /// 48 kHz expressed as an `AnalogRate`, so [`tone_amplitude`]'s DFT runs at the digital rate
    /// when it reads the AD's 48 kHz output samples (it only needs the sample period).
    fn digital_as_analog() -> AnalogRate {
        AnalogRate::new(48_000.0)
    }

    /// Drive a configured `ad` from `src` over one block and return its digital output samples.
    /// The AD's output is digital, so it can't be the schedule's voltage tap; a standalone silent
    /// analog source supplies that tap, and the AD samples are read white-box from the pool (the
    /// only `Lane::Sample` there, since the chain has a single converter).
    fn ad_samples(src: SineSource, ad: AdConverter, block_len: usize, seed: u64) -> Vec<f32> {
        let mut g = Graph::new();
        let s = g.add(src);
        let a = g.add(ad);
        g.connect(s, 0, a, 0);
        let tap = g.add(TestSource::new(Volts::new(0.0), Ohms::new(150.0)));
        g.set_output(tap, 0);

        let mut sched = compile(g, block_len, analog_rate(), seed).expect("valid converter chain");
        let mut sink = VoltageBuffer::zeros(block_len, analog_rate());
        sched.process(&mut sink);
        sched
            .output_pool
            .iter()
            .find(|l| matches!(l, Lane::Sample(_)))
            .expect("an AD output sample lane")
            .sample()
            .as_slice()
            .to_vec()
    }

    #[test]
    fn plus_4_dbu_calibrates_to_minus_18_dbfs_through_the_seam() {
        // +4 dBu = 1.2283 V RMS = 1.7372 V peak. Source 1 Ω into the AD's 1 MΩ input ⇒ divider
        // 1e6/(1+1e6) ≈ 0.999999, so the AD sees the full peak. Against a 13.80 V-peak reference:
        //   1.7372 / 13.80 = 0.12589 normalized peak ⇒ 20·log10(0.12589) = −18.0 dBFS.
        let peak = dbu_to_volts(4.0).get() * core::f32::consts::SQRT_2;
        let src = SineSource::new(1_000.0, Volts::new(peak), Volts::new(0.0), Ohms::new(1.0));
        let ad = AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(13.80),
            Ohms::new(1e6),
        );
        // 7680 analog ⇒ 960 digital = 20 whole cycles of 1 kHz at 48 kHz (48 samples/cycle).
        let out = ad_samples(src, ad, 7_680, 1);
        let amp = tone_amplitude(&out[480..], 1_000.0, digital_as_analog());
        assert_relative_eq!(sample_to_dbfs(amp), -18.0, epsilon = 0.1);
    }

    #[test]
    fn a_weak_anti_alias_filter_folds_back_more_than_a_strong_one() {
        // A 40 kHz tone is above the 24 kHz decimated Nyquist; unrejected it folds to 48 − 40 =
        // 8 kHz. A steep filter (the default 161 taps) attenuates it deep into the stopband; a
        // short one (15 taps) can't, so far more leaks back. Measure the 8 kHz alias bin.
        let tone = || SineSource::new(40_000.0, Volts::new(0.5), Volts::new(0.0), Ohms::new(1.0));
        let strong = AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(1.0),
            Ohms::new(1e6),
        );
        let weak = AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(1.0),
            Ohms::new(1e6),
        )
        .with_aa_taps(15);
        // 12288 analog ⇒ 1536 digital = 256 whole cycles of 8 kHz at 48 kHz (6 samples/cycle).
        let s = ad_samples(tone(), strong, 12_288, 1);
        let w = ad_samples(tone(), weak, 12_288, 1);
        let alias_strong = tone_amplitude(&s[200..], 8_000.0, digital_as_analog());
        let alias_weak = tone_amplitude(&w[200..], 8_000.0, digital_as_analog());
        assert!(
            alias_weak > alias_strong * 5.0,
            "a weak (short) AA filter must fold back far more: weak {alias_weak} vs strong \
             {alias_strong}"
        );
    }

    #[test]
    fn the_quantization_noise_floor_is_delta_over_two() {
        // TPDF-dithered quantization of silence: the output is pure dither noise of variance
        //   Δ²/12 (quantization) + Δ²/6 (TPDF, two ±½-LSB draws) = Δ²/4  ⇒  RMS = Δ/2,
        // independent of the signal. For a ±1.0 full scale Δ = 1/2^(N−1), so the floor is 2^−N:
        //   16-bit ⇒ 2^−16 = 1.526e-5;  24-bit ⇒ 2^−24 = 5.96e-8 (256× quieter).
        fn floor(bits: u32, seed: u64) -> f32 {
            let silence =
                SineSource::new(1_000.0, Volts::new(0.0), Volts::new(0.0), Ohms::new(1.0));
            let ad = AdConverter::new(
                digital_rate(),
                BitDepth::new(bits),
                Volts::new(1.0),
                Ohms::new(1e6),
            );
            // 80000 analog ⇒ 10000 digital samples: RMS converges to ~1% (≈ 1/√(2N)).
            rms(&ad_samples(silence, ad, 80_000, seed))
        }
        let floor_16 = floor(16, 1);
        let floor_24 = floor(24, 2);

        // Each floor matches Δ/2 = 2^−N, and more bits buy a much lower floor.
        assert_relative_eq!(floor_16, 2.0_f32.powi(-16), max_relative = 0.05);
        assert_relative_eq!(floor_24, 2.0_f32.powi(-24), max_relative = 0.05);
        assert!(
            floor_16 > floor_24 * 100.0,
            "more bits ⇒ a far lower noise floor: 16-bit {floor_16} vs 24-bit {floor_24}"
        );

        // SNR of a full-scale sine (RMS 1/√2) against the measured 16-bit floor:
        //   20·log10((1/√2) / floor) ≈ 6.02·16 − 3.01 = 93.3 dB — the flat-noise SNR law.
        let snr = 20.0 * ((1.0 / core::f32::consts::SQRT_2) / floor_16).log10();
        assert_relative_eq!(snr, 6.0206 * 16.0 - 3.01, epsilon = 0.5);
    }

    #[test]
    fn capstone_balanced_analog_through_ad_da_back_to_analog() {
        // The whole Story 1.6 chain, balanced-fronted, through the generalized carrier seam:
        //   sine(2 V) → balanced driver → balanced receiver → AD → DA → analog tap.
        // Every analog face is near-ideal (1 Ω out into ≥ 1 MΩ in) so dividers ≈ unity: the
        // receiver returns the 2 V differential single-ended; the AD digitizes it (2 V / 10 V
        // reference = 0.2 full scale); the DA reconstructs it (0.2 × 10 V = 2 V). A 1 kHz tone,
        // deep in the passband, should survive end-to-end at ≈ 2 V — the analog physics of
        // Stories 1.2–1.5 intact across the digital round trip.
        let mut g = Graph::new();
        let src = g.add(SineSource::new(
            1_000.0,
            Volts::new(2.0),
            Volts::new(0.0),
            Ohms::new(1.0),
        ));
        let drv = g.add(BalancedDriver::new(
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(1.0)));
        let ad = g.add(AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(1e6),
        ));
        let da = g.add(DaConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(150.0),
        ));
        g.connect(src, 0, drv, 0);
        g.connect(drv, 0, rcv, 0);
        g.connect(rcv, 0, ad, 0);
        g.connect(ad, 0, da, 0);
        g.set_output(da, 0);

        // 15360 analog ⇒ 1920 digital = 40 whole cycles of 1 kHz; drop the first half as the
        // combined AA + reconstruction filter transient (their group delays add).
        let block = 15_360;
        let mut sched = compile(g, block, analog_rate(), 0).expect("valid capstone chain");
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process(&mut out);
        let amp = tone_amplitude(&out.as_slice()[block / 2..], 1_000.0, analog_rate());
        assert_relative_eq!(amp, 2.0, max_relative = 0.02);
    }
}

/// Story 1.7.1 — the **events carrier seam** (the third carrier): the schedule pool carries
/// `Lane::Events` lanes pre-allocated to a per-port capacity, an event edge is a sparse
/// `EventRoute` copy, and `compile` rejects an event↔non-event edge as a `DomainMismatch`. No
/// queue and no voice yet (those are Tasks 1.7.2/1.7.4) — these test nodes are pure scaffolding to
/// exercise the plumbing, mirroring [`digital_seam`]. Tests inspect the private pools (white-box).
#[cfg(test)]
mod event_seam {
    use super::*;
    use crate::electrical::Ohms;
    use crate::node::{GainStage, TestSource};
    use crate::port::{EventFace, InputPort, OutputPort};
    use crate::signal::{EventMessage, TimedEvent, Volts};

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A note-on at offset 0 — the message the scaffolding source emits each block.
    fn note_on() -> TimedEvent {
        TimedEvent {
            offset: 0,
            message: EventMessage::NoteOn {
                note: 69, // A4
                velocity: 100,
            },
        }
    }

    /// An event source: no inputs, one event output it (re)fills with a single note-on each block.
    struct EventSource {
        outputs: [OutputPort; 1],
    }
    impl EventSource {
        fn new(capacity: usize) -> Self {
            Self {
                outputs: [EventFace::new(capacity).into()],
            }
        }
    }
    impl Node for EventSource {
        fn inputs(&self) -> &[InputPort] {
            &[]
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
            let ev = outputs[0].events_mut();
            ev.clear(); // a producer owns its lane each block — clear stale events, then emit.
            ev.push(note_on());
        }
    }

    /// An event sink: one event input, no outputs. A no-op — tests read its input lane.
    struct EventSink {
        inputs: [InputPort; 1],
    }
    impl EventSink {
        fn new(capacity: usize) -> Self {
            Self {
                inputs: [EventFace::new(capacity).into()],
            }
        }
    }
    impl Node for EventSink {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &[]
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], _outputs: &mut [Lane]) {}
    }

    /// A silent analog source supplies the (voltage) output tap so `process` can run, since the
    /// tap must be a voltage lane; the event component runs alongside it.
    fn analog_tap(g: &mut Graph) {
        let tap = g.add(TestSource::new(Volts::new(0.0), Ohms::new(150.0)));
        g.set_output(tap, 0);
    }

    #[test]
    fn event_lanes_are_sized_to_their_capacity() {
        // The pool holds one source-output and one sink-input event lane, each pre-allocated to its
        // port's capacity (the bound the hot path never grows past), and both start empty.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(32));
        let sink = g.add(EventSink::new(16));
        g.connect(src, 0, sink, 0);
        analog_tap(&mut g);
        let sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");

        let event_lanes: Vec<&Lane> = sched
            .output_pool
            .iter()
            .chain(sched.input_pool.iter())
            .filter(|l| matches!(l, Lane::Events(_)))
            .collect();
        assert_eq!(
            event_lanes.len(),
            2,
            "one source-output + one sink-input event lane"
        );
        let caps: Vec<usize> = event_lanes
            .iter()
            .map(|l| {
                assert_eq!(l.domain(), Domain::Events);
                assert!(l.is_empty(), "event lanes start empty");
                l.events().capacity()
            })
            .collect();
        assert!(caps.contains(&32) && caps.contains(&16), "got {caps:?}");
    }

    #[test]
    fn event_route_copies_events() {
        // Source emits a note-on; the EventRoute copies it into the sink's input lane.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(32));
        let sink = g.add(EventSink::new(32));
        g.connect(src, 0, sink, 0);
        analog_tap(&mut g);

        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process(&mut out);

        let sink_in = sched
            .input_pool
            .iter()
            .find(|l| matches!(l, Lane::Events(_)))
            .expect("a sink input event lane");
        assert_eq!(sink_in.events().as_slice(), &[note_on()]);

        // Running again must not accumulate — the source clears and the route overwrites.
        sched.process(&mut out);
        let sink_in = sched
            .input_pool
            .iter()
            .find(|l| matches!(l, Lane::Events(_)))
            .expect("a sink input event lane");
        assert_eq!(
            sink_in.events().len(),
            1,
            "events must not accumulate across blocks"
        );
    }

    #[test]
    fn event_output_fans_out_to_several_sinks() {
        // One event source into two sinks: each edge is its own EventRoute, so both receive it.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(8));
        let a = g.add(EventSink::new(8));
        let b = g.add(EventSink::new(8));
        g.connect(src, 0, a, 0);
        g.connect(src, 0, b, 0);
        analog_tap(&mut g);

        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event fan-out");
        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process(&mut out);

        let received: Vec<&Lane> = sched
            .input_pool
            .iter()
            .filter(|l| matches!(l, Lane::Events(_)))
            .collect();
        assert_eq!(received.len(), 2);
        for lane in received {
            assert_eq!(lane.events().as_slice(), &[note_on()]);
        }
    }

    #[test]
    fn rejects_event_to_analog_domain_mismatch() {
        // An event output into an analog input: no carrier bridges domains on a wire.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(8));
        let amp = g.add(GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        ));
        g.connect(src, 0, amp, 0);
        g.set_output(amp, 0);
        assert_eq!(
            compile(g, 16, analog_rate(), 0).err(),
            Some(CompileError::DomainMismatch {
                from_node: 0,
                from_port: 0,
                to_node: 1,
                to_port: 0,
            })
        );
    }

    // --- Task 1.7.2: external event queue + timestamped delivery into open event inputs. ---

    fn note_on_msg(note: u8) -> EventMessage {
        EventMessage::NoteOn {
            note,
            velocity: 100,
        }
    }
    fn note_off_msg(note: u8) -> EventMessage {
        EventMessage::NoteOff { note }
    }

    /// The single event input lane of these one-sink chains (the only `Events` lane in the input
    /// pool). White-box, as elsewhere in this file.
    fn sink_events(sched: &Schedule) -> &EventBuffer {
        sched
            .input_pool
            .iter()
            .find(|l| matches!(l, Lane::Events(_)))
            .expect("an event input lane")
            .events()
    }

    #[test]
    fn external_events_land_at_their_offsets() {
        // Two events due this block land at the matching block-relative offsets, in order.
        let mut g = Graph::new();
        let sink = g.add(EventSink::new(8));
        analog_tap(&mut g);
        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let id = sched.event_input(sink, 0).expect("open event input");

        let mut q = EventQueue::with_capacity(8);
        q.push(3, id, note_on_msg(69));
        q.push(10, id, note_off_msg(69));

        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process_with_events(&mut out, &mut q);

        assert_eq!(
            sink_events(&sched).as_slice(),
            &[
                TimedEvent {
                    offset: 3,
                    message: note_on_msg(69)
                },
                TimedEvent {
                    offset: 10,
                    message: note_off_msg(69)
                },
            ]
        );
        assert!(q.is_empty(), "both events were due and consumed");
    }

    #[test]
    fn events_bucket_across_blocks() {
        // An event past this block stays queued, then arrives next block at its rebased offset:
        // absolute 20 with block_len 16 ⇒ block 1, offset 20 − 16 = 4.
        let mut g = Graph::new();
        let sink = g.add(EventSink::new(8));
        analog_tap(&mut g);
        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let id = sched.event_input(sink, 0).expect("open event input");

        let mut q = EventQueue::with_capacity(8);
        q.push(3, id, note_on_msg(60));
        q.push(20, id, note_on_msg(62));

        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process_with_events(&mut out, &mut q);
        assert_eq!(
            sink_events(&sched).as_slice(),
            &[TimedEvent {
                offset: 3,
                message: note_on_msg(60)
            }]
        );
        assert_eq!(q.len(), 1, "the second event is not yet due");

        sched.process_with_events(&mut out, &mut q);
        assert_eq!(
            sink_events(&sched).as_slice(),
            &[TimedEvent {
                offset: 4,
                message: note_on_msg(62)
            }]
        );
        assert!(q.is_empty());
    }

    #[test]
    fn a_late_event_clamps_to_offset_zero() {
        // After one block the clock is at sample 16; an event stamped before that (a late arrival)
        // fires immediately, at offset 0, rather than being dropped or panicking.
        let mut g = Graph::new();
        let sink = g.add(EventSink::new(8));
        analog_tap(&mut g);
        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let id = sched.event_input(sink, 0).expect("open event input");

        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process(&mut out); // advance the clock to sample 16

        let mut q = EventQueue::with_capacity(4);
        q.push(5, id, note_on_msg(60)); // 5 < 16 — late
        sched.process_with_events(&mut out, &mut q);
        assert_eq!(
            sink_events(&sched).as_slice(),
            &[TimedEvent {
                offset: 0,
                message: note_on_msg(60)
            }]
        );
    }

    #[test]
    fn open_event_inputs_are_cleared_each_block() {
        // Events delivered one block don't linger into the next: the open input is cleared, so a
        // following block with no events sees silence.
        let mut g = Graph::new();
        let sink = g.add(EventSink::new(8));
        analog_tap(&mut g);
        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let id = sched.event_input(sink, 0).expect("open event input");

        let mut q = EventQueue::with_capacity(4);
        q.push(2, id, note_on_msg(60));
        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process_with_events(&mut out, &mut q);
        assert_eq!(sink_events(&sched).len(), 1);

        sched.process(&mut out); // no events this block
        assert!(
            sink_events(&sched).is_empty(),
            "an open event input is cleared each block"
        );
    }

    #[test]
    fn event_input_resolves_only_open_event_ports() {
        // Open event input → a handle; edge-fed event input → None (filled by the graph, not the
        // host); a non-event / nonexistent port → None.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(8));
        let fed = g.add(EventSink::new(8)); // node 1: event input fed by an edge
        let open = g.add(EventSink::new(8)); // node 2: event input left open
        g.connect(src, 0, fed, 0);
        analog_tap(&mut g); // node 3: the voltage tap
        let sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");

        assert!(
            sched.event_input(open, 0).is_some(),
            "an unwired event input is open"
        );
        assert!(
            sched.event_input(fed, 0).is_none(),
            "an edge-fed event input is not host-feedable"
        );
        assert!(
            sched.event_input(src, 0).is_none(),
            "the source has no input ports"
        );
        assert!(
            sched.event_input(NodeId(3), 0).is_none(),
            "the tap's port 0 is an analog output, not an event input"
        );
    }
}

/// Story 1.7.3 — control params & de-zippering: a swept knob reaches the engine as a **smoothed**
/// value (a within-block linear ramp), so it never clicks. The headline lesson is the contrast
/// with a raw jump; here we sweep [`GainStage::GAIN`] and assert the output glides continuously to
/// the new gain rather than snapping. White-box where convenient, as elsewhere in this file.
#[cfg(test)]
mod param_phenomena {
    use super::*;
    use crate::electrical::Ohms;
    use crate::node::{GainStage, TestSource};
    use crate::param::ParamQueue;
    use crate::signal::Volts;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn a_swept_gain_param_de_zippers_without_discontinuity() {
        // 1 V DC → GainStage(gain 1.0) → tap. Near-ideal faces (1 Ω out into 1 GΩ in, bridging
        // tap) make the output ≈ gain·1 V. Sweep the gain param 1 → 5: a de-zippered value ramps
        // there smoothly; a raw write would jump +4 V in a single sample. We assert no
        // sample-to-sample step exceeds a tiny bound, the ramp is monotonic, and it lands at 5 V.
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(1.0)));
        let amp = g.add(GainStage::new(
            1.0,
            Volts::new(100.0),
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        g.connect(src, 0, amp, 0);
        g.set_output(amp, 0);

        let block = 64;
        let mut sched = compile(g, block, rate(), 0).expect("valid param chain");
        let gain = sched.param(amp, GainStage::GAIN).expect("gain param");

        // Settled at the default gain 1.0 → output ≈ 1 V.
        let mut out = VoltageBuffer::zeros(block, rate());
        sched.process(&mut out);
        assert_relative_eq!(out.get(0).get(), 1.0, max_relative = 1e-3);

        // Aim at 5.0 and collect the whole glide. Smooth time 5 ms @ 384 kHz = 1920 samples = 30
        // blocks of 64; 40 blocks over-covers, so it reaches and holds 5 V.
        let mut q = ParamQueue::with_capacity(1);
        q.set(gain, 5.0);
        let mut swept = Vec::new();
        for b in 0..40 {
            if b == 0 {
                sched.process_with_params(&mut out, &mut q);
            } else {
                sched.process(&mut out);
            }
            swept.extend_from_slice(out.as_slice());
        }

        // No discontinuity: a de-zippered sweep moves at the ramp step (≈ (5−1)/1920 ≈ 0.0021
        // V/sample); a raw jump would show a ~4 V step. 0.005 cleanly separates the two.
        let max_step = swept
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            max_step < 0.005,
            "the sweep must not jump (max sample step {max_step} V)"
        );

        // Monotonic upward (no overshoot/ringing) and settled at the new gain.
        assert!(
            swept.windows(2).all(|w| w[1] - w[0] >= -1e-6),
            "a 1→5 glide should be non-decreasing"
        );
        assert_relative_eq!(*swept.last().unwrap(), 5.0, max_relative = 1e-3);
        // And it genuinely moved off the start (not stuck at 1 V).
        assert!(swept.iter().any(|&v| v > 2.0));
    }
}

/// Story 1.7.5 — the Epic-1 exit: a **played note travels the full chain**
/// `analog → AD → digital → DA → analog`, the engine's first end-to-end "play an instrument"
/// milestone. The voice is driven by the event lane (note-on at a chosen sample) and a smoothed
/// control param (level); the converters from Story 1.6 carry it across the digital domain and
/// back. (The swept-param de-zipper gate is also proven in [`param_phenomena`] on a clean DC
/// signal; here we show it survives end-to-end on the voice.) "Tests are the oracle" (§3.5) — the
/// fundamental level is a hand calc, inline.
#[cfg(test)]
mod playable_voice {
    use super::*;
    use crate::electrical::Ohms;
    use crate::node::{AdConverter, DaConverter, SynthVoice};
    use crate::param::ParamQueue;
    use crate::signal::{BitDepth, EventMessage, SampleRate, Volts};
    use crate::test_util::{rms, tone_amplitude};
    use approx::assert_relative_eq;

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn digital_rate() -> SampleRate {
        SampleRate::new(48_000.0)
    }

    /// `voice → AD → DA → analog tap`, all near-ideal analog faces. Returns the schedule and the
    /// voice's event-input handle.
    fn voice_through_converters(block: usize) -> (Schedule, EventInputId) {
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        let ad = g.add(AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(1e6),
        ));
        let da = g.add(DaConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(150.0),
        ));
        g.connect(voice, 0, ad, 0);
        g.connect(ad, 0, da, 0);
        g.set_output(da, 0);
        let sched = compile(g, block, analog_rate(), 0).expect("valid playable chain");
        let ev = sched.event_input(voice, 0).expect("voice event input");
        (sched, ev)
    }

    #[test]
    fn a_played_note_travels_analog_ad_digital_da_analog() {
        // Play A4 (note 69 = 440 Hz) and recover it after the round trip. The voice's default
        // sustain 0.7 and level 1.0 V make the analog sawtooth's fundamental
        //   (2/π)·sustain·level = 0.63662·0.7·1.0 = 0.4456 V.
        // 440 Hz sits deep in the 24 kHz passband, so the AD/DA pass it at unity ⇒ the output
        // fundamental is ≈ that, and the AD's anti-alias filter has quietly removed the saw's
        // ultrasonic harmonics that would otherwise fold (the oversampled-oscillator payoff).
        let block = 15_360; // 1920 digital samples — many 440 Hz cycles
        let (mut sched, ev) = voice_through_converters(block);
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_events(&mut out, &mut q);

        // Read a steady window of ~whole cycles from after the attack + combined converter group
        // delay, so the single-bin DFT lands cleanly on the 440 Hz bin (low leakage).
        let spc = analog_rate().as_hz() / 440.0; // samples per cycle
        let window = (spc * 8.0) as usize; // 8 whole cycles
        let tail = &out.as_slice()[block / 2..block / 2 + window];

        let fundamental = tone_amplitude(tail, 440.0, analog_rate());
        let expected = core::f32::consts::FRAC_2_PI * 0.7 * 1.0;
        assert_relative_eq!(fundamental, expected, max_relative = 0.05);
        // A real pitched note: the fundamental dominates a detuned bin by a wide margin.
        let detuned = tone_amplitude(tail, 550.0, analog_rate());
        assert!(
            fundamental > detuned * 5.0,
            "the note should be a clean 440 Hz tone, not noise ({fundamental} vs {detuned})"
        );
    }

    #[test]
    fn the_chain_is_silent_before_the_note() {
        // Causality across the converters: a note triggered late produces nothing earlier. Filter
        // latency can only *delay* energy, never advance it — and with no input the only thing the
        // AD emits is its sub-µV dither floor, far below any signal.
        let block = 8_192;
        let (mut sched, ev) = voice_through_converters(block);
        let mut q = EventQueue::with_capacity(4);
        let trigger = block as u64 * 3 / 4;
        q.push(
            trigger,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_events(&mut out, &mut q);

        // The first quarter is well before the trigger (and its latency): silent to the dither floor.
        let head = &out.as_slice()[..block / 4];
        assert!(
            rms(head) < 1e-3,
            "nothing should sound before the note is played, rms {}",
            rms(head)
        );
    }

    #[test]
    fn a_swept_level_de_zippers_on_the_played_voice() {
        // The control-param de-zipper, end-to-end on the voice: hold a high note (so many periods
        // resolve the ramp), then sweep LEVEL 1 → 4 V. The output's windowed RMS must climb
        // *smoothly* to ≈4× — a raw write would jump the whole change in a single window.
        let block = 8_192;
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        g.set_output(voice, 0);
        let mut sched = compile(g, block, analog_rate(), 0).expect("valid voice chain");
        let ev = sched.event_input(voice, 0).expect("voice event input");
        let level = sched.param(voice, SynthVoice::LEVEL).expect("level param");

        // Block 1: establish a sustained C7 (note 96 ≈ 2093 Hz) at the default 1 V.
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 96,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_events(&mut out, &mut q);

        // Block 2: aim LEVEL at 4 V and capture the glide (it ramps over the 5 ms smooth time).
        let mut pq = ParamQueue::with_capacity(1);
        pq.set(level, 4.0);
        sched.process_with_params(&mut out, &mut pq);

        // Window RMS over ~2 periods (note 96 period ≈ 183 samples). A smooth ramp spreads the
        // rise across many windows; assert it's non-decreasing, lands at ≈4× the start, and no
        // single window jumps by more than a fraction of the total change (rules out a step).
        let win = 366;
        let rms_windows: Vec<f32> = out.as_slice().chunks(win).map(rms).collect();
        let first = rms_windows[0];
        let last = *rms_windows.last().unwrap();
        assert_relative_eq!(last / first, 4.0, max_relative = 0.15);
        assert!(
            rms_windows.windows(2).all(|w| w[1] >= w[0] - 1e-4),
            "the level glide should be monotonic"
        );
        let total = last - first;
        let max_step = rms_windows
            .windows(2)
            .map(|w| w[1] - w[0])
            .fold(0.0_f32, f32::max);
        assert!(
            max_step < total * 0.5,
            "no single window may jump the whole change (max step {max_step} of {total})"
        );
    }
}

/// Story 3.4.1 — the real-time **hot-path robustness audit**, pinned as standing guards. The audit
/// found the `process` path panic-free and denormal-flushed; these tests keep it that way, because a
/// regression here surfaces on the audio thread — where a panic kills the stream and a denormal
/// storm blows the per-quantum CPU budget — not somewhere a unit test would otherwise catch it.
///
/// Two properties:
/// - **Totality over the cross-thread seam.** Param/event handles arrive from the external queues; a
///   stale or foreign one is skipped (`process_io` indexes them with `.get`), never a panic.
/// - **Exact silence / finiteness.** The voice reaches *exact* zero at idle and after release (the
///   linear ADSR hits 0, so `saw·0·level` is identically 0 — no denormal tail); the full converter
///   chain stays finite under sustained drive and quiet at idle (only the AD's dither floor).
#[cfg(test)]
mod hot_path_robustness {
    use super::*;
    use crate::electrical::Ohms;
    use crate::node::{AdConverter, DaConverter, SynthVoice};
    use crate::param::{ParamHandle, ParamQueue};
    use crate::signal::{BitDepth, EventMessage, SampleRate, Volts};
    use crate::test_util::rms;

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn digital_rate() -> SampleRate {
        SampleRate::new(48_000.0)
    }

    /// A bare voice → analog tap (no converters), with its event-input and level handles.
    fn voice_only(block: usize) -> (Schedule, EventInputId, ParamHandle) {
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        g.set_output(voice, 0);
        let sched = compile(g, block, analog_rate(), 0).expect("valid voice chain");
        let ev = sched.event_input(voice, 0).expect("voice event input");
        let lvl = sched.param(voice, SynthVoice::LEVEL).expect("level param");
        (sched, ev, lvl)
    }

    /// The full live patch: voice → AD → DA → analog tap, near-ideal faces.
    fn voice_through_converters(block: usize) -> (Schedule, EventInputId) {
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        let ad = g.add(AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(1e6),
        ));
        let da = g.add(DaConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(150.0),
        ));
        g.connect(voice, 0, ad, 0);
        g.connect(ad, 0, da, 0);
        g.set_output(da, 0);
        let sched = compile(g, block, analog_rate(), 0).expect("valid playable chain");
        let ev = sched.event_input(voice, 0).expect("voice event input");
        (sched, ev)
    }

    #[test]
    fn idle_voice_is_exactly_silent_over_many_blocks() {
        // No events ⇒ the envelope never leaves Idle ⇒ env == 0 ⇒ saw·0·level == 0, identically.
        // A denormal creep (an un-flushed asymptotic state) would show as a tiny non-zero tail; the
        // output must stay *exactly* 0.0 (and finite) over a long run.
        let block = 1024;
        let (mut sched, _ev, _lvl) = voice_only(block);
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        for _ in 0..200 {
            sched.process(&mut out);
            assert!(
                out.as_slice().iter().all(|&v| v == 0.0),
                "idle voice must be identically zero — any denormal creep is a bug"
            );
        }
    }

    #[test]
    fn a_released_note_decays_to_exact_zero() {
        // Note-on then note-off; after the (10 ms) release the envelope reaches exactly 0 — a linear
        // ramp clamped to 0, then Idle — so the tail is identically silent, no denormal residue.
        let block = 8_192;
        let (mut sched, ev, _lvl) = voice_only(block);
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        q.push(64, ev, EventMessage::NoteOff { note: 69 }); // 10 ms release ≪ the rest of the block
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_events(&mut out, &mut q);
        // The final stretch is long past the release: identically zero.
        let tail = &out.as_slice()[block - 2048..];
        assert!(
            tail.iter().all(|&v| v == 0.0),
            "the release must reach exact silence"
        );
        // A further idle block stays silent — state truly settled, not drifting.
        sched.process(&mut out);
        assert!(out.as_slice().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn a_sustained_note_through_converters_stays_finite() {
        // Hold a note across many blocks through the AD/DA FIR + edge IIR chain; every output sample
        // must stay finite (no NaN/inf from a runaway filter state) for the whole sustained run.
        let block = 1024;
        let (mut sched, ev) = voice_through_converters(block);
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        for _ in 0..400 {
            sched.process_with_events(&mut out, &mut q);
            assert!(
                out.as_slice().iter().all(|&v| v.is_finite()),
                "sustained output must stay finite"
            );
        }
    }

    #[test]
    fn idle_chain_through_converters_is_finite_and_quiet() {
        // At idle the chain carries only the AD's sub-µV dither floor: finite, and far below any
        // signal — proof there's no denormal / IIR blow-up when the input is silent.
        let block = 1024;
        let (mut sched, _ev) = voice_through_converters(block);
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        for _ in 0..200 {
            sched.process(&mut out);
            assert!(out.as_slice().iter().all(|&v| v.is_finite()));
        }
        assert!(
            rms(out.as_slice()) < 1e-3,
            "idle chain should be near-silent (dither only)"
        );
    }

    #[test]
    fn a_foreign_param_handle_is_skipped_not_panicked() {
        // A handle from another schedule (or a stale one) indexes past this schedule's smoother
        // store. `process_io` must skip it rather than panic — a panic would kill the audio stream.
        // The valid handle pushed alongside still applies.
        let block = 256;
        let (mut sched, _ev, lvl) = voice_only(block);
        let mut pq = ParamQueue::with_capacity(4);
        pq.set(ParamHandle(usize::MAX), 0.5); // bogus: way past the store
        pq.set(lvl, 2.0); // valid
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_params(&mut out, &mut pq); // must not panic
    }

    #[test]
    fn a_foreign_event_id_is_skipped_not_panicked() {
        // Same totality contract for the event lane: an out-of-range target id is skipped, never a
        // panic (and never the `events_mut` `unreachable!`); the valid note still sounds.
        let block = 1024;
        let (mut sched, ev, _lvl) = voice_only(block);
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            EventInputId(usize::MAX), // bogus target
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        q.push(
            0,
            ev, // valid target
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_events(&mut out, &mut q); // must not panic
        assert!(
            out.as_slice().iter().any(|&v| v != 0.0),
            "the valid note should still sound"
        );
    }
}
