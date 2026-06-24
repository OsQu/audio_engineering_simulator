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
    GainStage, Graph, InputZ, Ohms, ParamHandle, ParamQueue, PassiveSum, SampleRate, Schedule,
    Speaker, SynthVoice, VoltageBuffer, Volts, compile,
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

// --- Stories 3.2 / 3.3: the real-time surface the AudioWorklet drains. ----------------------------

/// Distinct control params [`RtEngine`] drives — the voice's five smoothed knobs (`LEVEL`,
/// `ATTACK_MS`, `DECAY_MS`, `SUSTAIN`, `RELEASE_MS`). Sizes the latest-wins [`ParamQueue`].
const PARAM_COUNT: usize = 5;
/// Event-queue capacity. Generous for human-rate input within one ~2.7 ms quantum; every queued
/// event drains the very next [`render_quantum`](RtEngine::render_quantum) so it never fills.
const EVENT_QUEUE_CAP: usize = 64;

/// The real-time engine surface (Stories 3.2 / 3.3): the pinned canonical patch
/// (`synth → modeled AD → modeled DA → speaker`) plus the implicit [`Capture`], driven **one
/// AudioWorklet quantum at a time** with its captured host block exposed **zero-copy** to JS, and
/// **played and tweaked live** from JS through named control setters.
///
/// Unlike [`BenchEngine`] (the frozen 3.1 gate fixture, which only returns a peak float so JS can
/// time a tight loop), this is the surface the worklet drains every callback:
/// [`render_quantum`](Self::render_quantum) renders exactly one block into an engine-owned host
/// buffer, and [`out_ptr`](Self::out_ptr) / [`out_len`](Self::out_len) let JS build a single
/// `Float32Array` view over WASM linear memory and read it each quantum — no marshalling, no
/// per-quantum allocation. The view stays valid for the session because the hot path is zero-alloc,
/// so linear memory never `grow`s mid-render to detach it.
///
/// **Story 3.3 — live control & playing.** The engine no longer drives its own notes; control
/// arrives from the host. It owns a [`ParamQueue`] + [`EventQueue`] and exposes named setters:
/// [`set_level`](Self::set_level) / [`set_attack_ms`](Self::set_attack_ms) /
/// [`set_decay_ms`](Self::set_decay_ms) / [`set_sustain`](Self::set_sustain) /
/// [`set_release_ms`](Self::set_release_ms) push **latest-wins target values** (the engine's own
/// `Smoother` de-zippers them — so *not* `AudioParam`), and [`note_on`](Self::note_on) /
/// [`note_off`](Self::note_off) push timestamped events. [`render_quantum`](Self::render_quantum)
/// drains both lanes via `process_io` each block. A note is stamped at the **block about to render**
/// (`blocks · BLOCK_LEN`) — "play at the next quantum," ~2.7 ms granularity, imperceptible for human
/// input and zero host-time math. (Precise `currentTime`→sample mapping matters only for *sequenced*
/// MIDI; deferred.) The named-setter API is deliberately specific — the generic, UI-enumerable param
/// API (`ParamDecl`s + `set_param(id, value)`) is Epic 4 / Story 4.1.
#[wasm_bindgen]
pub struct RtEngine {
    schedule: Schedule,
    capture: Capture,
    /// Pending control input, drained each `render_quantum` via `process_io`. Params are
    /// latest-wins target values; events are note-on/off stamped at the current block.
    params: ParamQueue,
    events: EventQueue,
    /// Per-quantum scratch: the speaker-voltage tap (analog rate) and the captured host samples.
    out: VoltageBuffer,
    host: Vec<f32>,
    /// The voice's event input, the target for [`note_on`](Self::note_on) /
    /// [`note_off`](Self::note_off).
    voice_ev: EventInputId,
    /// Resolved control-param handles, set once at construction — the live knobs.
    level: ParamHandle,
    attack_ms: ParamHandle,
    decay_ms: ParamHandle,
    sustain: ParamHandle,
    release_ms: ParamHandle,
    /// Quanta rendered so far. `blocks * BLOCK_LEN` is the absolute analog-sample time of the next
    /// block's first sample — the timeline [`note_on`](Self::note_on) /
    /// [`note_off`](Self::note_off) stamp against.
    blocks: u64,
}

#[wasm_bindgen]
impl RtEngine {
    /// Build and compile the pinned canonical patch and resolve the control handles. Panics only
    /// here (the construct/compile gate is allowed to); the patch is known-valid, so in practice it
    /// does not. Duplicates the patch wiring rather than sharing it with [`BenchEngine`], which stays
    /// frozen as the 3.1 fixture (≈10 lines — the cost the plan accepted to keep the gate intact).
    /// Starts **silent** — notes come from [`note_on`](Self::note_on) (Story 3.3).
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
        // Resolve the five smoothed knob handles once; the setters push targets to these.
        let level = schedule
            .param(voice, SynthVoice::LEVEL)
            .expect("the voice declares LEVEL");
        let attack_ms = schedule
            .param(voice, SynthVoice::ATTACK_MS)
            .expect("the voice declares ATTACK_MS");
        let decay_ms = schedule
            .param(voice, SynthVoice::DECAY_MS)
            .expect("the voice declares DECAY_MS");
        let sustain = schedule
            .param(voice, SynthVoice::SUSTAIN)
            .expect("the voice declares SUSTAIN");
        let release_ms = schedule
            .param(voice, SynthVoice::RELEASE_MS)
            .expect("the voice declares RELEASE_MS");

