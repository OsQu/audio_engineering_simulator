// Story 3.2, Phase A — static-page first sound (no bundler; the Vite/TS harness is Phase B / 3.2.5).
//
// Fetches the wasm bytes (the worklet scope has no reliable `fetch`) and hands them to the processor
// via `processorOptions` at node construction — the processor compiles + instantiates them itself in
// its constructor, so there is no init message and no ready/error handshake to race. Playback starts
// on a user gesture (browsers require one).
//
// Why bytes, not a compiled `WebAssembly.Module`: a Module is only structured-cloneable within one
// agent cluster, and an AudioWorklet is a separate realm — cloning it in can fail (silently, as a
// dropped message). An `ArrayBuffer` always clones, and recompiling in the worklet is the approach
// the WebKit/Emscripten guidance recommends anyway.

const statusEl = document.getElementById("status");
const startBtn = document.getElementById("start");
const setStatus = (m) => {
  statusEl.textContent = m;
};

let ctx; // created lazily on the user gesture

startBtn.addEventListener("click", async () => {
  if (ctx) return;
  startBtn.disabled = true;
  try {
    // Pin the context rate to the engine's host rate (48 kHz). If the device runs at another rate
    // the browser resamples for us; without this pin every quantum would be the wrong rate ⇒ wrong
    // pitch + drift. `latencyHint: 'interactive'` asks for the smallest cushion (tuned in 3.4).
    ctx = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
    setStatus(`AudioContext @ ${ctx.sampleRate} Hz — loading worklet…`);

    await ctx.audioWorklet.addModule("processor.js");
    setStatus("worklet module loaded — fetching wasm…");

    const bytes = await (await fetch("pkg/wasm_bindings_bg.wasm")).arrayBuffer();

    // Deliver the bytes at construction. processorOptions is structured-cloned (copied — there is no
    // transfer list on this constructor), so the ~410 KB is copied once at setup; negligible and off
    // the audio hot path. The processor compiles them synchronously in its constructor.
    const node = new AudioWorkletNode(ctx, "rt-processor", {
      outputChannelCount: [2],
      processorOptions: { bytes },
    });
    // Fired if the processor's constructor or process() throws — the node then goes silent.
    node.onprocessorerror = () => {
      setStatus("processor error — the worklet crashed (see console)");
      startBtn.disabled = false;
    };
    // The processor posts one of these from its constructor once init resolves.
    node.port.onmessage = (e) => {
      const d = e.data;
      if (d?.type === "ready") {
        const base = (ctx.baseLatency * 1000).toFixed(1);
        setStatus(
          `▶ playing — ${d.len} samples/quantum @ ${ctx.sampleRate} Hz, base latency ${base} ms`,
        );
      } else if (d?.type === "error") {
        setStatus(`worklet error: ${d.message}`);
        console.error("worklet init failed:", d.message, "\n", d.stack);
        startBtn.disabled = false;
      }
    };
    node.connect(ctx.destination);
    await ctx.resume();
    setStatus("node created — initializing engine in worklet…");
  } catch (err) {
    setStatus(`error: ${err}`);
    startBtn.disabled = false;
    ctx = undefined;
  }
});
