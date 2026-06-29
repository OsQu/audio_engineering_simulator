//! A block of sparse, timestamped control events — the MIDI/control carrier.

/// A control message: what happened, independent of when. The sparse, **non-dense** counterpart
/// to a sample — there is no per-sample value, only occasional messages.
///
/// MIDI-native values ride as the integers they are on the wire (note number, 0–127 velocity); a
/// consuming node maps them to whatever it needs (a frequency, a level). Only the note lifecycle is
/// carried; continuous controllers (CC) — which would drive a *control param*, blurring the two
/// input lanes — are deliberately not modeled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventMessage {
    /// Start a note: MIDI note number and 0–127 velocity.
    NoteOn {
        /// MIDI note number (0–127); 69 = A4 = 440 Hz.
        note: u8,
        /// Attack velocity (0–127).
        velocity: u8,
    },
    /// Release a note. Carries the note so a polyphonic consumer can match it to its voice and a
    /// monophonic one can ignore a stale release for a note it has already moved off.
    NoteOff {
        /// MIDI note number being released.
        note: u8,
    },
    /// A bare gate transition (open/closed) — a trigger without pitch, e.g. an envelope gate.
    Gate(bool),
}

/// An [`EventMessage`] stamped with the sample offset, within the block, at which it applies.
///
/// The offset is **block-relative** (0 = the block's first sample): the lane carries only the
/// events for the current block, so a consuming node dispatches each at its exact sample and the
/// timing is sample-accurate without any absolute clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedEvent {
    /// Sample offset within the block at which this event applies (0-based).
    pub offset: u32,
    /// The message.
    pub message: EventMessage,
}

/// A block's worth of timestamped events: a **sparse, bounded** lane, the carrier peer of
/// [`VoltageBuffer`](super::VoltageBuffer) / [`SampleBuffer`](super::SampleBuffer).
///
/// Unlike those dense buffers it holds *occasional* messages, not one value per sample — so it is
/// a list, sized by a **capacity** (max events per block), not a block length. The capacity is
/// allocated once at [`with_capacity`](Self::with_capacity) (i.e. at `compile`); the hot path only
/// [`push`](Self::push)es, [`clear`](Self::clear)s, and [`copy_from`](Self::copy_from)s within it.
/// On overflow the hot path **drops** the excess rather than reallocating or panicking — the
/// bound is honored, never the audio stream. Events are stored in insertion order; a producer is
/// expected to push them in nondecreasing `offset` order (the consumer dispatches them in order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBuffer {
    events: Vec<TimedEvent>,
    cap: usize,
}

impl EventBuffer {
    /// An empty buffer that can hold up to `cap` events per block. Allocates `cap` slots up front
    /// so the hot path never grows the backing store. `cap == 0` is a valid (always-dropping) lane.
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            events: Vec::with_capacity(cap),
            cap,
        }
    }

    /// Append `event` if there is room, returning `true` if it was stored. On overflow it is
    /// **dropped** and `false` returned — bounded, alloc-free, panic-free (the hot-path contract).
    #[inline]
    pub fn push(&mut self, event: TimedEvent) -> bool {
        if self.events.len() < self.cap {
            self.events.push(event);
            true
        } else {
            false
        }
    }

    /// Drop all events, keeping the allocated capacity for reuse next block (no free, no realloc).
    #[inline]
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Replace this buffer's contents with `other`'s, truncated to this buffer's capacity. The
    /// hot-path body of an event-route edge: clear, then copy up to `cap` (dropping any excess if
    /// the source carries more than this lane can hold). Alloc-free, panic-free.
    #[inline]
    pub fn copy_from(&mut self, other: &EventBuffer) {
        self.events.clear();
        let n = other.events.len().min(self.cap);
        self.events.extend_from_slice(&other.events[..n]);
    }

    /// The events in this block, in insertion order — the read view for a consuming node.
    #[inline]
    pub fn as_slice(&self) -> &[TimedEvent] {
        &self.events
    }

    /// How many events are currently in the block.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the block currently carries no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// The maximum number of events this lane can hold in a block.
    pub fn capacity(&self) -> usize {
        self.cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_on(offset: u32, note: u8) -> TimedEvent {
        TimedEvent {
            offset,
            message: EventMessage::NoteOn {
                note,
                velocity: 100,
            },
        }
    }

    #[test]
    fn push_stores_in_order_until_full() {
        let mut buf = EventBuffer::with_capacity(2);
        assert!(buf.is_empty());
        assert!(buf.push(note_on(0, 60)));
        assert!(buf.push(note_on(10, 64)));
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.as_slice(), &[note_on(0, 60), note_on(10, 64)]);
    }

    #[test]
    fn push_drops_on_overflow_without_growing() {
        let mut buf = EventBuffer::with_capacity(1);
        let backing = buf.events.as_ptr();
        assert!(buf.push(note_on(0, 60)));
        // Over capacity: dropped, not stored, and the backing allocation is untouched.
        assert!(!buf.push(note_on(1, 61)));
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.capacity(), 1);
        assert_eq!(buf.events.as_ptr(), backing, "overflow must not reallocate");
    }

    #[test]
    fn clear_keeps_capacity() {
        let mut buf = EventBuffer::with_capacity(4);
        let backing = buf.events.as_ptr();
        buf.push(note_on(0, 60));
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 4);
        assert_eq!(buf.events.as_ptr(), backing, "clear must not reallocate");
    }

    #[test]
    fn copy_from_replaces_contents_within_capacity() {
        let mut src = EventBuffer::with_capacity(4);
        src.push(note_on(0, 60));
        src.push(note_on(5, 62));

        let mut dst = EventBuffer::with_capacity(4);
        dst.push(note_on(99, 1)); // stale content, must be overwritten
        let backing = dst.events.as_ptr();
        dst.copy_from(&src);
        assert_eq!(dst.as_slice(), src.as_slice());
        assert_eq!(
            dst.events.as_ptr(),
            backing,
            "copy_from must not reallocate"
        );
    }

    #[test]
    fn copy_from_truncates_to_destination_capacity() {
        let mut src = EventBuffer::with_capacity(4);
        for i in 0..4 {
            src.push(note_on(i, 60 + i as u8));
        }
        let mut dst = EventBuffer::with_capacity(2);
        let backing = dst.events.as_ptr();
        dst.copy_from(&src);
        // Only the first two fit; the rest are dropped, and the backing store is unchanged.
        assert_eq!(dst.len(), 2);
        assert_eq!(dst.as_slice(), &src.as_slice()[..2]);
        assert_eq!(dst.events.as_ptr(), backing);
    }
}
