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
          case "loadPatch":
            try {
              this.engine.load_patch(d.patch); // throws on a bad patch; the live scene keeps running
            } catch (err) {
              this.port.postMessage({ type: "error", message: String(err?.message || err) });
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
