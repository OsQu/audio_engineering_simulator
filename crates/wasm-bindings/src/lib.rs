//! Browser/WASM bindings for the engine.
//!
//! Epic 3 hosts the engine in the browser. This crate is the JS-facing surface, built to
//! `wasm32-unknown-unknown` with `wasm-bindgen` (via `wasm-pack`). See the crate `README.md` for
//! the build + install commands.
//!
//! **Scope (Story 3.1):** only the *minimal compute-only* surface needed for the
//! faster-than-real-time **feasibility benchmark** — the gate that decides whether the oversampled
//! voltage chain can run inside an AudioWorklet. [`BenchEngine`] loops the engine entirely inside
//! WASM ([`BenchEngine::render_blocks`]) and is timed from JS, so there is **no per-quantum
//! marshalling and no `unsafe`** here yet. The zero-copy raw-memory `process` hot path (a
//! `Float32Array` view over linear memory) is a Story 3.2 concern, built when the worklet actually
//! drains output every quantum.

use capture::Capture;
use engine::{
    AdConverter, AnalogRate, BitDepth, DaConverter, EventMessage, EventQueue, Graph, InputZ, Ohms,
    SampleRate, Schedule, Speaker, SynthVoice, VoltageBuffer, Volts, compile,
};
use wasm_bindgen::prelude::*;

// --- The pinned canonical-patch config (the gate fixture; mirror in the benchmark page). --------
/// Oversampled analog rate — the "continuous" proxy (8× the host rate).
const ANALOG_RATE_HZ: f64 = 384_000.0;
/// Host / converter sample rate. `M = ANALOG/HOST = 8`.
const HOST_RATE_HZ: f64 = 48_000.0;
/// Samples per `process` block. The **real-time quantum**: 128 host frames × M = 1024 analog
/// samples (deliberately *not* the offline harness's 384 — the benchmark must measure the size the
/// AudioWorklet will actually call with).
const BLOCK_LEN: usize = 1024;
/// Fixed seed (determinism) and monitor full-scale reference (speaker volts → ±1.0).
const SEED: u64 = 0;
const FULL_SCALE_V: f32 = 1.0;
/// A4 (440 Hz) — the note held for the whole benchmark.
const NOTE: u8 = 69;

/// The feasibility benchmark's engine: the pinned canonical patch
/// (`synth → modeled AD → modeled DA → speaker`) plus the implicit [`Capture`], driven entirely
/// inside WASM. JS constructs it once, then times [`render_blocks`](Self::render_blocks).
///
/// This is the **compute-only** gate surface — it owns its scratch buffers and never marshals
/// per-block data across the boundary, so what the benchmark measures is the engine's raw
/// per-quantum cost, not glue overhead. Construction (the one fallible, allocating step) happens in
/// [`new`](Self::new); `render_blocks` is the zero-alloc hot loop.
#[wasm_bindgen]
pub struct BenchEngine {
    schedule: Schedule,
    capture: Capture,
    /// A single sustained note-on sits here; after the first block it drains empty and the voice
    /// holds the note (steady-state per-block cost is what the gate measures).
    events: EventQueue,
    /// Per-block scratch: the speaker-voltage tap (analog rate) and the captured host samples.
    out: VoltageBuffer,
    host: Vec<f32>,
}

#[wasm_bindgen]
impl BenchEngine {
    /// Build and compile the pinned canonical patch and queue a sustained A4. Panics only here
    /// (the engine's construct/compile gate is allowed to) — the patch is known-valid, so in
    /// practice it does not.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        let analog_rate = AnalogRate::new(ANALOG_RATE_HZ);
        let host_rate = SampleRate::new(HOST_RATE_HZ);

        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        let ad = g.add(AdConverter::new(
            host_rate,
            BitDepth::new(16),
            Volts::new(FULL_SCALE_V),
            Ohms::new(1e6),
        ));
        let da = g.add(DaConverter::new(
            host_rate,
            BitDepth::new(16),
            Volts::new(FULL_SCALE_V),
            Ohms::new(150.0),
        ));
        let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
        g.connect(voice, 0, ad, 0);
        g.connect(ad, 0, da, 0);
        g.connect(da, 0, spk, 0);
        g.set_output(spk, 0);

        let schedule = compile(g, BLOCK_LEN, analog_rate, SEED).expect("canonical patch compiles");
        let ev = schedule
            .event_input(voice, 0)
            .expect("the voice has an event input");
        let mut events = EventQueue::with_capacity(4);
        events.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: NOTE,
                velocity: 100,
            },
        );

        let capture = Capture::new(analog_rate, host_rate, FULL_SCALE_V);
        let host = vec![0.0_f32; capture.host_len(BLOCK_LEN)];
        Self {
            out: VoltageBuffer::zeros(BLOCK_LEN, analog_rate),
            host,
            schedule,
            capture,
            events,
        }
    }

    /// Render `n` blocks (each `BLOCK_LEN` analog samples → host samples) through the full chain,
    /// entirely inside WASM, and return the peak `|sample|` observed. The return value depends on
    /// every block, so it both **prevents the optimizer from eliding the work** and lets the caller
    /// sanity-check non-silence. This is the inner loop the benchmark times from JS.
    pub fn render_blocks(&mut self, n: usize) -> f32 {
        let mut peak = 0.0_f32;
        for _ in 0..n {
            self.schedule
                .process_with_events(&mut self.out, &mut self.events);
            self.capture.process(self.out.as_slice(), &mut self.host);
            peak = self.host.iter().fold(peak, |p, &s| p.max(s.abs()));
        }
        peak
    }

    /// Host samples produced per rendered block (`BLOCK_LEN / M`) — JS needs it to convert a block
    /// count into seconds of audio for the realtime ratio.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn host_samples_per_block(&self) -> usize {
        self.host.len()
    }

    /// Host sample rate in Hz — the other half of the seconds-of-audio conversion.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn host_rate_hz(&self) -> f64 {
        HOST_RATE_HZ
    }
}

impl Default for BenchEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compute surface is exercised **natively** (no browser needed): a few blocks of a
    /// sustained A4 through the full converter chain must produce non-silent host audio. Guards the
    /// patch wiring and the render loop; the in-browser run measures *speed*, this asserts *output*.
    #[test]
    fn render_blocks_produces_audible_output() {
        let mut engine = BenchEngine::new();
        // 32 blocks ≈ 85 ms at 384 kHz — well past the capture FIR latency and into the sustain.
        let peak = engine.render_blocks(32);
        assert!(peak > 0.05, "expected audible output, got peak {peak}");
    }

    #[test]
    fn host_geometry_is_the_pinned_config() {
        let engine = BenchEngine::new();
        assert_eq!(engine.host_samples_per_block(), 128); // 1024 / 8
        assert_eq!(engine.host_rate_hz(), 48_000.0);
    }
}
