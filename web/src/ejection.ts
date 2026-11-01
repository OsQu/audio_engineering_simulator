// Approved ejection policy U3 (Story 10.5): ejecting a rack-mounted device, or removing a rack with
// occupants, places every affected device at the nearest legal free position to its *current effective*
// location, preserving its effective room, wall, and facing (all inherited from the rack). The planner
// is pure and computes every placement before anything is written; a command either applies the whole
// plan or leaves the scene untouched. Remembered pre-mount positions are never restored.
//
// Search: in the rack's own wall elevation (view-independent, so removing a rack from the top view uses
// its wall), over the `GRID_MM` free-placement lattice inside the room bounds, by increasing squared
// distance from the device's effective slot rect, tie-broken by smaller y then smaller x. Obstacles are
// every other object on that wall (free gear, racks, gear mounted in other racks) plus the placements
// already allocated by this plan; occupants allocate in slot order. A removed rack's frame is not an
// obstacle; a retained rack (single eject) is.

import { type DeviceDescriptor, descriptorFor } from "./catalog";
import { resolveEffectivePlacement } from "./effective-placement";
import { deviceRect, GRID_MM, type LayoutCtx, placedItemsFor, rackById } from "./projection";
import type { Placement, Rack, Scene } from "./scene-store";
import { elevationToWorld, footprint, type Rect2, type Room, rectsOverlap } from "./spatial";

export type EjectionRequest =
  | { kind: "device"; deviceId: string }
  | { kind: "rack"; rackId: string };

export type EjectionFailure = "not-mounted" | "missing-rack" | "unresolved" | "no-free-space";

export type EjectionPlan =
  | { ok: true; placements: Readonly<Record<string, Placement>> }
  | { ok: false; reason: EjectionFailure; deviceId?: string };

/** The rack's wall elevation as a layout context (independent of the currently shown view). */
function rackElevation(
  scene: Scene,
  catalog: DeviceDescriptor[],
  rack: Rack,
  room: Room,
): LayoutCtx {
  return { space: rack.space, view: rack.wall, wall: rack.wall, room, scene, catalog };
}

/** Lattice candidates for a `w × h` box on a wall `length` long, nearest `origin` first. */
function candidatesNear(origin: Rect2, length: number, room: Room): { x: number; y: number }[] {
  const out: { x: number; y: number; d: number }[] = [];
  for (let x = 0; x <= length - origin.width; x += GRID_MM) {
    for (let y = 0; y <= room.height - origin.height; y += GRID_MM) {
      out.push({ x, y, d: (x - origin.x) ** 2 + (y - origin.y) ** 2 });
    }
  }
  return out.sort((a, b) => a.d - b.d || a.y - b.y || a.x - b.x);
}

/** Compute every ejected free placement, or a failure. Pure: never writes to `scene`. */
export function planEjection(
  scene: Scene,
  catalog: DeviceDescriptor[],
  request: EjectionRequest,
): EjectionPlan {
  let rack: Rack | undefined;
  let ejectees: string[];
  if (request.kind === "device") {
    const mount = scene.ui.placements[request.deviceId]?.rack;
    if (!mount) return { ok: false, reason: "not-mounted", deviceId: request.deviceId };
    rack = rackById(scene, mount.id);
    ejectees = [request.deviceId];
  } else {
    rack = rackById(scene, request.rackId);
    const rackId = request.rackId;
    ejectees = scene.patch.devices
      .filter((d) => scene.ui.placements[d.id]?.rack?.id === rackId)
      .map((d) => d.id)
      .sort((a, b) => {
        const slot = (id: string) => scene.ui.placements[id]?.rack?.uSlot ?? 0;
        return slot(a) - slot(b) || (a < b ? -1 : a > b ? 1 : 0);
      });
  }
  if (!rack) return { ok: false, reason: "missing-rack" };
  if (ejectees.length === 0) return { ok: true, placements: {} }; // an empty rack just goes
  const room = scene.ui.spaces.find((space) => space.id === rack.space)?.room;
  if (!room) return { ok: false, reason: "unresolved" };

  const ctx = rackElevation(scene, catalog, rack, room);
  const leaving = new Set(ejectees);
  const removedRack = request.kind === "rack" ? rack.id : null;
  const obstacles: Rect2[] = placedItemsFor(ctx)
    .filter((it) => !leaving.has(it.id) && it.id !== removedRack)
    .map((it) => it.rect);
  const length = rack.wall === "front" || rack.wall === "back" ? room.width : room.depth;

  const placements: Record<string, Placement> = {};
  for (const id of ejectees) {
    const effective = resolveEffectivePlacement(scene, catalog, id);
    const device = scene.patch.devices.find((d) => d.id === id);
    const descriptor = device ? descriptorFor(catalog, device.typeId) : undefined;
    const origin = device ? deviceRect(ctx, id, device.typeId) : null;
    if (effective.kind !== "studio-mounted" || !descriptor || !origin) {
      return { ok: false, reason: "unresolved", deviceId: id };
    }
    const spot = candidatesNear(origin, length, room).find(
      ({ x, y }) =>
        !obstacles.some((o) =>
          rectsOverlap({ x, y, width: origin.width, height: origin.height }, o),
        ),
    );
    if (!spot) return { ok: false, reason: "no-free-space", deviceId: id };
    obstacles.push({ x: spot.x, y: spot.y, width: origin.width, height: origin.height });
    placements[id] = {
      space: effective.space,
      wall: effective.wall,
      facing: effective.facing,
      position: elevationToWorld(
        rack.position,
        footprint(descriptor.formFactor),
        effective.wall,
        room,
        spot.x,
        spot.y,
      ),
    };
  }
  return { ok: true, placements };
}

/** Write a successful plan through the scene proxy; a removed rack goes only after its children move. */
export function applyEjection(
  scene: Scene,
  request: EjectionRequest,
  plan: Extract<EjectionPlan, { ok: true }>,
): void {
  for (const [id, placement] of Object.entries(plan.placements)) {
    scene.ui.placements[id] = placement;
  }
  if (request.kind === "rack") {
    scene.ui.racks = scene.ui.racks.filter((r) => r.id !== request.rackId);
  }
}

/** Plan, then apply all-or-nothing. Returns the plan so the caller can report a failure. */
export function eject(
  scene: Scene,
  catalog: DeviceDescriptor[],
  request: EjectionRequest,
): EjectionPlan {
  const plan = planEjection(scene, catalog, request);
  if (plan.ok) applyEjection(scene, request, plan);
  return plan;
}
