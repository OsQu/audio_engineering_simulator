//! Control parameters: the smoothed-continuous input lane (knobs, faders).
//!
//! The companion to the event lane. Where events are **sparse and sample-accurate** (a note lands
//! at one exact sample), control params are **dense and smoothed**: a knob has a value at every
//! instant, and moving it must not click. The two are deliberately separate mechanisms — events
//! are a routed [`Lane`](crate::Lane) carrier; params are a host→node side-channel, never wired
//! between devices.
//!
//! **The framework smooths; the node just reads.** A node *declares* its parameters as
//! [`ParamDecl`]s ([`Node::params`](crate::Node::params)); `compile` builds one [`Smoother`] per
//! declared param, owned by the schedule. The host pushes new target values onto a [`ParamQueue`]
//! (latest-wins); each block the schedule applies them and hands every node a [`Params`] view of
//! its current, **already-smoothed** values. De-zippering — a within-block linear ramp toward the
//! target — lives here, once, so no node reimplements it (which would make smoothing a per-node
//! detail, the very thing we avoid).
//!
//! Like the event queue and the schedule swap, the [`ParamQueue`] is shaped for single-producer /
//! single-consumer hand-off (host thread → audio thread); a fully lock-free shared-memory transport
//! is not yet built.

/// A node-local parameter identifier: its index in the node's
/// [`params()`](crate::Node::params) declaration list. A node names its params with `const`s
/// (e.g. `GainStage::GAIN`); the host addresses one with `(NodeId, ParamId)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamId(pub u32);

/// A parameter a node exposes for smoothed external control: its id, initial value, valid range,
/// and de-zipper time. The `default` is the value the node was constructed with (so an
/// uncontrolled param simply holds the construction value); `min`/`max` clamp incoming targets;
/// `smooth_ms` is the linear-ramp time a change glides over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamDecl {
    /// The node-local id this declaration defines.
    pub id: ParamId,
    /// Initial (and uncontrolled) value — the node's construction-time setting.
    pub default: f32,
    /// Lower bound; incoming targets clamp to it.
    pub min: f32,
    /// Upper bound; incoming targets clamp to it.
    pub max: f32,
    /// De-zipper glide time in milliseconds — how long a change ramps to its new target.
    pub smooth_ms: f32,
}

/// An opaque handle to one parameter of one node in a compiled [`Schedule`](crate::Schedule),
/// from [`Schedule::param`](crate::Schedule::param). Indexes that schedule's smoother store; means
/// nothing to another schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamHandle(pub(crate) usize);

/// One parameter's de-zipper state: a linear glide from its current value toward a target.
///
/// On a new target it sets a per-sample `step` that reaches the target in `smooth_samples`
/// samples; [`value_at`](Self::value_at) reads the ramped value within a block and
/// [`advance`](Self::advance) moves the block-start value forward one block. `current`/`step` are
/// `f64` so many small steps accumulate without drift (the accumulator-precision rule).
pub(crate) struct Smoother {
    current: f64,
    target: f32,
    step: f64,
    min: f32,
    max: f32,
    smooth_samples: f64,
}

impl Smoother {
    /// A smoother sitting at `default`, gliding over `smooth_samples` (clamped to ≥ 1) on a change.
    pub(crate) fn new(default: f32, min: f32, max: f32, smooth_samples: f64) -> Self {
        Self {
            current: f64::from(default),
            target: default,
            step: 0.0,
            min,
            max,
            smooth_samples: smooth_samples.max(1.0),
        }
    }

    /// Aim at a new target (clamped to `[min, max]`), recomputing the glide from where the value
    /// is now — so a fresh target mid-glide simply re-aims (latest-wins). Off the hot path.
    pub(crate) fn set_target(&mut self, value: f32) {
        let value = value.clamp(self.min, self.max);
        if value != self.target {
            self.target = value;
            self.step = (f64::from(value) - self.current) / self.smooth_samples;
        }
    }

