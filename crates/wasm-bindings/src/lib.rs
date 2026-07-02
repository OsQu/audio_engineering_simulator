//! Browser/WASM bindings for the engine.
//!
//! This crate is the JS-facing surface that hosts the engine in the browser, built to
//! `wasm32-unknown-unknown` with `wasm-bindgen` (via `wasm-pack`). See the crate `README.md` for
//! the build + install commands.
//!
//! **Scope:** [`SceneEngine`] is the real-time surface the AudioWorklet actually drains —
//! [`SceneEngine::render_quantum`] renders exactly one quantum (one engine block) into an
//! engine-owned host buffer, exposed **zero-copy** via [`SceneEngine::out_ptr`] /
//! [`out_len`](SceneEngine::out_len): JS builds one `Float32Array` view over WASM linear memory and
//! reads it every quantum, with no per-quantum marshalling. Returning a pointer is safe Rust
//! (`as_ptr`); the only `unsafe` is JS-side constructing the view — so there is still no `unsafe`
//! in this crate. It is **scene-driven**: built from a serialized [`Patch`] via the `devices` crate,
//! controlled **generically by device id**, and **hot-swapped** to a new scene at a block boundary.

use capture::Capture;
use devices::{BuildError, BuiltScene, Patch, build_patch, cable_types, descriptors};
use engine::{
    AnalogRate, EventMessage, EventQueue, ParamHandle, ParamQueue, SampleRate, VoltageBuffer,
};
use wasm_bindgen::prelude::*;

// --- Device catalog + scene ingress: the thin JS-value bridge over the `devices` crate. ----------

/// The device catalog as a structured JS value — what the UI fetches once to populate the gear
/// browser and drive panel rendering. Pure marshalling over [`devices::descriptors`]; the catalog
/// content lives in `devices`. Cold path (UI startup), so the serialize cost is irrelevant.
///
/// # Errors
/// Returns the serializer error as a `JsValue` if serialization fails (it does not in practice — the
/// descriptors are plain data).
#[wasm_bindgen]
pub fn catalog() -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&descriptors()).map_err(Into::into)
}

/// The cable catalog as a structured JS value — the realistic cable presets the UI offers when wiring
/// an analog connection (Story 4.4). Pure marshalling over [`devices::cable_types`]; the R·C content
/// lives in `devices`. Cold path (UI startup), like [`catalog`].
///
/// # Errors
/// Returns the serializer error as a `JsValue` if serialization fails (it does not in practice — the
/// cable types are plain data).
#[wasm_bindgen]
pub fn cable_catalog() -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&cable_types()).map_err(Into::into)
}

/// Deserialize a runnable [`Patch`] from the structured JS object the UI posts to the worklet.
///
/// The fallible ingress: a malformed patch returns `Err` rather than panicking on the audio thread.
/// Marshalling only — the IR and the build logic live in `devices`.
pub fn parse_patch(value: JsValue) -> Result<Patch, serde_wasm_bindgen::Error> {
    serde_wasm_bindgen::from_value(value)
}

// --- The pinned scene config (block length / rates / seed the SceneEngine compiles every scene at). --
/// Oversampled analog rate — the "continuous" proxy (8× the host rate).
const ANALOG_RATE_HZ: f64 = 384_000.0;
/// Host / converter sample rate. `M = ANALOG/HOST = 8`.
const HOST_RATE_HZ: f64 = 48_000.0;
/// Samples per `process` block. The **real-time quantum**: 128 host frames × M = 1024 analog
/// samples (deliberately *not* the offline harness's 384 — this is the size the AudioWorklet
/// actually calls with).
const BLOCK_LEN: usize = 1024;
/// Fixed seed (determinism) and monitor full-scale reference (speaker volts → ±1.0).
const SEED: u64 = 0;
const FULL_SCALE_V: f32 = 1.0;
/// A4 (440 Hz) — the note the tests strike.
#[cfg(test)]
const NOTE: u8 = 69;

// --- The real-time surface the AudioWorklet drains. ----------------------------------------------

