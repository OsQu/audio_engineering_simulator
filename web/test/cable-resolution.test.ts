// Story 10.6.1: unified endpoint resolution, hidden-face exits, mask passes, portals, held leads.
import { describe, expect, it } from "vitest";
import {
  type CableResolutionInput,
  EXIT_LEAD,
  edgeExit,
  type Membership,
  nearestEdge,
  passVisibleAt,
  resolveConnection,
  resolveEndpoint,
  resolveHeldCable,
  type SurfaceSilhouette,
  silhouetteContains,
} from "../src/cable-resolution";
import { cableRoute } from "../src/connections";
import type { Connection } from "../src/scene";
import type { FaceGeometry, ProjectedSlot, SocketGeometryIndex } from "../src/socket-geometry";
import type { ObjectLayer } from "../src/view-resolution";

// A face 100 × 50 face-local mm with one output socket (port 0) at (20, 10) and one input (port 0) at
// (80, 40). Every fixture device uses it.
const face = (device: string): FaceGeometry => ({
  width: 100,
  height: 50,
  sockets: [
    { id: `${device}:output:0`, aliases: [`${device}:output:0`], x: 20, y: 10, r: 3 },
    { id: `${device}:input:0`, aliases: [`${device}:input:0`], x: 80, y: 40, r: 3 },
  ],
});

type Frame = { x: number; y: number; width: number; height: number };

interface Dev {
  id: string;
  frame: Frame;
  /** The registered side that faces the operator; the sockets are on `socketsOn`. */
  shown: "front" | "back";
  socketsOn?: "front" | "back";
  layer?: ObjectLayer;
  order: number;
  membership?: Membership;
  suppressed?: boolean;
  pending?: boolean;
}

function input(devs: Dev[]): CableResolutionInput {
  const entries: Record<string, SocketGeometryIndex["entries"][string]> = {};
  const slots = new Map<string, ProjectedSlot[]>();
  const silhouettes = new Map<string, SurfaceSilhouette>();
  for (const d of devs) {
    const sockets = d.socketsOn ?? "front";
    const faceId = `studio:${d.id}`;
    entries[`${faceId}|${sockets}`] = d.pending
      ? { status: "pending", revision: "r", generation: 1 }
      : { status: "current", revision: "r", generation: 1, geometry: face(d.id) };
    slots.set(
      d.id,
      (["front", "back"] as const).map((side) => ({
        slot: { face: faceId, side },
        frame: d.frame,
        presented: side === d.shown,
      })),
    );
    silhouettes.set(d.id, {
      objectId: d.id,
      rect: d.frame,
      radius: 6,
      order: d.order,
      layer: d.layer ?? (d.shown === "front" ? "front-object" : "rear-object"),
    });
  }
  const byId = new Map(devs.map((d) => [d.id, d]));
  return {
    silhouettes,
    slots,
    geometry: { entries, generation: 1 },
    membership: (id) => byId.get(id)?.membership ?? (byId.has(id) ? "local" : "unresolved"),
    suppressed: (id) => byId.get(id)?.suppressed ?? false,
  };
}

const conn = (from: string, to: string): Connection => ({
  from: { device: from, port: 0 },
  to: { device: to, port: 0 },
});

// Frame scale 2: a 200 × 100 surface frame at (1000, 2000) for the 100 × 50 face.
const FRAME = { x: 1000, y: 2000, width: 200, height: 100 };

