// The pure, rendering-free logic behind patch cables — connection legality + cable geometry. **No DOM /
// Svelte imports** (only type-only imports of the scene/catalog IR): it is unit-tested in isolation, the
// project's "tests are the oracle" temperament applied to the UI (peer to `spatial.ts`).
//
// Two concerns live here:
//   1. **Legality** — is a proposed jack→jack drag a valid connection, and if so, oriented how? This
//      mirrors the engine's own rules so the UI can give live feedback *before* `loadPatch` compiles:
//      an output feeds an input; both ends share a carrier domain; a device can't patch itself; an input
//      takes exactly **one** source (the engine rejects fan-in), so dropping onto an occupied input
//      *replaces* the existing cable. Deeper cycles are the one illegality not visible locally — they
//      surface as a `BuildError` from compile (handled at the call site).
//   2. **Geometry** — the bezier a cable is drawn along, and hit-testing a point against it (for
//      click-to-delete). Coordinate-agnostic: it operates in the overlay's 2-D pixel space with **y
//      increasing downward** (SVG/screen convention), so a cable **sags downward** (+y).

import {
  type CableType,
  type Connector,
  connectorsCompatible,
  type PortDirection,
  type PortDomain,
} from "./catalog";
import type { CableSpec, Connection, PortRef } from "./scene";

/** One end of a prospective connection: a device's port plus the engine truth that governs legality. */
export interface Endpoint {
  /** Device instance id (matches a `DeviceInstance.id`). */
  device: string;
  /** Port id within its direction (inputs `0..n_in`, outputs `0..n_out`) — the `PortRef.port` index. */
  port: number;
  /** Whether this port is an input or an output. */
  direction: PortDirection;
  /** Carrier domain — must match the other end (the engine rejects a cross-domain edge). */
  domain: PortDomain;
  /** Physical connector shape — must match the other end within a domain (an XLR won't seat in a ¼"
   *  jack). Mirrors the `build_patch` connector check. */
  connector: Connector;
  /** Lane count (digital channels). A digital connection needs equal counts on both ends — an 8-wide
   *  send can't feed a 2-wide return. Mirrors `build_patch`'s `ChannelCountMismatch`. */
  channels: number;
  /** If this is one half of a **duplex** jack (USB-C, Ethernet — one connector, both directions), the
   *  paired port's id on the other direction. A duplex endpoint is bound to its **output** side, so this
   *  is the paired **input** id. Absent for an ordinary one-way jack. Mirrors `PortDescriptor.duplexPartner`. */
  duplexPartner?: number;
}

/** The verdict on a proposed connection. On success it carries the **oriented** `Connection`
 *  (from = output, to = input) and, if the target input was already driven, the connection it
 *  **replaces** (fan-in is illegal, so the caller drops the old edge before adding the new one). */
export type ConnectVerdict =
  | { ok: true; connection: Connection; replaces: Connection | null }
  | { ok: false; reason: string };

/** Does a `PortRef` name this exact `(device, port)`? */
function refIs(ref: PortRef, device: string, port: number): boolean {
  return ref.device === device && ref.port === port;
}

/** Would adding an edge from `fromDevice` (output side) to `toDevice` (input side) create a cycle in
 *  the device graph? True iff `toDevice` can already reach `fromDevice` via existing connections
 *  (each connection is a `from.device → to.device` edge) — adding the edge would then close a loop,
 *  which the engine rejects at compile. Pure DFS reachability, so it's testable and lets the UI reject
 *  a feedback loop *before* compile instead of committing a bad patch and handling an async error. */
export function wouldCreateCycle(
  fromDevice: string,
  toDevice: string,
  existing: Connection[],
): boolean {
  if (fromDevice === toDevice) return true;
  const seen = new Set<string>();
  const stack = [toDevice];
  while (stack.length > 0) {
    const cur = stack.pop();
    if (cur === undefined) break;
    if (cur === fromDevice) return true;
    if (seen.has(cur)) continue;
    seen.add(cur);
    for (const c of existing) {
      if (c.from.device === cur) stack.push(c.to.device);
    }
  }
  return false;
}

/** Evaluate a prospective drag between two jacks against the engine's connection rules, given the
 *  patch's `existing` connections. Order-independent — pass the two endpoints in either order. */
