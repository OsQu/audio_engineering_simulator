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

// A device's chassis box in surface coords (y-down from the top), the {x,y,width,height} shape SVG wants.
// Used to clip a front-plugged lead to the panel it hangs across, and to clamp a hidden-face estimate to
// the chassis silhouette. See `deviceSurfaceRect`.
export type SurfaceRect = { x: number; y: number; width: number; height: number };

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

// A resolved cable end: its surface point plus whether it came from the interior *estimate* (as opposed
// to a precise measured socket). The `estimate` flag is what the back-shown clamp keys on — an estimate
// on a device that sits below the cables needs clamping to the chassis edge; a measured socket never does.
type Resolved = { pt: Pt; estimate: boolean };

// The surface-local anchor for one end of a cable, tagged with how it was resolved. A faceplate can place
// a jack on **either** face, so the question is "is this jack on the currently-shown face?", not "is the
// device flipped?". When the jack's measured face matches the device's shown (effective) facing, anchor at
// the measured socket centre; otherwise (jack on the hidden face, or not yet measured) fall back to an
// estimate near the chassis centre, nudged toward the signal-flow direction. `null` when the device isn't
// in the shown space — a cross-space end is drawn as a portal, not a continuous cable.
//
// Facing is the **effective** facing (rack-aware): a rack-mounted unit shows whichever side its rack is
// turned to, so a flipped rack exposes its gear's back sockets even though each unit's own `facing` stays
// "front". A jack on the hidden face still has a layout box (backface-visibility only hides paint), but it
// sits under rotateY(180deg), so its measured centre is horizontally mirrored — using it would anchor the
// cable at the wrong place, hence the face-match gate. This is the single place that branch lives;
// `cableAnchor` and `cableEndpoints` both build on it.
function resolveAnchor(
  ctx: LayoutCtx,
  jackAnchors: Record<string, JackAnchor>,
  ref: PortRef,
  direction: "input" | "output",
  api: WorldApi,
): Resolved | null {
  const device = deviceById(ctx.scene, ref.device);
  const place = ctx.scene.ui.placements[ref.device];
  if (!device || !place || !inView(ctx, ref.device)) return null;
  const jack = jackAnchors[jackKey(ref.device, direction, ref.port)];
  // Precise: the real socket, but only when it's on the shown face (its measurement is trustworthy then).
  if (jack && jack.face === effectiveFacing(ctx.scene, ref.device))
    return { pt: jack, estimate: false };
  const rect = deviceRect(ctx, ref.device, device.typeId);
  if (!rect) return null;
  // Jack on the hidden face (or not yet measured): estimate near the chassis centre, nudged toward the
  // signal-flow direction (output right, input left) at a little above mid-height. The lead tucks under
  // the panel edges, so it only needs to look plausible where it emerges — an interior point reads as
  // wrapping around to the hidden face, rather than sprouting mid-faceplate.
  const wx = rect.x + rect.width * (direction === "output" ? 0.62 : 0.38);
  const wy = rect.y + rect.height * 0.45;
  return { pt: api.worldToSurface(wx, wy), estimate: true };
}

export function cableAnchor(
  ctx: LayoutCtx,
  jackAnchors: Record<string, JackAnchor>,
  ref: PortRef,
  direction: "input" | "output",
  api: WorldApi,
): Pt | null {
  return resolveAnchor(ctx, jackAnchors, ref, direction, api)?.pt ?? null;
}

// A device's chassis rect in surface coords: `deviceRect` gives world mm (y-up), so convert both corners
// through `worldToSurface` (which flips y about the room height) and normalise to top-left origin. `null`
// without a resolvable descriptor/placement. Shared by the tip-patch clip and the hidden-face clamp.
export function deviceSurfaceRect(
  ctx: LayoutCtx,
  deviceId: string,
  api: WorldApi,
): SurfaceRect | null {
  const device = deviceById(ctx.scene, deviceId);
  if (!device) return null;
  const rect = deviceRect(ctx, deviceId, device.typeId);
  if (!rect) return null;
  const c1 = api.worldToSurface(rect.x, rect.y);
  const c2 = api.worldToSurface(rect.x + rect.width, rect.y + rect.height);
  return {
    x: Math.min(c1.x, c2.x),
    y: Math.min(c1.y, c2.y),
    width: Math.abs(c2.x - c1.x),
    height: Math.abs(c2.y - c1.y),
  };
}

