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

/** 1U rack-slot height, millimetres. */
export const RACK_UNIT_MM = 44.45;
/** Standard 19" rack width, millimetres. */
export const RACK_WIDTH_MM = 482.6;
/** Nominal rack chassis depth, millimetres. */
const RACK_DEPTH_MM = 300;

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

/** Do two axis-aligned rectangles overlap? Touching edges (zero overlap area) do not count. */
export function rectsOverlap(a: Rect2, b: Rect2): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
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
