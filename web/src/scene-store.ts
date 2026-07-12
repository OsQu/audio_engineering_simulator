// The authoritative *scene* the UI owns, and its durable save format.
//
// A scene is the whole studio: UI-only spatial data (spaces, racks, where each device sits) plus the
// runnable `Patch` the engine builds from. This is the layer the architecture put in TS, not Rust:
// the save file is **versioned JSON** owned here; the engine only ever receives the current `patch`
// projection (no spaces, racks, or placement), which it deserializes and builds.
//
// localStorage is disposable (no real scenes are persisted anywhere), so there is **no migration** —
// a save that doesn't match the current schema is simply discarded and the default scene is used.

import type { Patch } from "./scene";
import type { Room, Vec3, Wall } from "./spatial";

/** Current save-format version. A saved scene at any other version is discarded (no migration). Bumped
 *  to 15 in Task 5.7.8: the default scene became the round-trip **monitoring loop** through the computer
 *  (synth → 8i6 → computer → 8i6 → speaker), so a stale v14 save (the earlier placeholder chain) is
 *  discarded to bring every studio up to the new default. (v14 grew the 8i6 to the full 206-param unit
 *  and added the `computer` peer.)
 *  v16: the stock devices (synth_voice / midi_controller / speaker / computer) were shrunk to compact,
 *  8i6-scale footprints and given proper faceplates, so their placements changed — discard stale v15. */
export const SCHEMA_VERSION = 16;

/** A space (room) in the studio — a UI grouping over the one engine graph (the engine never knows
 *  about rooms). A space is a **rectangular room**: gear stands against one of four walls, each an
 *  elevation view you turn between, over a top-down floor plan (Story 4.6). */
export interface Space {
  /** Stable space id, referenced by a device's `Placement.space` and a `Rack.space`. */
  id: string;
  /** Human display name. */
  name: string;
  /** The room's floor extent + wall height (world mm). The four walls (front/back/left/right) are
   *  derived from this rectangle; nothing stores per-wall geometry. */
  room: Room;
}

/** A 19" rack — a container of U-slots that rackmount gear mounts into. UI-only (the engine has no
 *  rooms or racks). Racks can be repositioned (Story 4.3.5). */
export interface Rack {
  /** Stable rack id, referenced by a device's `Placement.rack.id`. */
  id: string;
  /** The space this rack stands in. */
  space: string;
  /** Which wall the rack stands against — the elevation it (and its mounted gear) appears in. */
  wall: Wall;
  /** Which way the rack is turned. A rack is a physical box: turning it around (`"back"`) exposes the
   *  rear I/O of **all** its mounted gear at once. Mounted gear can't be flipped on its own (it's bolted
   *  in) — its shown side follows the rack's `facing` (see `effectiveFacing`); to flip one unit you eject
   *  it from the rack first. */
  facing: DeviceFacing;
  /** Lower-left-front corner of the **U-slot region**, world millimetres (the frame draws around it). */
  position: Vec3;
  /** Number of U-slots. */
  slots: number;
}

/** Which panel faces the operator — the device can be flipped front↔back directly. */
export type DeviceFacing = "front" | "back";

/** Toggle a facing front↔back — the one flip operation, shared by every "turn it around" control (a
 *  free-standing device, a rack, a bench device). */
export const flip = (f: DeviceFacing): DeviceFacing => (f === "back" ? "front" : "back");

/** One device's placement in the spatial world. The **single 3-D coordinate truth** (Story 4.3.2's
 *  model projects it to a rendered view): free-standing gear lives at `position`; rack-mounted gear's
 *  position is derived from its rack + U-slot instead. UI-only — never sent to the engine. */
export interface Placement {
  /** The id of the space (room) this device sits in. */
  space: string;
  /** Which wall this device stands against — the elevation it appears in. Mounted gear inherits its
   *  rack's wall (kept in sync when it mounts), mirroring how it inherits the rack's `space`. */
  wall: Wall;
  /** Free-standing lower-left-front corner, world mm. Used when not mounted in a rack. */
  position: Vec3;
  /** If mounted: which rack + bottom U-slot. Absent ⇒ free-standing at `position`. */
  rack?: { id: string; uSlot: number };
  /** Which panel faces the operator (front or back). */
  facing: DeviceFacing;
}

