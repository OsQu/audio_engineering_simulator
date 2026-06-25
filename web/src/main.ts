// Story 3.2 Phase B → Story 3.3. Brings up the engine in an AudioWorklet (fetch the wasm bytes —
// no reliable fetch in the worklet — hand them to the processor via `processorOptions`; it compiles
// + instantiates in its constructor), then wires live control: sliders push smoothed param targets
// and the computer keyboard pushes note events, both as `port.postMessage`s the worklet forwards
// onto RtEngine's setters. Web MIDI (Story 3.3.4) reuses the identical note path.

const statusEl = document.getElementById("status") as HTMLElement;
const startBtn = document.getElementById("start") as HTMLButtonElement;
const controlsEl = document.getElementById("controls") as HTMLElement;
const healthEl = document.getElementById("health") as HTMLElement;
const setStatus = (m: string): void => {
  statusEl.textContent = m;
};

// Story 3.4 — the real-time-health snapshot the worklet posts on a throttle (compute-budget overruns
// + worst render time, and the engine's input-flood queue drops). All running totals for the session.
type HealthMessage = {
  type: "health";
  quanta: number;
  overruns: number;
  maxMs: number;
  budgetMs: number;
  eventDrops: number;
  paramDrops: number;
};

function showHealth(h: HealthMessage): void {
  const plural = h.overruns === 1 ? "" : "s";
  healthEl.textContent =
    `health: ${h.overruns} overrun${plural} / ${h.quanta} quanta · ` +
    `worst ${h.maxMs.toFixed(2)} / ${h.budgetMs.toFixed(2)} ms budget · ` +
    `drops: ${h.eventDrops} event, ${h.paramDrops} param`;
}

let started = false; // guards against a second click while starting / once running

// --- Control transport: the messages the worklet's port.onmessage maps onto RtEngine setters. -----
type ParamName = "level" | "attack_ms" | "decay_ms" | "sustain" | "release_ms";
type ControlMessage =
  | { type: "param"; name: ParamName; value: number }
  | { type: "noteOn"; note: number; velocity: number }
  | { type: "noteOff"; note: number };

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
    // A single send helper, captured by every control handler below.
    const send = (msg: ControlMessage): void => node.port.postMessage(msg);

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
        // The engine is live: reveal the controls and arm the keyboard. Push the slider defaults
        // once so the engine matches the UI from the first note.
        controlsEl.hidden = false;
        wireControls(send);
      } else if (d?.type === "health") {
        showHealth(d as HealthMessage);
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

// --- Sliders → smoothed param targets, keyboard → note events. ------------------------------------

/** Wire the slider + keyboard (and, Story 3.3.4, MIDI) handlers to `send`. Called once on ready. */
function wireControls(send: (msg: ControlMessage) => void): void {
  bindSlider(send, "level", "level", "level-val", (v) => v.toFixed(2));
  bindSlider(send, "attack", "attack_ms", "attack-val", (v) => String(Math.round(v)));
  wireKeyboard(send);
  wireMidi(send);
}

/** Wire one range input to a param: send its value on input, and mirror it into its <output>. */
function bindSlider(
  send: (msg: ControlMessage) => void,
  inputId: string,
  name: ParamName,
  outId: string,
  fmt: (v: number) => string,
): void {
  const input = document.getElementById(inputId) as HTMLInputElement;
  const out = document.getElementById(outId) as HTMLOutputElement;
  const push = (): void => {
    const value = Number(input.value);
    out.textContent = fmt(value);
    send({ type: "param", name, value });
  };
  input.addEventListener("input", push);
  push(); // sync the engine to the slider's default immediately
}

// One octave of a piano keyboard laid over the QWERTY home rows: the white keys on A–K, the black
// keys on the W/E/T/Y/U row above — the de-facto layout (Ableton, many soft synths).
const KEY_SEMITONES: Record<string, number> = {
  a: 0, // C
  w: 1, // C#
  s: 2, // D
  e: 3, // D#
  d: 4, // E
  f: 5, // F
  t: 6, // F#
  g: 7, // G
  y: 8, // G#
  h: 9, // A
  u: 10, // A#
  j: 11, // B
  k: 12, // C (octave up)
};
const C4 = 60; // MIDI note for the base octave's C
const VELOCITY = 100;

/** Map the computer keyboard to note-on/off, suppressing auto-repeat and shifting octave on Z/X. */
function wireKeyboard(send: (msg: ControlMessage) => void): void {
  let octave = 0; // shifted by Z/X, in octaves
  const held = new Set<number>(); // MIDI notes currently down, to suppress key-repeat re-triggers

  const noteFor = (key: string): number | null => {
    const semis = KEY_SEMITONES[key];
    return semis === undefined ? null : C4 + 12 * octave + semis;
  };

  window.addEventListener("keydown", (e) => {
    if (e.repeat || e.metaKey || e.ctrlKey || e.altKey) return;
    const key = e.key.toLowerCase();
    if (key === "z" || key === "x") {
      octave = Math.max(-3, Math.min(3, octave + (key === "z" ? -1 : 1)));
      return;
    }
    const note = noteFor(key);
    if (note === null || held.has(note)) return;
    held.add(note);
    send({ type: "noteOn", note, velocity: VELOCITY });
  });

  window.addEventListener("keyup", (e) => {
    const note = noteFor(e.key.toLowerCase());
    // Release by the note the key currently maps to. (Releasing after an octave shift can leave a
    // note hanging; acceptable for the throwaway page — a held-key→note map is Epic 4 polish.)
    if (note === null || !held.has(note)) return;
    held.delete(note);
    send({ type: "noteOff", note });
  });
}

// --- Web MIDI (Story 3.3.4): the same note path, fed by a hardware controller. --------------------

/** Request Web MIDI access and route note-on/off from every input through the same `send` path. */
function wireMidi(send: (msg: ControlMessage) => void): void {
  const midiStatus = document.getElementById("midi-status") as HTMLElement;
  const nav = navigator as Navigator & {
    requestMIDIAccess?: () => Promise<MIDIAccess>;
  };
  if (!nav.requestMIDIAccess) {
    midiStatus.textContent = "MIDI: not supported in this browser";
    return;
  }
  nav.requestMIDIAccess().then(
    (access) => {
      const attach = (): void => {
        const names: string[] = [];
        for (const input of access.inputs.values()) {
          input.onmidimessage = (e) => handleMidi(send, e.data);
          names.push(input.name ?? "unknown");
        }
        midiStatus.textContent = names.length
          ? `MIDI: ${names.join(", ")}`
          : "MIDI: no inputs connected";
      };
      attach();
      access.onstatechange = attach; // re-attach when a device is plugged/unplugged
    },
    (err) => {
      midiStatus.textContent = `MIDI: access denied (${err})`;
    },
  );
}

/** Decode a raw MIDI message and forward note-on/off — note-on with velocity 0 means note-off. */
function handleMidi(send: (msg: ControlMessage) => void, data: Uint8Array | null): void {
  if (!data || data.length < 3) return;
  const status = data[0] & 0xf0; // strip the channel nibble
  const note = data[1];
  const velocity = data[2];
  if (status === 0x90 && velocity > 0) {
    send({ type: "noteOn", note, velocity });
  } else if (status === 0x80 || (status === 0x90 && velocity === 0)) {
    send({ type: "noteOff", note });
  }
}
