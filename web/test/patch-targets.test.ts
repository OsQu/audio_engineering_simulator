// Story 10.7.1: patchable socket targets and the client-pixel picker.
import { describe, expect, it } from "vitest";
import type { CableResolutionInput, Membership, SurfaceSilhouette } from "../src/cable-resolution";
import {
  MOUSE_TOLERANCE_PX,
  type PickContext,
  pickSocketTarget,
  resolveSocketTargets,
  type SocketTarget,
  TOUCH_TOLERANCE_PX,
} from "../src/patch-targets";
import type { FaceGeometry, ProjectedSlot, SocketGeometryIndex } from "../src/socket-geometry";

// A 100 × 50 face-local mm face with an output at (20, 10) and an input at (80, 40), both r = 3.
const face = (device: string, duplex = false): FaceGeometry => ({
  width: 100,
  height: 50,
  sockets: [
    {
      id: `${device}:output:0`,
      aliases: duplex ? [`${device}:output:0`, `${device}:input:0`] : [`${device}:output:0`],
      x: 20,
      y: 10,
      r: 3,
    },
    ...(duplex
      ? []
      : [{ id: `${device}:input:0`, aliases: [`${device}:input:0`], x: 80, y: 40, r: 3 }]),
  ],
});

type Frame = { x: number; y: number; width: number; height: number };

interface Dev {
  id: string;
  frame: Frame | null;
  shown?: "front" | "back";
  order: number;
  membership?: Membership;
  suppressed?: boolean;
  pending?: boolean;
  duplex?: boolean;
  silhouette?: boolean;
}

