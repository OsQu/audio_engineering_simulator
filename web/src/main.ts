// Story 3.2/3.3 → 4.1. Brings up the engine in an AudioWorklet from a *scene*: fetch the wasm bytes
// (no reliable fetch in the worklet) and the runnable patch, hand both to the processor via
// `processorOptions`; it instantiates the module and builds `SceneEngine(patch)` in its constructor.
// Control is **generic, by device id** — sliders push param targets and the keyboard/MIDI push notes,
// all addressed `(device, …)`. The scene is the page's source of truth: it can be saved to / loaded
// from localStorage (versioned JSON), and reloaded live to exercise the engine's hot-swap.

import type { Patch } from "./scene";
import { defaultScene, loadScene, type Scene, saveScene, setSceneParam } from "./scene-store";

const statusEl = document.getElementById("status") as HTMLElement;
const startBtn = document.getElementById("start") as HTMLButtonElement;
const controlsEl = document.getElementById("controls") as HTMLElement;
const healthEl = document.getElementById("health") as HTMLElement;
const setStatus = (m: string): void => {
  statusEl.textContent = m;
};

// The instrument the on-screen sliders and the keyboard address. (One instrument in the canonical
// scene; descriptor-driven multi-device panels are Story 4.2.)
const SYNTH = "synth";
// SynthVoice param ids (its `params()` order): LEVEL = 0, ATTACK_MS = 1 (the two sliders we expose).
const LEVEL = 0;
const ATTACK = 1;

// The page's authoritative scene: a saved one if present, else the default studio.
let scene: Scene = loadScene() ?? defaultScene();

// Story 3.4 — the worklet's ready handshake carries the engine's fixed signal-path group delay, so the
// page can compose end-to-end latency from it + the browser's measured base/output latency.
type ReadyMessage = { type: "ready"; len: number; signalPathLatencyMs: number };

function latencySummary(audio: AudioContext, ready: ReadyMessage): string {
  const baseMs = audio.baseLatency * 1000;
  const outputMs = (audio.outputLatency ?? 0) * 1000;
  const engineMs = ready.signalPathLatencyMs;
  const quantumMs = (ready.len / audio.sampleRate) * 1000;
  const outTotal = baseMs + outputMs + engineMs;
  const fmt = (n: number): string => n.toFixed(1);
  return (
    `▶ playing @ ${audio.sampleRate} Hz · ${ready.len} samp/quantum · ` +
    `latency: base ${fmt(baseMs)} + output ${fmt(outputMs)} + engine ${fmt(engineMs)} ` +
    `≈ ${fmt(outTotal)} ms out (+ up to ${fmt(quantumMs)} ms note quantum)`
  );
}

// Story 3.4 — the real-time-health snapshot the worklet posts on a throttle (compute-budget overruns +
// worst render time, and the engine's input-flood queue drops). All running totals for the session.
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

// --- Control transport: the messages the worklet maps onto SceneEngine, all addressed by device. ---
type ControlMessage =
  | { type: "param"; device: string; paramId: number; value: number }
  | { type: "noteOn"; device: string; note: number; velocity: number }
  | { type: "noteOff"; device: string; note: number }
  | { type: "loadPatch"; patch: Patch };