export function evaluateConnection(
  a: Endpoint,
  b: Endpoint,
  existing: Connection[],
): ConnectVerdict {
  // A **duplex** link (USB-C, Ethernet): one physical connector carries both directions. Both jacks
  // are duplex (each bound to its output side), so the drag authors a single duplex `Connection` that
  // `build_patch` expands into both edges. It intentionally forms a cycle — the engine breaks it with
  // one block of round-trip latency — so the feedback-loop check below is deliberately skipped.
  const aDup = a.duplexPartner !== undefined;
  const bDup = b.duplexPartner !== undefined;
  if (aDup || bDup) {
    if (!aDup || !bDup) {
      return { ok: false, reason: "a duplex (USB-C) cable needs a duplex jack on both ends" };
    }
    if (a.domain !== b.domain) {
      return { ok: false, reason: `domain mismatch: ${a.domain} → ${b.domain}` };
    }
    if (!connectorsCompatible(a.connector, b.connector)) {
      return { ok: false, reason: `connector mismatch: ${a.connector} → ${b.connector}` };
    }
    if (a.device === b.device) {
      return { ok: false, reason: "can't patch a device to itself" };
    }
    // Orient: `a` is the drag source (its output side → `b`'s input side). Symmetric — `build_patch`
    // adds the reverse leg from each jack's partner, so drag direction doesn't change the result.
    const from: PortRef = { device: a.device, port: a.port };
    const to: PortRef = { device: b.device, port: b.duplexPartner as number };
    // A duplex jack carries exactly one cable: a new one on either jack replaces the existing duplex
    // link that touches it. (Channel-count per direction is checked at build — the endpoints here only
    // carry the output-side width, so the exact per-leg match is `build_patch`'s `ChannelCountMismatch`.)
    const touches = (c: Connection, dev: string, outPort: number, inPort: number): boolean =>
      (c.from.device === dev && c.from.port === outPort) ||
      (c.to.device === dev && c.to.port === inPort);
    const replaces =
      existing.find(
        (c) =>
          c.duplex &&
          (touches(c, a.device, a.port, a.duplexPartner as number) ||
            touches(c, b.device, b.port, b.duplexPartner as number)),
      ) ?? null;
    return { ok: true, connection: { from, to, duplex: true }, replaces };
  }

  // Orient: exactly one output and one input. (Rejects output→output and input→input.)
  let out: Endpoint;
  let inp: Endpoint;
  if (a.direction === "output" && b.direction === "input") {
    out = a;
    inp = b;
  } else if (a.direction === "input" && b.direction === "output") {
    out = b;
    inp = a;
  } else {
    return { ok: false, reason: "connect an output to an input" };
  }

  // Same carrier domain — a cross-domain edge is a compile-time `DomainMismatch`.
  if (out.domain !== inp.domain) {
    return { ok: false, reason: `domain mismatch: ${out.domain} → ${inp.domain}` };
  }

  // Same physical connector — an XLR won't seat in a ¼" jack. Checked after domain (a cross-domain
  // pair is reported as a domain mismatch), mirroring `build_patch`'s domain-then-connector order.
  if (!connectorsCompatible(out.connector, inp.connector)) {
    return { ok: false, reason: `connector mismatch: ${out.connector} → ${inp.connector}` };
  }

  // A digital link must carry the same channel count on both ends (an 8-wide send can't feed a 2-wide
  // return). Mirrors the engine's `LaneCountMismatch` / `build_patch`'s `ChannelCountMismatch`.
  if (out.domain === "digital" && out.channels !== inp.channels) {
    return {
      ok: false,
      reason: `channel-count mismatch: ${out.channels}ch → ${inp.channels}ch`,
    };
  }

  // A device feeding its own input is a self-cycle.
  if (out.device === inp.device) {
    return { ok: false, reason: "can't patch a device to itself" };
  }

  // A longer feedback loop (the input's device can already reach the output's device).
  if (wouldCreateCycle(out.device, inp.device, existing)) {
    return { ok: false, reason: "would create a feedback loop" };
  }

  // Exact duplicate — this cable already exists.
  const duplicate = existing.some(
    (c) => refIs(c.from, out.device, out.port) && refIs(c.to, inp.device, inp.port),
  );
  if (duplicate) {
    return { ok: false, reason: "already connected" };
  }

  // Fan-in: an input accepts exactly one source, so an occupied input's cable is replaced (not rejected).
  const replaces = existing.find((c) => refIs(c.to, inp.device, inp.port)) ?? null;

  return {
    ok: true,
    connection: {
      from: { device: out.device, port: out.port },
      to: { device: inp.device, port: inp.port },
    },
    replaces,
  };
}

/** May a physical cable (`CableSpec` R·C) ride an edge of this domain? Only analog edges carry a cable;
 *  the engine ignores a `CableSpec` on a digital/event route, so the UI offers cables on analog only. */