    /// The ramped value at sample offset `i` within the current block — `current + step·i`, never
    /// past the target. Hot path: a mul-add and a clamp branch (a no-op once settled, `step == 0`).
    #[inline]
    pub(crate) fn value_at(&self, i: usize) -> f32 {
        clamp_toward(
            self.current + self.step * i as f64,
            f64::from(self.target),
            self.step,
        ) as f32
    }

    /// Advance the block-start value by one block of `block_len` samples; once it reaches the
    /// target the glide stops (`step = 0`). Called once per block, off the per-sample loop.
    pub(crate) fn advance(&mut self, block_len: usize) {
        self.current = clamp_toward(
            self.current + self.step * block_len as f64,
            f64::from(self.target),
            self.step,
        );
        if self.current == f64::from(self.target) {
            self.step = 0.0;
        }
    }
}

/// Clamp `v` so a positive `step` never overshoots above `target` (and a negative one never below).
#[inline]
fn clamp_toward(v: f64, target: f64, step: f64) -> f64 {
    if step > 0.0 {
        v.min(target)
    } else if step < 0.0 {
        v.max(target)
    } else {
        v
    }
}

/// A node's smoothed parameter values for the current block — the read view handed to
/// [`Node::process`](crate::Node::process).
///
/// A node reads a declared param with [`value_at_or`](Self::value_at_or) (within-block ramped) or
/// [`value_or`](Self::value_or) (block-start). The `_or` fallback is returned when the param isn't
/// present — i.e. when the node runs outside a schedule (a direct unit test passes
/// [`Params::EMPTY`]); in a compiled schedule every declared param is present and the fallback is
/// never used, so a node passes its own construction-time value as the natural default.
pub struct Params<'a> {
    smoothers: &'a [Smoother],
}

impl<'a> Params<'a> {
    /// A view over one node's smoothers — built by the schedule for each
    /// [`Node::process`](crate::Node::process) call.
    pub(crate) fn new(smoothers: &'a [Smoother]) -> Self {
        Self { smoothers }
    }
}

impl Params<'_> {
    /// An empty view — no params present. Every [`value_at_or`](Self::value_at_or) returns its
    /// fallback. Used by nodes run outside a schedule (direct unit tests) and by `process`'s
    /// no-input path.
    pub const EMPTY: Params<'static> = Params { smoothers: &[] };

    /// The within-block ramped value of param `id` at sample offset `i`, or `default` if the node
    /// has no such param (it isn't running under a schedule). Hot path.
    #[inline]
    pub fn value_at_or(&self, id: ParamId, i: usize, default: f32) -> f32 {
        match self.smoothers.get(id.0 as usize) {
            Some(s) => s.value_at(i),
            None => default,
        }
    }

    /// The block-start value of param `id`, or `default` if absent — the convenience for a param
    /// read once per block rather than per sample.
    #[inline]
    pub fn value_or(&self, id: ParamId, default: f32) -> f32 {
        self.value_at_or(id, 0, default)
    }
}

/// A bounded, **latest-wins** set of pending parameter changes from the host.
///
/// [`set`](Self::set) records a param's newest target, overwriting any earlier pending value for
/// the same handle — a knob only cares about where it is *now*, so a flurry of moves coalesces to
/// the last. The schedule drains it each block. Capacity bounds the number of *distinct* params
/// queued; on overflow a brand-new param's update is dropped rather than reallocating.
pub struct ParamQueue {
    updates: Vec<(ParamHandle, f32)>,
    cap: usize,
}

