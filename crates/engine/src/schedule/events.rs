//! The external event queue: the host→engine seam carrying timestamped control events.
//!
//! Events enter the running engine from outside the graph — a key pressed, a sequencer step — and
//! must land at an exact sample. The host stamps each with an **absolute sample time** and a
//! target [`EventInputId`] (resolved once from the compiled schedule), pushing them onto an
//! [`EventQueue`]; [`Schedule::process_with_events`](super::Schedule::process_with_events) drains
//! the ones due this block, buckets each to its block-relative offset, and writes it into the
//! target node's event input lane. Events beyond the block stay queued for a later one.
//!
//! Like the [`ScheduleSlot`](super::ScheduleSlot) swap seam, this is **single-producer /
//! single-consumer in shape**: events are handed from a UI/MIDI producer to the audio-thread
//! consumer. A genuinely lock-free shared-memory ring is not yet built; the queue's push/drain
//! interface is what that would swap its internals beneath.

use crate::signal::EventMessage;

/// An opaque handle to an **open** event input lane — an event-domain input port with no incoming
/// edge, so the host may feed it. Obtained from
/// [`Schedule::event_input`](super::Schedule::event_input); meaningful only to the schedule that
/// produced it (it indexes that schedule's lane pool).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventInputId(pub(crate) usize);

/// One queued event: an [`EventMessage`] stamped with the absolute sample time it applies at and
/// the input lane it targets.
#[derive(Debug, Clone, Copy)]
pub(crate) struct QueuedEvent {
    pub(crate) when: u64,
    pub(crate) target: EventInputId,
    pub(crate) message: EventMessage,
}

/// A bounded queue of timestamped events awaiting delivery into the engine.
///
/// Push events in **nondecreasing `when` order** (a sequencer/keyboard naturally does); the
/// consumer drains the due prefix each block and leaves the rest. The capacity is fixed at
/// construction — on overflow [`push`](Self::push) **drops** the event and returns `false` rather
/// than reallocating, so a flood of input can never stall the audio path.
pub struct EventQueue {
    events: Vec<QueuedEvent>,
    cap: usize,
}

impl EventQueue {
    /// A queue holding up to `cap` pending events, pre-allocated so pushes never reallocate.
    /// `cap == 0` is a valid (always-dropping) queue — what `process` uses for its no-event path.
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            events: Vec::with_capacity(cap),
            cap,
        }
    }

    /// Enqueue `message` to fire at absolute sample time `when` on input `target`. Returns `true`
    /// if stored, `false` if the queue was full (the event is dropped). Push in nondecreasing
    /// `when` order so the consumer can drain the due prefix without sorting.
    pub fn push(&mut self, when: u64, target: EventInputId, message: EventMessage) -> bool {
        if self.events.len() < self.cap {
            self.events.push(QueuedEvent {
                when,
                target,
                message,
            });
            true
        } else {
            false
        }
    }

    /// Number of events currently waiting.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether no events are waiting.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// The maximum number of events the queue can hold.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Drop all pending events.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Remove and yield every event due before absolute time `end` — the front prefix, since
    /// events are pushed in nondecreasing time order. Alloc-free: [`Vec::drain`] shifts the
    /// remainder in place. (An out-of-order push would simply defer its event to whichever block
    /// the prefix scan first reaches it in — never a panic.)
    pub(crate) fn drain_due(&mut self, end: u64) -> std::vec::Drain<'_, QueuedEvent> {
        let k = self.events.iter().take_while(|e| e.when < end).count();
        self.events.drain(..k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(note: u8) -> EventMessage {
        EventMessage::NoteOn {
            note,
            velocity: 100,
        }
    }

    #[test]
    fn push_until_full_then_drop() {
        let mut q = EventQueue::with_capacity(2);
        assert!(q.is_empty());
        assert!(q.push(0, EventInputId(0), msg(60)));
        assert!(q.push(10, EventInputId(0), msg(62)));
        assert!(!q.push(20, EventInputId(0), msg(64)), "full queue drops");
        assert_eq!(q.len(), 2);
        assert_eq!(q.capacity(), 2);
    }

    #[test]
    fn drain_due_takes_the_prefix_before_end() {
        let mut q = EventQueue::with_capacity(8);
        q.push(2, EventInputId(0), msg(60));
        q.push(5, EventInputId(0), msg(62));
        q.push(20, EventInputId(0), msg(64)); // beyond this block

        let due: Vec<u64> = q.drain_due(16).map(|e| e.when).collect();
        assert_eq!(due, vec![2, 5]);
        // The not-yet-due event remains for a later block.
        assert_eq!(q.len(), 1);
        let later: Vec<u64> = q.drain_due(32).map(|e| e.when).collect();
        assert_eq!(later, vec![20]);
        assert!(q.is_empty());
    }

    #[test]
    fn drain_due_with_nothing_due_is_empty() {
        let mut q = EventQueue::with_capacity(4);
        q.push(100, EventInputId(0), msg(60));
        assert_eq!(q.drain_due(16).count(), 0);
        assert_eq!(q.len(), 1);
    }
}
