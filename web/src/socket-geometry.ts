// The revisioned socket-geometry index (Story 10.4): where each physical socket sits on a presented face,
// in that face's own natural millimetres. Pure — no DOM, camera, placement, or time. The DOM adapter
// (face-geometry.ts) measures a face once per *revision* (descriptor/config, content layout, size, skin,
// fonts) and registers it here; camera pans, zooms, and placement moves never touch the index, they only
// change the frame a registration is projected into.
//
// A registration is keyed by `(face instance, side)`: one chassis slot carries two sides (the hidden one
// still lays out inside the flip card), and the workbench DUT's two slots each carry one. Every request
// for a new revision issues a fresh *generation*; a measurement taken for an older generation, a slot that
// was unregistered, an empty layout box, or a face that names one socket twice is rejected with an
// explicit outcome rather than silently overwriting good geometry.

import type { JackAnchor } from "./cable-view";
import type { DeviceFacing } from "./scene-store";
import type { FaceInstanceId } from "./view-resolution";

/** One physical socket. A duplex connector (USB-C) is one socket with an alias per endpoint direction. */
export interface SocketShape {
  /** Canonical physical id (the jack's primary endpoint key). */
  readonly id: string;
  /** Every endpoint key that plugs into this socket, including `id`. */
  readonly aliases: readonly string[];
  /** Centre and radius in face-local mm, measured as the side is seen when presented. */
  readonly x: number;
  readonly y: number;
  readonly r: number;
}

/** A side's measured layout: its frame size and sockets, in face-local mm. */
export interface FaceGeometry {
  readonly width: number;
  readonly height: number;
  readonly sockets: readonly SocketShape[];
  /** The slot's un-offset stage origin, for a stage whose layout isn't pure (the workbench stack): the
   *  frame is then `home + offset`. The studio derives its frame from the projection and omits it. */
  readonly home?: { readonly x: number; readonly y: number };
}

/** The slot + side a registration describes. */
export interface GeometrySlot {
  readonly face: FaceInstanceId;
  readonly side: DeviceFacing;
}

export type GeometryEntry =
  | { readonly status: "pending"; readonly revision: string; readonly generation: number }
  | {
      readonly status: "current";
      readonly revision: string;
      readonly generation: number;
      readonly geometry: FaceGeometry;
    };

export interface SocketGeometryIndex {
  readonly entries: Readonly<Record<string, GeometryEntry>>;
  /** Monotonic across the whole index, so a re-registered slot never reuses an old token. */
  readonly generation: number;
}

export type RegisterOutcome = "current" | "stale" | "unmounted" | "empty" | "duplicate";

export function createSocketGeometryIndex(): SocketGeometryIndex {
  return { entries: {}, generation: 0 };
}

export const geometryKey = (slot: GeometrySlot): string => `${slot.face}|${slot.side}`;

/** An opaque revision from everything that can move a socket within its face. Camera and placement are
 *  deliberately not inputs. */
export const geometryRevision = (parts: readonly (string | number | boolean)[]): string =>
  JSON.stringify(parts);

/** Ask for geometry at `revision`. Unchanged revision ⇒ the index is returned as-is (a current
 *  registration survives); a new or changed revision ⇒ the slot goes pending under a fresh generation. */
export function requestGeometry(
  index: SocketGeometryIndex,
  slot: GeometrySlot,
  revision: string,
): { index: SocketGeometryIndex; generation: number; measure: boolean } {
  const entry = index.entries[geometryKey(slot)];
  if (entry && entry.revision === revision) {
    return { index, generation: entry.generation, measure: entry.status === "pending" };
  }
  const generation = index.generation + 1;
  return {
    generation,
    measure: true,
    index: {
      generation,
      entries: {
        ...index.entries,
        [geometryKey(slot)]: { status: "pending", revision, generation },
      },
    },
  };
}

function duplicateAlias(geometry: FaceGeometry): boolean {
  const seen = new Set<string>();
  for (const socket of geometry.sockets) {
    for (const key of new Set([socket.id, ...socket.aliases])) {
      if (seen.has(key)) return true;
      seen.add(key);
    }
  }
  return false;
}

