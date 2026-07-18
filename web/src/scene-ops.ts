// Scene mutations: device/rack/space CRUD and connection edits. Pure given their inputs — they write
// through the `scene` proxy (reactive) but never reassign it, and **know nothing about the engine**:
// none of them hot-swap. The caller rebuilds the engine after the ones that change the runnable patch
// (commitCable, disconnect, setCableType, addDevice, removeDevice); the rest are UI-only furniture
// (spaces, racks, flip, cross-space moves) and need no rebuild. Each doc comment says which it is.

import {
  type CableType,
  type Connector,
  type DeviceDescriptor,
  descriptorFor,
  type PortDomain,
  type PortKind,
} from "./catalog";
import { type ConnectVerdict, cableSpec } from "./connections";
import { wallSpawn } from "./placement";
import { deviceById, FRAME_MARGIN, type LayoutCtx, type PlacedItem, rackById } from "./projection";
import type { Connection, DeviceInstance } from "./scene";
import { type BenchWatch, flip, newSpace, type Scene } from "./scene-store";
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

// Whether a device's events input is fed by a cable (an incoming connection to its events-in port). If
// so, host-injected notes are a no-op — the performance comes from the patched source instead — so the
// on-screen keybed shows disabled and the keyboard doesn't target it. Shared by both views' note routing.
export function eventsInputDriven(scene: Scene, desc: DeviceDescriptor, deviceId: string): boolean {
  const evPort = desc.ports.find((p) => p.direction === "input" && p.domain === "events");
  if (!evPort) return false;
  return scene.patch.connections.some((c) => c.to.device === deviceId && c.to.port === evPort.id);
}

// The connector kind of a connection (from its output port) — picks the cable's colour from the signal
// palette. Falls back to "line" (neutral grey) when the port can't be resolved.
export function connectionKind(scene: Scene, catalog: DeviceDescriptor[], c: Connection): PortKind {
  const dev = deviceById(scene, c.from.device);
  const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
  return desc?.ports.find((p) => p.direction === "output" && p.id === c.from.port)?.kind ?? "line";
}

// The physical connector of a connection (from its output port). Both ends share it — legality requires
// compatible connectors — so this is the axis the cable picker filters on and the default cable matches.
// `null` when the output port can't be resolved.
export function connectionConnector(
  scene: Scene,
  catalog: DeviceDescriptor[],
  c: Connection,
): Connector | null {
  const dev = deviceById(scene, c.from.device);
  const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
  return (
    desc?.ports.find((p) => p.direction === "output" && p.id === c.from.port)?.connector ?? null
  );
}

// The cable presets that physically fit `c` — those whose connector matches the connection's ports.
// The inspector picker offers only these (you can't plug an XLR cable into a ¼" jack).
export function cablesFor(
  scene: Scene,
  catalog: DeviceDescriptor[],
  cables: CableType[],
  c: Connection,
): CableType[] {
  const connector = connectionConnector(scene, catalog, c);
  return connector === null ? [] : cables.filter((ct) => ct.connector === connector);
}

// Apply a legal verdict to the patch: drop the replaced edge (fan-in is illegal, so a new cable into an
// occupied input replaces its source), add the new one. A fresh **analog** connection gets the default
// cable **matching its connector** (the shortest/least-lossy preset of that connector, since the catalog
// is ordered that way) — not always the ¼" patch cable; digital/event stay ideal. **Caller must hot-swap.**
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
  // Preserve every field the verdict carries — notably `duplex` (a USB-C link expands to both edges at
  // build); reconstructing from `from`/`to` alone silently dropped it, leaving a one-way cable.
  const conn: Connection = { ...v.connection };
  if (connectionDomain(scene, catalog, conn) === "analog") {
    const cable = cablesFor(scene, catalog, cables, conn)[0];
    if (cable) conn.cable = cableSpec(cable);
  }
  scene.patch.connections = [...conns, conn];
}

// Remove a cable. Anything it fed now reads silence. **Caller must hot-swap** (and clear any selection).
export function disconnect(scene: Scene, c: Connection): void {
  const k = connKey(c);
  scene.patch.connections = scene.patch.connections.filter((x) => connKey(x) !== k);
}