/// Capacity of the latest-wins [`ParamQueue`] — distinct controllable params pending within one
/// quantum. Generous for a small studio (an initial scene load pushes all overridden knobs at once);
/// a scene exceeding it drops the surplus (counted in `param_drops`). Revisit at scale.
const PARAM_QUEUE_CAP: usize = 256;
/// Event-queue capacity. Generous for human-rate input within one ~2.7 ms quantum; every queued
/// event drains the very next [`render_quantum`](SceneEngine::render_quantum) so it never fills.
const EVENT_QUEUE_CAP: usize = 64;

/// A scene compiled off-block and waiting to go live, plus the initial param values to apply once it
/// is installed (resolved against the *new* scene's handles, so they're applied after the swap).
struct Pending {
    scene: BuiltScene,
    initial: Vec<(ParamHandle, f32)>,
}

/// The real-time engine surface: a scene built from a serialized
/// [`Patch`] (via the `devices` crate) plus the implicit [`Capture`], driven **one AudioWorklet quantum
/// at a time** with its captured host block exposed **zero-copy** to JS, **played and tweaked live**
/// by device id, and **hot-swapped** to a new scene at a block boundary.
///
/// This is the surface the worklet drains every callback:
/// [`render_quantum`](Self::render_quantum) renders exactly one block into an engine-owned host buffer,
/// and [`out_ptr`](Self::out_ptr) / [`out_len`](Self::out_len) let JS build a single `Float32Array`
/// view over WASM linear memory and read it each quantum — no marshalling, no per-quantum allocation.
/// The view stays valid for the session because the hot path is zero-alloc, so linear memory never
/// `grow`s mid-render to detach it.
///
/// **Scene-driven control.** Built from a [`Patch`] by [`new`](Self::new); replaced live by
/// [`load_patch`](Self::load_patch) (compile off-block → install at the next block boundary). It owns a
/// [`ParamQueue`] + [`EventQueue`] and exposes **generic** control: [`set_param`](Self::set_param) by
/// `(device id, param id)` pushes latest-wins target values (the engine's own `Smoother` de-zippers
/// them — so *not* `AudioParam`), and [`note_on`](Self::note_on) / [`note_off`](Self::note_off) by
/// device id push timestamped events. [`render_quantum`](Self::render_quantum) drains both lanes via
/// `process_io` each block. A note is stamped at the **block about to render** (`blocks · BLOCK_LEN`) —
/// "play at the next quantum," ~2.7 ms granularity. (Precise `currentTime`→sample mapping matters only
/// for *sequenced* MIDI, which isn't modeled yet.) Addressing resolves through the live scene's
/// [`BuiltScene`] maps, so
/// it stays correct across a hot-swap.
#[wasm_bindgen]
pub struct SceneEngine {
    /// The live scene: compiled schedule + control resolution by device id.
    current: BuiltScene,
    /// A scene built off-block by [`load_patch`](Self::load_patch), installed at the next
    /// [`render_quantum`](Self::render_quantum) boundary (where the old one is dropped off-block).
    pending: Option<Pending>,
    capture: Capture,
    /// Pending control input, drained each `render_quantum` via `process_io`. Params are latest-wins
    /// target values; events are note-on/off stamped at the current block.
    params: ParamQueue,
    events: EventQueue,
    /// Per-quantum scratch: the speaker-voltage tap (analog rate) and the captured host samples.
    out: VoltageBuffer,
    host: Vec<f32>,
    /// Quanta rendered so far. `blocks * BLOCK_LEN` is the absolute analog-sample time of the next
    /// block's first sample — the timeline [`note_on`](Self::note_on) / [`note_off`](Self::note_off)
    /// stamp against.
    blocks: u64,
    /// Real-time-health counter: control **events dropped** because the queue was full — an
    /// input flood arriving faster than the audio thread drains it. The page polls it via
    /// [`event_drops`](Self::event_drops). The compute-budget side of health (a quantum overrunning its
    /// ~2.7 ms slot) is timed **JS-side** in the worklet, because the engine is deterministic and
    /// clock-free (no ambient `Instant`/`SystemTime`, per the determinism rule).
    event_drops: u32,
    /// Param updates dropped because the queue was full (latest-wins coalesces per param, so rare —
    /// only a very large scene's initial load could approach [`PARAM_QUEUE_CAP`]).
    param_drops: u32,
}