        // Sized for a human-rate burst within one ~2.7 ms quantum: every queued event/param is
        // drained the very next render_quantum (notes stamp at the next block, < block end), so
        // these never need to hold more than a quantum's worth.
        let params = ParamQueue::with_capacity(PARAM_COUNT);
        let events = EventQueue::with_capacity(EVENT_QUEUE_CAP);

        let capture = Capture::new(analog_rate, host_rate, FULL_SCALE_V);
        let host = vec![0.0_f32; capture.host_len(BLOCK_LEN)];
        Self {
            out: VoltageBuffer::zeros(BLOCK_LEN, analog_rate),
            host,
            schedule,
            capture,
            params,
            events,
            voice_ev,
            level,
            attack_ms,
            decay_ms,
            sustain,
            release_ms,
            blocks: 0,
        }
    }

    /// Render exactly one AudioWorklet quantum — one engine block (`BLOCK_LEN` analog samples) →
    /// `BLOCK_LEN / M` host samples — into the engine-owned host buffer. Drains both control lanes
    /// first: pending param targets re-aim their smoothers, and notes due this block fire at their
    /// offsets. Zero-alloc, no marshalling: read the result via [`out_ptr`](Self::out_ptr) /
    /// [`out_len`](Self::out_len).
    pub fn render_quantum(&mut self) {
        self.schedule
            .process_io(&mut self.out, &mut self.params, &mut self.events);
        self.capture.process(self.out.as_slice(), &mut self.host);
        self.blocks += 1;
    }

    // --- Live control (Story 3.3): named setters JS calls from the worklet. ----------------------

    /// Set the voice output **level** in volts (the master volume fader). Latest-wins target; the
    /// engine's `Smoother` glides to it (no zipper). Off the hot path — JS calls it on slider input.
    pub fn set_level(&mut self, volts: f32) {
        self.params.set(self.level, volts);
    }

    /// Set the envelope **attack** time in milliseconds. Latest-wins, smoothed.
    pub fn set_attack_ms(&mut self, ms: f32) {
        self.params.set(self.attack_ms, ms);
    }

    /// Set the envelope **decay** time in milliseconds. Latest-wins, smoothed.
    pub fn set_decay_ms(&mut self, ms: f32) {
        self.params.set(self.decay_ms, ms);
    }

    /// Set the envelope **sustain** level (0..=1). Latest-wins, smoothed.
    pub fn set_sustain(&mut self, level: f32) {
        self.params.set(self.sustain, level);
    }

    /// Set the envelope **release** time in milliseconds. Latest-wins, smoothed.
    pub fn set_release_ms(&mut self, ms: f32) {
        self.params.set(self.release_ms, ms);
    }

    /// Strike `note` (MIDI 0..=127) at `velocity`. Stamped at the **block about to render**
    /// (`blocks · BLOCK_LEN`) so it fires on that quantum's first sample — "play at the next
    /// quantum," ~2.7 ms granularity, imperceptible for live playing. Off the hot path.
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        let when = self.blocks * BLOCK_LEN as u64;
        self.events
            .push(when, self.voice_ev, EventMessage::NoteOn { note, velocity });
    }

    /// Release `note`. Stamped at the block about to render, like [`note_on`](Self::note_on).
    pub fn note_off(&mut self, note: u8) {
        let when = self.blocks * BLOCK_LEN as u64;
        self.events
            .push(when, self.voice_ev, EventMessage::NoteOff { note });
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

    /// Peak `|host sample|` over `n` rendered quanta — the native stand-in for "is it making sound".
    fn peak_over(engine: &mut RtEngine, n: usize) -> f32 {
        let mut peak = 0.0_f32;
        for _ in 0..n {
            engine.render_quantum();
            peak = engine.host.iter().fold(peak, |p, &s| p.max(s.abs()));
        }
        peak
    }

    /// Story 3.3: the engine starts **silent** (no internal note) and sounds only on
    /// [`note_on`](RtEngine::note_on). Guards both that the repeating-note demo is gone and that the
    /// event setter actually feeds the voice. The browser run proves it is *audible*; this asserts
    /// silence-then-output natively.
    #[test]
    fn rt_engine_silent_until_note_on() {
        let mut engine = RtEngine::new();
        // No note yet: a few quanta should be effectively silent (only sub-LSB dither/transient).
        let idle = peak_over(&mut engine, 8);
        assert!(
            idle < 0.01,
            "expected near-silence before note_on, got {idle}"
        );
        // Strike a note, then render past the capture FIR latency into the sustain.
        engine.note_on(NOTE, 100);
        let sounding = peak_over(&mut engine, 32);
        assert!(
            sounding > 0.05,
            "expected audible output after note_on, got {sounding}"
        );
    }

    /// The `set_level` knob reaches the voice: a low level produces a quieter output than the
    /// default for the same note. Confirms a param setter moves the smoother and the change is read
    /// in `process` (drained via `process_io`).
    #[test]
    fn rt_engine_level_setter_scales_output() {
        let mut loud = RtEngine::new();
        loud.note_on(NOTE, 100);
        let loud_peak = peak_over(&mut loud, 64);

        let mut quiet = RtEngine::new();
        quiet.set_level(0.2); // glides from the 1.0 default down over ~5 ms
        quiet.note_on(NOTE, 100);
        let quiet_peak = peak_over(&mut quiet, 64);

        assert!(
            quiet_peak < 0.5 * loud_peak,
            "lower LEVEL should be clearly quieter: quiet {quiet_peak} vs loud {loud_peak}"
        );
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
}