describe("nearestEdge / edgeExit", () => {
  it("picks the nearest edge (hand calc)", () => {
    // Frame (100, 200) 482.6 × 88.9, socket (130, 260):
    // left 30, right 482.6 + 100 − 130 = 452.6, top 60, bottom 288.9 − 260 = 28.9 → bottom.
    const rect = { x: 100, y: 200, width: 482.6, height: 88.9 };
    expect(nearestEdge(rect, { x: 130, y: 260 })).toBe("bottom");
    const exit = edgeExit(rect, { x: 130, y: 260 });
    expect(exit.point.x).toBeCloseTo(130);
    expect(exit.point.y).toBeCloseTo(288.9);
    expect(exit.lead.y).toBeCloseTo(288.9 + EXIT_LEAD);
  });

  it("breaks ties bottom → right → left → top", () => {
    // Centre of a 100 × 100 square: all four distances 50 → bottom.
    expect(nearestEdge({ x: 0, y: 0, width: 100, height: 100 }, { x: 50, y: 50 })).toBe("bottom");
    // 20 × 100 at (10, 40): bottom 60, right 10, left 10, top 40 → right beats left.
    expect(nearestEdge({ x: 0, y: 0, width: 20, height: 100 }, { x: 10, y: 40 })).toBe("right");
    // 100 × 100 at (5, 5): left 5, top 5 → left beats top.
    expect(nearestEdge({ x: 0, y: 0, width: 100, height: 100 }, { x: 5, y: 5 })).toBe("left");
    // 100 × 100 at (50, 5): top 5 only.
    expect(nearestEdge({ x: 0, y: 0, width: 100, height: 100 }, { x: 50, y: 5 })).toBe("top");
  });

  it("leads outward along each edge normal", () => {
    const rect = { x: 0, y: 0, width: 100, height: 100 };
    expect(edgeExit(rect, { x: 95, y: 50 }).lead).toEqual({ x: 100 + EXIT_LEAD, y: 50 });
    expect(edgeExit(rect, { x: 3, y: 50 }).lead).toEqual({ x: -EXIT_LEAD, y: 50 });
    expect(edgeExit(rect, { x: 50, y: 2 }).lead).toEqual({ x: 50, y: -EXIT_LEAD });
  });
});

describe("resolveEndpoint", () => {
  it("anchors a socket on the presented side at its projected centre", () => {
    // Output at face-local (20, 10), scale 2 → (1000 + 40, 2000 + 20).
    const end = resolveEndpoint(
      input([{ id: "a", frame: FRAME, shown: "front", order: 0 }]),
      { device: "a", port: 0 },
      "output",
    );
    expect(end).toEqual({ kind: "socket", objectId: "a", point: { x: 1040, y: 2020 } });
  });

  it("exits a hidden-face socket at the edge nearest its see-through position", () => {
    // Sockets on the front, back shown → seen through, mirrored: x = 100 − 20 = 80 → 1000 + 160 = 1160,
    // y = 2020. Distances: bottom 80, right 40, left 160, top 20 → top. Exit (1160, 2000), lead 24 up.
    const end = resolveEndpoint(
      input([{ id: "a", frame: FRAME, shown: "back", order: 0 }]),
      { device: "a", port: 0 },
      "output",
    );
    expect(end).toEqual({
      kind: "exit",
      objectId: "a",
      edge: "top",
      point: { x: 1160, y: 2000 },
      lead: { x: 1160, y: 2000 - EXIT_LEAD },
    });
  });

  it("is explicit about off-view, suppressed, pending, and unresolved ends", () => {
    const at = (dev: Partial<Dev>, device = "a") =>
      resolveEndpoint(
        input([{ id: "a", frame: FRAME, shown: "front", order: 0, ...dev }]),
        { device, port: 0 },
        "output",
      ).kind;
    expect(at({ membership: "off-view" })).toBe("off-view");
    expect(at({ suppressed: true })).toBe("suppressed");
    expect(at({ pending: true })).toBe("pending");
    expect(at({}, "ghost")).toBe("unresolved");
    // A port with no registered socket alias is unresolved, never an interior estimate.
    expect(
      resolveEndpoint(
        input([{ id: "a", frame: FRAME, shown: "front", order: 0 }]),
        { device: "a", port: 7 },
        "output",
      ).kind,
    ).toBe("unresolved");
  });
});

describe("cableRoute", () => {
  it("runs an exit end straight to its lead before the sag", () => {
    const route = cableRoute(
      { kind: "exit", point: { x: 0, y: 0 }, lead: { x: 0, y: 24 } },
      { kind: "socket", point: { x: 300, y: 24 } },
    );
    // dist 300 → sag 60; c1 = (100, 84), c2 = (200, 84).
    expect(route.d).toBe("M 0 0 L 0 24 C 100 84 200 84 300 24");
    expect(route.start).toEqual({ x: 0, y: 24 });
  });
});

