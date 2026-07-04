// Cable-rendering geometry + view membership: which cable ends are in the shown view, where each end
// anchors on the surface, and the portal-chip placement for a cross-view end. Pure given the layout ctx,
// the DOM-measured jack anchors, and the world api (all injected), so the estimate math (the chassis-edge
// fallback ratios, the default portal stub) is testable with a fake api.

import { deviceById, deviceRect, effectiveFacing, type LayoutCtx } from "./projection";
import type { Connection, PortRef } from "./scene";
import { connKey } from "./scene-ops";
import type { DeviceFacing, Scene } from "./scene-store";
import type { Wall } from "./spatial";
import type { WorldApi } from "./widgets/WorldView.svelte";

export type Pt = { x: number; y: number };

// A measured jack anchor: its surface-local centre plus which face (front/back) of the chassis it sits
// on. Keeping `x`/`y` at the top level (not nested) makes this structurally a `Pt`, so a jack anchor can
// be used wherever a `Pt` is expected. `face` lets the renderer tell a jack on the *shown* face (anchor
// precisely) from one on the hidden face (its measured centre is mirrored under rotateY(180deg)).
export type JackAnchor = Pt & { face: DeviceFacing };

// A jack's measurement key, "device:direction:port" (matches each jack's `data-jack` attribute).
export const jackKey = (device: string, direction: "input" | "output", port: number): string =>
  `${device}:${direction}:${port}`;

// How far a portal stub extends from its jack by default, in surface mm.
export const PORTAL_LEN = 180;

// Is a device visible in the current view — in the shown space *and* against the shown wall? Only one
// wall of one space renders at a time, so this is the "same view" test the cable renderer keys on.
export const inView = (ctx: LayoutCtx, deviceId: string): boolean => {
  const p = ctx.scene.ui.placements[deviceId];
  return p?.space === ctx.space && p.wall === ctx.wall;
};
// A cable with both ends in view draws as a full lead; exactly one end here → a portal stub toward the
// other view; neither → not shown. The engine sees a plain mono connection either way (UI-only).
export const bothInView = (ctx: LayoutCtx, c: Connection): boolean =>
  inView(ctx, c.from.device) && inView(ctx, c.to.device);
export const oneInView = (ctx: LayoutCtx, c: Connection): boolean =>
  inView(ctx, c.from.device) !== inView(ctx, c.to.device);

export const spaceName = (scene: Scene, id: string): string =>
  scene.ui.spaces.find((s) => s.id === id)?.name ?? id;

// A short label for where a cable's off-view end lives: the room name if it's in another space, else the
// wall name (a different wall of this same room).
export function otherEndLabel(
  ctx: LayoutCtx,
  wallLabels: Record<Wall, string>,
  deviceId: string,
): string {
  const p = ctx.scene.ui.placements[deviceId];
  if (!p) return "?";
  return p.space !== ctx.space ? spaceName(ctx.scene, p.space) : wallLabels[p.wall];
}

// A portal chip is identified per connection + which end is in view (each end shows its own chip in its
// own wall/room, so they move independently).
export const portalKey = (c: Connection, fromIn: boolean): string =>
  `${connKey(c)}|${fromIn ? "from" : "to"}`;

// A portal's offset from its jack anchor: the operator's dragged value, else the default stub placement
// (out toward the signal-flow direction, dropped a little below the jack).
export function portalOffset(
  scene: Scene,
  c: Connection,
  fromIn: boolean,
): { dx: number; dy: number } {
  return (
    scene.ui.portals?.[portalKey(c, fromIn)] ?? { dx: fromIn ? PORTAL_LEN : -PORTAL_LEN, dy: 36 }
  );
}

// The surface-local anchor for one end of a cable. A faceplate can place a jack on **either** face, so
// the question is "is this jack on the currently-shown face?", not "is the device flipped?". When the
// jack's measured face matches the device's shown (effective) facing, anchor at the measured socket
// centre; otherwise (jack on the hidden face, or not yet measured) fall back to the chassis-edge estimate
// (output → right, input → left). `null` when the device isn't in the shown space — a cross-space end is
// drawn as a portal, not a continuous cable.
//
// Facing is the **effective** facing (rack-aware): a rack-mounted unit shows whichever side its rack is
// turned to, so a flipped rack exposes its gear's back sockets even though each unit's own `facing` stays
// "front". A jack on the hidden face still has a layout box (backface-visibility only hides paint), but it
// sits under rotateY(180deg), so its measured centre is horizontally mirrored — using it would anchor the
// cable at the wrong place, hence the face-match gate.
export function cableAnchor(
  ctx: LayoutCtx,
  jackAnchors: Record<string, JackAnchor>,
  ref: PortRef,
  direction: "input" | "output",
  api: WorldApi,
): Pt | null {
  const device = deviceById(ctx.scene, ref.device);
  const place = ctx.scene.ui.placements[ref.device];
  if (!device || !place || !inView(ctx, ref.device)) return null;
  const jack = jackAnchors[jackKey(ref.device, direction, ref.port)];
  // Precise: the real socket, but only when it's on the shown face (its measurement is trustworthy then).
  if (jack && jack.face === effectiveFacing(ctx.scene, ref.device)) return jack;
  const rect = deviceRect(ctx, ref.device, device.typeId);
  if (!rect) return null;
  // Jack on the hidden face (or not yet measured): estimate near the chassis centre — nudged toward the
  // signal-flow direction (output right, input left) so the cable emerges toward its neighbour. This end
  // is drawn *behind* the device (hidden), so a rough estimate is enough; it just needs to look plausible
  // where it tucks under the edges.
  const wx = rect.x + rect.width * (direction === "output" ? 0.62 : 0.38);
  const wy = rect.y + rect.height * 0.45;
  return api.worldToSurface(wx, wy);
}
