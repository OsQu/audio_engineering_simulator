// Unified cable presentation: ordinary cables, portal stubs, and held leads all start from the
// same resolved endpoint state, then route and mask by the same rules. Pure — no DOM, Svelte, camera, or
// clock. Everything here is in **surface coordinates** (the stage's own y-down space, which the camera only
// scales and translates), the space the socket-geometry projection already produces.
//
// The pipeline:
//   1. `resolveEndpoint` turns a logical port into an explicit state: a socket on a presented face, an exit
//      from a hidden face, off-view (a portal address), suppressed (mid-flip), geometry-pending, or
//      unresolved. Nothing falls back to an interior estimate.
//   2. A hidden-face end leaves its chassis at the edge nearest the socket's see-through position (ties
//      bottom → right → left → top), through a short straight outward lead, then joins the sagging curve.
//      The other endpoint is never an input, so a portal or remote move cannot swap the side.
//   3. Every drawable cable gets mask *passes*: the main pass (cable band) excludes the silhouette of each
//      owner whose end is hidden-face; a front socket on a front object adds a foreground pass clipped to
//      that owner minus every silhouette ordered above it. Held leads are "in hand": one held pass
//      above objects, masked only by a hidden-face source owner. Renderers turn these sets into clip-paths
//      and hit paths share them (`passVisibleAt` is the same predicate, for tests and hit testing).

import { jackKey, PORTAL_LEN, portalKey, type SurfaceRect } from "./cable-view";
import { type CableRoute, cableRoute } from "./connections";
import type { Connection, PortRef } from "./scene";
import { connKey } from "./scene-ops";
import type { DeviceFacing, PortalOffset } from "./scene-store";
import {
  type ProjectedSlot,
  projectSocket,
  type SocketGeometryIndex,
  socketFor,
} from "./socket-geometry";
import type { ObjectLayer } from "./view-resolution";

export type SurfacePoint = { readonly x: number; readonly y: number };

/** The straight outward lead (surface units) a hidden-face end runs before it joins the sagging curve. */
export const EXIT_LEAD = 24;

/** A chassis outline in surface coords — the clip region passes and hit paths consume. */
export interface SurfaceSilhouette {
  readonly objectId: string;
  readonly rect: SurfaceRect;
  readonly radius: number;
  /** The object's position in the resolved total paint order; a later object covers an earlier one. */
  readonly order: number;
  readonly layer: ObjectLayer;
}

export type Membership = "local" | "off-view" | "unresolved";

export interface CableResolutionInput {
  /** Outlines of every object painted in this view (preview-aware). */
  readonly silhouettes: ReadonlyMap<string, SurfaceSilhouette>;
  /** Per object: the slot sides it registers geometry for, with their surface frames and whether each
   *  side faces the operator (else it is seen through the chassis, mirrored). */
  readonly slots: ReadonlyMap<string, readonly ProjectedSlot[]>;
  readonly geometry: SocketGeometryIndex;
  /** Is this object in the view (studio: same space + wall), on another view, or unresolvable? */
  readonly membership: (objectId: string) => Membership;
  /** The object (or its rack) is mid-flip, so no physical lead touching it is drawn. */
  readonly suppressed: (objectId: string) => boolean;
}

/** A chassis edge; the declaration order is the nearest-edge tie order. */
export type Edge = "bottom" | "right" | "left" | "top";
const EDGE_ORDER: readonly Edge[] = ["bottom", "right", "left", "top"];

export type EndpointPresentation =
  | {
      readonly kind: "socket";
      readonly objectId: string;
      /** The chassis side the socket is on. */
      readonly side: DeviceFacing;
      readonly point: SurfacePoint;
    }
  | {
      readonly kind: "exit";
      readonly objectId: string;
      readonly side: DeviceFacing;
      readonly edge: Edge;
      /** Where the lead crosses the chassis edge. */
      readonly point: SurfacePoint;
      /** `point` + EXIT_LEAD along the edge's outward normal: where the sagging curve starts. */
      readonly lead: SurfacePoint;
    }
  | { readonly kind: "off-view"; readonly objectId: string }
  | { readonly kind: "suppressed"; readonly objectId: string }
  | { readonly kind: "pending"; readonly objectId: string }
  | { readonly kind: "unresolved"; readonly objectId: string };

/** A physical (drawable) end: a presented socket or a hidden-face exit. */
export type PhysicalEnd = Extract<EndpointPresentation, { kind: "socket" | "exit" }>;