function input(devs: Dev[]): CableResolutionInput {
  const entries: Record<string, SocketGeometryIndex["entries"][string]> = {};
  const slots = new Map<string, ProjectedSlot[]>();
  const silhouettes = new Map<string, SurfaceSilhouette>();
  for (const d of devs) {
    const faceId = `studio:${d.id}`;
    // Sockets are always on the front side; `shown` decides whether that side is presented.
    entries[`${faceId}|front`] = d.pending
      ? { status: "pending", revision: "r", generation: 1 }
      : { status: "current", revision: "r", generation: 1, geometry: face(d.id, d.duplex) };
    const shown = d.shown ?? "front";
    slots.set(
      d.id,
      (["front", "back"] as const).map((side) => ({
        slot: { face: faceId, side },
        frame: d.frame,
        presented: side === shown,
      })),
    );
    if (d.frame && d.silhouette !== false) {
      silhouettes.set(d.id, {
        objectId: d.id,
        rect: d.frame,
        radius: 6,
        order: d.order,
        layer: shown === "front" ? "front-object" : "rear-object",
      });
    }
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

// Frame scale 2: 200 × 100 surface units at (1000, 2000). Output → (1040, 2020) r 6; input → (1160, 2080).
const FRAME = { x: 1000, y: 2000, width: 200, height: 100 };
const ids = (targets: readonly SocketTarget[]): string[] => targets.map((t) => t.id);

describe("resolveSocketTargets", () => {
  it("projects presented sockets with a scaled radius (hand calc)", () => {
    const targets = resolveSocketTargets(input([{ id: "a", frame: FRAME, order: 0 }]));
    expect(targets).toEqual([
      {
        id: "a:input:0",
        aliases: ["a:input:0"],
        objectId: "a",
        point: { x: 1160, y: 2080 },
        radius: 6,
        order: 0,
      },
      {
        id: "a:output:0",
        aliases: ["a:output:0"],
        objectId: "a",
        point: { x: 1040, y: 2020 },
        radius: 6,
        order: 0,
      },
    ]);
  });

  it.each<[string, Partial<Dev>]>([
    ["hidden face (sockets seen through the chassis)", { shown: "back" }],
    ["mid-flip (suppressed)", { suppressed: true }],
    ["off-view", { membership: "off-view" }],
    ["unresolved", { membership: "unresolved" }],
    ["geometry pending", { pending: true }],
    ["unprojectable frame (top view / not laid out)", { frame: null }],
  ])("excludes %s", (_name, patch) => {
    expect(resolveSocketTargets(input([{ id: "a", frame: FRAME, order: 0, ...patch }]))).toEqual(
      [],
    );
  });

  it("gives a focus-window face no targets: it is never one of the stage's slots", () => {
    // The stage input only carries stage faces; an object it doesn't list contributes nothing.
    expect(resolveSocketTargets(input([]))).toEqual([]);
  });

  it("drops a socket whose centre is covered by a silhouette ordered above its owner", () => {
    // b (order 1) covers a's output at (1040, 2020) but not its input at (1160, 2080).
    const cover = { x: 1000, y: 2000, width: 80, height: 40 };
    const targets = resolveSocketTargets(
      input([
        { id: "a", frame: FRAME, order: 0 },
        { id: "b", frame: cover, order: 1, shown: "back" },
      ]),
    );
    expect(ids(targets)).toEqual(["a:input:0"]);
  });

  it("keeps a socket covered only by a silhouette ordered below its owner", () => {
    const below = { x: 1000, y: 2000, width: 80, height: 40 };
    const targets = resolveSocketTargets(
      input([
        { id: "a", frame: FRAME, order: 1 },
        { id: "rack", frame: below, order: 0, shown: "back" },
      ]),
    );
    expect(ids(targets)).toEqual(["a:input:0", "a:output:0"]);
  });

  it("gives one duplex socket one target carrying both aliases", () => {
    const targets = resolveSocketTargets(
      input([{ id: "u", frame: FRAME, order: 0, duplex: true }]),
    );
    expect(targets).toHaveLength(1);
    expect(targets[0]?.aliases).toEqual(["u:output:0", "u:input:0"]);
  });

  it("orders an owner without a silhouette (the bench DUT) below everything", () => {
    const targets = resolveSocketTargets(
      input([{ id: "dut", frame: FRAME, order: 3, silhouette: false }]),
    );
    expect(targets.every((t) => t.order === -1)).toBe(true);
  });

  it("is deterministic for identical input", () => {
    const devs: Dev[] = [
      { id: "b", frame: { x: 0, y: 0, width: 100, height: 50 }, order: 1 },
      { id: "a", frame: FRAME, order: 0 },
    ];
    expect(resolveSocketTargets(input(devs))).toEqual(resolveSocketTargets(input(devs)));
  });
});

// --- Picking --------------------------------------------------------------------------------------------

const target = (id: string, x: number, y: number, radius = 1, order = 0): SocketTarget => ({
  id,
  aliases: [id],
  objectId: id.split(":")[0] ?? id,
  point: { x, y },
  radius,
  order,
});

const ctx = (patch: Partial<PickContext> = {}): PickContext => ({
  zoom: 1,
  origin: { x: 0, y: 0 },
  viewport: { left: 0, top: 0, right: 10_000, bottom: 10_000 },
  blockers: [],
  silhouettes: new Map(),
  pointerType: "mouse",
  ...patch,
});

describe("pickSocketTarget", () => {
  it("prefers an exact physical hit over a nearer centre that isn't hit", () => {
    // big: r 10 at (100, 100); pointer at (109, 100) → distance 9 ≤ 10 → exact.
    // small: r 1 at (114, 100); distance 5 > 1 → only "near".
    const pick = pickSocketTarget(
      [target("big:o:0", 100, 100, 10), target("small:o:0", 114, 100, 1)],
      { x: 109, y: 100 },
      ctx(),
    );
    expect(pick).toEqual({ kind: "exact", target: target("big:o:0", 100, 100, 10) });
  });

  it("otherwise picks the nearest centre within the minimum radius", () => {
    // a at distance 6, b at distance 4 (both > r 1, ≤ 8 px) → b.
    const pick = pickSocketTarget(
      [target("a:o:0", 100, 100), target("b:o:0", 110, 100)],
      { x: 106, y: 100 },
      ctx(),
    );
    expect(pick.kind === "near" && pick.target.id).toBe("b:o:0");
  });

  it("misses beyond the minimum radius", () => {
    const pick = pickSocketTarget([target("a:o:0", 100, 100)], { x: 108.5, y: 100 }, ctx());
    expect(pick).toEqual({ kind: "none", reason: "miss" });
  });

  it("breaks distance ties by paint order, then socket id", () => {
    // Both centres 5 px away.
    const left = target("a:o:0", 95, 100, 1, 0);
    const right = target("b:o:0", 105, 100, 1, 1);
    const byOrder = pickSocketTarget([left, right], { x: 100, y: 100 }, ctx());
    expect(byOrder.kind === "near" && byOrder.target.id).toBe("b:o:0");
    const sameOrder = pickSocketTarget(
      [target("b:o:0", 105, 100), target("a:o:0", 95, 100)],
      { x: 100, y: 100 },
      ctx(),
    );
    expect(sameOrder.kind === "near" && sameOrder.target.id).toBe("a:o:0");
  });

  it("does not skip the nearest socket for a farther one (legality is the caller's)", () => {
    const pick = pickSocketTarget(
      [target("illegal:o:0", 103, 100), target("legal:i:0", 93, 100)],
      { x: 100, y: 100 },
      ctx(),
    );
    expect(pick.kind === "near" && pick.target.id).toBe("illegal:o:0");
  });

  it.each([0.2, 1, 3])("keeps the %s× zoom tolerance at 8 screen px for a tiny socket", (zoom) => {
    // Socket r 0.5 surface units at (100, 100), origin (50, 20): centre client (50 + 100z, 20 + 100z).
    const sockets = [target("a:o:0", 100, 100, 0.5)];
    const cx = 50 + 100 * zoom;
    const cy = 20 + 100 * zoom;
    const c = ctx({ zoom, origin: { x: 50, y: 20 } });
    // Rendered radius 0.5z ≤ 1.5 px; max(8, 1.5 + 6 halo) = 8 → tolerance 8 px at every zoom.
    expect(pickSocketTarget(sockets, { x: cx + 7.9, y: cy }, c).kind).toBe("near");
    expect(pickSocketTarget(sockets, { x: cx + 8.1, y: cy }, c).kind).toBe("none");
  });

  it("uses the rendered radius when it exceeds the minimum (zoomed-in)", () => {
    // r 5 at zoom 3 → 15 px rendered; 14 px away is an exact hit.
    const pick = pickSocketTarget(
      [target("a:o:0", 100, 100, 5)],
      { x: 314, y: 300 },
      ctx({ zoom: 3 }),
    );
    expect(pick.kind).toBe("exact");
  });

  it("adds a 6 px halo beyond a zoomed-in socket's rendered edge", () => {
    // r 5 at zoom 3 → 15 px rendered; tolerance max(8, 15 + 6) = 21 px.
    const sockets = [target("a:o:0", 100, 100, 5)];
    expect(pickSocketTarget(sockets, { x: 320.5, y: 300 }, ctx({ zoom: 3 })).kind).toBe("near");
    expect(pickSocketTarget(sockets, { x: 321.5, y: 300 }, ctx({ zoom: 3 })).kind).toBe("none");
  });

  it("widens the minimum to 12 px for touch", () => {
    expect(MOUSE_TOLERANCE_PX).toBe(8);
    expect(TOUCH_TOLERANCE_PX).toBe(12);
    const sockets = [target("a:o:0", 100, 100)];
    expect(pickSocketTarget(sockets, { x: 111, y: 100 }, ctx()).kind).toBe("none");
    expect(pickSocketTarget(sockets, { x: 111, y: 100 }, ctx({ pointerType: "touch" })).kind).toBe(
      "near",
    );
  });

  it("clips by the viewport: pointer and target centre must both be inside", () => {
    const viewport = { left: 0, top: 0, right: 104, bottom: 500 };
    expect(
      pickSocketTarget([target("a:o:0", 100, 100)], { x: 106, y: 100 }, ctx({ viewport })),
    ).toEqual({ kind: "none", reason: "outside" });
    // Centre (106, 100) is outside even though the pointer (102, 100) is inside.
    expect(
      pickSocketTarget([target("a:o:0", 106, 100)], { x: 102, y: 100 }, ctx({ viewport })),
    ).toEqual({ kind: "none", reason: "miss" });
  });

  it("never reaches through a blocker", () => {
    const blockers = [{ left: 95, top: 95, right: 105, bottom: 105 }];
    // Pointer on the blocker.
    expect(
      pickSocketTarget([target("a:o:0", 110, 100)], { x: 104, y: 100 }, ctx({ blockers })),
    ).toEqual({ kind: "none", reason: "blocked" });
    // Target centre under the blocker, pointer beside it.
    expect(
      pickSocketTarget([target("a:o:0", 100, 100)], { x: 107, y: 100 }, ctx({ blockers })),
    ).toEqual({ kind: "none", reason: "miss" });
  });

  it("clips a partially exposed socket's disc by a silhouette above its owner", () => {
    // a's socket at (100, 100), order 0; b's chassis (order 1) covers x ≥ 104.
    const silhouettes = new Map<string, SurfaceSilhouette>([
      [
        "b",
        {
          objectId: "b",
          rect: { x: 104, y: 0, width: 100, height: 200 },
          radius: 0,
          order: 1,
          layer: "front-object",
        },
      ],
    ]);
    const sockets = [target("a:o:0", 100, 100, 1, 0)];
    expect(pickSocketTarget(sockets, { x: 103, y: 100 }, ctx({ silhouettes })).kind).toBe("near");
    expect(pickSocketTarget(sockets, { x: 106, y: 100 }, ctx({ silhouettes })).kind).toBe("none");
  });
});
