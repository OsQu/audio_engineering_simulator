// The engine session: the view-agnostic consumer surface over the worklet engine (PROJECT_PLAN §7 —
// the UI is a pure consumer of the engine API, factored here so a second view root can construct the
// same interaction path). It owns the engine lifecycle, the live readout state streaming from the
// worklet, the authoritative `Scene`, and the param/config control lanes that mutate it. Note routing
// and patching move in over the rest of Story 6.1. A class with `$state` fields (the codebase's first
// `.svelte.ts` rune module), constructed once per view root with its initial scene — App seeds from
// `loadScene() ?? defaultScene()`; the 6.3 workbench will seed from the URL.

import {
  type CableType,
  configDefault,
  type DeviceDescriptor,
  type ParamDescriptor,
} from "./catalog";
import {
  type ControlMessage,
  type EngineControl,
  type HealthMessage,
  healthSummary,
  type ReadyMessage,
  startEngine,
} from "./engine";
import { DEFAULT_VELOCITY } from "./notes";
import * as params from "./params";
import { deviceById } from "./projection";
import type { Patch } from "./scene";
import { loadScene, type Scene, saveScene, setSceneConfig, setSceneParam } from "./scene-store";

// Monitor (listening) volume — a host-side output gain *outside* the simulation, persisted on its own
// (a per-listener setting, not scene/simulation data). Defaults low so it doesn't blast.
const VOLUME_KEY = "aes.volume";
function loadVolume(): number {
  const s = localStorage.getItem(VOLUME_KEY);
  if (s === null) return 0.25;
  const raw = Number(s);
  return Number.isFinite(raw) ? Math.max(0, Math.min(1, raw)) : 0.25;
}

/** One entry in the bench MIDI monitor: a note event as it was routed through {@link
 *  SceneSession.playNote}. `sent` is false when the engine wasn't up yet (the event went nowhere) — a
 *  debugging signal in itself. `seq` is a monotonic id for a stable list key. */
export interface MidiLogEntry {
  seq: number;
  on: boolean;
  note: number;
  velocity: number;
  device: string;
  sent: boolean;
}

/** How many recent note events the MIDI monitor keeps (newest-first, older ones drop off). */
const MIDI_LOG_MAX = 32;

export class SceneSession {
  // --- Engine lifecycle + status ------------------------------------------------------------------
  status = $state("idle");
  health = $state("");
  midiStatus = $state("MIDI: requesting access…");
  started = $state(false);
  ready = $state(false);
  // The fetched device catalog (arrives on `ready`) — controls render from it, not hardcoded ids.
  catalog = $state<DeviceDescriptor[]>([]);
  // Cable presets the picker offers for analog connections (fetched with the device catalog).
  cables = $state<CableType[]>([]);
  // The control-message sink into the worklet, or null before the engine is up.
  send = $state<((msg: ControlMessage) => void) | null>(null);

  // --- Live readout state (streamed from the worklet) ---------------------------------------------
  // Master output peak (linear, ±1.0 = full scale), from the worklet's throttled level message.
  level = $state(0);
  // Live device meter readings from the node→host lane, keyed by device id (values in readout-id
  // order). Updated ~47×/s from the worklet's `readouts` message.
  readings = $state<Record<string, number[]>>({});
  // Static per-connection loading loss in dB (or null for digital/event connections), by connection
  // index (matching scene.patch.connections order). Seeded on `ready`, refreshed after each hot-swap.
  losses = $state<(number | null)[]>([]);
  // Up-to-date device descriptors built with current configuration
  deviceDescriptors = $state<Record<string, DeviceDescriptor>>({});
  // Signal-path latency in ms (the whole-engine group delay: schedule chain + capture), from the
  // `ready` message. A single engine scalar — held here so a view (the bench debug header) can show it
  // without recomputing. The scene view's status line already renders the fuller `latencySummary`.
  latencyMs = $state(0);