// --- Computer USB enumeration ---------------------------------------------------------------------
// A `computer` has no channel count of its own — it adopts whatever the attached interface publishes.
// After a USB duplex cable to a computer is committed or removed, re-derive that computer's
// `usb_sends`/`usb_returns` structural config from the interface now on the other end (or revert to the
// built-in 2×2 when nothing's attached), resetting its routing matrix to the loopback default. This is
// the derive-from-the-published-face rule made a live gesture (plugging a cable); Rust's
// `ChannelCountMismatch` stays the backstop. Config → recompile, so the **caller must hot-swap**.
const COMPUTER_TYPE = "computer";
const USB_SENDS_KEY = "usb_sends";
const USB_RETURNS_KEY = "usb_returns";
const DEFAULT_USB_CHANNELS = 2; // the built-in sound card, shown when nothing is attached

const isComputer = (scene: Scene, deviceId: string): boolean =>
  deviceById(scene, deviceId)?.typeId === COMPUTER_TYPE;

// The channel counts a device publishes on its USB jack — its USB **output** (the "send", → the
// computer's `usb_sends`) and USB **input** (the "return", → `usb_returns`) lane counts, from its
// descriptor. `null` if it has no USB output/input pair.
function usbShape(
  scene: Scene,
  catalog: DeviceDescriptor[],
  deviceId: string,
): { sends: number; returns: number } | null {
  const dev = deviceById(scene, deviceId);
  const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
  if (!desc) return null;
  const send = desc.ports.find((p) => p.direction === "output" && p.connector === "usb");
  const ret = desc.ports.find((p) => p.direction === "input" && p.connector === "usb");
  return send && ret ? { sends: send.channels, returns: ret.channels } : null;
}

// The non-computer interface currently cabled to `computerId` over USB (the peer of its one duplex USB
// link), or `null` if nothing's attached. A computer↔computer USB link is deliberately skipped — neither
// side adapts to the other (the equal-count check still guards that edge).
function attachedInterface(
  scene: Scene,
  catalog: DeviceDescriptor[],
  computerId: string,
): string | null {
  for (const c of scene.patch.connections) {
    if (!c.duplex) continue;
    const ends = [c.from.device, c.to.device];
    if (!ends.includes(computerId)) continue;
    const peer = ends.find((id) => id !== computerId);
    if (!peer || isComputer(scene, peer)) continue;
    if (connectionConnector(scene, catalog, c) !== "usb") continue;
    return peer;
  }
  return null;
}

// The computer's current `(sends, returns)` from its config, defaulting to the built-in 2×2.
function currentUsb(dev: DeviceInstance): [number, number] {
  const get = (key: string): number =>
    dev.config?.find((cs) => cs.key === key)?.value ?? DEFAULT_USB_CHANNELS;
  return [get(USB_SENDS_KEY), get(USB_RETURNS_KEY)];
}