impl ParamQueue {
    /// A queue holding pending updates for up to `cap` distinct params, pre-allocated so
    /// [`set`](Self::set) never reallocates. `cap == 0` is the always-empty queue `process` uses.
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            updates: Vec::with_capacity(cap),
            cap,
        }
    }

    /// Set `target` (the value it'll glide to) for parameter `handle`, latest-wins. Returns `true`
    /// if recorded; `false` only if this is a new handle and the queue is full (update dropped).
    pub fn set(&mut self, handle: ParamHandle, target: f32) -> bool {
        if let Some(slot) = self.updates.iter_mut().find(|(h, _)| *h == handle) {
            slot.1 = target;
            true
        } else if self.updates.len() < self.cap {
            self.updates.push((handle, target));
            true
        } else {
            false
        }
    }

    /// Drop all pending updates. Used when swapping schedules: a [`ParamHandle`] indexes the *old*
    /// schedule's smoother store, so a stale update must not apply to the new one (where the same index
    /// could be a different param). The companion to [`EventQueue::clear`](crate::EventQueue::clear).
    pub fn clear(&mut self) {
        self.updates.clear();
    }

    /// Number of distinct params with a pending update.
    pub fn len(&self) -> usize {
        self.updates.len()
    }

    /// Whether nothing is pending.
    pub fn is_empty(&self) -> bool {
        self.updates.is_empty()
    }

    /// The maximum number of distinct params the queue holds.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Remove and yield all pending `(handle, target)` updates. Alloc-free (drains in place).
    pub(crate) fn drain(&mut self) -> std::vec::Drain<'_, (ParamHandle, f32)> {
        self.updates.drain(..)
    }
}

/// The de-zipper glide length in samples for `smooth_ms` at analog rate `rate_hz` (≥ 1 sample).
pub(crate) fn smooth_samples(smooth_ms: f32, rate_hz: f64) -> f64 {
    (f64::from(smooth_ms) * 1e-3 * rate_hz).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn settled_smoother_holds_its_value() {
        let s = Smoother::new(2.0, 0.0, 10.0, 100.0);
        assert_eq!(s.value_at(0), 2.0);
        assert_eq!(s.value_at(50), 2.0); // no target change ⇒ flat, no ramp
    }

    #[test]
    fn a_change_ramps_linearly_then_holds() {
        // Glide 0 → 1 over 100 samples: step 0.01/sample. Read within a block and advance it.
        let mut s = Smoother::new(0.0, 0.0, 1.0, 100.0);
        s.set_target(1.0);
        assert_relative_eq!(s.value_at(0), 0.0, epsilon = 1e-6);
        assert_relative_eq!(s.value_at(10), 0.10, epsilon = 1e-6);
        assert_relative_eq!(s.value_at(50), 0.50, epsilon = 1e-6);

        // After 100 samples it reaches the target and stops there (no overshoot).
        s.advance(100);
        assert_relative_eq!(s.value_at(0), 1.0, epsilon = 1e-6);
        assert_relative_eq!(s.value_at(40), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn value_never_overshoots_within_a_block() {
        // A block longer than the glide must clamp at the target, not sail past it.
        let mut s = Smoother::new(0.0, 0.0, 1.0, 10.0);
        s.set_target(1.0);
        assert_relative_eq!(s.value_at(10), 1.0, epsilon = 1e-6);
        assert_relative_eq!(s.value_at(1000), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn target_is_clamped_to_range() {
        let mut s = Smoother::new(0.0, 0.0, 1.0, 1.0);
        s.set_target(5.0); // above max
        s.advance(1);
        assert_relative_eq!(s.value_at(0), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn queue_is_latest_wins_and_bounded() {
        let mut q = ParamQueue::with_capacity(2);
        assert!(q.set(ParamHandle(0), 1.0));
        assert!(q.set(ParamHandle(0), 2.0)); // same handle: coalesces, no new slot
        assert_eq!(q.len(), 1);
        assert!(q.set(ParamHandle(1), 9.0));
        assert!(
            !q.set(ParamHandle(2), 3.0),
            "a new handle past capacity is dropped"
        );

        let drained: Vec<(ParamHandle, f32)> = q.drain().collect();
        assert_eq!(drained, vec![(ParamHandle(0), 2.0), (ParamHandle(1), 9.0)]);
        assert!(q.is_empty());
    }

    #[test]
    fn params_view_falls_back_when_absent() {
        let p = Params::EMPTY;
        assert_eq!(p.value_or(ParamId(0), 3.5), 3.5);
        assert_eq!(p.value_at_or(ParamId(7), 3, -1.0), -1.0);
    }
}