describe("resolveConnection mask passes", () => {
  // front-src (front-shown, order 2) → rear-dst (back-shown, input on the *front* → hidden exit, order 0);
  // a third front device `cover` ordered above front-src (order 3); a rear device `under` (order 1).
  const devs: Dev[] = [
    { id: "rear-dst", frame: { x: 0, y: 0, width: 200, height: 100 }, shown: "back", order: 0 },
    { id: "under", frame: { x: 400, y: 0, width: 200, height: 100 }, shown: "back", order: 1 },
    { id: "front-src", frame: FRAME, shown: "front", order: 2 },
    { id: "cover", frame: { x: 1100, y: 2050, width: 200, height: 100 }, shown: "front", order: 3 },
  ];

  it("main excludes hidden-face owners; a front socket adds a foreground pass minus objects above", () => {
    const r = resolveConnection(input(devs), conn("front-src", "rear-dst"));
    expect(r.kind).toBe("full");
    if (r.kind !== "full") return;
    expect(r.from.kind).toBe("socket");
    expect(r.to.kind).toBe("exit");
    expect(r.passes).toEqual([
      { kind: "main", exclude: ["rear-dst"] },
      { kind: "foreground", owner: "front-src", include: "front-src", exclude: ["cover"] },
    ]);
  });

  it("a rear socket needs no foreground pass", () => {
    const rear: Dev[] = [
      { id: "s", frame: FRAME, shown: "back", socketsOn: "back", order: 0 },
      {
        id: "d",
        frame: { x: 0, y: 0, width: 200, height: 100 },
        shown: "back",
        socketsOn: "back",
        order: 1,
      },
    ];
    const r = resolveConnection(input(rear), conn("s", "d"));
    expect(r.kind === "full" && r.passes).toEqual([{ kind: "main", exclude: [] }]);
  });

  it("a foreground pass never paints over a device ordered above its owner", () => {
    const res = input(devs);
    const r = resolveConnection(res, conn("front-src", "rear-dst"));
    if (r.kind !== "full") throw new Error("expected full");
    const fg = r.passes[1];
    if (!fg) throw new Error("expected foreground");
    // Inside front-src but also inside cover (overlap at 1150, 2080) → hidden; inside front-src only → shown.
    expect(passVisibleAt(fg, res.silhouettes, { x: 1150, y: 2080 })).toBe(false);
    expect(passVisibleAt(fg, res.silhouettes, { x: 1050, y: 2030 })).toBe(true);
    // Outside the owner → the foreground pass is not there (the main pass is).
    expect(passVisibleAt(fg, res.silhouettes, { x: 900, y: 1900 })).toBe(false);
    // The main pass is clipped from the hidden owner along the whole curve.
    const main = r.passes[0];
    if (!main) throw new Error("expected main");
    expect(passVisibleAt(main, res.silhouettes, { x: 100, y: 50 })).toBe(false);
    expect(passVisibleAt(main, res.silhouettes, { x: 100, y: 150 })).toBe(true);
  });

  it("is deterministic", () => {
    expect(resolveConnection(input(devs), conn("front-src", "rear-dst"))).toEqual(
      resolveConnection(input(devs), conn("front-src", "rear-dst")),
    );
  });
});

