// The pure, rendering-free spatial model for the studio world — the data + math behind device
// placement and the 2-D views. **No DOM / Svelte imports**: it is unit-tested in isolation (the
// project's "tests are the oracle" temperament, applied to the UI).
//
// The world stores a **single 3-D coordinate truth** per object (position + size + facing); every
// rendered view is a *projection* of it. Storing per-view 2-D positions is the trap that drifts views
// out of sync, so we never do it. Story 4.3 renders only the front elevation; the projection seam
// here is general so top/side views (Story 4.6) are a few lines, not a refactor.

import type { FormFactor } from "./catalog";

/** A point in the world's 3-D space, millimetres. The single coordinate truth. Axes: +x right, +y up,
 *  +z toward the viewer (out of the wall). A device's position is its lower-left-front corner. */
export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

/** A box size, millimetres: extent along x / y / z. */
export interface Size3 {
  width: number;
  height: number;
  depth: number;
}

/** A 2-D rectangle in a projected view, millimetres (+x right, +y up). The renderer maps it into
 *  screen space (and flips y for CSS top-down). */
export interface Rect2 {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Which 2-D plane a 3-D box projects onto: front uses (x,y), top uses (x,z), side uses (z,y). Story
 *  4.3 renders only "front"; "top"/"side" exist so the projection seam is proven (Story 4.6). */
export type ViewKind = "front" | "top" | "side";

/** Which wall of a rectangular room a device stands against — the elevation view it appears in. A
 *  placement carries this tag (Story 4.6); the wall view is a *projection* of its 3-D truth, never a
 *  stored per-view position. front/back run along x; left/right run along z (the shorter sides when
 *  depth < width). */
export type Wall = "front" | "back" | "left" | "right";

/** A rectangular room's floor extent (millimetres): `width` along x, `depth` along z. `height` is the
 *  wall's vertical extent (along y) — carried for the elevation/top-view chrome, not the projection
 *  math. The four walls fall out of this rectangle; nothing stores per-wall geometry. */
export interface Room {
  width: number;
  depth: number;
  height: number;
}

/** 1U rack-slot height, millimetres. */
export const RACK_UNIT_MM = 44.45;
/** Standard 19" rack width, millimetres. */
export const RACK_WIDTH_MM = 482.6;
/** Nominal rack chassis depth, millimetres. */
export const RACK_DEPTH_MM = 300;

/** The physical box of a device, derived from its catalog form factor: a rackmount unit's box comes
 *  from its U-count + the standard 19" width/depth; desktop gear carries its own authored box. */
export function footprint(form: FormFactor): Size3 {
  if (form.kind === "rackmount") {
    return {
      width: RACK_WIDTH_MM,
      height: form.rackUnits * RACK_UNIT_MM,
      depth: RACK_DEPTH_MM,
    };
  }
  return { width: form.widthMm, height: form.heightMm, depth: form.depthMm };
}

/** Project a positioned 3-D box onto a 2-D view by selecting which two axes map to the view plane —
 *  the one discipline that keeps multiple views in sync (never store per-view 2-D positions). */
export function project(pos: Vec3, size: Size3, view: ViewKind): Rect2 {
  switch (view) {
    case "front":
      return { x: pos.x, y: pos.y, width: size.width, height: size.height };
    case "top":
      return { x: pos.x, y: pos.z, width: size.width, height: size.depth };
    case "side":
      return { x: pos.z, y: pos.y, width: size.depth, height: size.height };
  }
}

/** A device's world-axis box **oriented for the wall it stands against**. A unit's panel always faces
 *  *into* the room, so against the left/right walls it's turned 90° about the vertical: the panel width
 *  now runs along z (depth into the room runs along x). front/back keep the catalog orientation (panel
 *  width along x, depth along z). This is what makes a device's elevation always show its panel width,
 *  and its top-view footprint sit the right way round, on every wall. */
export function orientedSize(size: Size3, wall: Wall): Size3 {
  return wall === "left" || wall === "right"
    ? { width: size.depth, height: size.height, depth: size.width }
    : size;
}

/** Project a positioned 3-D box onto one wall's **elevation** — the horizontal axis runs *along* that
 *  wall, the vertical is always world y (up). This is the multi-view generalization of `project(…,
 *  "front")` (Story 4.6): each wall is a projection of the single 3-D truth, never a stored 2-D position.
 *
 *  Convention — you stand inside the room looking outward at the wall, screen-x increasing to *your
 *  right*. Turning between walls flips left/right where it physically should:
 *  - **front** (far wall, looking +z): screen-x = world x (identity — matches the pre-4.6 single view).
 *  - **back** (near wall, turned 180°, looking −z): x mirrored about the room width.
 *  - **left** (x=0 wall, turned left, looking −x): screen-x = world z (uses the depth axis).
 *  - **right** (x=W wall, turned right, looking +x): z mirrored about the room depth.
 *  A box on `[a, a+w]` mirrored about extent `E` becomes `[E-(a+w), E-a]` — same width, origin reflected. */
export function wallProjection(pos: Vec3, size: Size3, wall: Wall, room: Room): Rect2 {
  switch (wall) {
    case "front":
      return { x: pos.x, y: pos.y, width: size.width, height: size.height };
    case "back":
      return {
        x: room.width - (pos.x + size.width),
        y: pos.y,
        width: size.width,
        height: size.height,
      };
    case "left":
      return { x: pos.z, y: pos.y, width: size.depth, height: size.height };
    case "right":
      return {
        x: room.depth - (pos.z + size.depth),
        y: pos.y,
        width: size.depth,
        height: size.height,
      };
  }
}

/** Inverse of {@link wallProjection} for the in-view axes: given an item dragged to elevation position
 *  `(elevX along the wall, elevY = height)`, recover its world lower-left-front corner. The
 *  perpendicular-to-wall axis isn't touched by an elevation drag, so it's carried over from `pos`. Pass
 *  the **catalog** size (unoriented); the mirror uses the *oriented* extent internally. This is what lets
 *  a device dragged on a mirrored (back/right) or rotated (left/right) elevation land where the cursor is. */
export function elevationToWorld(
  pos: Vec3,
  size: Size3,
  wall: Wall,
  room: Room,
  elevX: number,
  elevY: number,
): Vec3 {
  const os = orientedSize(size, wall);
  switch (wall) {
    case "front":
      return { x: elevX, y: elevY, z: pos.z };
    case "back":
      return { x: room.width - os.width - elevX, y: elevY, z: pos.z };
    case "left":
      return { x: pos.x, y: elevY, z: elevX };
    case "right":
      return { x: pos.x, y: elevY, z: room.depth - os.depth - elevX };
  }
}

/** Which wall a floor point is nearest — used to re-tag a device's `wall` when it's dragged across the
 *  top-down floor plan (Story 4.6.4). Compares the point's distance to each of the four wall edges. */
export function nearestWall(center: { x: number; z: number }, room: Room): Wall {
  const dLeft = center.x;
  const dRight = room.width - center.x;
  const dBack = center.z;
  const dFront = room.depth - center.z;
  const min = Math.min(dLeft, dRight, dBack, dFront);
  if (min === dLeft) return "left";
  if (min === dRight) return "right";
  if (min === dBack) return "back";
  return "front";
}

/** Do two axis-aligned rectangles overlap? Touching edges (zero overlap area) do not count. */
export function rectsOverlap(a: Rect2, b: Rect2): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/** Snap a coordinate to the nearest multiple of `step` — the free-placement grid the world layer drops
 *  gear onto (Story 4.6). A non-positive step is a no-op (returns `v` unchanged), so a caller can pass 0
 *  to disable snapping without a branch. */
export function snapToGrid(v: number, step: number): number {
  return step > 0 ? Math.round(v / step) * step : v;
}

// --- Rack U-slot model -----------------------------------------------------------------------------

/** A rack: a vertical column of `slots` U-slots (slot 0 at the bottom). */
export interface RackSpec {
  slots: number;
}

/** A device occupying a rack: its bottom slot (0-based) and U-height. */
export interface RackOccupant {
  startSlot: number;
  rackUnits: number;
}

/** Does an occupant lie wholly within the rack's bounds (and span at least 1U)? */
export function fitsInRack(rack: RackSpec, occ: RackOccupant): boolean {
  return occ.rackUnits >= 1 && occ.startSlot >= 0 && occ.startSlot + occ.rackUnits <= rack.slots;
}

/** Do two occupants' U-runs overlap? */
export function rackRunsOverlap(a: RackOccupant, b: RackOccupant): boolean {
  return a.startSlot < b.startSlot + b.rackUnits && b.startSlot < a.startSlot + a.rackUnits;
}

/** Can `occ` be placed in `rack` given the `existing` occupants — in-bounds and no slot collision?
 *  The placement-legality predicate the world layer (Stories 4.3.4 / 4.3.5) gates moves on. */
export function canPlaceInRack(
  rack: RackSpec,
  occ: RackOccupant,
  existing: RackOccupant[],
): boolean {
  return fitsInRack(rack, occ) && !existing.some((e) => rackRunsOverlap(occ, e));
}

/** The free start-slot nearest `desired` where a `units`-high device fits in `rack` given the
 *  `existing` occupants (exclude the device being placed), or `null` if nothing fits. Searches
 *  outward from `desired` — the drag-snap target finder (Story 4.3.5). */
export function nearestFreeSlot(
  rack: RackSpec,
  existing: RackOccupant[],
  units: number,
  desired: number,
): number | null {
  const maxStart = rack.slots - units;
  if (units < 1 || maxStart < 0) return null;
  const start = Math.max(0, Math.min(maxStart, Math.round(desired)));
  for (let d = 0; d <= rack.slots; d++) {
    for (const s of d === 0 ? [start] : [start - d, start + d]) {
      if (
        s >= 0 &&
        s <= maxStart &&
        canPlaceInRack(rack, { startSlot: s, rackUnits: units }, existing)
      ) {
        return s;
      }
    }
  }
  return null;
}