/** A manual offset (surface mm) for a cross-view cable's portal chip from its jack anchor — lets the
 *  operator drag a portal out of the way. Keyed per connection + end (see `portalKey` in the app). */
export interface PortalOffset {
  dx: number;
  dy: number;
}

/** One pinned entry on the bench debug watch-list: a device instance's param, structural config, or
 *  readout, addressed by kind + id. `id` is the stringified param/readout id (its position in the
 *  exposed list) or the config key — a uniform string so it round-trips + keys cleanly. Lives in
 *  `scene.ui` so pins survive the `wasm:watch` reload (like the rest of the bench state). */
export interface BenchWatch {
  device: string;
  kind: "param" | "config" | "readout";
  id: string;
}

/** UI-only scene data — never sent to the engine. The spatial world: spaces, racks, where each device
 *  sits, and any moved portal chips. Placement keys are device instance ids (matching a patch
 *  `DeviceInstance.id`); `portals` keys are `${connectionKey}|${end}`. */
export interface SceneUi {
  spaces: Space[];
  racks: Rack[];
  placements: Record<string, Placement>;
  /** Manual portal-chip offsets; absent entries fall back to the default placement. */
  portals: Record<string, PortalOffset>;
  /** Per-device bench state on the flat **workbench** — drag offset (surface mm) + which face is turned
   *  toward the operator (the DUT ignores facing; it shows both faces at once). The bench is a 2-D layout,
   *  not a spatial room — no walls/racks/`placements`, so it can't reuse `Placement`. Absent entries sit
   *  at their signal-flow-stack slot, front-facing. Unused by the scene view; round-trips through the
   *  bench's URL persistence like the rest of `ui`. Optional so scene-view saves omit it. */
  bench?: Record<string, { x: number; y: number; facing?: DeviceFacing }>;
  /** Bench debug watch-list: the pinned params/configs/readouts the debug panel monitors live. Optional
   *  (only the bench writes it); round-trips through the bench URL so pins survive a `wasm:watch` reload. */
  benchWatch?: BenchWatch[];
}

/** A whole scene: a version stamp, UI-only spatial data, and the runnable patch. The unit we save/load. */
export interface Scene {
  schemaVersion: number;
  ui: SceneUi;
  patch: Patch;
}

/** Default room dimensions for a space (world mm) — a 4 m × 3 m room, 1.4 m of wall shown. */
const DEFAULT_ROOM: Room = { width: 4000, depth: 3000, height: 1400 };

/** A fresh space with default room dimensions — used by the default scene and the "add space" control. */
export function newSpace(id: string, name: string): Space {
  return { id, name, room: { ...DEFAULT_ROOM } };
}

/** The studio's single default space — a 4 m × 3 m control room, 1.4 m of wall shown. */
const CONTROL_ROOM: Space = newSpace("control-room", "Control Room");

/** A free-standing placement on the floor against `wall`, at floor position `(x, z)` world mm. */
function free(wall: Wall, x: number, z: number): Placement {
  return {
    space: CONTROL_ROOM.id,
    wall,
    position: { x, y: 0, z },
    facing: "front",
  };
}

/** The default studio: the classic **interface monitoring loop**, closed through the computer. A MIDI
 * controller plays a synth into the Scarlett 8i6's first combo input; the 8i6 records it over USB to the
 * computer (an 8-lane "send", metered per lane), the computer loops it straight back (send 1 → return 1),
 * and the 8i6 monitors the returning signal out Line Out 1 to the speaker. So the sound travels the full
 * round-trip **through the computer** — the return edge carries one block of latency (the computer's USB
 * output is a round-trip-latency source), which is exactly what lets the loop close without a feedback
 * cycle. The 8i6's routing matrix sits at its identity default (Pre 1 → USB 1 on the record side,
 * DAW 1 → Line Out 1 on the playback side), and the unit is switched on with its monitor open, so you
 * play out of the box by focusing the controller (its events cable also drives the synth's keybed).
 *
 * All five devices are desktop gear standing along the **front** wall (no rack). The `typeId`s match the
 * `devices` catalog; device ids are what control messages address. */
