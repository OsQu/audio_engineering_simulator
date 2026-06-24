//! Browser/WASM bindings for the engine.
//!
//! Epic 3 hosts the engine in the browser. This crate is the JS-facing surface, built to
//! `wasm32-unknown-unknown` with `wasm-bindgen` (via `wasm-pack`). See the crate `README.md` for
//! the build + install commands.
//!
//! **Scope (Story 3.1):** the *minimal compute-only* surface needed for the faster-than-real-time
//! **feasibility benchmark** — the gate that decided whether the oversampled voltage chain can run
//! inside an AudioWorklet. [`BenchEngine`] loops the engine entirely inside WASM
//! ([`BenchEngine::render_blocks`]) and is timed from JS, so there is **no per-quantum marshalling
//! and no `unsafe`** there. It is now **frozen** as that fixture.
//!
//! **Scope (Story 3.2):** [`RtEngine`] is the real-time surface the AudioWorklet actually drains —
//! [`RtEngine::render_quantum`] renders exactly one quantum (one engine block) into an
//! engine-owned host buffer, exposed **zero-copy** via [`RtEngine::out_ptr`] /
//! [`out_len`](RtEngine::out_len): JS builds one `Float32Array` view over WASM linear memory and
//! reads it every quantum, with no per-quantum marshalling. Returning a pointer is safe Rust
//! (`as_ptr`); the only `unsafe` is JS-side constructing the view — so there is still no `unsafe`
//! in this crate.