export function cableAllowed(domain: PortDomain): boolean {
  return domain === "analog";
}

/** The R·C spec a `Connection` carries for a cable preset (the subset the engine reads). */
export function cableSpec(ct: CableType): CableSpec {
  return { resistanceOhms: ct.resistanceOhms, capacitanceFarads: ct.capacitanceFarads };
}

/** The catalog type id whose R·C matches `spec`, or `""` when there is no cable (ideal wire) or the spec
 *  matches no preset. The inverse of {@link cableSpec}, so the picker can show a connection's current
 *  cable type from the R·C the patch stores (the CableSpec is the single source of truth — no extra id
 *  field on the IR). */
export function cableTypeIdFor(cables: CableType[], spec: CableSpec | undefined): string {
  if (!spec) return "";
  const match = cables.find(
    (c) =>
      c.resistanceOhms === spec.resistanceOhms && c.capacitanceFarads === spec.capacitanceFarads,
  );
  return match?.typeId ?? "";
}

// --- Cable geometry (2-D pixel space, y-down) ------------------------------------------------------

/** A 2-D point in the overlay's pixel space (y increases downward). */
export interface Point {
  x: number;
  y: number;
}

/** Sag as a fraction of the endpoint distance, clamped — a longer cable hangs lower, but not without
 *  bound. Tuned for a natural patch-cable droop, not physical accuracy. */
const SAG_RATIO = 0.2;
const MIN_SAG = 16;
const MAX_SAG = 220;

const clamp = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v));

/** The cubic-bezier control points for a cable between `p0` and `p3`: `[p0, c1, c2, p3]`. The two
 *  control points sit a third of the way in horizontally and **droop below** their own endpoint by the
 *  sag (so the cable hangs like a real patch lead). */
export function cableControlPoints(p0: Point, p3: Point): [Point, Point, Point, Point] {
  const dx = p3.x - p0.x;
  const dist = Math.hypot(dx, p3.y - p0.y);
  const sag = clamp(dist * SAG_RATIO, MIN_SAG, MAX_SAG);
  const c1: Point = { x: p0.x + dx / 3, y: p0.y + sag };
  const c2: Point = { x: p3.x - dx / 3, y: p3.y + sag };
  return [p0, c1, c2, p3];
}

/** The cable's path as an SVG `d` string (`M … C …`) — a pure string, so it stays test-friendly. */
export function cablePathData(p0: Point, p3: Point): string {
  const [a, c1, c2, b] = cableControlPoints(p0, p3);
  return `M ${a.x} ${a.y} C ${c1.x} ${c1.y} ${c2.x} ${c2.y} ${b.x} ${b.y}`;
}

/** A point on the cubic bezier at parameter `t ∈ [0, 1]` (Bernstein form). */
function cubicAt(p0: Point, c1: Point, c2: Point, p3: Point, t: number): Point {
  const u = 1 - t;
  const w0 = u * u * u;
  const w1 = 3 * u * u * t;
  const w2 = 3 * u * t * t;
  const w3 = t * t * t;
  return {
    x: w0 * p0.x + w1 * c1.x + w2 * c2.x + w3 * p3.x,
    y: w0 * p0.y + w1 * c1.y + w2 * c2.y + w3 * p3.y,
  };
}

/** Distance from point `q` to segment `a`–`b` (Euclidean). */
function distanceToSegment(q: Point, a: Point, b: Point): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  // Degenerate segment: fall back to the point distance.
  const t = len2 === 0 ? 0 : clamp(((q.x - a.x) * dx + (q.y - a.y) * dy) / len2, 0, 1);
  const px = a.x + t * dx;
  const py = a.y + t * dy;
  return Math.hypot(q.x - px, q.y - py);
}

/** Shortest distance from `q` to the cable curve between `p0` and `p3`, approximated by sampling the
 *  bezier into `samples` segments. Used for click-to-delete hit-testing. */
export function distanceToCable(p0: Point, p3: Point, q: Point, samples = 24): number {
  const [a, c1, c2, b] = cableControlPoints(p0, p3);
  let prev = a;
  let min = Number.POSITIVE_INFINITY;
  for (let i = 1; i <= samples; i++) {
    const cur = cubicAt(a, c1, c2, b, i / samples);
    min = Math.min(min, distanceToSegment(q, prev, cur));
    prev = cur;
  }
  return min;
}

/** Is `q` within `threshold` pixels of the cable curve? (Click-to-delete predicate.) */
export function isPointNearCable(p0: Point, p3: Point, q: Point, threshold: number): boolean {
  return distanceToCable(p0, p3, q) <= threshold;
}