export type CablePass =
  /** The cable band: above rear objects, below front objects, minus hidden-face owners. */
  | { readonly kind: "main"; readonly exclude: readonly string[] }
  /** A front socket's redraw: inside its owner, minus everything ordered above the owner. */
  | {
      readonly kind: "foreground";
      readonly owner: string;
      readonly include: string;
      readonly exclude: readonly string[];
    }
  /** A held lead is in hand: above every object, minus a hidden-face source owner. */
  | { readonly kind: "held"; readonly exclude: readonly string[] };

/** SVG path data shared by every pass of a cable, plus the sagging curve's endpoints (an exit end's
 *  `lead`), for distance/hit sampling. */
export type CablePath = CableRoute;

export type HiddenReason = "not-local" | "suppressed" | "pending" | "unresolved";

export type ResolvedConnection =
  | {
      readonly kind: "full";
      readonly key: string;
      readonly connection: Connection;
      readonly from: PhysicalEnd;
      readonly to: PhysicalEnd;
      readonly path: CablePath;
      readonly passes: readonly CablePass[];
    }
  | {
      readonly kind: "portal";
      readonly key: string;
      readonly connection: Connection;
      /** The persisted-offset key (`portalKey`): which end of the connection is local. */
      readonly portalKey: string;
      readonly fromLocal: boolean;
      readonly local: PhysicalEnd;
      /** The remote (off-view) device. */
      readonly remote: string;
      /** The local physical base the chip offset is measured from (socket point or exit lead). */
      readonly base: SurfacePoint;
      readonly chip: SurfacePoint;
      readonly path: CablePath;
      readonly passes: readonly CablePass[];
    }
  | {
      readonly kind: "none";
      readonly key: string;
      readonly connection: Connection;
      readonly reason: HiddenReason;
    };

// --- Endpoints -----------------------------------------------------------------------------------------

/** The chassis edge nearest `p`. Ties resolve bottom → right → left → top. */
export function nearestEdge(rect: SurfaceRect, p: SurfacePoint): Edge {
  const distance: Record<Edge, number> = {
    bottom: rect.y + rect.height - p.y,
    right: rect.x + rect.width - p.x,
    left: p.x - rect.x,
    top: p.y - rect.y,
  };
  let best: Edge = EDGE_ORDER[0] as Edge;
  for (const edge of EDGE_ORDER) if (distance[edge] < distance[best]) best = edge;
  return best;
}

const clampTo = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v));

/** The hidden-face exit for a socket seen through its chassis at `p`. */
export function edgeExit(
  rect: SurfaceRect,
  p: SurfacePoint,
): { edge: Edge; point: SurfacePoint; lead: SurfacePoint } {
  const edge = nearestEdge(rect, p);
  const x = clampTo(p.x, rect.x, rect.x + rect.width);
  const y = clampTo(p.y, rect.y, rect.y + rect.height);
  const right = rect.x + rect.width;
  const bottom = rect.y + rect.height;
  switch (edge) {
    case "bottom":
      return { edge, point: { x, y: bottom }, lead: { x, y: bottom + EXIT_LEAD } };
    case "right":
      return { edge, point: { x: right, y }, lead: { x: right + EXIT_LEAD, y } };
    case "left":
      return { edge, point: { x: rect.x, y }, lead: { x: rect.x - EXIT_LEAD, y } };
    case "top":
      return { edge, point: { x, y: rect.y }, lead: { x, y: rect.y - EXIT_LEAD } };
  }
}

/** One cable end's presentation. A presented-side socket wins over a see-through one (the workbench DUT
 *  registers each side in its own presented slot); pending geometry is not absence. */
export function resolveEndpoint(
  input: CableResolutionInput,
  ref: PortRef,
  direction: "input" | "output",
): EndpointPresentation {
  const objectId = ref.device;
  const membership = input.membership(objectId);
  if (membership === "unresolved") return { kind: "unresolved", objectId };
  if (membership === "off-view") return { kind: "off-view", objectId };
  if (input.suppressed(objectId)) return { kind: "suppressed", objectId };
  const alias = jackKey(ref.device, direction, ref.port);
  let pending = false;
  let hidden: EndpointPresentation | null = null;
  for (const { slot, frame, presented } of input.slots.get(objectId) ?? []) {
    if (!frame) continue;
    const found = socketFor(input.geometry, slot, alias);
    if (found.status === "pending") pending = true;
    if (found.status !== "current") continue;
    const point = projectSocket(found.geometry, found.socket, frame, presented);
    if (presented) return { kind: "socket", objectId, side: slot.side, point };
    hidden ??= { kind: "exit", objectId, side: slot.side, ...edgeExit(frame, point) };
  }
  if (hidden) return hidden;
  return pending ? { kind: "pending", objectId } : { kind: "unresolved", objectId };
}

