// Pure rect/item derivation for the spatial stage: how a scene's racks and devices project into the
// currently-shown view (one of the four wall elevations, or the top-down floor plan). No Svelte, no
// DOM — every function takes an explicit context so it stays unit-testable (Vitest, node env).
//
// The rect logic is shared across views: a rack projects its frame footprint, a device its panel
// footprint (or, when rack-mounted, a slot inside the already-projected rack frame — elevation only,
// since mounted gear is hidden in the top view). Each shape has one projection path that branches on
// `ctx.view`, not the four near-duplicate helpers this replaces.

import { type DeviceDescriptor, descriptorFor } from "./catalog";
import type { DeviceFacing, Rack, Scene } from "./scene-store";
import {
  footprint,
  orientedSize,
  project,
  RACK_DEPTH_MM,
  RACK_UNIT_MM,
  RACK_WIDTH_MM,
  type Rect2,
  type Room,
  type Size3,
  type Wall,
  wallProjection,
} from "./spatial";

export const FRAME_MARGIN = 14; // mm of rack frame drawn around the U-slot region
export const GRID_MM = 50; // free-placement snap grid (world mm) — eases aligning gear on the floor

/** What the current view shows. `wall === null` ⇔ `view === "top"`. */
export type ViewCtx = { space: string; view: Wall | "top"; wall: Wall | null; room: Room };
/** The layout inputs every rect/placement computation needs. */
export type LayoutCtx = ViewCtx & { scene: Scene; catalog: DeviceDescriptor[] };

/** One item to render on the stage: its id, its rect in the current view, and its z-order. */
export type PlacedItem = { id: string; rect: Rect2; background?: boolean; z: number };

export const deviceById = (scene: Scene, id: string) =>
  scene.patch.devices.find((d) => d.id === id);
export const rackById = (scene: Scene, id: string) => scene.ui.racks.find((r) => r.id === id);
export const isRack = (scene: Scene, id: string): boolean => rackById(scene, id) !== undefined;

// Which panel side a device actually shows. A **rack-mounted** unit is bolted in, so it can't be flipped
// on its own — its side follows the rack's `facing` (turn the whole rack around to reach the gear's rear
// I/O). A **free-standing** unit follows its own `facing`. A device with no placement is a **workbench**
// device: it follows its bench `facing` (the flat bench has no rooms/racks). Defaults to "front". This is
// the single source of truth for rendering, z-order, and jack anchoring — shared by both view roots.
export function effectiveFacing(scene: Scene, deviceId: string): DeviceFacing {
  const place = scene.ui.placements[deviceId];
  if (!place) return scene.ui.bench?.[deviceId]?.facing ?? "front";
  if (place.rack) return rackById(scene, place.rack.id)?.facing ?? place.facing;
  return place.facing;
}

// How many U a device occupies — 0 if it isn't rackmount gear (so it never mounts in a rack).
export function deviceUnits(catalog: DeviceDescriptor[], typeId: string): number {
  const desc = descriptorFor(catalog, typeId);
  return desc && desc.formFactor.kind === "rackmount" ? desc.formFactor.rackUnits : 0;
}

// A rack's frame footprint (world mm) — the U-slot column plus the drawn margin, RACK_DEPTH deep.
export const rackFrameSize = (rack: Rack): Size3 => ({
  width: RACK_WIDTH_MM + 2 * FRAME_MARGIN,
  height: rack.slots * RACK_UNIT_MM + 2 * FRAME_MARGIN,
  depth: RACK_DEPTH_MM,
});

// A rack's rect in the current view: its top-down floor footprint in the plan view (racks show as one
// box; mounted gear is hidden), else its frame in the current wall's elevation (the draggable box drawn
// behind its gear).
export function rackRect(ctx: ViewCtx, rack: Rack): Rect2 {
  const size = orientedSize(rackFrameSize(rack), rack.wall);
  return ctx.view === "top"
    ? project(rack.position, size, "top")
    : wallProjection(rack.position, size, rack.wall, ctx.room);
}

// A device's rect in the current view. In the **top plan**: its floor footprint (wall-oriented so a
// side-wall unit sits the right way round). In a **wall elevation**: derived from its rack + U-slot when
// mounted (its elevation width is always the panel width — a unit faces into the room on every wall — so
// only the horizontal position is wall-dependent, hence placed inside the already-projected rack frame),
// else its free-standing position projected onto its wall. `null` without a descriptor/placement.
export function deviceRect(ctx: LayoutCtx, deviceId: string, typeId: string): Rect2 | null {
  const desc = descriptorFor(ctx.catalog, typeId);
  const place = ctx.scene.ui.placements[deviceId];
  if (!desc || !place) return null;
  const size = footprint(desc.formFactor);
  if (ctx.view === "top") {
    return project(place.position, orientedSize(size, place.wall), "top");
  }
  if (place.rack) {
    const rack = rackById(ctx.scene, place.rack.id);
    if (!rack) return null;
    const frame = rackRect(ctx, rack);
    return {
      x: frame.x + FRAME_MARGIN,
      y: frame.y + FRAME_MARGIN + place.rack.uSlot * RACK_UNIT_MM,
      width: RACK_WIDTH_MM,
      height: size.height,
    };
  }
  return wallProjection(place.position, orientedSize(size, place.wall), place.wall, ctx.room);
}

// The items to render, for the current view. In a **wall elevation**: rack frames (behind) then that
// wall's devices (on top), with `z` interleaving them with the cable layer (z 2) so a continuous cable
// occludes right: a **back-shown** device sits *below* the cables (z 1) so a lead reaches its rear
// sockets, a **front-shown** one *above* (z 3) so cables tuck behind its panel. A cable plugged into a
// visible front socket is occluded near the plug; the renderer redraws a short lead-tip above the panel
// (see `tipPatchEnd`) so the plug still reads as seated. In the **top-down plan**: the whole room's
// racks + free-standing gear as floor footprints (mounted gear is hidden inside its rack; cables aren't
// drawn — no visible jacks from above).
export function placedItemsFor(ctx: LayoutCtx): PlacedItem[] {
  const { scene, space } = ctx;
  if (ctx.view === "top") {
    return [
      ...scene.ui.racks
        .filter((r) => r.space === space)
        .map((r): PlacedItem => ({ id: r.id, rect: rackRect(ctx, r), background: true, z: 0 })),
      ...scene.patch.devices
        .filter((d) => {
          const p = scene.ui.placements[d.id];
          return p?.space === space && !p.rack; // mounted gear lives inside its rack box
        })
        .map((d) => ({ id: d.id, rect: deviceRect(ctx, d.id, d.typeId), z: 3 }))
        .filter((it): it is PlacedItem => it.rect !== null),
    ];
  }
  return [
    ...scene.ui.racks
      .filter((r) => r.space === space && r.wall === ctx.wall)
      .map((r): PlacedItem => ({ id: r.id, rect: rackRect(ctx, r), background: true, z: 0 })),
    ...scene.patch.devices
      .filter((d) => {
        const p = scene.ui.placements[d.id];
        return p?.space === space && p.wall === ctx.wall;
      })
      .map((d) => ({
        id: d.id,
        rect: deviceRect(ctx, d.id, d.typeId),
        z: effectiveFacing(scene, d.id) === "back" ? 1 : 3,
      }))
      .filter((it): it is PlacedItem => it.rect !== null),
  ];
}
