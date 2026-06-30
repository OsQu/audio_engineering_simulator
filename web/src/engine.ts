// Engine bring-up + control transport — the non-UI half of the harness, extracted from main.ts so
// the Svelte app (App.svelte) is a thin consumer.
//
// `startEngine` fetches the wasm bytes (no reliable fetch in the worklet) and hands them plus the
// runnable patch to the AudioWorklet via `processorOptions`; the processor instantiates the module,
// builds `SceneEngine(patch)`, and on `ready` posts back the engine geometry **and the device
// catalog** (`wasm_bindgen.catalog()`, called in the worklet where the wasm instance lives). Control
// is generic, by device id: `send` posts param/note/loadPatch messages the worklet maps onto the
// engine. Keyboard + Web MIDI are wired here onto the same `send`.

import type { DeviceDescriptor } from "./catalog";
import type { Patch } from "./scene";

/** The messages the worklet maps onto SceneEngine, all addressed by device id. */
export type ControlMessage =
  | { type: "param"; device: string; paramId: number; value: number }
  | { type: "noteOn"; device: string; note: number; velocity: number }
  | { type: "noteOff"; device: string; note: number }
  | { type: "loadPatch"; patch: Patch };

/** The worklet's ready handshake: engine geometry + group delay + the fetched device catalog. */
export type ReadyMessage = {
  type: "ready";
  len: number;
  signalPathLatencyMs: number;
  catalog: DeviceDescriptor[];
};

/** The throttled real-time-health snapshot (compute-budget overruns + engine queue drops). */
export type HealthMessage = {
  type: "health";
  quanta: number;
  overruns: number;
  maxMs: number;
  budgetMs: number;
  eventDrops: number;
  paramDrops: number;
};

/** The output peak (linear, ±1.0 = full scale) posted ~47×/s to drive the master VU meter. */
export type LevelMessage = {
  type: "level";
  peak: number;
};

/** Handle returned by `startEngine` for host-side controls that sit *outside* the simulation. */
export interface EngineControl {
  /** Set the monitor (listening) volume: a Web Audio gain after the engine, before the speakers.
   *  Purely how loud it is in your headphones — it does not touch the modeled signal or the meter. */
  setVolume: (gain: number) => void;
}

/** Callbacks the host drives as the worklet reports back. */
export interface EngineHandlers {
  onStatus: (message: string) => void;
  onHealth: (health: HealthMessage) => void;
  /** Output peak for the master meter (linear, ±1.0 = full scale). */
  onLevel: (peak: number) => void;
  /** Fired once the engine is live: hands back the catalog and the `send` channel. */
  onReady: (ready: ReadyMessage, send: (msg: ControlMessage) => void) => void;
}