/// The (handle, value) pairs to apply for a scene's saved param values: each device's `ParamSetting`s
/// resolved against the built scene. Unknown ids are skipped — a forward/backward-compatible patch may
/// name a param this build doesn't have.
fn initial_params(patch: &Patch, scene: &BuiltScene) -> Vec<(ParamHandle, f32)> {
    let mut out = Vec::new();
    for device in &patch.devices {
        for setting in &device.params {
            if let Some(handle) = scene.param(&device.id, setting.id) {
                out.push((handle, setting.value));
            }
        }
    }
    out
}

// Native (Rust-only) construction + scene management — the wasm surface below parses a JS patch and
// then calls these, and the tests use them directly (a `JsValue` needs a JS realm).
impl SceneEngine {
    /// Build the engine for an initial scene, applying the patch's saved param values so it matches the
    /// scene from the first block. The fallible, allocating step (build + compile) — off the audio path.
    ///
    /// # Errors
    /// A [`BuildError`] if the patch can't be assembled or compiled.
    pub(crate) fn from_patch(patch: &Patch) -> Result<Self, BuildError> {
        let analog_rate = AnalogRate::new(ANALOG_RATE_HZ);
        let host_rate = SampleRate::new(HOST_RATE_HZ);
        let current = Self::build_scene(patch)?;

        let capture = Capture::new(analog_rate, host_rate, FULL_SCALE_V);
        let host = vec![0.0_f32; capture.host_len(BLOCK_LEN)];
        let mut params = ParamQueue::with_capacity(PARAM_QUEUE_CAP);
        for (handle, value) in initial_params(patch, &current) {
            let _ = params.set(handle, value); // applied on the first render_quantum
        }

        Ok(Self {
            current,
            pending: None,
            capture,
            params,
            events: EventQueue::with_capacity(EVENT_QUEUE_CAP),
            out: VoltageBuffer::zeros(BLOCK_LEN, analog_rate),
            host,
            blocks: 0,
            event_drops: 0,
            param_drops: 0,
        })
    }

    /// Compile a patch into a [`BuiltScene`] at the pinned block length / analog rate / seed (so the
    /// same scene reproduces bit-for-bit).
    fn build_scene(patch: &Patch) -> Result<BuiltScene, BuildError> {
        build_patch(patch, BLOCK_LEN, AnalogRate::new(ANALOG_RATE_HZ), SEED)
    }

    /// Build a replacement scene off-block and queue it for install at the next block boundary.
    ///
    /// # Errors
    /// A [`BuildError`] if the patch can't be assembled or compiled (the live scene keeps running).
    pub(crate) fn queue_patch(&mut self, patch: &Patch) -> Result<(), BuildError> {
        let scene = Self::build_scene(patch)?;
        let initial = initial_params(patch, &scene);
        self.pending = Some(Pending { scene, initial });
        Ok(())
    }

    /// Enqueue a latest-wins param target, counting a drop if the queue was full. The setter routes
    /// through here so the drop accounting lives in one place.
    fn push_param(&mut self, handle: ParamHandle, value: f32) {
        if !self.params.set(handle, value) {
            self.param_drops = self.param_drops.saturating_add(1);
        }
    }

    /// The live scene's meter readings: `(device id, values in readout-id order)` from the last block
    /// rendered. The native form behind the wasm [`readouts`](Self::readouts) getter (a `JsValue`
    /// needs a JS realm), so tests can assert readings without one.
    pub(crate) fn readout_readings(&self) -> Vec<(String, Vec<f32>)> {
        self.current.readout_snapshot()
    }
}

#[wasm_bindgen]
impl SceneEngine {
    /// Build and compile the engine from an initial scene `patch` (a structured JS object). Starts
    /// **silent** — notes arrive via [`note_on`](Self::note_on).
    ///
    /// # Errors
    /// Throws (Err → a JS exception) with a legible message if the patch can't be parsed or built.
    #[wasm_bindgen(constructor)]
    pub fn new(patch: JsValue) -> Result<SceneEngine, JsValue> {
        let patch =
            parse_patch(patch).map_err(|e| JsValue::from_str(&format!("invalid patch: {e}")))?;
        Self::from_patch(&patch).map_err(|e| JsValue::from_str(&format!("build failed: {e}")))
    }

