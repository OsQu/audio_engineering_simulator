// Appended after the wasm-bindgen `--target no-modules` glue (which defines the global
// `wasm_bindgen`) by rt/build.sh — the served file is rt/processor.js, not this one. Do not load
// this file directly.
//
// Story 3.2, Phase A: the AudioWorklet that drains `RtEngine` one quantum at a time. The wasm
// Module is compiled on the main thread and posted in (AudioWorkletGlobalScope has no reliable
// `fetch`); we `initSync` it here, construct the engine, then read its captured host block
// zero-copy via a single Float32Array view over wasm linear memory.

class RtProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ready = false;
    this.port.onmessage = (e) => {
      if (!e.data || e.data.type !== "init") return;
      // Exceptions here do NOT reach the main thread's try/catch — they just abort this handler and
      // the node never reports `ready` (a silent hang). So catch and post the error back instead.
      try {
        // `wasm_bindgen` and `RtEngine` come from the glue concatenated ahead of this file.
        if (typeof wasm_bindgen === "undefined") {
          throw new Error("wasm_bindgen global missing — glue not concatenated ahead of processor");
        }
        // Hand initSync the raw bytes: it compiles them synchronously here (allowed off the main
        // thread) via `new WebAssembly.Module(bytes)`. The main thread posts bytes, not a Module,
        // because a Module can't be cloned into this scope in some browsers.
        const wasm = wasm_bindgen.initSync({ module: e.data.bytes });
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
    };
    // If the init message itself fails to deserialize in this scope, `message` never fires —
    // `messageerror` does. Report it so the failure isn't silent.
    this.port.onmessageerror = () => {
      this.port.postMessage({ type: "error", message: "messageerror: init message failed to deserialize in worklet" });
    };
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
