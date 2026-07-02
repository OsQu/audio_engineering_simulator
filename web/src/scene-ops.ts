// Scene mutations: device/rack/space CRUD and connection edits. Pure given their inputs — they write
// through the `scene` proxy (reactive) but never reassign it, and **know nothing about the engine**:
// none of them hot-swap. The caller rebuilds the engine after the ones that change the runnable patch
// (commitCable, disconnect, setCableType, addDevice, removeDevice); the rest are UI-only furniture
// (spaces, racks, flip, cross-space moves) and need no rebuild. Each doc comment says which it is.

import {
  type CableType,
  type DeviceDescriptor,
  descriptorFor,
  type PortDomain,
  type PortKind,
} from "./catalog";
import { type ConnectVerdict, cableSpec } from "./connections";
import { wallSpawn } from "./placement";
import { deviceById, FRAME_MARGIN, type LayoutCtx, type PlacedItem, rackById } from "./projection";
import type { Connection } from "./scene";
import { newSpace, type Scene } from "./scene-store";
import { footprint, RACK_DEPTH_MM, RACK_UNIT_MM, RACK_WIDTH_MM, type Size3 } from "./spatial";

// A stable key for a connection (its two endpoints), for the {#each} in the cable overlay.
export const connKey = (c: Connection): string =>
  `${c.from.device}:${c.from.port}->${c.to.device}:${c.to.port}`;

// The carrier domain of a connection (from its output port), or null if unknown.
export function connectionDomain(
  scene: Scene,
  catalog: DeviceDescriptor[],
  c: Connection,
): PortDomain | null {
  const dev = deviceById(scene, c.from.device);
  const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
  return desc?.ports.find((p) => p.direction === "output" && p.id === c.from.port)?.domain ?? null;
}

// The connector kind of a connection (from its output port) — picks the cable's colour from the signal
// palette. Falls back to "line" (neutral grey) when the port can't be resolved.
export function connectionKind(scene: Scene, catalog: DeviceDescriptor[], c: Connection): PortKind {
  const dev = deviceById(scene, c.from.device);
  const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
  return desc?.ports.find((p) => p.direction === "output" && p.id === c.from.port)?.kind ?? "line";
}

// Apply a legal verdict to the patch: drop the replaced edge (fan-in is illegal, so a new cable into an
// occupied input replaces its source), add the new one. A fresh **analog** connection gets a transparent
// default cable (the first preset); digital/event stay ideal. **Caller must hot-swap.**
export function commitCable(
  scene: Scene,
  catalog: DeviceDescriptor[],
  cables: CableType[],
  v: ConnectVerdict,
): void {
  if (!v.ok) return;
  let conns = scene.patch.connections;
  if (v.replaces) {
    const rk = connKey(v.replaces);
    conns = conns.filter((c) => connKey(c) !== rk);
  }
  const conn: Connection = { from: v.connection.from, to: v.connection.to };
  if (connectionDomain(scene, catalog, conn) === "analog" && cables[0])
    conn.cable = cableSpec(cables[0]);
  scene.patch.connections = [...conns, conn];
}

// Remove a cable. Anything it fed now reads silence. **Caller must hot-swap** (and clear any selection).
export function disconnect(scene: Scene, c: Connection): void {
  const k = connKey(c);
  scene.patch.connections = scene.patch.connections.filter((x) => connKey(x) !== k);
}

// Set (or clear, `""` ⇒ ideal wire) the cable type on a connection — the cable's R·C is baked into the
// edge at compile, so **the caller must hot-swap** to apply it.
export function setCableType(
  scene: Scene,
  cables: CableType[],
  c: Connection,
  typeId: string,
): void {
  const idx = scene.patch.connections.findIndex((x) => connKey(x) === connKey(c));
  if (idx < 0) return;
  const preset = typeId ? cables.find((ct) => ct.typeId === typeId) : undefined;
  const updated: Connection = { from: { ...c.from }, to: { ...c.to } };
  if (preset) updated.cable = cableSpec(preset);
  scene.patch.connections[idx] = updated;
}