export function defaultScene(): Scene {
  const patch: Patch = {
    devices: [
      { id: "ctrl", typeId: "midi_controller" },
      { id: "synth", typeId: "synth_voice" },
      // The interface is switched on with its monitor open (both are device-level param groups; a real
      // 8i6 boots powered-off, so the scene turns it on). The routing matrix stays at its identity
      // default — precisely the record + playback path this loop needs.
      {
        id: "if",
        typeId: "scarlett_8i6",
        params: [
          { id: 204, value: 1.0 }, // Monitor level
          { id: 205, value: 1.0 }, // Power
        ],
      },
      { id: "computer", typeId: "computer" },
      { id: "spk", typeId: "speaker" },
    ],
    connections: [
      // Controller MIDI-OUT → synth MIDI-IN: play out of the box by focusing the controller.
      { from: { device: "ctrl", port: 0 }, to: { device: "synth", port: 0 } },
      // Synth → 8i6 combo input 1 (analog).
      { from: { device: "synth", port: 0 }, to: { device: "if", port: 0 } },
      // 8i6 USB send (8-lane) → computer (records + meters the sends).
      { from: { device: "if", port: 0 }, to: { device: "computer", port: 0 } },
      // Computer USB return (6-lane) → 8i6 USB return. This edge is *delayed* (the computer declares its
      // USB output a round-trip-latency source), which is what breaks the otherwise-cyclic loop.
      { from: { device: "computer", port: 0 }, to: { device: "if", port: 7 } },
      // 8i6 Line Out 1 (the monitored return) → speaker.
      { from: { device: "if", port: 2 }, to: { device: "spk", port: 0 } },
    ],
    output: { device: "spk", port: 0 },
  };

  const frontZ = 2600; // desktop gear stands near the front wall (room depth 3000)
  return {
    schemaVersion: SCHEMA_VERSION,
    ui: {
      spaces: [CONTROL_ROOM],
      racks: [],
      placements: {
        synth: free("front", 300, frontZ),
        ctrl: free("front", 1100, frontZ),
        if: free("front", 2000, frontZ),
        computer: free("front", 2900, frontZ),
        spk: free("front", 3600, frontZ),
      },
      portals: {},
    },
    patch,
  };
}

const STORAGE_KEY = "aes.scene";

/** Serialize a scene to the durable JSON string (human-readable, debuggable, diffable). */
export function serializeScene(scene: Scene): string {
  return JSON.stringify(scene);
}

/** Parse a saved scene string, or `null` if it's unreadable or not the current schema. No migration:
 *  a mismatched version is discarded (localStorage is disposable). Pure — no localStorage, so it's
 *  unit-testable. */
export function parseScene(raw: string): Scene | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const scene = parsed as Partial<Scene>;
  if (scene.schemaVersion !== SCHEMA_VERSION) return null;
  if (!scene.patch || !scene.ui) return null;
  return { schemaVersion: SCHEMA_VERSION, ui: scene.ui, patch: scene.patch };
}

/** Persist a scene to localStorage as versioned JSON. */
export function saveScene(scene: Scene): void {
  localStorage.setItem(STORAGE_KEY, serializeScene(scene));
}

/** Load the saved scene, or `null` if none / unreadable / not the current schema. */
export function loadScene(): Scene | null {
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw === null ? null : parseScene(raw);
}

/** Set a device's control-param value in the scene (so it persists on save), creating the entry if
 * needed. The live engine is driven separately; this keeps the saved scene in sync with the knobs. */
export function setSceneParam(scene: Scene, device: string, paramId: number, value: number): void {
  const dev = scene.patch.devices.find((d) => d.id === device);
  if (!dev) return;
  dev.params ??= [];
  const existing = dev.params.find((p) => p.id === paramId);
  if (existing) existing.value = value;
  else dev.params.push({ id: paramId, value });
}

/** Set a device's **structural config** value in the scene, creating the entry if needed. Unlike a
 * param, this changes how the device is *built*, so the caller must rebuild the engine (hot-swap). */
export function setSceneConfig(scene: Scene, device: string, key: string, value: number): void {
  const dev = scene.patch.devices.find((d) => d.id === device);
  if (!dev) return;
  dev.config ??= [];
  const existing = dev.config.find((c) => c.key === key);
  if (existing) existing.value = value;
  else dev.config.push({ key, value });
}