// Where does the segment from `inside` (a point within `rect`) toward `outside` leave the rect? Returns
// the exit point on the rect boundary, or `null` when `outside` is also inside (no crossing — a degenerate
// overlap). Pure parametric clip: P(t) = inside + t·(outside − inside), the smallest t in (0, 1] at which
// P hits an edge. A zero axis-component simply skips that pair of edges (axis-parallel segments).
function segmentRectExit(inside: Pt, outside: Pt, rect: SurfaceRect): Pt | null {
  const dx = outside.x - inside.x;
  const dy = outside.y - inside.y;
  const left = rect.x;
  const right = rect.x + rect.width;
  const top = rect.y;
  const bottom = rect.y + rect.height;
  const EPS = 1e-9;
  let best = Number.POSITIVE_INFINITY;
  // A candidate crossing at parameter `t` counts if it's ahead of `inside` (t > 0), within the segment
  // (t ≤ 1), and its point lies on the rect's perimeter (within the opposite pair of edges).
  const consider = (t: number): void => {
    if (t <= EPS || t > 1 + EPS) return;
    const x = inside.x + t * dx;
    const y = inside.y + t * dy;
    if (x >= left - EPS && x <= right + EPS && y >= top - EPS && y <= bottom + EPS && t < best) {
      best = t;
    }
  };
  if (Math.abs(dx) > EPS) {
    consider((left - inside.x) / dx);
    consider((right - inside.x) / dx);
  }
  if (Math.abs(dy) > EPS) {
    consider((top - inside.y) / dy);
    consider((bottom - inside.y) / dy);
  }
  if (!Number.isFinite(best)) return null;
  return { x: inside.x + best * dx, y: inside.y + best * dy };
}

// Clamp one resolved end to the chassis silhouette when it needs it. An interior *estimate* on a
// **back-shown** device (one that sits *below* the cable layer) would otherwise show mid-panel in thin
// air — the estimate only reads right when the device is *above* the cables and hides it. So we stop the
// lead where the segment toward the cable's `other` end crosses the device rect, so it tucks behind the
// chassis. A measured end, a front-shown device, or an `other` end inside the same rect (degenerate
// overlap → no crossing) all keep the resolved point unchanged.
function clampEnd(ctx: LayoutCtx, deviceId: string, end: Resolved, other: Pt, api: WorldApi): Pt {
  if (!end.estimate || effectiveFacing(ctx.scene, deviceId) !== "back") return end.pt;
  const rect = deviceSurfaceRect(ctx, deviceId, api);
  if (!rect) return end.pt;
  return segmentRectExit(end.pt, other, rect) ?? end.pt;
}

// Both of a cable's ends resolved to surface points, with the back-shown estimate clamp applied per end.
// The single source of a drawn cable's geometry: `oneCable` and its tip patch both call this, so the
// z-layer lead and the overlay copy trace the exact same path. `null` when either end isn't in view (that
// connection is a cross-view portal, not a continuous cable). Each end clamps toward the *other* end's
// resolved (pre-clamp) point, so the two clamps are independent of application order.
export function cableEndpoints(
  ctx: LayoutCtx,
  jackAnchors: Record<string, JackAnchor>,
  c: Connection,
  api: WorldApi,
): { a: Pt; b: Pt } | null {
  const from = resolveAnchor(ctx, jackAnchors, c.from, "output", api);
  const to = resolveAnchor(ctx, jackAnchors, c.to, "input", api);
  if (!from || !to) return null;
  return {
    a: clampEnd(ctx, c.from.device, from, to.pt, api),
    b: clampEnd(ctx, c.to.device, to, from.pt, api),
  };
}

// Does this cable end need a "chassis patch" — the lead redrawn *above* the panels? A front-shown device
// renders above the single cable layer, so a cable plugged into a socket on its visible front face is
// occluded by the panel (which paints over the plug and the stretch of lead that crosses the chassis).
// When true, the renderer redraws the cable's full path in the overlay layer above the gear, clipped to
// this device's chassis rect, so the portion the panel hides paints over it — a lead that reads as
// plugged into the front socket and hanging across the front of the chassis, continuous to the edge.
//
// Only fires for an end whose jack is *measured on the shown front face* (a precise anchor to clip around)
// of a *front-shown* device (the only ones that sit above the cables). A back-shown device already sits
// below the cables, and an unmeasured or hidden-face end anchors to an estimate — neither has a visible
// front socket to patch over.
export function tipPatchEnd(
  ctx: LayoutCtx,
  jackAnchors: Record<string, JackAnchor>,
  ref: PortRef,
  direction: "input" | "output",
): boolean {
  if (!inView(ctx, ref.device)) return false;
  const facing = effectiveFacing(ctx.scene, ref.device);
  if (facing !== "front") return false;
  const jack = jackAnchors[jackKey(ref.device, direction, ref.port)];
  return jack !== undefined && jack.face === facing;
}