    /// Replace the running scene. Compiles the new `patch` **off-block** (here, between quanta) and
    /// queues it; the swap installs at the next [`render_quantum`](Self::render_quantum) boundary,
    /// dropping the old scene there. This is the structural-edit path — value-only knob changes go
    /// through [`set_param`](Self::set_param) with no recompile.
    ///
    /// # Errors
    /// Throws with a legible message if the patch can't be parsed or built; the live scene keeps running.
    pub fn load_patch(&mut self, patch: JsValue) -> Result<(), JsValue> {
        let patch =
            parse_patch(patch).map_err(|e| JsValue::from_str(&format!("invalid patch: {e}")))?;
        self.queue_patch(&patch)
            .map_err(|e| JsValue::from_str(&format!("build failed: {e}")))
    }

    /// Render exactly one AudioWorklet quantum — one engine block (`BLOCK_LEN` analog samples) →
    /// `BLOCK_LEN / M` host samples — into the engine-owned host buffer.
    ///
    /// First **installs a queued scene** if one is pending: swap it in at this block boundary (the old
    /// scene — and its schedule — drops here, between blocks, off the per-sample path), clear the stale
    /// control queues (their handles index the old scene's stores), and apply the new scene's saved
    /// param values. Then drains both control lanes and renders one block. The steady path (no pending
    /// scene) is zero-alloc; read the result via [`out_ptr`](Self::out_ptr) / [`out_len`](Self::out_len).
    pub fn render_quantum(&mut self) {
        if let Some(pending) = self.pending.take() {
            self.current = pending.scene; // old BuiltScene (+ schedule) dropped here, off-block
            self.params.clear();
            self.events.clear();
            // The fresh schedule's event clock (`sample_pos`) restarts at 0, so the note-stamping clock
            // must restart with it — `note_on` stamps `blocks * BLOCK_LEN`, which has to track the
            // schedule's elapsed samples. Without this reset, a note after a deep-session swap lands
            // tens of thousands of samples in the new schedule's future (a multi-second firing lag).
            self.blocks = 0;
            for (handle, value) in pending.initial {
                let _ = self.params.set(handle, value);
            }
        }
        self.current
            .schedule_mut()
            .process_io(&mut self.out, &mut self.params, &mut self.events);
        self.capture.process(self.out.as_slice(), &mut self.host);
        self.blocks += 1;
    }

    // --- Live control: generic, by device id — resolved through the live scene. --------------------

    /// Set a smoothed control param by **device id + device-level param id** to `value` (latest-wins;
    /// the engine's `Smoother` de-zippers it — so *not* `AudioParam`). A no-op if the device/param is
    /// unknown in the live scene. Off the hot path — JS calls it on slider input.
    pub fn set_param(&mut self, device: &str, param_id: u32, value: f32) {
        if let Some(handle) = self.current.param(device, param_id) {
            self.push_param(handle, value);
        }
    }

    /// Strike `note` (MIDI 0..=127) at `velocity` on `device`'s event input. Stamped at the **block
    /// about to render** (`blocks · BLOCK_LEN`) so it fires on that quantum's first sample — "play at
    /// the next quantum," ~2.7 ms granularity. A no-op if the device has no event input; an overflow of
    /// a full queue is dropped and counted in [`event_drops`](Self::event_drops). Off the hot path.
    pub fn note_on(&mut self, device: &str, note: u8, velocity: u8) {
        if let Some(ev) = self.current.event_input(device) {
            let when = self.blocks * BLOCK_LEN as u64;
            if !self
                .events
                .push(when, ev, EventMessage::NoteOn { note, velocity })
            {
                self.event_drops = self.event_drops.saturating_add(1);
            }
        }
    }

    /// Release `note` on `device`. Stamped at the block about to render, like [`note_on`](Self::note_on);
    /// a no-op if the device has no event input.
    pub fn note_off(&mut self, device: &str, note: u8) {
        if let Some(ev) = self.current.event_input(device) {
            let when = self.blocks * BLOCK_LEN as u64;
            if !self.events.push(when, ev, EventMessage::NoteOff { note }) {
                self.event_drops = self.event_drops.saturating_add(1);
            }
        }
    }

    /// Control **events dropped** because the queue was full, since construction — a real-time-health
    /// counter the page polls each report. Zero under normal play; climbs only under an input flood
    /// the audio thread can't drain in time (exactly the case a lock-free SAB ring buffer would
    /// address if it ever bites — this counter is the evidence that would justify building one).
    #[must_use]
    pub fn event_drops(&self) -> u32 {
        self.event_drops
    }

