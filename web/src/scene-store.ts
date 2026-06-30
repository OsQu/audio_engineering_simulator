// The authoritative *scene* the UI owns, and its durable save format.
//
// A scene is the whole studio: UI data (placement/spaces, not yet built) plus the runnable `Patch`
// the engine builds from. This is the layer the architecture put in TS, not Rust: the save file is
// **versioned JSON** owned here, with load-time migration; the engine only ever receives the current
// `patch` projection, which it deserializes and builds.

import type { Patch } from "./scene";

/** Current save-format version. Bump when the saved shape changes, and add a migration step below. */
export const SCHEMA_VERSION = 1;

/** UI-only scene data — never sent to the engine. Reserved for the spatial world (not yet built). */
export interface SceneUi {
  /** Per-device placement (rack/space/position). Lands with the spatial world (not yet built). */
  placements?: Record<string, { x: number; y: number; space?: string }>;
}

/** A whole scene: a version stamp, UI-only data, and the runnable patch. The unit we save/load. */
export interface Scene {
  schemaVersion: number;
  ui: SceneUi;
  patch: Patch;
}

/** The default studio: the chain `synth → gain → AD → DA → speaker`, tapped at the speaker. The gain
 * stage is unity by default (a passthrough) — it gives the panel UI a second controllable device with a
 * knob + power switch. The `typeId`s match the `devices` catalog; device ids are what control messages
 * address. */
export function defaultScene(): Scene {
  return {
    schemaVersion: SCHEMA_VERSION,
    ui: {},
    patch: {
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
    },
  };
}

const STORAGE_KEY = "aes.scene";

/** Persist a scene to localStorage as versioned JSON (human-readable, debuggable, diffable). */
export function saveScene(scene: Scene): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(scene));
}

/** Load the saved scene, migrated to the current schema, or `null` if none / unreadable. */
export function loadScene(): Scene | null {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw === null) return null;
  try {
    return migrate(JSON.parse(raw));
  } catch (err) {
    console.error("failed to load saved scene:", err);
    return null;
  }
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

/** Bring a parsed save forward to the current schema. Only v1 exists, so this validates and passes
 * through; future versions add vN→vN+1 steps here, before the engine ever sees the patch. */
function migrate(raw: unknown): Scene {
  if (typeof raw !== "object" || raw === null) throw new Error("scene is not an object");
  const scene = raw as Partial<Scene>;
  const version = typeof scene.schemaVersion === "number" ? scene.schemaVersion : 0;
  if (version > SCHEMA_VERSION) {
    throw new Error(`scene schemaVersion ${version} is newer than supported ${SCHEMA_VERSION}`);
  }
  // (no migration steps yet — v1 is the first format)
  if (!scene.patch) throw new Error("scene has no patch");

  return {
    schemaVersion: SCHEMA_VERSION,
    ui: scene.ui ?? {},
    patch: scene.patch,
  };
}