describe("portals", () => {
  const local = (remote: Membership): CableResolutionInput =>
    input([
      { id: "src", frame: FRAME, shown: "back", order: 0 },
      {
        id: "far",
        frame: { x: 0, y: 0, width: 200, height: 100 },
        shown: "front",
        order: 1,
        membership: remote,
      },
    ]);

  it("one local end → a portal from its physical end, with the ordinary passes", () => {
    const r = resolveConnection(local("off-view"), conn("src", "far"));
    expect(r.kind).toBe("portal");
    if (r.kind !== "portal") return;
    // Hidden-face exit (see the endpoint test): lead (1160, 1976); default offset (+180, +36).
    expect(r.base).toEqual({ x: 1160, y: 2000 - EXIT_LEAD });
    expect(r.chip).toEqual({ x: 1160 + 180, y: 2000 - EXIT_LEAD + 36 });
    expect(r.passes).toEqual([{ kind: "main", exclude: ["src"] }]);
    expect(r.remote).toBe("far");
    expect(r.portalKey).toBe("src:0->far:0|from");
  });

  it("applies a persisted offset after the local end resolves", () => {
    const r = resolveConnection(local("off-view"), conn("src", "far"), {
      "src:0->far:0|from": { dx: -50, dy: 10 },
    });
    expect(r.kind === "portal" && r.chip).toEqual({ x: 1110, y: 1986 });
  });

  it("the local end is invariant when only the remote end's membership changes", () => {
    const full = resolveConnection(local("local"), conn("src", "far"));
    const portal = resolveConnection(local("off-view"), conn("src", "far"));
    if (full.kind !== "full" || portal.kind !== "portal") throw new Error("kinds");
    expect(portal.local).toEqual(full.from);
  });

  it("neither end local → nothing drawn", () => {
    const r = resolveConnection(
      input([
        { id: "a", frame: FRAME, shown: "front", order: 0, membership: "off-view" },
        { id: "b", frame: FRAME, shown: "front", order: 1, membership: "off-view" },
      ]),
      conn("a", "b"),
    );
    expect(r).toMatchObject({ kind: "none", reason: "not-local" });
  });

  it("a suppressed end hides the whole lead, full or portal (U1)", () => {
    const r = resolveConnection(
      input([
        { id: "a", frame: FRAME, shown: "front", order: 0, suppressed: true },
        { id: "b", frame: FRAME, shown: "front", order: 1, membership: "off-view" },
      ]),
      conn("a", "b"),
    );
    expect(r).toMatchObject({ kind: "none", reason: "suppressed" });
  });
});

describe("held leads (in hand)", () => {
  const pick = {
    source: { device: "a", port: 0 },
    direction: "output" as const,
    pointer: { x: 0, y: 0 },
  };

  it("a visible source → a lead with one unmasked held pass", () => {
    const r = resolveHeldCable(input([{ id: "a", frame: FRAME, shown: "front", order: 0 }]), pick);
    expect(r).toMatchObject({ kind: "lead", passes: [{ kind: "held", exclude: [] }] });
    expect(r.kind === "lead" && r.path.start).toEqual({ x: 1040, y: 2020 });
  });

  it("a hidden-face source exits like any cable and masks only its own chassis", () => {
    const r = resolveHeldCable(input([{ id: "a", frame: FRAME, shown: "back", order: 0 }]), pick);
    expect(r).toMatchObject({ kind: "lead", passes: [{ kind: "held", exclude: ["a"] }] });
  });

  it("re-resolves from the logical source: moving the device moves the lead", () => {
    const moved = { ...FRAME, x: 2000 };
    const r = resolveHeldCable(input([{ id: "a", frame: moved, shown: "front", order: 0 }]), pick);
    expect(r.kind === "lead" && r.path.start).toEqual({ x: 2040, y: 2020 });
  });

  it("off-view → floating; mid-flip → hidden (the pick survives)", () => {
    expect(
      resolveHeldCable(
        input([{ id: "a", frame: FRAME, shown: "front", order: 0, membership: "off-view" }]),
        pick,
      ),
    ).toEqual({ kind: "floating", pointer: { x: 0, y: 0 } });
    expect(
      resolveHeldCable(
        input([{ id: "a", frame: FRAME, shown: "front", order: 0, suppressed: true }]),
        pick,
      ),
    ).toEqual({ kind: "hidden", reason: "suppressed" });
  });
});

describe("silhouetteContains", () => {
  it("honours the rounded corners", () => {
    const s: SurfaceSilhouette = {
      objectId: "a",
      rect: { x: 0, y: 0, width: 100, height: 50 },
      radius: 10,
      order: 0,
      layer: "front-object",
    };
    expect(silhouetteContains(s, { x: 50, y: 25 })).toBe(true);
    // (1, 1) is in the bounding box but outside the corner arc centred (10, 10): √162 ≈ 12.7 > 10.
    expect(silhouetteContains(s, { x: 1, y: 1 })).toBe(false);
    expect(silhouetteContains(s, { x: 101, y: 25 })).toBe(false);
  });
});
