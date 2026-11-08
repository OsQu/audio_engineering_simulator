//! The DAW control facade — the host's off-block seam onto a recorder node's transport, tracks, and
//! file-byte streams.
//!
//! A node like [`MultitrackRecorder`](crate::MultitrackRecorder) owns state the generic control
//! surface can't reach: the [`Transport`], the per-track faders/arm/monitor/input assignment, and the
//! per-track playback/record [`ByteRing`]s. Those aren't params (no smoother store),
//! events (no queue), or readouts (no scalar slot) — the schedule's existing handle stores don't hold
//! them, they live *inside the node*. `DawControl` is the dyn-safe trait that exposes them, reached via
//! the defaulted [`Node::daw`](crate::Node::daw) hook (the phantom-/group-delay-hook precedent): the
//! host resolves `device → node → daw()` and drives the DAW here.
//!
//! **Off the per-sample loop.** Every op is a host gesture between quanta (transport buttons, fader
//! moves, the worklet feeding/draining a block of file bytes) — never `process`. The trait is
//! object-safe at the cost of dynamic dispatch. The byte ops still don't allocate: the worklet drains
//! the record stream from its render callback, on the audio thread, so
//! [`drain_record_into`](DawControl::drain_record_into) copies into a caller-owned buffer.

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

    /// Drain buffered raw PCM from track `track`'s **record** stream into `out`, for the host to append
    /// to a file. Copies as many whole `f32` frames as are buffered and fit in `out`, and returns the
    /// byte count (0 when nothing is buffered or the track is out of range). Allocation-free; size
    /// `out` to the record ring's capacity to empty it in one call.
    fn drain_record_into(&mut self, track: usize, out: &mut [u8]) -> usize;
}

/// Bytes per recorded frame: one mono `f32`.
const FRAME_BYTES: usize = std::mem::size_of::<f32>();

/// Drain up to `out.len()` buffered bytes out of `ring` into `out`, in whole frames, and return the
/// count. Shared helper for a [`DawControl::drain_record_into`] impl. The count never exceeds
/// `ring.len()`, so the all-or-nothing [`ByteRing::read`] always succeeds. Allocation-free.
pub(crate) fn drain_ring_into(ring: &mut ByteRing, out: &mut [u8]) -> usize {
    let n = ring.len().min(out.len()) / FRAME_BYTES * FRAME_BYTES;
    match out.get_mut(..n) {
        Some(dst) => {
            if ring.read(dst) {
                n
            } else {
                0
            }
        }
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drain copies whole frames only, bounded by both the buffered bytes and `out`; the rest stays
    /// queued for the next drain.
    #[test]
    fn drain_ring_into_copies_whole_frames_that_fit() {
        let mut ring = ByteRing::with_capacity(64);
        assert!(ring.write(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]));

        // 7-byte buffer → one whole 4-byte frame.
        let mut small = [0u8; 7];
        assert_eq!(drain_ring_into(&mut ring, &mut small), 4);
        assert_eq!(&small[..4], &[1, 2, 3, 4]);

        // A roomy buffer takes the remaining 8 bytes, then the ring is empty.
        let mut big = [0u8; 32];
        assert_eq!(drain_ring_into(&mut ring, &mut big), 8);
        assert_eq!(&big[..8], &[5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(drain_ring_into(&mut ring, &mut big), 0);
    }
}
