// Appended after the wasm-bindgen `--target no-modules` glue (which defines the global `wasm_bindgen`)
// by build-wasm.sh — the served file is public/processor.js, not this one. Do not load this directly.
//
// The AudioWorklet that drains `SceneEngine` one quantum at a time. The wasm bytes
// AND the runnable patch arrive via `processorOptions` (AudioWorkletGlobalScope has no reliable
// `fetch`); we instantiate the module and build `SceneEngine(patch)` synchronously in the constructor,
// then read its captured host block zero-copy via one Float32Array view over wasm linear memory.
// Control is generic, by device id; a `loadPatch` message hot-swaps the running scene.

// Post a real-time-health snapshot ~4×/second (≈ 96 of the 375 quanta/s at 128 frames /
// 48 kHz). Cheap and responsive; far below any rate where the postMessage itself would matter.
const HEALTH_REPORT_EVERY = 96;

// Post the output peak ~47×/second (every 8 quanta) to drive the master VU meter — smooth enough for
// a moving level, still negligible postMessage traffic. The peak is the max |sample| since the last
// report (the meter widget smooths the rest via a CSS transition).
const LEVEL_REPORT_EVERY = 8;

// Post the device meter readings on the same ~47×/second cadence as the master level — a handful of
// scalars (each meter node's VU/dBu/dBFS), read from the engine's node→host readout lane.
const READOUT_REPORT_EVERY = 8;

// Drain each DAW track's recorded-PCM ring to the host ~every 4 quanta (≈11 ms — well within the ring's
// ~64-block slack, so it never overflows). Bytes flow worklet → main → OPFS worker; the audio thread
// never touches disk. Transport state (playhead/rolling/recording) rides the readout cadence.
const RECORD_DRAIN_EVERY = 4;

// The per-instance device descriptors, keyed by scene device id: each device described against *its
// own* config, so a config-driven face (the computer's usb_sends/usb_returns) reports its actual shape
// (channels/params/readouts), not the type catalog's default. Recomputed whenever the scene builds or
// hot-swaps. `config` is optional in the IR — default to [] (every key falls back to its default).
function deviceDescriptors(devices) {
  if (typeof wasm_bindgen.describe_device !== "function")
    throw new Error("describe_device missing from glue");
  const map = {};
  for (const device of devices) {
    map[device.id] = wasm_bindgen.describe_device(device.typeId, device.config ?? []);
  }
  return map;
}

class SceneProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.ready = false;
    // A throw here aborts construction and fires `onprocessorerror` on the main thread (with no
    // detail), so catch and post the message back ourselves for a legible status line.
    try {
      const bytes = options?.processorOptions?.bytes;
      const patch = options?.processorOptions?.patch;
      if (!bytes) throw new Error("no wasm bytes in processorOptions");
      if (!patch) throw new Error("no patch in processorOptions");
      // `wasm_bindgen` and `SceneEngine` come from the glue concatenated ahead of this file.
      if (typeof wasm_bindgen === "undefined") {
        throw new Error("wasm_bindgen global missing — glue not concatenated ahead of processor");
      }
      // initSync compiles the bytes synchronously here (allowed off the main thread, any size).
      const wasm = wasm_bindgen.initSync({ module: bytes });
      if (!wasm?.memory) throw new Error("initSync returned no memory export");
      if (typeof wasm_bindgen.SceneEngine !== "function")
        throw new Error("SceneEngine missing from glue");

      this.memory = wasm.memory;
      // The device catalog (descriptors) is fetched here, where the wasm instance lives, and posted
      // to the page in `ready` — the main thread has no wasm instance of its own to call it on.
      if (typeof wasm_bindgen.catalog !== "function") throw new Error("catalog missing from glue");
      const catalog = wasm_bindgen.catalog();
      // The cable catalog (realistic R·C presets) ships alongside the device catalog — same reason:
      // the wasm instance lives here, and the UI's cable picker (Story 4.4) needs the presets.
      const cables =
        typeof wasm_bindgen.cable_catalog === "function" ? wasm_bindgen.cable_catalog() : [];
      // Build the engine from the scene's patch. Throws (Result→exception) on a bad patch — caught below.
      this.engine = new wasm_bindgen.SceneEngine(patch);
      this.len = this.engine.out_len(); // host samples per quantum (= 128 = the render quantum)
      this.view = new Float32Array(this.memory.buffer, this.engine.out_ptr(), this.len);
      this.ready = true;

      // Real-time health. In the single-threaded in-worklet model there is no render-ahead
      // ring to under/overflow; a glitch is instead a quantum whose compute exceeds its slot. The budget
      // is one quantum of audio (len host frames at the global `sampleRate`, ≈ 2.67 ms). performance.now()
      // is the only sub-quantum clock here; if it's missing we simply don't time (counters stay 0).
      this.budgetMs = (this.len / sampleRate) * 1000;
      this.canTime = typeof performance !== "undefined" && typeof performance.now === "function";
      this.overruns = 0; // quanta whose render exceeded the budget (all-time)
      this.maxMs = 0; // worst single-quantum render time seen (all-time)
      this.quanta = 0; // quanta rendered, for the report throttle
      this.levelPeak = 0; // max |sample| since the last level report, for the VU meter
      this.lossesDirty = false; // re-send connection losses after a hot-swap installs the new scene
      // Device ids of the current scene, for the DAW drain/report loop. A device is a DAW iff its
      // live `track_count > 0`, so non-DAW devices fall out naturally (no computer-typeId coupling).
      this.deviceIds = patch.devices.map((dev) => dev.id);

      // Live control. The main thread posts generic, device-addressed messages; we forward
      // them onto the engine, which only enqueues (latest-wins target / timestamped event), applied by
      // the next render_quantum's process_io drain — so this is off the hot path. A `loadPatch` builds a
      // new scene off-block (the compile happens here, between quanta) and queues it; render_quantum
      // installs it at the next block boundary.
      this.port.onmessage = (e) => {
        const d = e.data;
        if (!this.ready || !d) return;
        switch (d.type) {
          case "param":
            this.engine.set_param(d.device, d.paramId, d.value);
            break;
          case "noteOn":
            this.engine.note_on(d.device, d.note, d.velocity);
            break;
          case "noteOff":
            this.engine.note_off(d.device, d.note);
            break;
          // --- DAW transport + tracks (a `computer` device), off the hot path (enqueue/immediate) ---
          case "transport":
            if (d.action === "play") this.engine.transport_play(d.device);
            else if (d.action === "stop") this.engine.transport_stop(d.device);
            else if (d.action === "recordOn") this.engine.transport_record_enable(d.device, true);
            else if (d.action === "recordOff") this.engine.transport_record_enable(d.device, false);
            break;
          case "seek":
            this.engine.transport_seek(d.device, d.pos);
            break;
          case "trackInput":
            this.engine.set_track_input(d.device, d.track, d.lane);
            break;
          case "trackArm":
            this.engine.set_track_armed(d.device, d.track, d.armed);
            break;
          case "trackMonitor":
            this.engine.set_track_monitoring(d.device, d.track, d.on);
            break;
          case "trackLevel":
            this.engine.set_track_level(d.device, d.track, d.level);
            break;
          case "feedPlayback":
            // Push a chunk of playback PCM into the track's ring; the engine consumes it in
            // render_quantum. Main tops up by playhead occupancy, so a full-ring reject is not expected.
            this.engine.feed_playback(d.device, d.track, d.bytes);
            break;
          case "loadPatch":
            try {
              this.engine.load_patch(d.patch); // throws on a bad patch; the live scene keeps running
              this.deviceIds = d.patch.devices.map((dev) => dev.id); // for the DAW drain/report loop
              // The new scene's connection losses go live at the next render_quantum swap; flag a
              // post-swap re-send so the page's static readouts (cable inspector / levels panel) refresh.
              this.lossesDirty = true;
              // The new scene may have resized a config-driven face (e.g. the computer re-enumerated to
              // an attached interface), so re-push the per-instance descriptors for the faceplates.
              this.port.postMessage({
                type: "deviceDescriptors",
                deviceDescriptors: deviceDescriptors(d.patch.devices),
              });
            } catch (err) {
              this.port.postMessage({
                type: "error",
                message: String(err?.message || err),
              });
            }
            break;
        }
      };

      // Report the engine's fixed signal-path group delay (AD + DA + capture FIRs) so the page can sum
      // it with the browser's measured base/output latency into an end-to-end figure.
      this.port.postMessage({
        type: "ready",
        len: this.len,
        signalPathLatencyMs: this.engine.signal_path_latency_ms,
        catalog,
        cables,
        // Static per-connection loading loss (dB, or null for digital) for the initial scene.
        losses: this.engine.connection_losses(),
        // Per-instance device descriptors (by scene id), sized to each device's config.
        deviceDescriptors: deviceDescriptors(patch.devices),
      });
    } catch (err) {
      this.port.postMessage({
        type: "error",
        message: String(err?.message || err),
        stack: err?.stack ? String(err.stack) : null,
      });
    }
  }

  process(_inputs, outputs) {
    if (!this.ready) return true; // emit silence until the engine is initialized; stay alive

    // Time the render against its budget — this is the "underrun" of the single-threaded model.
    const t0 = this.canTime ? performance.now() : 0;
    this.engine.render_quantum();
    if (this.canTime) {
      const dt = performance.now() - t0;
      if (dt > this.maxMs) this.maxMs = dt;
      if (dt > this.budgetMs) this.overruns++;
    }
    this.quanta++;

    // Re-acquire the view only if wasm memory grew and detached the backing buffer. The zero-alloc hot
    // path should never trigger this; it's a cheap guard, not an expected path.
    if (this.view.length !== this.len) {
      this.view = new Float32Array(this.memory.buffer, this.engine.out_ptr(), this.len);
    }

    // One engine block == one render quantum (1024 analog ÷ M=8 = 128 host samples = 128 frames), so
    // the captured block maps 1:1 onto the output. Duplicate the mono block to every channel.
    const out = outputs[0];
    for (let ch = 0; ch < out.length; ch++) {
      out[ch].set(this.view);
    }

    // Track the output peak for the VU meter, posting it on a throttle (the widget smooths the decay).
    for (let i = 0; i < this.view.length; i++) {
      const a = Math.abs(this.view[i]);
      if (a > this.levelPeak) this.levelPeak = a;
    }
    if (this.quanta % LEVEL_REPORT_EVERY === 0) {
      this.port.postMessage({ type: "level", peak: this.levelPeak });
      this.levelPeak = 0;
    }

    // Device meter readings (node→host readout lane), same cadence — keyed by device id so the page
    // routes each to its panel's meter screen. Survives hot-swaps: the snapshot reads the live scene.
    if (this.quanta % READOUT_REPORT_EVERY === 0) {
      this.port.postMessage({ type: "readouts", readings: this.engine.readouts() });
    }

    // DAW record drain: hand each capturing track's freshly-recorded PCM to the host to append to its
    // take file. The record rings only hold bytes while a track is capturing, so idle tracks (and
    // non-DAW devices, track_count 0) drain nothing. Bytes are transferred, not copied.
    if (this.quanta % RECORD_DRAIN_EVERY === 0) {
      for (const device of this.deviceIds) {
        const tracks = this.engine.track_count(device);
        for (let t = 0; t < tracks; t++) {
          const bytes = this.engine.drain_record(device, t);
          if (bytes.length > 0) {
            this.port.postMessage({ type: "recorded", device, track: t, bytes }, [bytes.buffer]);
          }
        }
      }
    }

    // DAW transport report: playhead + rolling/recording per DAW device, on the readout cadence, so the
    // UI can animate the playhead and light the transport buttons.
    if (this.quanta % READOUT_REPORT_EVERY === 0) {
      const states = [];
      for (const device of this.deviceIds) {
        if (this.engine.track_count(device) > 0) {
          states.push({
            device,
            playhead: this.engine.playhead(device),
            rolling: this.engine.is_rolling(device),
            recording: this.engine.is_recording(device),
          });
        }
      }
      if (states.length > 0) this.port.postMessage({ type: "transports", states });
    }

    // A hot-swap just installed a new scene (render_quantum above did the swap): re-send its static
    // connection losses once so the cable inspector / levels panel refresh, then clear the flag.
    if (this.lossesDirty) {
      this.port.postMessage({ type: "losses", losses: this.engine.connection_losses() });
      this.lossesDirty = false;
    }

    // Throttled health snapshot: the compute-budget side (overruns / worst render) plus the engine's
    // input-flood side (queue drops). Both are running totals so the page can show drift over a session.
    if (this.quanta % HEALTH_REPORT_EVERY === 0) {
      this.port.postMessage({
        type: "health",
        quanta: this.quanta,
        overruns: this.overruns,
        maxMs: this.maxMs,
        budgetMs: this.budgetMs,
        eventDrops: this.engine.event_drops(),
        paramDrops: this.engine.param_drops(),
      });
    }
    return true;
  }
}

registerProcessor("scene-processor", SceneProcessor);
