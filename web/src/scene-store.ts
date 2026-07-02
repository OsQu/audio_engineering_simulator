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
 *  to 7 when a space became a rectangular room with four walls (Story 4.6): spaces gained a `room` and
 *  placements/racks a `wall` tag, so a stale v6 save doesn't lack them. */
export const SCHEMA_VERSION = 7;

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
  /** Lower-left-front corner of the **U-slot region**, world millimetres (the frame draws around it). */
  position: Vec3;
  /** Number of U-slots. */
  slots: number;
}

/** Which panel faces the operator — the device can be flipped front↔back directly. */
export type DeviceFacing = "front" | "back";

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

/** UI-only scene data — never sent to the engine. The spatial world: spaces, racks, and where each
 *  device sits. Placement keys are device instance ids (matching a patch `DeviceInstance.id`). */
export interface SceneUi {
  spaces: Space[];
  racks: Rack[];
  placements: Record<string, Placement>;
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

/** A rack-mounted placement at `uSlot` of `rackId` on `wall` (its free `position` is where it stands if
 *  unmounted). Mounted gear shares its rack's wall. */
function mounted(
  rackId: string,
  uSlot: number,
  wall: Wall,
  freeX: number,
  freeZ: number,
): Placement {
  return {
    space: CONTROL_ROOM.id,
    wall,
    position: { x: freeX, y: 0, z: freeZ },
    rack: { id: rackId, uSlot },
    facing: "front",
  };
}

/** The default studio: the chain `synth → gain → VU → AD → digital meter → DA → speaker`, tapped at
 * the speaker. The two meters sit either side of the AD so gain-staging across the converter is
 * visible out of the box: the VU reads the analog level in dBu, the digital meter the same signal in
 * dBFS. The gain stage and meters are unity passthroughs.
 *
 * Spatial layout (Story 4.6): the rackmount gain/VU/AD/meter/DA mount in an 8U rack against the **back**
 * wall (z≈0); the synth and speaker (desktop gear) stand along the **front** wall (z near the room's far
 * depth), where the window to the live room is. So the signal chain runs front→back→front — its cables
 * cross walls, drawn as portal stubs in each elevation. The `typeId`s match the `devices` catalog;
 * device ids are what control messages address. */
export function defaultScene(): Scene {
  const patch: Patch = {
    devices: [
      { id: "synth", typeId: "synth_voice" },
      { id: "gain", typeId: "gain_stage" },
      { id: "vu", typeId: "vu_meter" },
      { id: "ad", typeId: "ad_converter" },
      { id: "dig", typeId: "digital_meter" },
      { id: "da", typeId: "da_converter" },
      { id: "spk", typeId: "speaker" },
    ],
    connections: [
      { from: { device: "synth", port: 0 }, to: { device: "gain", port: 0 } },
      { from: { device: "gain", port: 0 }, to: { device: "vu", port: 0 } },
      { from: { device: "vu", port: 0 }, to: { device: "ad", port: 0 } },
      { from: { device: "ad", port: 0 }, to: { device: "dig", port: 0 } },
      { from: { device: "dig", port: 0 }, to: { device: "da", port: 0 } },
      { from: { device: "da", port: 0 }, to: { device: "spk", port: 0 } },
    ],
    output: { device: "spk", port: 0 },
  };

  const rackX = 760; // the back-wall rack's world-x
  const frontZ = 2600; // desktop gear stands near the front wall (room depth 3000)
  return {
    schemaVersion: SCHEMA_VERSION,
    ui: {
      spaces: [CONTROL_ROOM],
      racks: [
        {
          id: "rack-1",
          space: CONTROL_ROOM.id,
          wall: "back",
          position: { x: rackX, y: 0, z: 0 },
          slots: 8,
        },
      ],
      placements: {
        synth: free("front", 200, frontZ),
        gain: mounted("rack-1", 0, "back", rackX, 0),
        vu: mounted("rack-1", 1, "back", rackX, 0),
        ad: mounted("rack-1", 2, "back", rackX, 0),
        dig: mounted("rack-1", 3, "back", rackX, 0),
        da: mounted("rack-1", 4, "back", rackX, 0),
        spk: free("front", 2800, frontZ),
      },
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