use capture::Capture;
use engine::{
    AdConverter, AnalogRate, BitDepth, DaConverter, EventInputId, EventMessage, EventQueue,
    GainStage, Graph, InputZ, Ohms, PassiveSum, SampleRate, Schedule, Speaker, SynthVoice,
    VoltageBuffer, Volts, compile,
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

    /// Build a **scaling stress patch**: `channels` parallel `synth → gain → AD → DA` chains summed
    /// into one speaker (so `4·channels + 2` nodes). Mixes the heavy nodes (synth, AD, DA) with
    /// cheap ones (the gain stage, the N-input passive sum) to map **headroom vs. node count** — the
    /// hundreds-of-nodes / stadium-routing question the 1-channel gate ([`new`](Self::new)) can't
    /// answer. All voices hold the same note (steady-state load); the summed level clamps at the
    /// capture, which is fine — this measures compute, not audio. `channels` is clamped to ≥ 1.
    #[must_use]
    pub fn scaled(channels: usize) -> BenchEngine {
        let channels = channels.max(1);
        let analog_rate = AnalogRate::new(ANALOG_RATE_HZ);
        let host_rate = SampleRate::new(HOST_RATE_HZ);

        let mut g = Graph::new();
        let mut voices = Vec::with_capacity(channels);
        let mut das = Vec::with_capacity(channels);
        let mut sum_inputs = Vec::with_capacity(channels);
        for _ in 0..channels {
            let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
            let gain = g.add(GainStage::new(
                1.0,
                Volts::new(10.0),
                InputZ::new(Ohms::new(10_000.0)),
                Ohms::new(150.0),
            ));
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
            g.connect(voice, 0, gain, 0);
            g.connect(gain, 0, ad, 0);
            g.connect(ad, 0, da, 0);
            voices.push(voice);
            das.push(da);
            sum_inputs.push(InputZ::new(Ohms::new(10_000.0)));
        }
        let sum = g.add(PassiveSum::new(sum_inputs, Ohms::new(150.0)));
        for (i, da) in das.iter().enumerate() {
            g.connect(*da, 0, sum, i);
        }
        let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
        g.connect(sum, 0, spk, 0);
        g.set_output(spk, 0);

        let schedule = compile(g, BLOCK_LEN, analog_rate, SEED).expect("scaled patch compiles");
        let mut events = EventQueue::with_capacity(channels + 1);
        for voice in &voices {
            let ev = schedule
                .event_input(*voice, 0)
                .expect("each voice has an event input");
            events.push(
                0,
                ev,
                EventMessage::NoteOn {
                    note: NOTE,
                    velocity: 100,
                },
            );
        }

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

// --- Story 3.2: the real-time surface the AudioWorklet drains. -----------------------------------

/// Repeating-note demo cadence: the voice sounds for this many blocks, rests for the next, and
/// repeats. Pleasanter than a held sawtooth drone, and it proves the event lane keeps advancing
/// correctly over a long live session. At 384 kHz / `BLOCK_LEN` ≈ 375 blocks/s.
const NOTE_ON_BLOCKS: u64 = 165; // ≈ 0.44 s sounding
const NOTE_OFF_BLOCKS: u64 = 90; // ≈ 0.24 s rest

/// The real-time engine surface (Story 3.2): the pinned canonical patch
/// (`synth → modeled AD → modeled DA → speaker`) plus the implicit [`Capture`], driven **one
/// AudioWorklet quantum at a time** with its captured host block exposed **zero-copy** to JS.
///
/// Unlike [`BenchEngine`] (the frozen 3.1 gate fixture, which only returns a peak float so JS can
/// time a tight loop), this is the surface the worklet drains every callback:
/// [`render_quantum`](Self::render_quantum) renders exactly one block into an engine-owned host
/// buffer, and [`out_ptr`](Self::out_ptr) / [`out_len`](Self::out_len) let JS build a single
/// `Float32Array` view over WASM linear memory and read it each quantum — no marshalling, no
/// per-quantum allocation. The view stays valid for the session because the hot path is zero-alloc,
/// so linear memory never `grow`s mid-render to detach it.
#[wasm_bindgen]
pub struct RtEngine {
    schedule: Schedule,
    capture: Capture,
    events: EventQueue,
    /// Per-quantum scratch: the speaker-voltage tap (analog rate) and the captured host samples.
    out: VoltageBuffer,
    host: Vec<f32>,
    /// The voice's event input, kept so the repeating-note demo can re-trigger it each cycle.
    voice_ev: EventInputId,
    /// Quanta rendered so far. `blocks * BLOCK_LEN` is the absolute analog-sample time of the next
    /// block's first sample — the timeline the [`EventQueue`] timestamps against.
    blocks: u64,
    /// Repeating-note state: whether the note is currently sounding, and the block it next toggles.
    note_on: bool,
    next_toggle: u64,
}

#[wasm_bindgen]
impl RtEngine {
    /// Build and compile the pinned canonical patch and strike the first note. Panics only here
    /// (the construct/compile gate is allowed to); the patch is known-valid, so in practice it does
    /// not. Duplicates the patch wiring rather than sharing it with [`BenchEngine`], which stays
    /// frozen as the 3.1 fixture (≈10 lines — the cost the plan accepted to keep the gate intact).
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
        let voice_ev = schedule
            .event_input(voice, 0)
            .expect("the voice has an event input");

        // Strike the first note at t = 0; render_quantum drives the repeating cadence thereafter.
        let mut events = EventQueue::with_capacity(4);
        events.push(
            0,
            voice_ev,
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
            voice_ev,
            blocks: 0,
            note_on: true,
            next_toggle: NOTE_ON_BLOCKS,
        }
    }

    /// Render exactly one AudioWorklet quantum — one engine block (`BLOCK_LEN` analog samples) →
    /// `BLOCK_LEN / M` host samples — into the engine-owned host buffer, advancing the
    /// repeating-note demo. Zero-alloc, no marshalling: read the result via
    /// [`out_ptr`](Self::out_ptr) / [`out_len`](Self::out_len).
    pub fn render_quantum(&mut self) {
        // Drive the repeating note. Events are timestamped in absolute analog samples; the block
        // about to be processed covers `[blocks·BLOCK_LEN, (blocks+1)·BLOCK_LEN)`, so a note pushed
        // at `blocks·BLOCK_LEN` fires on its first sample.
        if self.blocks == self.next_toggle {
            let when = self.blocks * BLOCK_LEN as u64;
            if self.note_on {
                self.events
                    .push(when, self.voice_ev, EventMessage::NoteOff { note: NOTE });
                self.note_on = false;
                self.next_toggle = self.blocks + NOTE_OFF_BLOCKS;
            } else {
                self.events.push(
                    when,
                    self.voice_ev,
                    EventMessage::NoteOn {
                        note: NOTE,
                        velocity: 100,
                    },
                );
                self.note_on = true;
                self.next_toggle = self.blocks + NOTE_ON_BLOCKS;
            }
        }
        self.schedule
            .process_with_events(&mut self.out, &mut self.events);
        self.capture.process(self.out.as_slice(), &mut self.host);
        self.blocks += 1;
    }

    /// Pointer to the captured host block in WASM linear memory. JS builds **one**
    /// `new Float32Array(memory.buffer, out_ptr(), out_len())` view after construction and reads it
    /// every quantum — zero-copy. `as_ptr` is safe Rust; the only `unsafe` is JS-side building the
    /// view. (Valid for the session: the zero-alloc hot path never `grow`s memory to detach it.)
    #[must_use]
    pub fn out_ptr(&self) -> *const f32 {
        self.host.as_ptr()
    }

    /// Host samples in the block view (`BLOCK_LEN / M`).
    #[must_use]
    pub fn out_len(&self) -> usize {
        self.host.len()
    }

    /// Host sample rate in Hz. JS pins `AudioContext({ sampleRate })` to this so the worklet's
    /// quantum rate matches the engine's output rate — otherwise every quantum is the wrong rate
    /// (wrong pitch + drift).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn host_rate_hz(&self) -> f64 {
        HOST_RATE_HZ
    }
}

