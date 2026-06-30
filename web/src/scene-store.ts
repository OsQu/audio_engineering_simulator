// The authoritative *scene* the UI owns, and its durable save format.
//
// A scene is the whole studio: UI-only spatial data (where each device sits, in which space) plus the
// runnable `Patch` the engine builds from. This is the layer the architecture put in TS, not Rust:
// the save file is **versioned JSON** owned here; the engine only ever receives the current `patch`
// projection (no spaces, no placement), which it deserializes and builds.
//
// localStorage is disposable (no real scenes are persisted anywhere), so there is **no migration** —
// a save that doesn't match the current schema is simply discarded and the default scene is used.

import type { Patch } from "./scene";
import type { Vec3 } from "./spatial";

/** Current save-format version. A saved scene at any other version is discarded (no migration). */
export const SCHEMA_VERSION = 2;

/** A space (room) in the studio — a UI grouping over the one engine graph (the engine never knows
 *  about rooms). Multiple spaces + switching arrive in Story 4.3.6; the default scene has one. */
export interface Space {
  /** Stable space id, referenced by a device's `Placement.space`. */
  id: string;
  /** Human display name. */
  name: string;
}

/** One device's placement in the spatial world: the **single 3-D coordinate truth** (Story 4.3.2's
 *  model projects it to a rendered view) plus which space it lives in. UI-only — never sent to the
 *  engine. Rack/desk containment, facing, and clearance (back access) extend this in Stories
 *  4.3.5–4.3.6. */
export interface Placement {
  /** The id of the space (room) this device sits in. */
  space: string;
  /** The device's lower-left-front corner in the space's coordinates, millimetres. */
  position: Vec3;
}

/** UI-only scene data — never sent to the engine. The spatial world: the studio's spaces and where
 *  each device sits. Placement keys are device instance ids (matching a patch `DeviceInstance.id`). */
export interface SceneUi {
  spaces: Space[];
  placements: Record<string, Placement>;
}

/** A whole scene: a version stamp, UI-only spatial data, and the runnable patch. The unit we save/load. */
export interface Scene {
  schemaVersion: number;
  ui: SceneUi;
  patch: Patch;
}

/** The studio's single default space. */
const DEFAULT_SPACE: Space = { id: "control-room", name: "Control Room" };
/** Horizontal spacing between default free-standing devices, millimetres (a bit wider than a rack). */
const DEFAULT_SPACING_MM = 550;

/** The default studio: the chain `synth → gain → AD → DA → speaker`, tapped at the speaker, laid out
 * left-to-right on the floor of the one default space. The gain stage is unity (a passthrough) — it
 * gives the panel UI a second controllable device with a knob + power switch. The `typeId`s match the
 * `devices` catalog; device ids are what control messages address. */
export function defaultScene(): Scene {
  const patch: Patch = {
    devices: [
      { id: "synth", typeId: "synth_voice" },
      { id: "gain", typeId: "gain_stage" },
      { id: "ad", typeId: "ad_converter" },
      { id: "da", typeId: "da_converter" },
      { id: "spk", typeId: "speaker" },
    ],
    connections: [
      { from: { device: "synth", port: 0 }, to: { device: "gain", port: 0 } },
      { from: { device: "gain", port: 0 }, to: { device: "ad", port: 0 } },
      { from: { device: "ad", port: 0 }, to: { device: "da", port: 0 } },
      { from: { device: "da", port: 0 }, to: { device: "spk", port: 0 } },
    ],
    output: { device: "spk", port: 0 },
  };

  const placements: Record<string, Placement> = {};
  patch.devices.forEach((device, i) => {
    placements[device.id] = {
      space: DEFAULT_SPACE.id,
      position: { x: i * DEFAULT_SPACING_MM, y: 0, z: 0 },
    };
  });

  return {
    schemaVersion: SCHEMA_VERSION,
    ui: { spaces: [DEFAULT_SPACE], placements },
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
