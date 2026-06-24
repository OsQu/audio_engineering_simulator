// Story 3.2, Phase A — static-page first sound (no bundler; the Vite/TS harness is Phase B / 3.2.5).
//
// Compiles the wasm on the main thread, hands the compiled Module to the AudioWorklet, and starts
// playback on a user gesture (browsers require one to start audio). The worklet does the rest.

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

    // Fetch the wasm bytes here (no reliable fetch in the worklet) and post the *bytes* across, not
    // a compiled WebAssembly.Module: a Module can't be structured-cloned into AudioWorkletGlobalScope
    // in some browsers (the message is silently dropped). An ArrayBuffer always transfers; the
    // worklet compiles it synchronously, which is allowed off the main thread.
    const bytes = await (await fetch("pkg/wasm_bindings_bg.wasm")).arrayBuffer();
    setStatus("wasm fetched — creating node…");

    const node = new AudioWorkletNode(ctx, "rt-processor", { outputChannelCount: [2] });
    // Fired if the processor's constructor or process() throws — the node then goes silent.
    node.onprocessorerror = () => {
      setStatus("processor error — the worklet crashed (see console)");
      startBtn.disabled = false;
    };
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
    // A failed structured-clone *into* the worklet fires messageerror there; a failed clone of the
    // reply back fires it here. Surface both so a dropped message can never hide again.
    node.port.onmessageerror = (e) => {
      setStatus("messageerror on main port (reply failed to deserialize)");
      console.error("main port messageerror:", e);
    };
    node.connect(ctx.destination);
    await ctx.resume();

    setStatus("node created — initializing engine in worklet…");
    // Transfer the ArrayBuffer (zero-copy; main no longer needs it).
    node.port.postMessage({ type: "init", bytes }, [bytes]);
  } catch (err) {
    setStatus(`error: ${err}`);
    startBtn.disabled = false;
    ctx = undefined;
  }
});