  // --- The authoritative scene + its control-param lane -------------------------------------------
  // The page's authoritative scene: the view root constructs the session with it. Held as `$state` so
  // every consumer (App today, a second view root tomorrow) reads it reactively.
  scene = $state<Scene>() as Scene;
  // Live control-param values, keyed `device:paramId`, mirrored into the scene on change so they
  // persist on save. Re-seeded from the scene whenever it's (re)loaded / the engine hot-swaps.
  paramValues = $state<Record<string, number>>({});
  // Notes currently sounding, for the keybed highlight — fed by every source (mouse, QWERTY, MIDI) so
  // the on-screen keys light up whichever way you play. Target selection stays view-side; `playNote`
  // takes the device explicitly.
  heldNotes = $state<number[]>([]);
  // A rolling log (newest first) of the note events funnelled through `playNote` — every source (the
  // on-screen keybed, QWERTY, Web MIDI) routes through there, so this is the one place to see whether a
  // MIDI event was actually emitted and to which device. Drives the bench debug MIDI monitor.
  midiLog = $state<MidiLogEntry[]>([]);
  #midiSeq = 0;

  // --- Monitor volume -----------------------------------------------------------------------------
  volume = $state(loadVolume());
  #setVolume: ((gain: number) => void) | null = null;
  // Resume the (suspended) AudioContext — must be called from a user gesture (see `resume`).
  #resume: (() => Promise<void>) | null = null;

  constructor(initialScene: Scene) {
    this.scene = initialScene;
  }

  // A device's current reading for a readout id, or the meter floor if none has arrived yet. An arrow
  // property so it can be passed as a callback (`this` stays bound to the instance).
  readingFor = (device: string, id: number): number => this.readings[device]?.[id] ?? -120;

  onVolume(v: number): void {
    this.volume = v;
    this.#setVolume?.(v);
    localStorage.setItem(VOLUME_KEY, String(v));
  }

  // Resume the AudioContext so audio flows. `start()` leaves it suspended (the catalog still arrives) —
  // this must be called from a user gesture. The scene view calls it right after start (its start button
  // is the gesture); the workbench calls it on the first interaction. A no-op before the engine is up.
  async resume(): Promise<void> {
    await this.#resume?.();
  }

  // Route one note-on/off to an explicit target device: update the held set (for the highlight) and
  // post it to the worklet. Target selection (which device a keyboard/MIDI/on-screen note plays) stays
  // view-side — the view resolves it and calls this. A no-op when the engine isn't up yet.
  playNote(device: string, on: boolean, note: number, velocity: number = DEFAULT_VELOCITY): void {
    const send = this.send;
    // Log every attempt (including ones that go nowhere because the engine isn't up) so the bench MIDI
    // monitor shows exactly what was triggered and where — the first thing to check when a note is silent.
    const entry: MidiLogEntry = {
      seq: this.#midiSeq++,
      on,
      note,
      velocity,
      device,
      sent: send !== null,
    };
    this.midiLog = [entry, ...this.midiLog].slice(0, MIDI_LOG_MAX);
    if (!send) return;
    if (on) {
      if (!this.heldNotes.includes(note)) this.heldNotes = [...this.heldNotes, note];
      send({ type: "noteOn", device, note, velocity });
    } else {
      this.heldNotes = this.heldNotes.filter((n) => n !== note);
      send({ type: "noteOff", device, note });
    }
  }

  // A plain (non-proxied) deep copy of the patch for crossing to the worklet: `$state` wraps the scene
  // in a reactive Proxy, which `postMessage` cannot structured-clone (DataCloneError).
  plainPatch(): Patch {
    return $state.snapshot(this.scene.patch);
  }

  // The current value of a device-local param (live override else descriptor default), bound to the map.
  paramValue(deviceId: string, desc: DeviceDescriptor, id: number): number {
    return params.paramValue(this.paramValues, deviceId, desc, id);
  }

  // A knob move touches all three param lanes at once — the live map (UI), the scene (for save), and the
  // engine (live) — so keep them in sync in this one visible place; they mustn't drift apart.
  onParamInput(device: string, p: ParamDescriptor, value: number): void {
    this.paramValues[params.key(device, p.id)] = value;
    setSceneParam(this.scene, device, p.id, value);
    this.send?.({ type: "param", device, paramId: p.id, value });
  }

