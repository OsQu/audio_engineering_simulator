// Pure scene builders for the device workbench. Kept out of the `.svelte` shell so the logic is
// node-testable and the component stays thin. The workbench runs the device-under-test on a bench with
// no spatial UI — these wrap a runnable `Patch` in a bare `Scene` (empty `ui`, since the workbench
// doesn't use WorldView). Nothing is auto-wired: the user adds and patches any other gear by hand.

import type { DeviceDescriptor } from "./catalog";
import type { Patch, PortRef } from "./scene";
import { SCHEMA_VERSION, type Scene } from "./scene-store";

/** The device-under-test's instance id on the bench. */
export const BENCH_DEVICE = "dev";
/** The instance id the fallback bench tap addresses: a speaker the user patches the DUT through. */
export const SPEAKER_DEVICE = "spk";

/** A guaranteed-valid device to boot on before the catalog is known: the worklet only posts the catalog
 *  after `SceneEngine(patch)` builds, so we boot this known-good type first, then resolve the requested
 *  `typeId` against the catalog that comes back. `synth_voice` is a core device. */
export const BOOTSTRAP_TYPE = "synth_voice";
/** The speaker's type id — its descriptor resolves the fallback tap. */
const SPEAKER_TYPE = "speaker";

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
 *  is session-fatal. A digital-only-output device therefore can't be tapped directly; the bench listens at
 *  the speaker instead (patch the digital output through the DA→speaker) — see {@link defaultBenchTap}. */
export function analogOutputPort(desc: DeviceDescriptor): number | undefined {
  return desc.ports.find((p) => p.direction === "output" && p.domain === "analog")?.id;
}

/** The initial monitored tap for a bench scene: the DUT's own first analog output if it has one, else the
 *  speaker's analog tap (the monitor terminus). The fallback is what makes a **digital-only** device (e.g.
 *  the computer) benchable — you patch its digital output through the DA→speaker and listen at the speaker.
 *  `undefined` only if neither has an analog output (the speaker always does, so in practice defined). */
export function defaultBenchTap(
  dut: DeviceDescriptor,
  speaker: DeviceDescriptor,
): PortRef | undefined {
  const dutOut = analogOutputPort(dut);
  if (dutOut !== undefined) return { device: BENCH_DEVICE, port: dutOut };
  const spkOut = analogOutputPort(speaker);
  if (spkOut !== undefined) return { device: SPEAKER_DEVICE, port: spkOut };
  return undefined;
}

/** The bench scene for a device-under-test: the DUT alone, unwired — plus the speaker when the DUT has
 *  no analog output and the tap falls back to it. `output` starts at {@link defaultBenchTap}
 *  (user-retargetable by clicking an analog output jack). Needs the live catalog to resolve the
 *  speaker descriptor + the default tap; returns `undefined` if the speaker is missing from the catalog
 *  or no analog tap resolves (a catalog regression). */
export function benchScene(dut: DeviceDescriptor, catalog: DeviceDescriptor[]): Scene | undefined {
  const speaker = catalog.find((d) => d.typeId === SPEAKER_TYPE);
  if (!speaker) return undefined;
  const output = defaultBenchTap(dut, speaker);
  if (!output) return undefined;
  // The patch's output must name a device in the patch: when the tap falls back to the speaker (a
  // digital-only DUT), the speaker instance comes onto the bench with it.
  const devices = [{ id: BENCH_DEVICE, typeId: dut.typeId }];
  if (output.device === SPEAKER_DEVICE) devices.push({ id: SPEAKER_DEVICE, typeId: SPEAKER_TYPE });
  return sceneOf({ devices, connections: [], output });
}
