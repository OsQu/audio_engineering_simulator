//! Scalar readouts: the node→host measurement lane.
//!
//! Readout control lane that communicates node values to host. Control [`params`](crate::param) flow
//! host→node (knobs); [`events`](crate::EventQueue) are a routed carrier between devices; **readouts
//! flow node→host** — a meter or probe node computes a scalar each block (a VU reading, a dBFS
//! level) and the host reads it back to drive a meter.
//!
//! **The node computes; the schedule snapshots.** A node *declares* its readouts as
//! [`ReadoutDecl`]s ([`Node::readouts`](crate::Node::readouts)) and writes their current values in
//! [`read_readouts`](crate::Node::read_readouts); `compile` reserves one slot per declared readout
//! in a schedule-owned store, and each block — once, after every node has `process`ed — the schedule
//! pulls every node's readings into it. The host resolves a [`ReadoutHandle`] via
//! [`Schedule::readout`](crate::Schedule::readout) and reads the latest value with
//! [`Schedule::readout_value`](crate::Schedule::readout_value). It is the mirror image of the param
//! lane: where params are smoothed, dense *inputs*, a readout is a plain per-block scalar *output* —
//! no range, no de-zipper (a measurement isn't clamped or ramped).
//!
//! Like the param/event queues, the store is single-consumer (audio thread writes, host thread
//! reads); the read is a plain memory poll after the block, so no lock-free transport is needed
//! while the engine runs single-threaded inside the AudioWorklet.

/// A node-local readout identifier: its index in the node's
/// [`readouts()`](crate::Node::readouts) declaration list. A node names its readouts with `const`s
/// (e.g. `VuMeter::VU`); the host addresses one with `(NodeId, ReadoutId)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadoutId(pub u32);

/// A scalar a node exposes for the host to read back — a meter reading, a level. Its `id` is the
/// node-local index; the value itself lives in the schedule's readout store, refreshed each block
/// from [`Node::read_readouts`](crate::Node::read_readouts).
///
/// Unlike a [`ParamDecl`](crate::ParamDecl) it carries no range or smoothing: a readout is an output
/// measurement, not a controllable input. It is a struct (rather than `readouts()` returning a bare
/// count) to mirror the param decl and leave room for future fields (a nominal reference for UI
/// scaling, say) without a signature change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadoutDecl {
    /// The node-local id this declaration defines (its position in the node's `readouts()` list).
    pub id: ReadoutId,
}

/// An opaque handle to one readout of one node in a compiled [`Schedule`](crate::Schedule), from
/// [`Schedule::readout`](crate::Schedule::readout). Indexes that schedule's readout store; means
/// nothing to another schedule (the companion to [`ParamHandle`](crate::ParamHandle)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadoutHandle(pub(crate) usize);