    /// Param updates dropped because the queue was full. ~Always 0 (latest-wins coalesces to one slot
    /// per knob); exposed for symmetry with [`event_drops`](Self::event_drops).
    #[must_use]
    pub fn param_drops(&self) -> u32 {
        self.param_drops
    }

    /// The live scene's meter readings as a structured JS value — an array of `[deviceId, values]`
    /// pairs (values in readout-id order), from the last block rendered. The worklet polls this on a
    /// throttle and posts it to the page to drive device meter screens. Off the hot path; tiny (a few
    /// scalars), so the per-poll serialize is negligible.
    ///
    /// # Errors
    /// Returns the serializer error as a `JsValue` if serialization fails (it does not in practice).
    pub fn readouts(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.readout_readings()).map_err(Into::into)
    }

    /// The live scene's per-connection **loading loss** in dB as a JS value — an array indexed by
    /// scene connection order, each entry a number or `null` (digital/event connections have no
    /// resistive loading). Static (changes only on a structural edit), so the worklet ships it in
    /// `ready` and again after a hot-swap, not per frame.
    ///
    /// # Errors
    /// Returns the serializer error as a `JsValue` if serialization fails (it does not in practice).
    pub fn connection_losses(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(self.current.connection_losses()).map_err(Into::into)
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

    /// The engine's fixed **signal-path group delay** in milliseconds — the modeled AD + DA FIRs plus
    /// the implicit [`Capture`] decimator (all linear-phase). One component of the round-trip latency
    /// the page reports; the dominant terms (the browser's `baseLatency` / `outputLatency`) are
    /// measured JS-side, and the note-stamping quantum (~2.7 ms) is the input-side granularity. A
    /// constant for the pinned patch (three matched 161-tap FIRs at 384 kHz ≈ 0.625 ms). Off the hot path.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn signal_path_latency_ms(&self) -> f64 {
        let samples =
            self.current.schedule().group_delay_samples() + self.capture.group_delay_samples();
        samples / ANALOG_RATE_HZ * 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SceneEngine: built from a scene, controlled generically, hot-swappable. -------------------
    use devices::{Connection, DeviceInstance, ParamSetting, Patch, PortRef};

    fn dev(id: &str, type_id: &str) -> DeviceInstance {
        DeviceInstance {
            id: id.into(),
            type_id: type_id.into(),
            params: vec![],
        }
    }

    fn conn(from: &str, from_port: u32, to: &str, to_port: u32) -> Connection {
        Connection {
            from: PortRef {
                device: from.into(),
                port: from_port,
            },
            to: PortRef {
                device: to.into(),
                port: to_port,
            },
            cable: None,
        }
    }

    /// The canonical patch (`synth → AD → DA → speaker`) as a scene.
    fn canonical_patch() -> Patch {
        Patch {
            devices: vec![
                dev("synth", "synth_voice"),
                dev("ad", "ad_converter"),
                dev("da", "da_converter"),
                dev("spk", "speaker"),
            ],
            connections: vec![
                conn("synth", 0, "ad", 0),
                conn("ad", 0, "da", 0),
                conn("da", 0, "spk", 0),
            ],
            output: PortRef {
                device: "spk".into(),
                port: 0,
            },
        }
    }

    /// Peak `|host sample|` over `n` rendered quanta — the native stand-in for "is it making sound".
    fn peak_over(engine: &mut SceneEngine, n: usize) -> f32 {
        let mut peak = 0.0_f32;
        for _ in 0..n {
            engine.render_quantum();
            peak = engine.host.iter().fold(peak, |p, &s| p.max(s.abs()));
        }
        peak
    }

    /// Peak `|host sample|` over the **tail** (quanta `skip..n`) — used when a param is gliding to its
    /// target over the first few blocks and only the settled steady state is meaningful.
    fn peak_tail(engine: &mut SceneEngine, n: usize, skip: usize) -> f32 {
        let mut peak = 0.0_f32;
        for block in 0..n {
            engine.render_quantum();
            if block >= skip {
                peak = engine.host.iter().fold(peak, |p, &s| p.max(s.abs()));
            }
        }
        peak
    }

    /// The scene-built engine starts **silent** and sounds only on [`note_on`](SceneEngine::note_on),
    /// addressed by device id. The browser run proves it is *audible*; this asserts silence-then-output
    /// natively.
    #[test]
    fn scene_engine_silent_until_note_on() {
        let mut engine =
            SceneEngine::from_patch(&canonical_patch()).expect("canonical patch builds");
        let idle = peak_over(&mut engine, 8);
        assert!(
            idle < 0.01,
            "expected near-silence before note_on, got {idle}"
        );

        engine.note_on("synth", NOTE, 100);
        let sounding = peak_over(&mut engine, 32);
        assert!(
            sounding > 0.05,
            "expected audible output after note_on, got {sounding}"
        );
    }

    /// Generic `set_param(device, id, value)` reaches the right node: a low LEVEL is clearly quieter
    /// than the default for the same note — so `(device id, param id)` addressing lands on the smoother.
    #[test]
    fn scene_engine_param_setter_scales_output() {
        let mut loud = SceneEngine::from_patch(&canonical_patch()).expect("builds");
        loud.note_on("synth", NOTE, 100);
        let loud_peak = peak_over(&mut loud, 64);

        let mut quiet = SceneEngine::from_patch(&canonical_patch()).expect("builds");
        quiet.set_param("synth", 0, 0.2); // LEVEL (device param 0), glides from the 1.0 default
        quiet.note_on("synth", NOTE, 100);
        let quiet_peak = peak_over(&mut quiet, 64);

        assert!(
            quiet_peak < 0.5 * loud_peak,
            "lower LEVEL should be clearly quieter: quiet {quiet_peak} vs loud {loud_peak}"
        );
    }

    /// `load_patch` hot-swaps the running scene: an engine playing patch A, after loading patch B
    /// (the same chain but with the synth's LEVEL saved at 0), goes silent on the next note — proving
    /// the swap installed B *and* applied B's saved param values, resolved through the new scene.
    #[test]
    fn scene_engine_hot_swaps_to_a_loaded_patch() {
        let mut engine = SceneEngine::from_patch(&canonical_patch()).expect("A builds");
        engine.note_on("synth", NOTE, 100);
        assert!(peak_over(&mut engine, 32) > 0.05, "A should be audible");

        // Patch B: canonical, but the synth's LEVEL (device param 0) saved at 0.
        let mut b = canonical_patch();
        b.devices[0].params = vec![ParamSetting { id: 0, value: 0.0 }];
        engine.queue_patch(&b).expect("B builds");

        engine.render_quantum(); // installs B at this boundary, clears stale queues, applies LEVEL=0
        engine.note_on("synth", NOTE, 100); // resolves through B
        // LEVEL glides to 0 over ~5 ms; the settled tail is silence.
        let tail = peak_tail(&mut engine, 64, 16);
        assert!(
            tail < 0.01,
            "after loading B (LEVEL 0) the voice should be silent, got {tail}"
        );
    }

    /// Regression: after a hot-swap **deep into a session**, a note must fire *promptly* on the fresh
    /// schedule. The new schedule's event clock (`sample_pos`) restarts at 0 on install, so the
    /// note-stamping clock (`blocks`) must restart with it — otherwise a note lands tens of thousands
    /// of samples in the fresh schedule's future (the "multi-second lag after loading" bug). Renders
    /// ~50 blocks first, so an un-reset `blocks` would defer the note far past the measured window.
    #[test]
    fn scene_engine_note_fires_promptly_after_deep_swap() {
        let mut engine = SceneEngine::from_patch(&canonical_patch()).expect("builds");
        let _ = peak_over(&mut engine, 50); // advance well past block 0

        engine
            .queue_patch(&canonical_patch())
            .expect("reload builds");
        engine.render_quantum(); // installs the fresh scene + resets the note clock
        engine.note_on("synth", NOTE, 100);

        // Audible within a handful of blocks (past the FIR latency) — not delayed by the ~50 elapsed.
        let prompt = peak_over(&mut engine, 12);
        assert!(
            prompt > 0.05,
            "note should fire promptly after a deep-session swap, got {prompt}"
        );
    }

    /// Reloading a scene leaves a working engine: after a no-op reload of the same patch (fresh
    /// schedule, zeroed node state), a new note is still audible — the swap doesn't wedge the engine.
    #[test]
    fn scene_engine_reload_keeps_engine_alive() {
        let mut engine = SceneEngine::from_patch(&canonical_patch()).expect("builds");
        engine
            .queue_patch(&canonical_patch())
            .expect("reload builds");
        engine.render_quantum(); // installs the fresh scene
        engine.note_on("synth", NOTE, 100);
        assert!(
            peak_over(&mut engine, 32) > 0.05,
            "engine still sounds after a reload"
        );
    }

    /// The event-drop health counter still holds: flooding past the queue capacity drops
    /// and counts the excess — never a panic — and the running total doesn't move once drained.
    #[test]
    fn scene_engine_counts_dropped_events_under_a_flood() {
        let mut engine = SceneEngine::from_patch(&canonical_patch()).expect("builds");
        for _ in 0..(EVENT_QUEUE_CAP + 10) {
            engine.note_on("synth", NOTE, 100);
        }
        assert_eq!(engine.event_drops(), 10);
        assert_eq!(
            engine.param_drops(),
            0,
            "params never overflow (latest-wins)"
        );

        engine.render_quantum();
        engine.note_on("synth", NOTE, 100);
        assert_eq!(
            engine.event_drops(),
            10,
            "no new drop once the queue has drained"
        );
    }

    /// The signal-path latency for the canonical chain is the three matched 161-tap converter FIRs
    /// (AD decimator + DA interpolator + capture decimator). Hand calc: each contributes
    /// (161 − 1)/2 = 80 analog samples ⇒ 240 total / 384 000 Hz = 0.625 ms.
    #[test]
    fn scene_engine_reports_signal_path_latency() {
        let engine = SceneEngine::from_patch(&canonical_patch()).expect("builds");
        assert!(
            (engine.signal_path_latency_ms() - 0.625).abs() < 1e-6,
            "expected 0.625 ms, got {}",
            engine.signal_path_latency_ms()
        );
    }

    /// The exposed buffer geometry is the pinned config, and the pointer is real — the contract JS
    /// relies on to size and place its `Float32Array` view.
    #[test]
    fn scene_engine_exposes_block_geometry() {
        let engine = SceneEngine::from_patch(&canonical_patch()).expect("builds");
        assert_eq!(engine.out_len(), 128); // 1024 / 8
        assert_eq!(engine.host_rate_hz(), 48_000.0);
        assert!(!engine.out_ptr().is_null());
    }

    /// A canonical patch with a `vu_meter` inline (`synth → vu → ad → da → spk`), tapped at the
    /// speaker.
    fn metered_patch() -> Patch {
        Patch {
            devices: vec![
                dev("synth", "synth_voice"),
                dev("vu", "vu_meter"),
                dev("ad", "ad_converter"),
                dev("da", "da_converter"),
                dev("spk", "speaker"),
            ],
            connections: vec![
                conn("synth", 0, "vu", 0),
                conn("vu", 0, "ad", 0),
                conn("ad", 0, "da", 0),
                conn("da", 0, "spk", 0),
            ],
            output: PortRef {
                device: "spk".into(),
                port: 0,
            },
        }
    }

    /// The VU device's VU reading (readout 0) from the current snapshot.
    fn vu_reading(engine: &SceneEngine) -> f32 {
        engine
            .readout_readings()
            .into_iter()
            .find(|(d, _)| d == "vu")
            .expect("vu device present in the snapshot")
            .1[0]
    }

    /// The node→host lane surfaced through `SceneEngine`: a `VuMeter` reads near its floor while idle,
    /// then climbs well above it once a note has played and the ~300 ms ballistics settle — a live
    /// reading addressed by `(device, id)`, moving with the signal.
    #[test]
    fn scene_engine_reports_meter_readings() {
        let mut engine = SceneEngine::from_patch(&metered_patch()).expect("metered patch builds");

        for _ in 0..8 {
            engine.render_quantum();
        }
        let idle = vu_reading(&engine);
        assert!(idle < -40.0, "idle VU sits near its floor, got {idle}");

        engine.note_on("synth", NOTE, 100);
        for _ in 0..600 {
            // ~1.6 s, well past the 300 ms VU ballistics
            engine.render_quantum();
        }
        let sounding = vu_reading(&engine);
        assert!(
            sounding > -20.0,
            "VU should climb with signal, got {sounding}"
        );
    }
}