const isPhysical = (end: EndpointPresentation): end is PhysicalEnd =>
  end.kind === "socket" || end.kind === "exit";

/** Where the sagging curve starts for a physical end. */
export const curveStart = (end: PhysicalEnd): SurfacePoint =>
  end.kind === "exit" ? end.lead : end.point;

// --- Passes --------------------------------------------------------------------------------------------

const unique = (ids: readonly string[]): string[] => [...new Set(ids)];

/** The mask passes for a settled cable (full or portal) with these physical ends. */
export function settledPasses(
  silhouettes: ReadonlyMap<string, SurfaceSilhouette>,
  ends: readonly PhysicalEnd[],
): CablePass[] {
  const passes: CablePass[] = [
    {
      kind: "main",
      exclude: unique(ends.filter((e) => e.kind === "exit").map((e) => e.objectId)),
    },
  ];
  for (const end of ends) {
    if (end.kind !== "socket") continue;
    const owner = silhouettes.get(end.objectId);
    if (owner?.layer !== "front-object") continue;
    if (passes.some((p) => p.kind === "foreground" && p.owner === owner.objectId)) continue;
    passes.push({
      kind: "foreground",
      owner: owner.objectId,
      include: owner.objectId,
      exclude: [...silhouettes.values()]
        .filter((s) => s.order > owner.order)
        .sort((a, b) => a.order - b.order)
        .map((s) => s.objectId),
    });
  }
  return passes;
}

/** Is `p` inside the rounded rect (corner radius clamped to half the shorter side)? */
export function silhouetteContains(s: SurfaceSilhouette, p: SurfacePoint): boolean {
  const { x, y, width, height } = s.rect;
  if (p.x < x || p.x > x + width || p.y < y || p.y > y + height) return false;
  const r = Math.max(0, Math.min(s.radius, width / 2, height / 2));
  const cx = clampTo(p.x, x + r, x + width - r);
  const cy = clampTo(p.y, y + r, y + height - r);
  return (p.x - cx) ** 2 + (p.y - cy) ** 2 <= r * r;
}

/** A rounded rect as SVG path data (clockwise), the outline `silhouetteContains` tests. */
export function roundedRectPath(s: SurfaceSilhouette): string {
  const { x, y, width, height } = s.rect;
  const r = Math.max(0, Math.min(s.radius, width / 2, height / 2));
  const right = x + width;
  const bottom = y + height;
  return (
    `M ${x + r} ${y} H ${right - r} A ${r} ${r} 0 0 1 ${right} ${y + r} ` +
    `V ${bottom - r} A ${r} ${r} 0 0 1 ${right - r} ${bottom} ` +
    `H ${x + r} A ${r} ${r} 0 0 1 ${x} ${bottom - r} ` +
    `V ${y + r} A ${r} ${r} 0 0 1 ${x + r} ${y} Z`
  );
}

/** The visibility predicate the renderer's clip-paths implement: a pass paints (and takes hits) at `p`
 *  iff `p` is inside its include silhouette (if any) and outside every excluded one. */
export function passVisibleAt(
  pass: CablePass,
  silhouettes: ReadonlyMap<string, SurfaceSilhouette>,
  p: SurfacePoint,
): boolean {
  if (pass.kind === "foreground") {
    const include = silhouettes.get(pass.include);
    if (!include || !silhouetteContains(include, p)) return false;
  }
  return pass.exclude.every((id) => {
    const s = silhouettes.get(id);
    return !s || !silhouetteContains(s, p);
  });
}

/** SVG ids for a silhouette's inside / outside clips under a per-svg `prefix`. */
const safeId = (id: string): string =>
  id.replace(/[^a-zA-Z0-9_-]/g, (ch) => `_${ch.charCodeAt(0)}`);
export const insideClipId = (prefix: string, objectId: string): string =>
  `${prefix}-in-${safeId(objectId)}`;
export const outsideClipId = (prefix: string, objectId: string): string =>
  `${prefix}-out-${safeId(objectId)}`;

/** The clip chain a renderer nests (outermost first) to paint a pass: inside its include silhouette,
 *  outside every excluded one. Nesting intersects them, so overlapping exclusions stay excluded, and
 *  `clip-path` (unlike `mask`) also clips hit testing. */