// Re-enumerate the computer end of `conn` (if either end is a computer) against the interface now cabled
// to it: adopt the peer's published send/return counts, or fall back to 2×2 with nothing attached.
// Returns whether the shape changed. On a change the matrix is reset to the loopback default — the
// crosspoint ids reshuffle with the return count, so saved overrides no longer map (reset, not remap).
// A no-op for a connection touching no computer, or a computer↔computer USB link. **Caller must hot-swap.**
export function enumerateComputerUsb(
  scene: Scene,
  catalog: DeviceDescriptor[],
  conn: Connection,
): boolean {
  const computerId = [conn.from.device, conn.to.device].find((id) => isComputer(scene, id));
  if (!computerId) return false;

  const peer = attachedInterface(scene, catalog, computerId);
  const shape = peer ? usbShape(scene, catalog, peer) : null;
  const sends = shape?.sends ?? DEFAULT_USB_CHANNELS;
  const returns = shape?.returns ?? DEFAULT_USB_CHANNELS;

  const dev = deviceById(scene, computerId);
  if (!dev) return false;
  const [curSends, curReturns] = currentUsb(dev);
  if (curSends === sends && curReturns === returns) return false; // unchanged — keep the routing

  dev.config = [
    ...(dev.config ?? []).filter((cs) => cs.key !== USB_SENDS_KEY && cs.key !== USB_RETURNS_KEY),
    { key: USB_SENDS_KEY, value: sends },
    { key: USB_RETURNS_KEY, value: returns },
  ];
  dev.params = []; // reset crosspoints to the construction loopback default
  return true;
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

// Flip a **free-standing** unit front↔back to reach its rear I/O (no clearance step — flipping is
// direct). Rack-mounted gear is bolted in and can't be flipped on its own — turn its rack around
// (`toggleRackFlip`) or eject it first. UI-only.
export function toggleFlip(scene: Scene, id: string): void {
  const place = scene.ui.placements[id];
  if (!place || place.rack) return;
  place.facing = flip(place.facing);
}

// Turn a whole rack around front↔back, exposing (or hiding) the rear I/O of all its mounted gear at
// once — the mounted gear's shown side follows the rack (see `effectiveFacing`). UI-only.
export function toggleRackFlip(scene: Scene, id: string): void {
  const rack = rackById(scene, id);
  if (!rack) return;
  rack.facing = flip(rack.facing);
}

// Turn a **workbench** device around front↔back. The bench has no rooms/racks/placements, so its facing
// lives on the bench entry (beside its drag offset); `effectiveFacing` reads it. UI-only — the DUT shows
// both faces at once and never calls this; only the supporting cast rotates.
export function toggleBenchFacing(scene: Scene, id: string): void {
  scene.ui.bench ??= {};
  scene.ui.bench[id] ??= { x: 0, y: 0 };
  const entry = scene.ui.bench[id];
  entry.facing = flip(entry.facing ?? "front");
}

// --- Bench debug watch-list (Story 6.4) -----------------------------------------------------------
// Pin management for the bench debug panel: the pinned set lives on `scene.ui.benchWatch` so it
// round-trips through the bench URL (pins survive a `wasm:watch` reload). UI-only — the engine never
// sees `ui`. Pure scene writes, like the rest of this module.

// A stable key for a watch item (device + kind + id) — for equality + the {#each}.
export const watchKey = (w: BenchWatch): string => `${w.device}|${w.kind}|${w.id}`;

// Whether an item is currently pinned.
export function isWatched(scene: Scene, item: BenchWatch): boolean {
  const key = watchKey(item);
  return (scene.ui.benchWatch ?? []).some((w) => watchKey(w) === key);
}

// Pin/unpin an item on the watch-list (toggle). Creates the list on first pin; leaves it in place
// (possibly empty) once created so the URL keeps a stable shape.
export function toggleWatch(scene: Scene, item: BenchWatch): void {
  scene.ui.benchWatch ??= [];
  const key = watchKey(item);
  const list = scene.ui.benchWatch;
  const at = list.findIndex((w) => watchKey(w) === key);
  if (at >= 0) list.splice(at, 1);
  else list.push({ device: item.device, kind: item.kind, id: item.id });
}

// Eject a device from its rack, leaving it free-standing at the position it already carried (so it can
// then be flipped on its own). No-op for gear that isn't racked. UI-only.
export function unmount(scene: Scene, id: string): void {
  const place = scene.ui.placements[id];
  if (!place?.rack) return;
  place.rack = undefined;
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

// Add a catalog device to the **workbench** bench: a new instance appended to the flat stack (source →
// DUT → monitor → …added gear), unwired, reading silence until patched. Unlike the scene view's
// `addDevice`, a bench device gets no wall/rack placement — the bench is one flat layout, so its offset
// and facing live lazily in `scene.ui.bench` (created on first drag/flip) and it just appends to the
// stack. **Caller must hot-swap.** Returns the new instance id.
export function addBenchDevice(scene: Scene, typeId: string): string {
  let n = 1;
  while (scene.patch.devices.some((d) => d.id === `${typeId}-${n}`)) n++;
  const id = `${typeId}-${n}`;
  scene.patch.devices.push({ id, typeId });
  return id;
}

// Remove a device (never the output tap, which would invalidate the patch): drop it from the patch, its
// connections, its placement, and any bench offset. Anything it fed now reads silence. **Caller must
// hot-swap.**
export function removeDevice(scene: Scene, id: string): void {
  if (scene.patch.output.device === id) return;
  scene.patch.devices = scene.patch.devices.filter((d) => d.id !== id);
  scene.patch.connections = scene.patch.connections.filter(
    (c) => c.from.device !== id && c.to.device !== id,
  );
  delete scene.ui.placements[id];
  if (scene.ui.bench) delete scene.ui.bench[id];
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
  ctx.scene.ui.racks.push({
    id: `rack-${n}`,
    space: ctx.space,
    wall,
    facing: "front",
    position,
    slots,
  });
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