  // A structural config's current value in the scene, falling back to the descriptor's build default —
  // the mirror of `paramValue` for the (recompile-on-change) config lane.
  configValue(deviceId: string, desc: DeviceDescriptor, key: string): number {
    const set = deviceById(this.scene, deviceId)?.config?.find((c) => c.key === key);
    return set?.value ?? configDefault(desc, key);
  }

  // A structural config toggle (INST/hi-Z): unlike a knob, this changes how the device is *built*, so it
  // edits the scene and rebuilds the engine (the same hot-swap repatching uses) rather than a live param.
  onConfigInput(device: string, key: string, value: number): void {
    setSceneConfig(this.scene, device, key, value);
    this.hotSwap();
  }

  // A structural edit → rebuild the engine from the new patch (compile + ScheduleSlot hot-swap, in the
  // worklet, the Story 4.1 path) and re-apply param values. Edits are rare gestures, so the off-block
  // compile cost is acceptable; the live audio thread swaps at a block boundary.
  hotSwap(): void {
    const send = this.send;
    if (!send) return;
    send({ type: "loadPatch", patch: this.plainPatch() });
    this.paramValues = params.seedParamValues(this.scene, this.catalog);
    params.pushParams(send, this.scene, this.catalog, this.paramValues);
  }

  save(): void {
    saveScene(this.scene);
    this.status = "scene saved";
  }

  // Load the saved scene (if any) into the session + engine. Returns whether a scene was loaded, so the
  // view root can resync its own view state (e.g. the current space) to the new scene's spaces.
  load(): boolean {
    const loaded = loadScene();
    if (!loaded) {
      this.status = "no saved scene";
      return false;
    }
    this.scene = loaded;
    this.paramValues = params.seedParamValues(this.scene, this.catalog);
    this.send?.({ type: "loadPatch", patch: this.plainPatch() }); // hot-swap the engine to the saved scene
    this.status = "scene loaded";
    return true;
  }

  reload(): void {
    this.send?.({ type: "loadPatch", patch: this.plainPatch() }); // re-apply current scene — proves glitch-free swap
    this.status = "scene reloaded (hot-swap)";
  }

  // Bring the engine up: instantiate the worklet with the current patch + monitor volume, wire the
  // streaming callbacks to this session's state, and on `ready` seed + push the param values so the
  // engine matches the scene from the start. Leaves the AudioContext **suspended** — call `resume()`
  // from a user gesture to hear anything. `onReady` lets the view root finish bring-up with its own
  // view-side pieces (e.g. the scene view requests Web MIDI there).
  async start(
    onReady: (r: ReadyMessage, send: (msg: ControlMessage) => void) => void,
  ): Promise<void> {
    if (this.started) return;
    this.started = true;
    try {
      const control: EngineControl = await startEngine(
        this.plainPatch(),
        {
          onStatus: (m) => {
            this.status = m;
          },
          onHealth: (h: HealthMessage) => {
            this.health = healthSummary(h);
          },
          onLevel: (peak) => {
            this.level = peak;
          },
          onReadouts: (r) => {
            this.readings = Object.fromEntries(r);
          },
          onLosses: (l) => {
            this.losses = l;
          },
          onDeviceDescriptors: (deviceDescriptors) => {
            this.deviceDescriptors = deviceDescriptors;
          },
          onReady: (r: ReadyMessage, sendFn) => {
            this.catalog = r.catalog;
            this.cables = r.cables;
            this.losses = r.losses;
            this.deviceDescriptors = r.deviceDescriptors;
            this.latencyMs = r.signalPathLatencyMs;
            this.send = sendFn;
            this.ready = true;
            this.paramValues = params.seedParamValues(this.scene, r.catalog);
            params.pushParams(sendFn, this.scene, r.catalog, this.paramValues);
            onReady(r, sendFn);
          },
        },
        this.volume,
      );
      this.#setVolume = control.setVolume;
      this.#resume = control.resume;
    } catch (err) {
      this.status = `error: ${err}`;
      this.started = false;
    }
  }
}