impl Default for RtEngine {
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

    /// The scaling patch compiles and runs at a multi-channel size and still produces output — the
    /// wiring (N chains → N-input sum → speaker, N note-ons) holds as `channels` grows.
    #[test]
    fn scaled_patch_runs_multichannel() {
        let mut engine = BenchEngine::scaled(8);
        let peak = engine.render_blocks(32);
        assert!(peak > 0.05, "expected audible output, got peak {peak}");
    }

    /// `RtEngine`'s per-quantum surface is exercised **natively** (no browser): a handful of quanta
    /// of the sustained first note through the full converter chain must produce non-silent host
    /// audio in the engine-owned buffer. Guards the patch wiring and the quantum loop; the browser
    /// run proves it is *audible*, this asserts it has *output*.
    #[test]
    fn rt_engine_renders_audible_quanta() {
        let mut engine = RtEngine::new();
        let mut peak = 0.0_f32;
        // 32 quanta ≈ 85 ms — past the capture FIR latency and into the sustained first note.
        for _ in 0..32 {
            engine.render_quantum();
            peak = engine.host.iter().fold(peak, |p, &s| p.max(s.abs()));
        }
        assert!(peak > 0.05, "expected audible output, got peak {peak}");
    }

    /// The exposed buffer geometry is the pinned config, and the pointer is real — the contract JS
    /// relies on to size and place its `Float32Array` view.
    #[test]
    fn rt_engine_exposes_pinned_block_geometry() {
        let engine = RtEngine::new();
        assert_eq!(engine.out_len(), 128); // 1024 / 8
        assert_eq!(engine.host_rate_hz(), 48_000.0);
        assert!(!engine.out_ptr().is_null());
    }

    /// The repeating-note demo cadence cycles deterministically: the note releases after
    /// `NOTE_ON_BLOCKS` and re-triggers after the following `NOTE_OFF_BLOCKS`. The toggle fires on
    /// the quantum whose start `blocks == next_toggle` (i.e. after that many quanta have advanced
    /// `blocks`), so the on-phase needs `NOTE_ON_BLOCKS + 1` quanta to flip.
    #[test]
    fn rt_engine_note_cadence_cycles() {
        let mut engine = RtEngine::new();
        assert!(engine.note_on);
        for _ in 0..NOTE_ON_BLOCKS + 1 {
            engine.render_quantum();
        }
        assert!(!engine.note_on, "note should release after NOTE_ON_BLOCKS");
        for _ in 0..NOTE_OFF_BLOCKS {
            engine.render_quantum();
        }
        assert!(
            engine.note_on,
            "note should re-trigger after NOTE_OFF_BLOCKS"
        );
    }
}
