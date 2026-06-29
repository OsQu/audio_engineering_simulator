//! Ground-loop hum: a deterministic 50/60 Hz tone coupled common-mode onto a cabled edge.
//!
//! A ground loop drives a low-frequency current onto the cable's conductors equally, so the hum is
//! the same on every conductor of an edge and cancels at a balanced receiver's difference while
//! surviving on an unbalanced line. The amplitude is phenomenological (the induced voltage isn't
//! derived from loop geometry); only the tone itself is generated here.

use crate::rng::Rng;
use crate::signal::{AnalogRate, Volts};

/// A deterministic ground-loop hum generator: `amp·sin(phase)` per sample. `Copy` so every
/// conductor of one edge holds an identical generator (same seeded phase, same increment) — the hum
/// is common-mode and cancels at a balanced receiver.
#[derive(Clone, Copy)]
pub(super) struct HumGen {
    phase: f64,
    dphase: f64,
    amp: f32,
}

impl HumGen {
    /// A hum at `freq_hz`/`amp`, with its initial phase drawn from `stream` (one draw) so the phase
    /// is deterministic per edge and stable regardless of topology. The increment is fixed from the
    /// frequency and the analog sample period. Off the hot path.
    pub(super) fn new(freq_hz: f64, amp: Volts, rate: AnalogRate, stream: &mut Rng) -> Self {
        Self {
            phase: f64::from(stream.next_f32_unit()) * core::f64::consts::TAU,
            dphase: core::f64::consts::TAU * freq_hz * rate.seconds_per_sample(),
            amp: amp.get(),
        }
    }

    /// Next hum sample, advancing the phase. Hot path: no allocation, no panic.
    #[inline]
    pub(super) fn step(&mut self) -> f32 {
        let v = self.amp * self.phase.sin() as f32;
        self.phase += self.dphase;
        if self.phase >= core::f64::consts::TAU {
            self.phase -= core::f64::consts::TAU;
        }
        v
    }
}
