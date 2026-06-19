//! The schedule-swap seam: hot-swap a running [`Schedule`] without stalling the audio path.

use super::Schedule;
use crate::signal::VoltageBuffer;

/// Holds the active [`Schedule`] and lets a freshly compiled one replace it in O(1).
///
/// A graph edit (load a scene, repatch a cable, reroute a node) means a *new* schedule, built
/// off the audio path by [`compile`](super::compile). [`install`](Self::install) swaps the new
/// one in by moving a single `Box` pointer — no allocation, no free, no lock on the calling
/// thread — and hands the old schedule back so it can be **dropped off the audio path**
/// (deallocating its buffers is not something to do mid-block). That is the property this seam
/// exists to prove: a scene reload is a pointer swap, never a stall.
///
/// A [`Schedule`] is stateful (filter and node state), so it can't be shared across threads
/// behind an atomic — the swap is an ownership *handoff*, not shared-pointer publication.
/// Exercised single-threaded here; the lock-free cross-thread channel that carries a new
/// schedule from a builder thread to the audio thread arrives with the real worklet in Epic 3.
pub struct ScheduleSlot {
    current: Box<Schedule>,
}

impl ScheduleSlot {
    /// Wrap a compiled schedule as the active one. (Boxing is setup-time, off the audio path.)
    #[must_use]
    pub fn new(schedule: Schedule) -> Self {
        Self {
            current: Box::new(schedule),
        }
    }

    /// Process one block through the active schedule. Hot path — delegates straight through.
    pub fn process(&mut self, out: &mut VoltageBuffer) {
        self.current.process(out);
    }

    /// Swap in `next` as the active schedule, returning the old one for off-path drop. O(1): a
    /// single pointer move, no allocation or free on this thread. Compile and box `next` off
    /// the audio path, then call this at a block boundary.
    #[must_use = "drop the returned old schedule off the audio path, not mid-block"]
    pub fn install(&mut self, next: Box<Schedule>) -> Box<Schedule> {
        std::mem::replace(&mut self.current, next)
    }

    /// The active schedule's block length.
    pub fn block_len(&self) -> usize {
        self.current.block_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Ohms;
    use crate::schedule::compile;
    use crate::signal::{AnalogRate, Volts};
    use crate::{Graph, TestSource};

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A one-node graph emitting a constant `level`, tapped at the source output.
    fn dc_schedule(level: f32) -> Schedule {
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(level), Ohms::new(150.0)));
        g.set_output(src, 0);
        compile(g, 4, rate()).expect("valid graph")
    }

    #[test]
    fn install_swaps_the_active_schedule() {
        let mut slot = ScheduleSlot::new(dc_schedule(1.0));
        let mut out = VoltageBuffer::zeros(4, rate());

        slot.process(&mut out);
        assert!(out.as_slice().iter().all(|&v| v == 1.0), "schedule A runs");

        // Build the replacement off-path, then hot-swap it in.
        let old = slot.install(Box::new(dc_schedule(2.0)));
        slot.process(&mut out);
        assert!(
            out.as_slice().iter().all(|&v| v == 2.0),
            "schedule B is now active"
        );

        // The old schedule came back intact — drop it here (off-path in a real engine).
        drop(old);
    }

    #[test]
    fn block_len_reflects_the_active_schedule() {
        let slot = ScheduleSlot::new(dc_schedule(1.0));
        assert_eq!(slot.block_len(), 4);
    }
}