export function passClipIds(pass: CablePass, prefix: string): string[] {
  return [
    ...(pass.kind === "foreground" ? [insideClipId(prefix, pass.include)] : []),
    ...pass.exclude.map((id) => outsideClipId(prefix, id)),
  ];
}

// --- Connections ---------------------------------------------------------------------------------------

/** The default portal offset from its local base: out toward the signal flow, dropped a little. */
export const defaultPortalOffset = (fromLocal: boolean): PortalOffset => ({
  dx: fromLocal ? PORTAL_LEN : -PORTAL_LEN,
  dy: 36,
});

const hiddenReason = (ends: readonly EndpointPresentation[]): HiddenReason => {
  if (ends.some((e) => e.kind === "unresolved")) return "unresolved";
  if (ends.some((e) => e.kind === "suppressed")) return "suppressed";
  if (ends.some((e) => e.kind === "pending")) return "pending";
  return "not-local";
};

/** One connection's presentation: full cable (both ends local), portal (exactly one), or none. */
export function resolveConnection(
  input: CableResolutionInput,
  connection: Connection,
  portalOffsets: Readonly<Record<string, PortalOffset>> = {},
): ResolvedConnection {
  const key = connKey(connection);
  const from = resolveEndpoint(input, connection.from, "output");
  const to = resolveEndpoint(input, connection.to, "input");
  if (isPhysical(from) && isPhysical(to)) {
    return {
      kind: "full",
      key,
      connection,
      from,
      to,
      path: cableRoute(from, to),
      passes: settledPasses(input.silhouettes, [from, to]),
    };
  }
  const fromLocal = isPhysical(from) && to.kind === "off-view";
  const toLocal = isPhysical(to) && from.kind === "off-view";
  if (fromLocal || toLocal) {
    const local = (fromLocal ? from : to) as PhysicalEnd;
    const pk = portalKey(connection, fromLocal);
    const offset = portalOffsets[pk] ?? defaultPortalOffset(fromLocal);
    const base = curveStart(local);
    const chip = { x: base.x + offset.dx, y: base.y + offset.dy };
    return {
      kind: "portal",
      key,
      connection,
      portalKey: pk,
      fromLocal,
      local,
      remote: fromLocal ? connection.to.device : connection.from.device,
      base,
      chip,
      path: cableRoute(local, { kind: "point", point: chip }),
      passes: settledPasses(input.silhouettes, [local]),
    };
  }
  return { kind: "none", key, connection, reason: hiddenReason([from, to]) };
}

export function resolveConnections(
  input: CableResolutionInput,
  connections: readonly Connection[],
  portalOffsets: Readonly<Record<string, PortalOffset>> = {},
): ResolvedConnection[] {
  return connections.map((c) => resolveConnection(input, c, portalOffsets));
}

// --- Held leads ----------------------------------------------------------------------------------------

export interface HeldCableInput {
  /** The logical source port (never a cached anchor). */
  readonly source: PortRef;
  readonly direction: "input" | "output";
  /** The pointer (or snapped target) in surface coords; null before the first move. */
  readonly pointer: SurfacePoint | null;
}

export type ResolvedHeldCable =
  /** Source physically present: a lead from it to the pointer, in hand. */
  | {
      readonly kind: "lead";
      readonly source: PhysicalEnd;
      readonly path: CablePath;
      readonly passes: readonly CablePass[];
    }
  /** Source on another view: only the floating end at the pointer (the banner names the source). */
  | { readonly kind: "floating"; readonly pointer: SurfacePoint | null }
  /** Source mid-flip / geometry pending / unresolvable: no physical lead; the pick survives. */
  | { readonly kind: "hidden"; readonly reason: Exclude<HiddenReason, "not-local"> };

/** A held cable re-resolved from its logical source each time — it follows device/rack previews, view
 *  changes, and settled flips because it never stores an absolute anchor. */
export function resolveHeldCable(
  input: CableResolutionInput,
  pick: HeldCableInput,
): ResolvedHeldCable {
  const source = resolveEndpoint(input, pick.source, pick.direction);
  if (source.kind === "off-view") return { kind: "floating", pointer: pick.pointer };
  if (source.kind === "suppressed" || source.kind === "pending" || source.kind === "unresolved") {
    return { kind: "hidden", reason: source.kind };
  }
  const start = curveStart(source);
  const point = pick.pointer ?? start;
  return {
    kind: "lead",
    source,
    path: cableRoute(source, { kind: "point", point }),
    passes: [{ kind: "held", exclude: source.kind === "exit" ? [source.objectId] : [] }],
  };
}
