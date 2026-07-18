//! A bounded, frame-safe byte FIFO — the host↔engine transport for bulk audio file bytes.
//!
//! The Story 5.11 DAW moves **opaque file bytes** across the sim↔host boundary: recorded WAV bytes
//! out to disk (OPFS), file bytes back in for playback. Like the
//! [`EventQueue`](crate::EventQueue), this is **single-producer / single-consumer in shape** and
//! **pre-allocated** — [`write`](ByteRing::write) never reallocates, and on a full ring it drops the
//! whole chunk rather than a fragment. A genuinely lock-free shared-memory ring (a
//! `SharedArrayBuffer`) can later swap in beneath this same write/read interface.
//!
//! **Frame-safe by all-or-nothing.** Both [`write`](ByteRing::write) and [`read`](ByteRing::read)
//! move a whole chunk or nothing at all (returning `false`). So a too-slow *consumer* drops an
//! entire recorded block (an honest gap), and a too-slow *producer* yields an entire silent block on
//! read (an honest underrun) — never a half-frame that would desync the PCM sample stream. The DAW
//! keeps one ring **per track per direction**, so N tracks stream independently (overdub: several
//! play while one records) with no cross-talk.

/// A fixed-capacity circular byte buffer with all-or-nothing chunk transfers.
///
/// Capacity is set at construction; all storage is allocated then. `head` is the index of the
/// oldest stored byte and `len` the number stored, so the free space is `capacity − len` and the
/// write position is `(head + len) mod capacity`.
pub struct ByteRing {
    buf: Vec<u8>,
    head: usize,
    len: usize,
}

