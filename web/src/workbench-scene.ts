// Pure scene builders for the device workbench (Epic 6). Kept out of the `.svelte` shell so the logic is
// node-testable and the component stays thin. The workbench runs one device on a bench with no spatial
// UI — these wrap a minimal runnable `Patch` in a bare `Scene` (empty `ui`, since the workbench doesn't
// use WorldView). Device-only + silent: no source or monitor chain (that is the Story-6.3 rig).

import type { DeviceDescriptor } from "./catalog";
import type { Patch } from "./scene";
import { SCHEMA_VERSION, type Scene } from "./scene-store";

/** The single device's instance id on the bench. */
export const BENCH_DEVICE = "dev";
/** A guaranteed-valid device to boot on before the catalog is known: the worklet only posts the catalog
 *  after `SceneEngine(patch)` builds, so we boot this known-good type first, then resolve the requested
 *  `typeId` against the catalog that comes back. `synth_voice` is a core device present since Epic 2. */
export const BOOTSTRAP_TYPE = "synth_voice";

/** Wrap a runnable patch in a minimal Scene — empty spatial `ui` (the workbench has no WorldView). */
function sceneOf(patch: Patch): Scene {
  return {
    schemaVersion: SCHEMA_VERSION,
    ui: { spaces: [], racks: [], placements: {}, portals: {} },
    patch,
  };
}

/** The bootstrap scene: a lone `synth_voice`, output-tapped at port 0 — just enough for the engine to
 *  build so its constructor posts the catalog. Silent (nothing drives it). */
export function bootstrapScene(): Scene {
  return sceneOf({
    devices: [{ id: BENCH_DEVICE, typeId: BOOTSTRAP_TYPE }],
    connections: [],
    output: { device: BENCH_DEVICE, port: 0 },
  });
}

/** The device's first **analog** output port id, or `undefined` if it has none. The output tap is
 *  rendered as a *voltage*, so it must be an analog port — tapping a digital output (e.g. the 8i6's port-0
 *  USB send, or any output on a digital-only device like the computer) makes `render_quantum` fault, which
 *  is session-fatal. A digital-only-output device therefore can't be tapped without a DA — that is the
 *  Story-6.3 monitor chain — so the workbench must check this and refuse rather than build such a scene. */
export function analogOutputPort(desc: DeviceDescriptor): number | undefined {
  return desc.ports.find((p) => p.direction === "output" && p.domain === "analog")?.id;
}

/** The minimal scene for a specific device: the device alone, output-tapped at the given analog output
 *  port (from {@link analogOutputPort}). Device-only + silent — no source/monitor rig yet (Story 6.3). */
export function deviceScene(typeId: string, outputPort: number): Scene {
  return sceneOf({
    devices: [{ id: BENCH_DEVICE, typeId }],
    connections: [],
    output: { device: BENCH_DEVICE, port: outputPort },
  });
}
