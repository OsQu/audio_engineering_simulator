// Patchable socket hit targets (Story 10.7). Pure — no DOM, Svelte, camera object, or clock.
//
// Two steps:
//   1. `resolveSocketTargets` derives the patchable sockets of a view from the same cable-resolution input
//      that draws its cables. A socket is a target only when its owner is local and unsuppressed (not
//      mid-flip), its side is presented, its geometry is current, and its centre is exposed (no silhouette
//      ordered above the owner contains it). Hidden-face sockets, off-view, pending, top-view and
//      focus-window sockets therefore never appear. One physical socket is one target carrying every
//      endpoint alias (a duplex USB socket).
//   2. `pickSocketTarget` picks the socket a client point means, in client pixels after the camera: a named
//      minimum radius per pointer type (or the rendered
//      radius plus a small edge halo, when zoomed in), clipped by the viewport and by UI blockers (focus windows,
//      toolbars, banners) and by any silhouette above the owner (the partial-exposure clip). An exact
//      physical hit wins; otherwise the nearest centre; ties go to paint order, then the stable socket ID.
//      The picker is legality-blind — the caller reports the nearest socket's verdict, so an illegal
//      nearest socket is never skipped for a farther legal one.

import {
  type CableResolutionInput,
  type SurfacePoint,
  type SurfaceSilhouette,
  silhouetteContains,
} from "./cable-resolution";
import { geometryKey, projectSocket } from "./socket-geometry";

/** Minimum screen-space hit radius (CSS px) for a mouse or pen — tune in the browser, not inline. */
export const MOUSE_TOLERANCE_PX = 8;
/** Minimum screen-space hit radius (CSS px) for touch. */
export const TOUCH_TOLERANCE_PX = 12;
/** A halo (CSS px) beyond a socket's rendered edge that still means that socket. Without it, a zoomed-in
 *  socket's tolerance collapses to its own outline and a press just outside it starts a device move. */
export const EDGE_HALO_PX = 6;

/** One physical, patchable socket in surface coordinates. */
export interface SocketTarget {
  /** Canonical physical socket id (`SocketShape.id`) — the stable tie-breaker. */
  readonly id: string;
  /** Every endpoint key ("device:direction:port") that plugs into this socket. */
  readonly aliases: readonly string[];
  readonly objectId: string;
  /** The presented-side centre (surface units). */
  readonly point: SurfacePoint;
  /** The rendered radius in surface units (face-local `r` × the frame's scale). */
  readonly radius: number;
  /** The owner's paint order; −1 when the owner has no silhouette (the workbench DUT). */
  readonly order: number;
}

const byId = (a: SocketTarget, b: SocketTarget): number => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);

/** Is `p` covered by a silhouette painted above paint order `order` (other than `owner` itself)? */
function coveredAbove(
  silhouettes: ReadonlyMap<string, SurfaceSilhouette>,
  owner: string,
  order: number,
  p: SurfacePoint,
): boolean {
  for (const s of silhouettes.values()) {
    if (s.objectId !== owner && s.order > order && silhouetteContains(s, p)) return true;
  }
  return false;
}

/** The view's patchable sockets, sorted by socket id (deterministic for identical input). */
export function resolveSocketTargets(input: CableResolutionInput): SocketTarget[] {
  const targets: SocketTarget[] = [];
  for (const [objectId, slots] of input.slots) {
    if (input.membership(objectId) !== "local" || input.suppressed(objectId)) continue;
    const order = input.silhouettes.get(objectId)?.order ?? -1;
    for (const { slot, frame, presented } of slots) {
      if (!frame || !presented) continue;
      const entry = input.geometry.entries[geometryKey(slot)];
      if (entry?.status !== "current") continue;
      const { geometry } = entry;
      const scale = Math.min(frame.width / geometry.width, frame.height / geometry.height);
      for (const socket of geometry.sockets) {
        const point = projectSocket(geometry, socket, frame, true);
        if (coveredAbove(input.silhouettes, objectId, order, point)) continue;
        targets.push({
          id: socket.id,
          aliases: socket.aliases,
          objectId,
          point,
          radius: socket.r * scale,
          order,
        });
      }
    }
  }
  return targets.sort(byId);
}

