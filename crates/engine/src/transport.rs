//! The DAW transport — a play/stop/record state machine on an in-simulation digital-domain playhead.
//!
//! Story 5.11's `computer` records and plays tracks against one transport whose clock is the
//! **in-simulation digital domain**, never the host's capture clock (PROJECT_PLAN §5.6: "clock is a
//! real rate, not a label"). The [`playhead`](Transport::playhead) is a `u64` counter in **digital
//! samples** that advances one digital block per processed block **while rolling** and holds while
//! stopped. One digital block is **128 samples @ 48 kHz** — the analog block (1024) ÷ M (8), the
//! integer decimation the schedule already enforces. It is the DAW's *own* counter, distinct from
//! [`Schedule`](crate::Schedule)'s analog-rate external-event `sample_pos`, so when Story 5.3 lets a
//! DAW clock drift from the interface clock it is already modelled as its own digital clock.
//!
//! **Rolling and record-enable are independent — the overdub invariant.** Playback happens whenever
//! the transport is [`rolling`](Transport::is_rolling); recording happens only when it is *also*
//! [`record_enabled`](Transport::record_enabled) (and, per track, armed — the deck's concern). So
//! arming a new take and toggling record on **does not stop the tracks already playing back**: a new
//! track records while recorded ones play, on the one shared playhead. There is no "record mode"
//! that supersedes playback — that would forbid overdubbing, the core multitrack act.
//!
//! The state is a pure function of the commands applied (play/stop/seek/record-enable) and the
//! number of rolling blocks — no ambient time, no entropy — so a run is deterministic and replayable.

/// The DAW transport: a digital-sample playhead plus rolling/record-enable state.
///
/// Host commands ([`play`](Self::play) / [`stop`](Self::stop) / [`seek`](Self::seek) /
/// [`set_record_enabled`](Self::set_record_enabled)) mutate it; the deck [`advance`](Self::advance)s
/// it once per processed block. All operations are trivial integer/boolean work — safe on the hot
/// path (no allocation, no panic: the playhead saturates rather than overflowing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Transport {
    /// Digital-sample position of the **next** block's first sample. Starts at 0.
    playhead: u64,
    /// Whether the transport is playing (advancing) as opposed to stopped (held).
    rolling: bool,
    /// Whether recording is enabled — independent of `rolling`; armed tracks capture only when both
    /// this and `rolling` hold.
    record_enabled: bool,
}

impl Transport {
    /// A stopped transport at position 0 with recording disabled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The digital-sample position of the next block's first sample.
    #[must_use]
    pub fn playhead(&self) -> u64 {
        self.playhead
    }

    /// Whether the transport is rolling (playing back). Gates **playback**.
    #[must_use]
    pub fn is_rolling(&self) -> bool {
        self.rolling
    }

    /// Whether recording is enabled (independent of rolling).
    #[must_use]
    pub fn record_enabled(&self) -> bool {
        self.record_enabled
    }