/** Compose the end-to-end latency line from the engine group delay + the browser's measured latency. */
export function latencySummary(audio: AudioContext, ready: ReadyMessage): string {
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

/** Format a health snapshot for the readout line. */
export function healthSummary(h: HealthMessage): string {
  const plural = h.overruns === 1 ? "" : "s";
  return (
    `health: ${h.overruns} overrun${plural} / ${h.quanta} quanta · ` +
    `worst ${h.maxMs.toFixed(2)} / ${h.budgetMs.toFixed(2)} ms budget · ` +
    `drops: ${h.eventDrops} event, ${h.paramDrops} param`
  );
}

/**
 * Bring the engine up in an AudioWorklet from `patch`, wiring the worklet's messages to `handlers`.
 * Resolves once the node is created (audio starts on `ready`); throws on setup failure so the caller
 * can surface it. The returned cleanup is unused today (the page lives for the session).
 */
export async function startEngine(
  patch: Patch,
  handlers: EngineHandlers,
  initialVolume: number,
): Promise<EngineControl> {
  // Pin the context rate to the engine's host rate (48 kHz); the browser resamples to the device.
  // Without this pin every quantum is the wrong rate ⇒ wrong pitch + drift. latencyHint
  // "interactive" = the smallest output buffer (lowest latency); the single-threaded in-worklet
  // engine can't grow its own render-ahead buffer, so this browser buffer is the only jitter cushion.

  const audio = new AudioContext({
    sampleRate: 48000,
    latencyHint: "interactive",
  });
  handlers.onStatus(`AudioContext @ ${audio.sampleRate} Hz — loading worklet…`);

  // public/ assets are served from the web root by Vite (dev and build).
  await audio.audioWorklet.addModule("/processor.js");
  handlers.onStatus("worklet module loaded — fetching wasm…");
  const bytes = await (await fetch("/wasm_bindings_bg.wasm")).arrayBuffer();

  // Deliver the wasm bytes *and* the runnable patch at construction; the processor instantiates the
  // module and builds SceneEngine(patch) synchronously in its constructor.
  const node = new AudioWorkletNode(audio, "scene-processor", {
    outputChannelCount: [2],
    processorOptions: { bytes, patch },
  });
  const send = (msg: ControlMessage): void => node.port.postMessage(msg);

  node.onprocessorerror = () => {
    handlers.onStatus("processor error — the worklet crashed (see console)");
  };
  node.port.onmessage = (e: MessageEvent) => {
    const d = e.data;
    if (d?.type === "ready") {
      const ready = d as ReadyMessage;
      handlers.onStatus(latencySummary(audio, ready));
      handlers.onReady(ready, send);
    } else if (d?.type === "health") {
      handlers.onHealth(d as HealthMessage);
    } else if (d?.type === "level") {
      handlers.onLevel((d as LevelMessage).peak);
    } else if (d?.type === "error") {
      handlers.onStatus(`worklet error: ${d.message}`);
      console.error("worklet error:", d.message, "\n", d.stack);
    }
  };
  // A monitor-gain node **outside the simulation**: it scales the final output for comfortable
  // listening (the engine blasts at full scale otherwise) without altering the modeled signal — so the
  // VU meter, which reads the engine's output buffer in the worklet, still shows the true level.
  const monitor = audio.createGain();
  monitor.gain.value = Math.max(0, initialVolume);
  node.connect(monitor);
  monitor.connect(audio.destination);
  await audio.resume();
  handlers.onStatus("node created — initializing engine in worklet…");

  return {
    setVolume: (gain: number): void => {
      monitor.gain.value = Math.max(0, gain);
    },
  };
}

// --- Keyboard: one octave of a piano over the QWERTY home rows ------------------------------------

// White keys on A–K, black keys on the W/E/T/Y/U row above — the de-facto layout (Ableton, many
// soft synths).
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

/** Map the computer keyboard to note-on/off on `device`, suppressing auto-repeat; Z/X shift octave. */
export function wireKeyboard(send: (msg: ControlMessage) => void, device: string): void {
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
    send({ type: "noteOn", device, note, velocity: VELOCITY });
  });

  window.addEventListener("keyup", (e) => {
    const note = noteFor(e.key.toLowerCase());
    if (note === null || !held.has(note)) return;
    held.delete(note);
    send({ type: "noteOff", device, note });
  });
}

// --- Web MIDI: the same note path, fed by a hardware controller. ----------------------------------

/** Request Web MIDI access and route note-on/off from every input through `send`, onto `device`. */
export function wireMidi(
  send: (msg: ControlMessage) => void,
  device: string,
  onStatus: (message: string) => void,
): void {
  const nav = navigator as Navigator & {
    requestMIDIAccess?: () => Promise<MIDIAccess>;
  };
  if (!nav.requestMIDIAccess) {
    onStatus("MIDI: not supported in this browser");
    return;
  }
  nav.requestMIDIAccess().then(
    (access) => {
      const attach = (): void => {
        const names: string[] = [];
        for (const input of access.inputs.values()) {
          input.onmidimessage = (e) => handleMidi(send, device, e.data);
          names.push(input.name ?? "unknown");
        }
        onStatus(names.length ? `MIDI: ${names.join(", ")}` : "MIDI: no inputs connected");
      };
      attach();
      access.onstatechange = attach; // re-attach when a device is plugged/unplugged
    },
    (err) => {
      onStatus(`MIDI: access denied (${err})`);
    },
  );
}

/** Decode a raw MIDI message and forward note-on/off — note-on with velocity 0 means note-off. */
function handleMidi(
  send: (msg: ControlMessage) => void,
  device: string,
  data: Uint8Array | null,
): void {
  if (!data || data.length < 3) return;
  const status = data[0] & 0xf0; // strip the channel nibble
  const note = data[1];
  const velocity = data[2];
  if (status === 0x90 && velocity > 0) {
    send({ type: "noteOn", device, note, velocity });
  } else if (status === 0x80 || (status === 0x90 && velocity === 0)) {
    send({ type: "noteOff", device, note });
  }
}