// Add a new space (room). UI-only furniture — no hot-swap. Returns the new space's id so the caller can
// switch to it.
export function addSpace(scene: Scene): string {
  let n = scene.ui.spaces.length + 1;
  while (scene.ui.spaces.some((s) => s.id === `space-${n}`)) n++;
  const space = newSpace(`space-${n}`, `Space ${n}`);
  scene.ui.spaces.push(space);
  return space.id;
}

// Send a free-standing device to another space (it lands at that space's floor origin). UI-only.
export function moveDeviceToSpace(scene: Scene, id: string, spaceId: string): void {
  const place = scene.ui.placements[id];
  if (!place) return;
  place.rack = undefined;
  place.space = spaceId;
  place.position = { x: 0, y: 0, z: 0 };
}

// Move a rack to another space; its mounted gear follows. UI-only.
export function moveRackToSpace(scene: Scene, id: string, spaceId: string): void {
  const rack = rackById(scene, id);
  if (!rack) return;
  rack.space = spaceId;
  for (const d of scene.patch.devices) {
    const place = scene.ui.placements[d.id];
    if (place?.rack?.id === id) place.space = spaceId;
  }
}

// Flip a unit front↔back to reach its rear I/O (no clearance step — flipping is direct). UI-only.
export function toggleFlip(scene: Scene, id: string): void {
  const place = scene.ui.placements[id];
  if (!place) return;
  place.facing = place.facing === "back" ? "front" : "back";
}

// Add gear from the catalog: a new instance placed free-standing on the wall in view (just past the
// existing gear). Its ports read silence until patched (Story 4.4). **Caller must hot-swap.**
export function addDevice(ctx: LayoutCtx, placedItems: PlacedItem[], typeId: string): void {
  const rightX = placedItems.reduce((m, it) => Math.max(m, it.rect.x + it.rect.width), 0);
  let n = 1;
  while (ctx.scene.patch.devices.some((d) => d.id === `${typeId}-${n}`)) n++;
  const id = `${typeId}-${n}`;
  const desc = descriptorFor(ctx.catalog, typeId);
  const size = desc ? footprint(desc.formFactor) : { width: 0, height: 0, depth: 0 };
  const { wall, position } = wallSpawn(ctx, size, rightX + 60);
  ctx.scene.patch.devices.push({ id, typeId });
  ctx.scene.ui.placements[id] = { space: ctx.space, wall, position, facing: "front" };
}

// Remove a device (never the output tap, which would invalidate the patch): drop it from the patch, its
// connections, and its placement. Anything it fed now reads silence. **Caller must hot-swap.**
export function removeDevice(scene: Scene, id: string): void {
  if (scene.patch.output.device === id) return;
  scene.patch.devices = scene.patch.devices.filter((d) => d.id !== id);
  scene.patch.connections = scene.patch.connections.filter(
    (c) => c.from.device !== id && c.to.device !== id,
  );
  delete scene.ui.placements[id];
}

// Add a rack — purely UI furniture (the engine has no racks), so no hot-swap.
export function addRack(ctx: LayoutCtx, placedItems: PlacedItem[]): void {
  const rightX = placedItems.reduce((m, it) => Math.max(m, it.rect.x + it.rect.width), 0);
  let n = 1;
  while (ctx.scene.ui.racks.some((r) => r.id === `rack-${n}`)) n++;
  const slots = 8;
  const frameSize: Size3 = {
    width: RACK_WIDTH_MM + 2 * FRAME_MARGIN,
    height: slots * RACK_UNIT_MM + 2 * FRAME_MARGIN,
    depth: RACK_DEPTH_MM,
  };
  const { wall, position } = wallSpawn(ctx, frameSize, rightX + 60);
  ctx.scene.ui.racks.push({ id: `rack-${n}`, space: ctx.space, wall, position, slots });
}

// Remove a rack — UI furniture, so no hot-swap. Un-mounts its gear, leaving each unit free-standing at
// the position it already had.
export function removeRack(scene: Scene, id: string): void {
  for (const d of scene.patch.devices) {
    const place = scene.ui.placements[d.id];
    if (place?.rack?.id === id) place.rack = undefined; // un-mount; keep its free position
  }
  scene.ui.racks = scene.ui.racks.filter((r) => r.id !== id);
}