    /// Whether armed tracks capture this block: rolling **and** record-enabled. Gates **recording**
    /// (the deck ANDs this with each track's arm). Toggling record-enable flips this **without**
    /// changing `rolling`, so playback continues — the overdub gate.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.rolling && self.record_enabled
    }

    /// Start rolling (play). Idempotent; leaves the playhead where it is.
    pub fn play(&mut self) {
        self.rolling = true;
    }

    /// Stop rolling. Idempotent; the playhead holds its position (no rewind — that's an explicit
    /// [`seek`](Self::seek)).
    pub fn stop(&mut self) {
        self.rolling = false;
    }

    /// Jump the playhead to an absolute digital-sample position, whether rolling or stopped.
    pub fn seek(&mut self, pos: u64) {
        self.playhead = pos;
    }

    /// Enable or disable recording, independently of rolling. Does not start/stop playback.
    pub fn set_record_enabled(&mut self, on: bool) {
        self.record_enabled = on;
    }

    /// Advance the playhead by `frames` digital samples for one processed block, **only while
    /// rolling** (a no-op while stopped). Returns the block's **start** position — the playhead
    /// before advancing — so the caller can address the `[start, start + frames)` region it just
    /// processed. Saturates at `u64::MAX` rather than wrapping.
    pub fn advance(&mut self, frames: u64) -> u64 {
        let start = self.playhead;
        if self.rolling {
            self.playhead = self.playhead.saturating_add(frames);
        }
        start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One digital block is 128 samples @ 48 kHz (analog 1024 ÷ M = 8). While rolling, the playhead
    /// advances exactly that per block; each `advance` returns the block's start position.
    const DIGITAL_BLOCK: u64 = 1024 / 8; // = 128

    #[test]
    fn a_fresh_transport_is_stopped_at_zero() {
        let t = Transport::new();
        assert_eq!(t.playhead(), 0);
        assert!(!t.is_rolling());
        assert!(!t.record_enabled());
        assert!(!t.is_recording());
    }

    #[test]
    fn rolling_advances_one_digital_block_per_block() {
        let mut t = Transport::new();
        t.play();
        // Four blocks: starts are 0, 128, 256, 384; playhead ends at 512.
        for i in 0..4u64 {
            let start = t.advance(DIGITAL_BLOCK);
            assert_eq!(start, i * DIGITAL_BLOCK, "block {i} start");
        }
        assert_eq!(t.playhead(), 4 * DIGITAL_BLOCK); // 512
    }

    #[test]
    fn stopped_holds_the_playhead() {
        let mut t = Transport::new();
        t.play();
        t.advance(DIGITAL_BLOCK);
        t.advance(DIGITAL_BLOCK);
        assert_eq!(t.playhead(), 2 * DIGITAL_BLOCK); // 256
        t.stop();
        // Advancing while stopped is a no-op; the start it reports is the held position.
        assert_eq!(t.advance(DIGITAL_BLOCK), 2 * DIGITAL_BLOCK);
        assert_eq!(t.advance(DIGITAL_BLOCK), 2 * DIGITAL_BLOCK);
        assert_eq!(t.playhead(), 2 * DIGITAL_BLOCK, "held while stopped");
    }

    #[test]
    fn seek_repositions_exactly_rolling_or_stopped() {
        let mut t = Transport::new();
        t.seek(1_000);
        assert_eq!(t.playhead(), 1_000);
        t.play();
        t.advance(DIGITAL_BLOCK);
        assert_eq!(t.playhead(), 1_000 + DIGITAL_BLOCK); // 1128
        // Seek mid-roll jumps precisely; the next block continues from there.
        t.seek(42);
        assert_eq!(t.advance(DIGITAL_BLOCK), 42);
        assert_eq!(t.playhead(), 42 + DIGITAL_BLOCK); // 170
    }

    /// The overdub gate: toggling record-enable flips `is_recording` **without** stopping playback —
    /// so an armed track can start capturing while already-recorded tracks keep rolling.
    #[test]
    fn record_enable_is_independent_of_playback() {
        let mut t = Transport::new();
        t.play();
        // Rolling but not record-enabled: playback yes, recording no.
        assert!(t.is_rolling());
        assert!(!t.is_recording());
        t.advance(DIGITAL_BLOCK);

        // Punch record in mid-roll — still rolling, now also recording; the playhead never stalled.
        t.set_record_enabled(true);
        assert!(t.is_rolling(), "playback continues");
        assert!(t.is_recording(), "and now capturing");
        let start = t.advance(DIGITAL_BLOCK);
        assert_eq!(
            start, DIGITAL_BLOCK,
            "playhead advanced across the punch-in"
        );

        // Punch record back out — playback keeps going.
        t.set_record_enabled(false);
        assert!(t.is_rolling());
        assert!(!t.is_recording());
    }

    #[test]
    fn record_enabled_while_stopped_is_not_recording() {
        // Arming a track before hitting play must not capture — recording needs rolling too.
        let mut t = Transport::new();
        t.set_record_enabled(true);
        assert!(t.record_enabled());
        assert!(
            !t.is_recording(),
            "stopped ⇒ not recording despite record-enable"
        );
    }

    #[test]
    fn advance_saturates_rather_than_overflowing() {
        let mut t = Transport::new();
        t.seek(u64::MAX - 10);
        t.play();
        assert_eq!(t.advance(DIGITAL_BLOCK), u64::MAX - 10);
        assert_eq!(t.playhead(), u64::MAX, "saturates, no panic/wrap");
    }
}