impl ByteRing {
    /// A ring holding up to `cap` bytes, pre-allocated so writes never reallocate. `cap == 0` is a
    /// valid always-full ring (every non-empty write drops; every non-empty read underruns).
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: vec![0u8; cap],
            head: 0,
            len: 0,
        }
    }

    /// Total capacity in bytes.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Bytes currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the ring holds no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Free space in bytes — the largest chunk [`write`](Self::write) would currently accept.
    #[must_use]
    pub fn free(&self) -> usize {
        self.buf.len() - self.len
    }

    /// Discard all stored bytes.
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    /// Append `src` as one indivisible unit. Returns `true` if stored, or `false` (writing nothing)
    /// if `src` wouldn't fit in the free space — so a partial, framing-tearing write can't happen.
    /// Allocation-free: copies into pre-allocated storage, wrapping at the end.
    pub fn write(&mut self, src: &[u8]) -> bool {
        if src.is_empty() {
            return true; // moving zero bytes always succeeds (and avoids a mod-by-zero at cap 0)
        }
        let cap = self.buf.len();
        if src.len() > cap - self.len {
            return false;
        }
        let tail = (self.head + self.len) % cap;
        let first = (cap - tail).min(src.len());
        self.buf[tail..tail + first].copy_from_slice(&src[..first]);
        let rest = src.len() - first;
        if rest > 0 {
            self.buf[..rest].copy_from_slice(&src[first..]);
        }
        self.len += src.len();
        true
    }

    /// Fill `dst` from the oldest bytes as one indivisible unit, consuming them, and return `true`.
    /// If fewer than `dst.len()` bytes are stored, leaves `dst` and the ring untouched and returns
    /// `false` (an underrun the caller handles as silence). Allocation-free.
    pub fn read(&mut self, dst: &mut [u8]) -> bool {
        if dst.is_empty() {
            return true;
        }
        if dst.len() > self.len {
            return false;
        }
        let cap = self.buf.len();
        let first = (cap - self.head).min(dst.len());
        dst[..first].copy_from_slice(&self.buf[self.head..self.head + first]);
        let rest = dst.len() - first;
        if rest > 0 {
            dst[first..].copy_from_slice(&self.buf[..rest]);
        }
        self.head = (self.head + dst.len()) % cap;
        self.len -= dst.len();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_round_trips_in_order() {
        let mut r = ByteRing::with_capacity(8);
        assert!(r.is_empty());
        assert!(r.write(&[1, 2, 3]));
        assert!(r.write(&[4, 5]));
        assert_eq!(r.len(), 5);
        assert_eq!(r.free(), 3);

        let mut a = [0u8; 3];
        assert!(r.read(&mut a));
        assert_eq!(a, [1, 2, 3]);
        let mut b = [0u8; 2];
        assert!(r.read(&mut b));
        assert_eq!(b, [4, 5]);
        assert!(r.is_empty());
    }

    #[test]
    fn write_is_all_or_nothing_when_full() {
        let mut r = ByteRing::with_capacity(4);
        assert!(r.write(&[1, 2, 3]));
        // Only 1 byte free — a 2-byte chunk is dropped whole, leaving the ring unchanged.
        assert!(!r.write(&[9, 9]));
        assert_eq!(r.len(), 3);
        // A 1-byte chunk still fits.
        assert!(r.write(&[4]));
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn read_underruns_whole_leaving_the_ring_intact() {
        let mut r = ByteRing::with_capacity(4);
        assert!(r.write(&[1, 2]));
        let mut dst = [0u8; 3]; // asking for more than is stored
        assert!(!r.read(&mut dst));
        assert_eq!(dst, [0, 0, 0], "dst untouched on underrun");
        assert_eq!(r.len(), 2, "ring untouched on underrun");
    }

    #[test]
    fn writes_and_reads_wrap_around_the_end() {
        // Capacity 4: fill, drain 3 (head→3), then a 3-byte write must wrap 1 → 3,0,1.
        let mut r = ByteRing::with_capacity(4);
        assert!(r.write(&[1, 2, 3, 4]));
        let mut three = [0u8; 3];
        assert!(r.read(&mut three));
        assert_eq!(three, [1, 2, 3]);
        // Now head = 3, len = 1 (holding 4). Write 3 more → wraps past the end.
        assert!(r.write(&[5, 6, 7]));
        assert_eq!(r.len(), 4);
        let mut all = [0u8; 4];
        assert!(r.read(&mut all));
        assert_eq!(all, [4, 5, 6, 7], "wrapped bytes read back in order");
    }

    #[test]
    fn zero_and_empty_transfers_are_no_ops() {
        let mut r = ByteRing::with_capacity(0);
        assert!(r.write(&[]), "empty write on a zero-cap ring succeeds");
        assert!(r.read(&mut []), "empty read succeeds");
        assert!(!r.write(&[1]), "any real write on a zero-cap ring drops");
    }

    /// The overdub composition: three independent rings — two inbound streams draining while a third
    /// outbound stream fills — never cross-talk, and each inbound stream round-trips its own WAV
    /// bytes bit-exactly. This is the primitive guarantee behind "record one track while two play".
    #[test]
    fn independent_streams_do_not_cross_talk() {
        use crate::wav::{WavSpec, decode_wav, encode_wav};

        let spec = WavSpec {
            sample_rate_hz: 48_000,
            channels: 1,
        };
        let take_a = encode_wav(&[0.1, 0.2, 0.3], spec);
        let take_b = encode_wav(&[-0.4, -0.5], spec);

        // Two inbound (playback) rings and one outbound (record) ring.
        let mut in_a = ByteRing::with_capacity(256);
        let mut in_b = ByteRing::with_capacity(256);
        let mut out_c = ByteRing::with_capacity(256);

        assert!(in_a.write(&take_a));
        assert!(in_b.write(&take_b));
        // The recorder fills its own stream in parallel — different bytes, no shared state.
        assert!(out_c.write(&[0xDE, 0xAD, 0xBE, 0xEF]));

        // Drain A and B independently; each must recover exactly its own take.
        let mut back_a = vec![0u8; take_a.len()];
        assert!(in_a.read(&mut back_a));
        let mut back_b = vec![0u8; take_b.len()];
        assert!(in_b.read(&mut back_b));

        assert_eq!(decode_wav(&back_a).unwrap().0, vec![0.1, 0.2, 0.3]);
        assert_eq!(decode_wav(&back_b).unwrap().0, vec![-0.4, -0.5]);
        // The outbound stream is unaffected by the inbound drains.
        assert_eq!(out_c.len(), 4);
    }
}