/** Register a measurement taken for `generation`. Only the slot's still-current generation applies. */
export function registerGeometry(
  index: SocketGeometryIndex,
  slot: GeometrySlot,
  generation: number,
  geometry: FaceGeometry,
): { index: SocketGeometryIndex; outcome: RegisterOutcome } {
  const key = geometryKey(slot);
  const entry = index.entries[key];
  if (!entry) return { index, outcome: "unmounted" };
  if (entry.generation !== generation) return { index, outcome: "stale" };
  if (!(geometry.width > 0 && geometry.height > 0)) return { index, outcome: "empty" };
  if (duplicateAlias(geometry)) return { index, outcome: "duplicate" };
  return {
    outcome: "current",
    index: {
      ...index,
      entries: {
        ...index.entries,
        [key]: { status: "current", revision: entry.revision, generation, geometry },
      },
    },
  };
}

/** Drop a slot's registration (its DOM unmounted). A late measurement then resolves as `unmounted`. */
export function unregisterGeometry(
  index: SocketGeometryIndex,
  slot: GeometrySlot,
): SocketGeometryIndex {
  const key = geometryKey(slot);
  if (!(key in index.entries)) return index;
  const { [key]: _removed, ...entries } = index.entries;
  return { ...index, entries };
}

export type SocketLookup =
  | { readonly status: "current"; readonly socket: SocketShape; readonly geometry: FaceGeometry }
  | { readonly status: "pending" }
  | { readonly status: "absent" };

/** Find the socket an endpoint key plugs into on one slot side. Pending is not absent: the face is
 *  presented, its geometry just isn't known yet. */
export function socketFor(
  index: SocketGeometryIndex,
  slot: GeometrySlot,
  endpoint: string,
): SocketLookup {
  const entry = index.entries[geometryKey(slot)];
  if (!entry) return { status: "absent" };
  if (entry.status === "pending") return { status: "pending" };
  const socket = entry.geometry.sockets.find((s) => s.aliases.includes(endpoint));
  return socket ? { status: "current", socket, geometry: entry.geometry } : { status: "absent" };
}

/** A face's frame on the stage surface (surface units, y-down). */
export interface SurfaceFrame {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** Project a face-local socket centre into a stage frame. A side seen through the chassis (the hidden
 *  side of a flip card) is mirrored horizontally, exactly as rotateY(180deg) places it. */
export function projectSocket(
  geometry: FaceGeometry,
  socket: SocketShape,
  frame: SurfaceFrame,
  presented: boolean,
): { x: number; y: number } {
  const sx = frame.width / geometry.width;
  const sy = frame.height / geometry.height;
  const x = presented ? socket.x : geometry.width - socket.x;
  return { x: frame.x + x * sx, y: frame.y + socket.y * sy };
}

/** One registered slot side as a stage presents it right now. */
export interface ProjectedSlot {
  readonly slot: GeometrySlot;
  /** Where the slot's frame sits on the stage surface; `null` ⇒ not projectable (off-view). */
  readonly frame: SurfaceFrame | null;
  /** Whether this side faces the operator (else it is seen through the chassis, mirrored). */
  readonly presented: boolean;
}

/** The surface anchor of every current socket on the given slot sides, keyed by every endpoint alias —
 *  the contract cable routing and patching consume. Pending and unprojectable slots contribute nothing. */
export function projectAnchors(
  index: SocketGeometryIndex,
  slots: readonly ProjectedSlot[],
): Record<string, JackAnchor> {
  const anchors: Record<string, JackAnchor> = {};
  for (const { slot, frame, presented } of slots) {
    const entry = index.entries[geometryKey(slot)];
    if (!frame || entry?.status !== "current") continue;
    for (const socket of entry.geometry.sockets) {
      const pt = projectSocket(entry.geometry, socket, frame, presented);
      for (const alias of socket.aliases) anchors[alias] = { ...pt, face: slot.side };
    }
  }
  return anchors;
}
