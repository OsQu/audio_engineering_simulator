// Placement legality + drag-commit logic: given an elevation/floor drag, decide whether a spot is
// legal and, on commit, write the result back through the scene proxy. Pure given a LayoutCtx — no
// Svelte, no DOM — so the rack-snap and wall-retag rules are unit-testable.
//
// Mutation contract (see the split plan): these functions may write *through* the `ctx.scene` proxy
// (property writes stay reactive regardless of which file they run in), but never reassign it. None
// of them touch the engine — the caller hot-swaps where the table in the plan says to (placement is
// UI-only furniture, so it never does).

import { descriptorFor } from "./catalog";
import {
  deviceById,
  deviceUnits,
  FRAME_MARGIN,
  isRack,
  type LayoutCtx,
  type PlacedItem,
  rackById,
  rackFrameSize,
  rackRect,
  type ViewCtx,
} from "./projection";
import {
  elevationToWorld,
  footprint,
  nearestFreeSlot,
  nearestWall,
  orientedSize,
  RACK_UNIT_MM,
  type RackOccupant,
  type Rect2,
  rectsOverlap,
  type Size3,
  type Vec3,
  type Wall,
} from "./spatial";

// The rack occupants (start-slot + U-height) in `rackId`, excluding one device — the collision set the
// slot finder needs.
export function rackOccupants(ctx: LayoutCtx, rackId: string, excludeId: string): RackOccupant[] {
  const occ: RackOccupant[] = [];
  for (const d of ctx.scene.patch.devices) {
    if (d.id === excludeId) continue;
    const place = ctx.scene.ui.placements[d.id];
    if (place?.rack?.id === rackId) {
      occ.push({ startSlot: place.rack.uSlot, rackUnits: deviceUnits(ctx.catalog, d.typeId) });
    }
  }
  return occ;
}

// If elevation `(x,y)` lands over an open rack on the shown wall, the nearest free start-slot a
// `units`-high device fits at — else null. The drag-snap target. `(x,y)` are elevation coords (the
// rack is compared via its projected frame), so this works identically on every wall.
export function rackSlotAt(
  ctx: LayoutCtx,
  excludeId: string,
  x: number,
  y: number,
  units: number,
): { rackId: string; slot: number } | null {
  for (const rack of ctx.scene.ui.racks) {
    if (rack.space !== ctx.space || rack.wall !== ctx.wall) continue;
    const frame = rackRect(ctx, rack);
    const slotOy = frame.y + FRAME_MARGIN;
    const within =
      x >= frame.x &&
      x <= frame.x + frame.width &&
      y >= slotOy &&
      y <= slotOy + rack.slots * RACK_UNIT_MM;
    if (!within) continue;
    const desired = Math.floor((y - slotOy) / RACK_UNIT_MM);
    const slot = nearestFreeSlot(
      { slots: rack.slots },
      rackOccupants(ctx, rack.id, excludeId),
      units,
      desired,
    );
    if (slot !== null) return { rackId: rack.id, slot };
  }
  return null;
}

// Legality for live drag feedback + the commit gate. In the **top-down plan** any floor spot is legal
// (free layout — overlaps are the operator's business). In a **wall elevation** racks reposition freely
// and a device is legal if it can mount in a rack at `(x,y)` or stands free without overlapping another
// item (its elevation width is always the panel width, since a unit faces the room). `placedItems` is
// the caller's already-derived set — passed in, not recomputed.
export function canPlace(
  ctx: LayoutCtx,
  placedItems: PlacedItem[],
  id: string,
  x: number,
  y: number,
): boolean {
  if (ctx.view === "top" || isRack(ctx.scene, id)) return true;
  const device = deviceById(ctx.scene, id);
  if (!device) return false;
  const desc = descriptorFor(ctx.catalog, device.typeId);
  if (!desc) return false;
  const units = deviceUnits(ctx.catalog, device.typeId);
  if (units > 0 && rackSlotAt(ctx, id, x, y, units)) return true;
  const size = footprint(desc.formFactor);
  const candidate: Rect2 = { x, y, width: size.width, height: size.height };
  return !placedItems.some((it) => it.id !== id && rectsOverlap(candidate, it.rect));
}