startBtn.addEventListener("click", async () => {
  if (started) return;
  started = true;
  startBtn.disabled = true;
  try {
    // Pin the context rate to the engine's host rate (48 kHz); the browser resamples to the device.
    // Without this pin every quantum is the wrong rate ⇒ wrong pitch + drift. latencyHint
    // "interactive" = the smallest output buffer (lowest latency); the single-threaded in-worklet
    // engine can't grow its own render-ahead buffer, so this browser buffer is the only jitter
    // cushion. The 3.1 spike showed ~46× real-time headroom, so overruns are implausible here.
    const audio = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
    setStatus(`AudioContext @ ${audio.sampleRate} Hz — loading worklet…`);

    // public/ assets are served from the web root by Vite (dev and build).
    await audio.audioWorklet.addModule("/processor.js");
    setStatus("worklet module loaded — fetching wasm…");
    const bytes = await (await fetch("/wasm_bindings_bg.wasm")).arrayBuffer();

    // Deliver the wasm bytes *and* the runnable patch at construction; the processor instantiates
    // the module and builds SceneEngine(patch) synchronously in its constructor.
    const node = new AudioWorkletNode(audio, "scene-processor", {
      outputChannelCount: [2],
      processorOptions: { bytes, patch: scene.patch },
    });
    const send = (msg: ControlMessage): void => node.port.postMessage(msg);

    node.onprocessorerror = () => {
      setStatus("processor error — the worklet crashed (see console)");
      started = false;
      startBtn.disabled = false;
    };
    node.port.onmessage = (e: MessageEvent) => {
      const d = e.data;
      if (d?.type === "ready") {
        setStatus(latencySummary(audio, d as ReadyMessage));
        controlsEl.hidden = false;
        wireControls(send);
      } else if (d?.type === "health") {
        showHealth(d as HealthMessage);
      } else if (d?.type === "error") {
        setStatus(`worklet error: ${d.message}`);
        console.error("worklet error:", d.message, "\n", d.stack);
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

// --- Sliders → param targets, keyboard/MIDI → notes, plus save/load/reload. Called once on ready. ---

function wireControls(send: (msg: ControlMessage) => void): void {
  bindSlider(send, "level", LEVEL, "level-val", (v) => v.toFixed(2));
  bindSlider(send, "attack", ATTACK, "attack-val", (v) => String(Math.round(v)));
  wireKeyboard(send);
  wireMidi(send);
  wireSceneButtons(send);
}

/** Wire one range input to a synth param: send its value live AND record it in the scene (so it
 * persists on save), and mirror it into its <output>. Initializes from the scene if it has a value. */
function bindSlider(
  send: (msg: ControlMessage) => void,
  inputId: string,
  paramId: number,
  outId: string,
  fmt: (v: number) => string,
): void {
  const input = document.getElementById(inputId) as HTMLInputElement;
  const out = document.getElementById(outId) as HTMLOutputElement;
  // Reflect a saved value if the scene carries one for this param.
  const saved = scene.patch.devices
    .find((d) => d.id === SYNTH)
    ?.params?.find((p) => p.id === paramId);
  if (saved) input.value = String(saved.value);

  const push = (): void => {
    const value = Number(input.value);
    out.textContent = fmt(value);
    setSceneParam(scene, SYNTH, paramId, value); // keep the scene in sync for save
    send({ type: "param", device: SYNTH, paramId, value });
  };
  input.addEventListener("input", push);
  push(); // sync the engine + scene to the slider's current value immediately
}

/** Save / load / reload the scene. Save writes localStorage; load swaps the engine to the saved scene
 * and refreshes the sliders; reload re-applies the current scene live (exercises the hot-swap). */
function wireSceneButtons(send: (msg: ControlMessage) => void): void {
  const saveBtn = document.getElementById("save") as HTMLButtonElement;
  const loadBtn = document.getElementById("load") as HTMLButtonElement;
  const reloadBtn = document.getElementById("reload") as HTMLButtonElement;

  saveBtn.addEventListener("click", () => {
    saveScene(scene);
    setStatus("scene saved");
  });

  loadBtn.addEventListener("click", () => {
    const loaded = loadScene();
    if (!loaded) {
      setStatus("no saved scene");
      return;
    }
    scene = loaded;
    send({ type: "loadPatch", patch: scene.patch }); // hot-swap the engine to the saved scene
    syncSliders();
    setStatus("scene loaded");
  });

  reloadBtn.addEventListener("click", () => {
    send({ type: "loadPatch", patch: scene.patch }); // re-apply current scene — proves glitch-free swap
    setStatus("scene reloaded (hot-swap)");
  });
}

/** Push the current scene's saved synth param values back into the sliders + their <output>s. */
function syncSliders(): void {
  const params = scene.patch.devices.find((d) => d.id === SYNTH)?.params ?? [];
  const set = (
    inputId: string,
    outId: string,
    paramId: number,
    fmt: (v: number) => string,
  ): void => {
    const value = params.find((p) => p.id === paramId)?.value;
    if (value === undefined) return;
    (document.getElementById(inputId) as HTMLInputElement).value = String(value);
    (document.getElementById(outId) as HTMLOutputElement).textContent = fmt(value);
  };
  set("level", "level-val", LEVEL, (v) => v.toFixed(2));
  set("attack", "attack-val", ATTACK, (v) => String(Math.round(v)));
}

// One octave of a piano keyboard over the QWERTY home rows: white keys on A–K, black keys on the
// W/E/T/Y/U row above — the de-facto layout (Ableton, many soft synths).
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

/** Map the computer keyboard to note-on/off on the synth, suppressing auto-repeat; Z/X shift octave. */
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
    send({ type: "noteOn", device: SYNTH, note, velocity: VELOCITY });
  });

  window.addEventListener("keyup", (e) => {
    const note = noteFor(e.key.toLowerCase());
    if (note === null || !held.has(note)) return;
    held.delete(note);
    send({ type: "noteOff", device: SYNTH, note });
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
    send({ type: "noteOn", device: SYNTH, note, velocity });
  } else if (status === 0x80 || (status === 0x90 && velocity === 0)) {
    send({ type: "noteOff", device: SYNTH, note });
  }
}