/** A client-pixel rectangle (the shape of `DOMRect`, without depending on the DOM). */
export interface ClientRect {
  readonly left: number;
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
}

export type PointerKind = "mouse" | "pen" | "touch";

/** What the stage adapter reads from the DOM at press/move/release time. */
export interface PickContext {
  /** Client px per surface unit (the camera zoom). */
  readonly zoom: number;
  /** Where surface (0, 0) sits in client px. */
  readonly origin: { readonly x: number; readonly y: number };
  /** The stage viewport; nothing outside it is a target. */
  readonly viewport: ClientRect;
  /** UI that covers the stage (focus windows, toolbars, banners); geometric search never reaches through. */
  readonly blockers: readonly ClientRect[];
  /** Silhouettes for the partial-exposure clip (the same map the targets were resolved from). */
  readonly silhouettes: ReadonlyMap<string, SurfaceSilhouette>;
  readonly pointerType: PointerKind;
}

export type SocketPick =
  | { readonly kind: "exact" | "near"; readonly target: SocketTarget }
  | { readonly kind: "none"; readonly reason: "outside" | "blocked" | "miss" };

const inside = (r: ClientRect, p: { x: number; y: number }): boolean =>
  p.x >= r.left && p.x <= r.right && p.y >= r.top && p.y <= r.bottom;

/** The minimum screen-space radius for a pointer type. */
export const toleranceFor = (pointerType: PointerKind): number =>
  pointerType === "touch" ? TOUCH_TOLERANCE_PX : MOUSE_TOLERANCE_PX;

/** Better candidate first: higher paint order, then smaller socket id. */
function wins(a: SocketTarget, b: SocketTarget): boolean {
  return a.order !== b.order ? a.order > b.order : byId(a, b) < 0;
}

/** The socket a client point means, or why there is none. */
export function pickSocketTarget(
  targets: readonly SocketTarget[],
  client: { readonly x: number; readonly y: number },
  ctx: PickContext,
): SocketPick {
  if (!inside(ctx.viewport, client)) return { kind: "none", reason: "outside" };
  if (ctx.blockers.some((b) => inside(b, client))) return { kind: "none", reason: "blocked" };
  const zoom = ctx.zoom > 0 ? ctx.zoom : 1;
  const surface = { x: (client.x - ctx.origin.x) / zoom, y: (client.y - ctx.origin.y) / zoom };
  const minimum = toleranceFor(ctx.pointerType);

  let exact: SocketTarget | null = null;
  let near: SocketTarget | null = null;
  let nearDistance = Number.POSITIVE_INFINITY;
  for (const target of targets) {
    const centre = {
      x: ctx.origin.x + target.point.x * zoom,
      y: ctx.origin.y + target.point.y * zoom,
    };
    if (!inside(ctx.viewport, centre)) continue;
    if (ctx.blockers.some((b) => inside(b, centre))) continue;
    // Partial exposure: the part of a socket's disc under a higher silhouette is not this socket.
    if (coveredAbove(ctx.silhouettes, target.objectId, target.order, surface)) continue;
    const rendered = target.radius * zoom;
    const distance = Math.hypot(client.x - centre.x, client.y - centre.y);
    if (distance <= rendered) {
      if (!exact || wins(target, exact)) exact = target;
      continue;
    }
    if (distance > Math.max(minimum, rendered + EDGE_HALO_PX)) continue;
    if (distance < nearDistance || (distance === nearDistance && near && wins(target, near))) {
      near = target;
      nearDistance = distance;
    }
  }
  if (exact) return { kind: "exact", target: exact };
  if (near) return { kind: "near", target: near };
  return { kind: "none", reason: "miss" };
}
