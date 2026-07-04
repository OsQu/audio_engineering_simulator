// The engine session: the view-agnostic consumer surface over the worklet engine (PROJECT_PLAN §7 —
// the UI is a pure consumer of the engine API, factored here so a second view root can construct the
// same interaction path). This first slice owns the engine lifecycle + the live readout state that
// stream in from the worklet; the scene, param/config lanes, note routing, and patching move in over
// the rest of Story 6.1. A class with `$state` fields (the codebase's first `.svelte.ts` rune module),
// constructed once per view root — App builds one in its script and reads it throughout.

import type { CableType, DeviceDescriptor } from "./catalog";
import {
  type ControlMessage,
  type EngineControl,
  type HealthMessage,
  healthSummary,
  type ReadyMessage,
  startEngine,
} from "./engine";
import type { Patch } from "./scene";

// Monitor (listening) volume — a host-side output gain *outside* the simulation, persisted on its own
// (a per-listener setting, not scene/simulation data). Defaults low so it doesn't blast.
const VOLUME_KEY = "aes.volume";
function loadVolume(): number {
  const s = localStorage.getItem(VOLUME_KEY);
  if (s === null) return 0.25;
  const raw = Number(s);
  return Number.isFinite(raw) ? Math.max(0, Math.min(1, raw)) : 0.25;
}

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

  // --- Monitor volume -----------------------------------------------------------------------------
  volume = $state(loadVolume());
  #setVolume: ((gain: number) => void) | null = null;

  // A device's current reading for a readout id, or the meter floor if none has arrived yet. An arrow
  // property so it can be passed as a callback (`this` stays bound to the instance).
  readingFor = (device: string, id: number): number => this.readings[device]?.[id] ?? -120;

  onVolume(v: number): void {
    this.volume = v;
    this.#setVolume?.(v);
    localStorage.setItem(VOLUME_KEY, String(v));
  }

  // Bring the engine up: instantiate the worklet with the initial patch + monitor volume, and wire the
  // streaming callbacks to this session's state. `onReady` lets the view root finish bring-up with the
  // pieces still living view-side this task (param seeding, MIDI) — those fold in over the rest of 6.1.
  async start(
    initialPatch: Patch,
    onReady: (r: ReadyMessage, send: (msg: ControlMessage) => void) => void,
  ): Promise<void> {
    if (this.started) return;
    this.started = true;
    try {
      const control: EngineControl = await startEngine(
        initialPatch,
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
          onReady: (r: ReadyMessage, sendFn) => {
            this.catalog = r.catalog;
            this.cables = r.cables;
            this.losses = r.losses;
            this.send = sendFn;
            this.ready = true;
            onReady(r, sendFn);
          },
        },
        this.volume,
      );
      this.#setVolume = control.setVolume;
    } catch (err) {
      this.status = `error: ${err}`;
      this.started = false;
    }
  }
}
