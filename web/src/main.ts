// Story 3.2, Phase B — Vite/TS harness. Functionally identical to the rt/ static page: fetch the
// wasm bytes (no reliable fetch in the worklet), hand them to the processor via `processorOptions`,
// and start on a user gesture. The processor (public/processor.js, a classic no-modules script) does
// the compile + instantiate in its constructor.

const statusEl = document.getElementById("status") as HTMLElement;
const startBtn = document.getElementById("start") as HTMLButtonElement;
const setStatus = (m: string): void => {
	statusEl.textContent = m;
};

let started = false; // guards against a second click while starting / once running

startBtn.addEventListener("click", async () => {
	if (started) return;
	started = true;
	startBtn.disabled = true;
	try {
		// Pin the context rate to the engine's host rate (48 kHz); the browser resamples to the device.
		// Without this pin every quantum is the wrong rate ⇒ wrong pitch + drift.
		const audio = new AudioContext({
			sampleRate: 48000,
			latencyHint: "interactive",
		});

		setStatus(`AudioContext @ ${audio.sampleRate} Hz — loading worklet…`);

		// public/ assets are served from the web root by Vite (dev and build).
		await audio.audioWorklet.addModule("/processor.js");
		setStatus("worklet module loaded — fetching wasm…");

		const bytes = await (await fetch("/wasm_bindings_bg.wasm")).arrayBuffer();

		// Deliver the bytes at construction; the processor compiles + instantiates synchronously in its
		// constructor. No init message / ready handshake to race.
		const node = new AudioWorkletNode(audio, "rt-processor", {
			outputChannelCount: [2],
			processorOptions: { bytes },
		});
		node.onprocessorerror = () => {
			setStatus("processor error — the worklet crashed (see console)");
			started = false;
			startBtn.disabled = false;
		};
		node.port.onmessage = (e: MessageEvent) => {
			const d = e.data;
			if (d?.type === "ready") {
				const base = (audio.baseLatency * 1000).toFixed(1);
				setStatus(
					`▶ playing — ${d.len} samples/quantum @ ${audio.sampleRate} Hz, base latency ${base} ms`,
				);
			} else if (d?.type === "error") {
				setStatus(`worklet error: ${d.message}`);
				console.error("worklet init failed:", d.message, "\n", d.stack);
				started = false;
				startBtn.disabled = false;
			}
		};
		node.connect(audio.destination);
		await audio.resume();
		setStatus("node created — initializing engine in worklet…");
	} catch (err) {
		setStatus(`error: ${err}`);
		started = false;
		startBtn.disabled = false;
	}
});
