// Cable-view vocabulary shared by the stages: socket alias keys, the projected-anchor shape, surface
// rects, and the scene view's portal labels/keys. Endpoint resolution, exits, paths, and mask passes live
// in cable-resolution.ts, the single pipeline full cables, portals, and held leads share.

import type { Connection } from "./scene";
import { connKey } from "./scene-ops";
import type { DeviceFacing, Scene } from "./scene-store";
import type { Wall } from "./spatial";

export type Pt = { x: number; y: number };

// A box in surface coords (y-down from the top), the {x,y,width,height} shape SVG wants — chassis
// silhouettes and slot frames.
export type SurfaceRect = { x: number; y: number; width: number; height: number };

// A socket anchor: its surface-local centre plus which face (front/back) of the chassis it sits on —
// projected from face-local geometry (socket-geometry.ts `projectAnchors`). Keeping `x`/`y` at the top
// level (not nested) makes this structurally a `Pt`, so a jack anchor can be used wherever a `Pt` is
// expected. `face` lets the renderer tell a jack on the *shown* face (anchor precisely) from one on the
// hidden face (projected mirrored, as rotateY(180deg) places it).
export type JackAnchor = Pt & { face: DeviceFacing };

// A jack's measurement key, "device:direction:port" (matches each jack's `data-jack` attribute).
export const jackKey = (device: string, direction: "input" | "output", port: number): string =>
  `${device}:${direction}:${port}`;

// How far a portal stub extends from its jack by default, in surface mm.
export const PORTAL_LEN = 180;

export const spaceName = (scene: Scene, id: string): string =>
  scene.ui.spaces.find((s) => s.id === id)?.name ?? id;

// A short label for where a cable's off-view end lives: the room name if it's in another space, else the
// wall name (a different wall of this same room). Scene-view-only (portals).
export function otherEndLabel(
  scene: Scene,
  space: string,
  wallLabels: Record<Wall, string>,
  deviceId: string,
): string {
  const p = scene.ui.placements[deviceId];
  if (!p) return "?";
  return p.space !== space ? spaceName(scene, p.space) : wallLabels[p.wall];
}

// A portal chip is identified per connection + which end is in view (each end shows its own chip in its
// own wall/room, so they move independently).
export const portalKey = (c: Connection, fromIn: boolean): string =>
  `${connKey(c)}|${fromIn ? "from" : "to"}`;