// Commit a drag (only ever called for a legal spot). Routes by view: the top-down plan repositions on
// the floor, the wall elevations map back through `elevationToWorld`.
export function moveTo(ctx: LayoutCtx, id: string, x: number, y: number): void {
  if (ctx.view === "top") {
    moveToTop(ctx, id, x, y);
    return;
  }
  if (!ctx.wall) return;
  const rack = rackById(ctx.scene, id);
  if (rack) {
    rack.position = elevationToWorld(rack.position, rackFrameSize(rack), rack.wall, ctx.room, x, y);
    return;
  }
  const device = deviceById(ctx.scene, id);
  const place = ctx.scene.ui.placements[id];
  if (!device || !place) return;
  const units = deviceUnits(ctx.catalog, device.typeId);
  const hit = units > 0 ? rackSlotAt(ctx, id, x, y, units) : null;
  if (hit) {
    const rack = rackById(ctx.scene, hit.rackId);
    place.rack = { id: hit.rackId, uSlot: hit.slot };
    if (rack) {
      place.space = rack.space; // a mounted device lives in its rack's space…
      place.wall = rack.wall; // …and against its rack's wall
    }
  } else {
    const desc = descriptorFor(ctx.catalog, device.typeId);
    const size = desc ? footprint(desc.formFactor) : { width: 0, height: 0, depth: 0 };
    place.rack = undefined;
    place.position = elevationToWorld(place.position, size, place.wall, ctx.room, x, y);
  }
}

// Commit a floor-plan drag: `(x,y)` is `(world x, world z)`. Reposition on the floor and **re-tag the
// wall** the item now sits against (its box centre decides), so it appears in that wall's elevation. A
// rack's mounted gear follows its wall.
export function moveToTop(ctx: LayoutCtx, id: string, x: number, y: number): void {
  const rack = rackById(ctx.scene, id);
  if (rack) {
    rack.position = { x, y: rack.position.y, z: y };
    const s = orientedSize(rackFrameSize(rack), rack.wall);
    const w = nearestWall({ x: x + s.width / 2, z: y + s.depth / 2 }, ctx.room);
    if (w !== rack.wall) {
      rack.wall = w;
      for (const d of ctx.scene.patch.devices) {
        const pl = ctx.scene.ui.placements[d.id];
        if (pl?.rack?.id === rack.id) pl.wall = w; // mounted gear follows its rack's wall
      }
    }
    return;
  }
  const device = deviceById(ctx.scene, id);
  const place = ctx.scene.ui.placements[id];
  if (!device || !place) return;
  const desc = descriptorFor(ctx.catalog, device.typeId);
  const s = orientedSize(
    desc ? footprint(desc.formFactor) : { width: 0, height: 0, depth: 0 },
    place.wall,
  );
  place.rack = undefined;
  place.position = { x, y: place.position.y, z: y };
  place.wall = nearestWall({ x: x + s.width / 2, z: y + s.depth / 2 }, ctx.room);
}

// The wall + world position a newly-added item spawns at: flush against the wall in view, at elevation
// `elevX` (just past the existing gear). Top view has no wall in view, so it falls back to the front.
export function wallSpawn(
  ctx: ViewCtx,
  size: Size3,
  elevX: number,
): { wall: Wall; position: Vec3 } {
  const wall = ctx.wall ?? "front";
  const FLUSH = 400; // nominal depth of the against-the-wall zone, world mm
  const seed: Vec3 =
    wall === "front"
      ? { x: 0, y: 0, z: ctx.room.depth - FLUSH }
      : wall === "right"
        ? { x: ctx.room.width - FLUSH, y: 0, z: 0 }
        : { x: 0, y: 0, z: 0 }; // back / left sit against the origin walls
  return { wall, position: elevationToWorld(seed, size, wall, ctx.room, elevX, 0) };
}
