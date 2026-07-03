// Engine bring-up + control transport — the non-UI half of the harness, extracted from main.ts so
// the Svelte app (App.svelte) is a thin consumer.
//
// `startEngine` fetches the wasm bytes (no reliable fetch in the worklet) and hands them plus the
// runnable patch to the AudioWorklet via `processorOptions`; the processor instantiates the module,
// builds `SceneEngine(patch)`, and on `ready` posts back the engine geometry **and the device
// catalog** (`wasm_bindgen.catalog()`, called in the worklet where the wasm instance lives). Control
// is generic, by device id: `send` posts param/note/loadPatch messages the worklet maps onto the
// engine. Keyboard + Web MIDI are wired here onto the same `send`.

import type { CableType, DeviceDescriptor } from "./catalog";
import { clampOctave, DEFAULT_VELOCITY, noteForKey, octaveShiftFor } from "./notes";
import type { Patch } from "./scene";

/** The messages the worklet maps onto SceneEngine, all addressed by device id. */
export type ControlMessage =
  | { type: "param"; device: string; paramId: number; value: number }
  | { type: "noteOn"; device: string; note: number; velocity: number }
  | { type: "noteOff"; device: string; note: number }
  | { type: "loadPatch"; patch: Patch };

/** The worklet's ready handshake: engine geometry + group delay + the fetched device & cable catalogs,
 *  plus the initial scene's static per-connection loading loss. */
export type ReadyMessage = {
  type: "ready";
  len: number;
  signalPathLatencyMs: number;
  catalog: DeviceDescriptor[];
  cables: CableType[];
  /** Per scene connection (same index as the patch's connection list): loading loss in dB, or `null`
   *  for a digital/event connection (ideal, no resistive loading). */
  losses: (number | null)[];
};

/** Live device meter readings, posted ~47×/s: one entry per metering device, `[deviceId, values]`
 *  with values in readout-id order (matching the device descriptor's `readouts`). */
export type ReadoutsMessage = {
  type: "readouts";
  readings: [string, number[]][];
};

/** Refreshed static per-connection loading loss, posted once after a hot-swap installs a new scene. */
export type LossesMessage = {
  type: "losses";
  losses: (number | null)[];
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
  /** Live device meter readings (node→host lane), ~47×/s: `[deviceId, values]` per metering device.
   *  Optional — the UI wires it when it renders meter screens (Story 4.5.7). */
  onReadouts?: (readings: [string, number[]][]) => void;
  /** Refreshed per-connection loading loss (dB, or `null`) after a structural edit. Optional, like
   *  `onReadouts`. The initial losses arrive on `ready`. */
  onLosses?: (losses: (number | null)[]) => void;
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
    } else if (d?.type === "readouts") {
      handlers.onReadouts?.((d as ReadoutsMessage).readings);
    } else if (d?.type === "losses") {
      handlers.onLosses?.((d as LossesMessage).losses);
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

// --- Keyboard + Web MIDI note capture -------------------------------------------------------------
//
// Both surfaces *detect* note-on/off and hand them to a `NoteSink`; neither knows **which** device
// plays. The caller (App) routes each note to the currently-focused instrument (Story 4.8), so
// capture follows focus instead of being bolted to one synth at startup. The QWERTY note layout +
// octave logic live in notes.ts.

/** A note event from a keyboard/controller: on/off, MIDI note, and velocity (0 / ignored for off). */
export type NoteSink = (on: boolean, note: number, velocity: number) => void;

/** Whether keystrokes should be treated as text — a form control (a knob's range input, a select, a
 *  field) has focus — so typing to adjust a control doesn't also play notes. */
function editingText(): boolean {
  const el = document.activeElement;
  if (!(el instanceof HTMLElement)) return false;
  return (
    el.tagName === "INPUT" ||
    el.tagName === "SELECT" ||
    el.tagName === "TEXTAREA" ||
    el.isContentEditable
  );
}

/** Capture the computer keyboard as a keybed: A–K + the black-key row play notes, Z/X transpose the
 *  octave, auto-repeat is suppressed, and keystrokes are ignored while a form control has focus.
 *  Returns a **detach** function — the caller attaches it only while an instrument surface is open,
 *  and detaching releases anything still held so a note can't hang past the surface closing. */
export function wireKeyboard(onNote: NoteSink): () => void {
  let octave = 0; // shifted by Z/X, in octaves
  const held = new Set<number>(); // MIDI notes currently down, to suppress key-repeat re-triggers

  const onKeyDown = (e: KeyboardEvent): void => {
    if (e.repeat || e.metaKey || e.ctrlKey || e.altKey || editingText()) return;
    const key = e.key.toLowerCase();
    const shift = octaveShiftFor(key);
    if (shift !== null) {
      octave = clampOctave(octave + shift);
      return;
    }
    const note = noteForKey(key, octave);
    if (note === null || held.has(note)) return;
    held.add(note);
    onNote(true, note, DEFAULT_VELOCITY);
  };

  const onKeyUp = (e: KeyboardEvent): void => {
    const note = noteForKey(e.key, octave);
    if (note === null || !held.has(note)) return;
    held.delete(note);
    onNote(false, note, 0);
  };

  window.addEventListener("keydown", onKeyDown);
  window.addEventListener("keyup", onKeyUp);
  return () => {
    for (const note of held) onNote(false, note, 0); // release held notes on unfocus
    held.clear();
    window.removeEventListener("keydown", onKeyDown);
    window.removeEventListener("keyup", onKeyUp);
  };
}

/** Request Web MIDI access **once** (the permission) and route every input's note-on/off to `onNote`.
 *  The *target* device is the caller's concern (it follows focus), so this only decodes + forwards —
 *  access is not re-requested when focus moves. */
export function wireMidi(onNote: NoteSink, onStatus: (message: string) => void): void {
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
          input.onmidimessage = (e) => handleMidi(onNote, e.data);
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
function handleMidi(onNote: NoteSink, data: Uint8Array | null): void {
  if (!data || data.length < 3) return;
  const status = data[0] & 0xf0; // strip the channel nibble
  const note = data[1];
  const velocity = data[2];
  if (status === 0x90 && velocity > 0) onNote(true, note, velocity);
  else if (status === 0x80 || (status === 0x90 && velocity === 0)) onNote(false, note, 0);
}
