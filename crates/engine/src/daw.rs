//! The DAW control facade — the host's off-block seam onto a recorder node's transport, tracks, and
//! file-byte streams (Story 5.11).
//!
//! A node like [`MultitrackRecorder`](crate::MultitrackRecorder) owns state the generic control
//! surface can't reach: the [`Transport`], the per-track faders/arm/monitor/input assignment, and the
//! per-track playback/record [`ByteRing`]s. Those aren't params (no smoother store),
//! events (no queue), or readouts (no scalar slot) — the schedule's existing handle stores don't hold
//! them, they live *inside the node*. `DawControl` is the dyn-safe trait that exposes them, reached via
//! the defaulted [`Node::daw`](crate::Node::daw) hook (the phantom-/group-delay-hook precedent): the
//! host resolves `device → node → daw()` and drives the DAW here.
//!
//! **Off the hot path.** Every op is a host gesture between quanta (transport buttons, fader moves,
//! the worklet feeding/draining a block of file bytes) — never `process`. So the byte ops may allocate
//! ([`drain_record`](DawControl::drain_record) returns a fresh `Vec`) and the trait is object-safe at
//! the cost of dynamic dispatch, neither of which touches the per-sample loop.

use crate::byte_ring::ByteRing;
use crate::transport::Transport;

/// The host-facing control surface of a DAW/recorder node: its [`Transport`], per-track channel-strip
/// controls, and per-track file-byte streams. Implemented by
/// [`MultitrackRecorder`](crate::MultitrackRecorder) and reached through
/// [`Node::daw`](crate::Node::daw). Object-safe — the host holds it as `&mut dyn DawControl`.
///
/// All methods are **off the audio thread** (host gestures between blocks); track/lane indices out of
/// range are silent no-ops (or an empty drain), never a panic.
pub trait DawControl {
    /// Number of mono tracks this DAW carries — the valid `track` index range is `0..track_count()`.
    fn track_count(&self) -> usize;

    /// The transport, read-only — for the playhead and rolling/recording state the UI polls.
    fn transport(&self) -> &Transport;

    /// The transport, mutable — play/stop/seek/record-enable.
    fn transport_mut(&mut self) -> &mut Transport;

    /// Assign track `track`'s record/monitor source to send lane `lane`. No-op for a bad track.
    fn set_track_input(&mut self, track: usize, lane: usize);

    /// Arm or disarm track `track` for recording. No-op for a bad track.
    fn set_track_armed(&mut self, track: usize, armed: bool);

    /// Enable or disable input monitoring for track `track`. No-op for a bad track.
    fn set_track_monitoring(&mut self, track: usize, monitoring: bool);

    /// Set track `track`'s fader (de-zippered, clamped to the node's gain range). No-op for a bad track.
    fn set_track_level(&mut self, track: usize, level: f32);

    /// Feed `bytes` of raw PCM into track `track`'s **playback** stream, ahead of the playhead.
    /// All-or-nothing (the [`ByteRing`] drops a chunk that wouldn't fit whole): returns `true` if
    /// stored, `false` if it didn't fit (the host should retry the chunk next block) or the track is
    /// out of range.
    fn feed_playback(&mut self, track: usize, bytes: &[u8]) -> bool;

    /// Drain **all** currently buffered raw PCM from track `track`'s **record** stream, for the host to
    /// append to a file. Returns the bytes (a whole number of `f32` frames — the recorder only ever
    /// writes whole frames), or an empty `Vec` when nothing is buffered or the track is out of range.
    fn drain_record(&mut self, track: usize) -> Vec<u8>;
}

/// Drain every currently buffered byte out of `ring` into a fresh `Vec`. Shared helper for a
/// [`DawControl::drain_record`] impl: reads exactly `ring.len()` bytes (always available, so the
/// all-or-nothing [`ByteRing::read`] succeeds), leaving the ring empty. Off the hot path (it allocates).
#[must_use]
pub fn drain_ring(ring: &mut ByteRing) -> Vec<u8> {
    let mut out = vec![0u8; ring.len()];
    let _ = ring.read(&mut out);
    out
}
