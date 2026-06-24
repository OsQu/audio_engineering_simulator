// Appended after the wasm-bindgen `--target no-modules` glue (which defines the global
// `wasm_bindgen`) by build-wasm.sh — the served file is public/processor.js, not this one. Do not
// load this file directly.
//
// Story 3.2, Phase B: the AudioWorklet that drains `RtEngine` one quantum at a time. The wasm bytes
// arrive via `processorOptions` (AudioWorkletGlobalScope has no reliable `fetch`); we compile +
// instantiate them synchronously in the constructor, construct the engine, then read its captured
// host block zero-copy via a single Float32Array view over wasm linear memory. No init message and
// no ready/error handshake — the engine is live before the first process() call.

class RtProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.ready = false;
    // A throw here aborts construction and fires `onprocessorerror` on the main thread (with no
    // detail), so catch and post the message back ourselves for a legible status line.
    try {
      const bytes = options?.processorOptions?.bytes;
      if (!bytes) throw new Error("no wasm bytes in processorOptions");
      // `wasm_bindgen` and `RtEngine` come from the glue concatenated ahead of this file.
      if (typeof wasm_bindgen === "undefined") {
        throw new Error("wasm_bindgen global missing — glue not concatenated ahead of processor");
      }
      // initSync compiles the bytes synchronously here (allowed off the main thread, any size) via
      // `new WebAssembly.Module(bytes)` + instantiate.
      const wasm = wasm_bindgen.initSync({ module: bytes });
      if (!wasm || !wasm.memory) throw new Error("initSync returned no memory export");
      if (typeof wasm_bindgen.RtEngine !== "function") throw new Error("RtEngine missing from glue");

      this.memory = wasm.memory;
      this.engine = new wasm_bindgen.RtEngine();
      this.len = this.engine.out_len(); // host samples per quantum (= 128 = the render quantum)
      this.view = new Float32Array(this.memory.buffer, this.engine.out_ptr(), this.len);
      this.ready = true;
      this.port.postMessage({ type: "ready", len: this.len });
    } catch (err) {
      this.port.postMessage({
        type: "error",
        message: String((err && err.message) || err),
        stack: err && err.stack ? String(err.stack) : null,
      });
    }
  }

  process(_inputs, outputs) {
    if (!this.ready) return true; // emit silence until the engine is initialized; stay alive

    this.engine.render_quantum();

    // Re-acquire the view only if wasm memory grew and detached the backing buffer. The zero-alloc
    // hot path should never trigger this; it's a cheap guard, not an expected path.
    if (this.view.length !== this.len) {
      this.view = new Float32Array(this.memory.buffer, this.engine.out_ptr(), this.len);
    }

    // One engine block == one render quantum (1024 analog ÷ M=8 = 128 host samples = 128 frames),
    // so the captured block maps 1:1 onto the output. Duplicate the mono block to every channel.
    const out = outputs[0];
    for (let ch = 0; ch < out.length; ch++) {
      out[ch].set(this.view);
    }
    return true;
  }
}

registerProcessor("rt-processor", RtProcessor);
