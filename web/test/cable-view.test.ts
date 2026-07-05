import { describe, expect, it } from "vitest";
import {
  bothInView,
  type CableLayout,
  cableAnchor,
  cableEndpoints,
  deviceSurfaceRect,
  oneInView,
  otherEndLabel,
  PORTAL_LEN,
  portalKey,
  portalOffset,
  tipPatchEnd,
} from "../src/cable-view";
import type { DeviceDescriptor } from "../src/catalog";
import { deviceById, deviceRect, effectiveFacing, type LayoutCtx } from "../src/projection";
import type { Connection } from "../src/scene";
import type { Placement, Scene } from "../src/scene-store";
import type { Room, Wall } from "../src/spatial";

const ROOM: Room = { width: 4000, depth: 3000, height: 1400 };
const WALL_LABELS: Record<Wall, string> = {
  front: "Front",
  back: "Back",
  left: "Left",
  right: "Right",
};

// A 200×100×150 desktop device.
const DESK: DeviceDescriptor = {
  typeId: "amp",
  name: "Amp",
  formFactor: { kind: "desktop", widthMm: 200, heightMm: 100, depthMm: 150 },
  params: [],
  ports: [],
  readouts: [],
};
const CATALOG = [DESK];

// An identity world api — worldToSurface passes coords through, so the estimate math is readable.
const idApi = {
  worldToSurface: (x: number, y: number) => ({ x, y }),
  clientToSurface: (x: number, y: number) => ({ x, y }),
};

const place = (space: string, wall: Wall, facing: "front" | "back" = "front"): Placement => ({
  space,
  wall,
  position: { x: 0, y: 0, z: 0 },
  facing,
});

function makeScene(
  placements: Record<string, Placement>,
  portals: Record<string, { dx: number; dy: number }> = {},
): Scene {
  return {
    schemaVersion: 99,
    ui: {
      spaces: [
        { id: "s1", name: "Studio", room: ROOM },
        { id: "s2", name: "Booth", room: ROOM },
      ],
      racks: [],
      placements,
      portals,
    },
    patch: {
      devices: Object.keys(placements).map((id) => ({ id, typeId: "amp" })),
      connections: [],
      output: { device: "", port: 0 },
    },
  };
}

function ctxOf(scene: Scene, wall: Wall = "front"): LayoutCtx {
  return { space: "s1", view: wall, wall, room: ROOM, scene, catalog: CATALOG };
}

// The scene view's CableLayout — the projection-backed seam App feeds cable-view. Every geometry test
// below runs through it, so it stays a parity guard on the scene-view cable behavior after the decouple.
function layoutOf(scene: Scene, wall: Wall = "front"): CableLayout {
  const ctx = ctxOf(scene, wall);
  return {
    inView: (id) => {
      const p = scene.ui.placements[id];
      return p?.space === ctx.space && p.wall === ctx.wall;
    },
    faceAnchorable: (id, face) => face === effectiveFacing(scene, id),
    rect: (id) => {
      const d = deviceById(scene, id);
      return d ? deviceRect(ctx, id, d.typeId) : null;
    },
    clampsEstimate: (id) => effectiveFacing(scene, id) === "back",
    frontPatchOver: (id) => effectiveFacing(scene, id) === "front",
  };
}

describe("inView / bothInView / oneInView", () => {
  const scene = makeScene({
    a: place("s1", "front"),
    b: place("s1", "back"),
    c: place("s2", "front"),
  });
  const L = layoutOf(scene, "front");
  const conn = (from: string, to: string): Connection => ({
    from: { device: from, port: 0 },
    to: { device: to, port: 0 },
  });

  it("inView is true only for the shown space + wall", () => {
    expect(L.inView("a")).toBe(true); // s1/front — shown
    expect(L.inView("b")).toBe(false); // s1/back — other wall
    expect(L.inView("c")).toBe(false); // s2 — other space
  });
  it("bothInView needs both ends shown; oneInView needs exactly one", () => {
    expect(bothInView(L, conn("a", "a"))).toBe(true);
    expect(bothInView(L, conn("a", "b"))).toBe(false);
    expect(oneInView(L, conn("a", "b"))).toBe(true); // a shown, b not
    expect(oneInView(L, conn("b", "c"))).toBe(false); // neither shown
  });
});

describe("otherEndLabel", () => {
  const scene = makeScene({ b: place("s1", "back"), c: place("s2", "front") });
  it("names the room when the other end is in a different space", () => {
    expect(otherEndLabel(scene, "s1", WALL_LABELS, "c")).toBe("Booth");
  });
  it("names the wall when the other end is a different wall of this room", () => {
    expect(otherEndLabel(scene, "s1", WALL_LABELS, "b")).toBe("Back");
  });
  it("is '?' for an unplaced device", () => {
    expect(otherEndLabel(scene, "s1", WALL_LABELS, "ghost")).toBe("?");
  });
});

describe("portalKey / portalOffset", () => {
  const conn: Connection = { from: { device: "a", port: 0 }, to: { device: "b", port: 1 } };
  it("keys per connection + end", () => {
    expect(portalKey(conn, true)).toBe("a:0->b:1|from");
    expect(portalKey(conn, false)).toBe("a:0->b:1|to");
  });
  it("defaults the stub out toward signal flow, dropped below the jack", () => {
    const scene = makeScene({});
    expect(portalOffset(scene, conn, true)).toEqual({ dx: PORTAL_LEN, dy: 36 });
    expect(portalOffset(scene, conn, false)).toEqual({ dx: -PORTAL_LEN, dy: 36 });
  });
  it("returns the persisted offset when the operator has dragged the chip", () => {
    const scene = makeScene({}, { "a:0->b:1|from": { dx: 5, dy: 7 } });
    expect(portalOffset(scene, conn, true)).toEqual({ dx: 5, dy: 7 });
  });
});

describe("cableAnchor", () => {
  const ref = (device: string, port = 0) => ({ device, port });

  it("front-facing output: estimates near the chassis, nudged right (0.62) at 0.45 height", () => {
    const scene = makeScene({ a: place("s1", "front", "front") });
    // deviceRect(front) = {x:0,y:0,w:200,h:100}; output wx = 200·0.62 = 124, wy = 100·0.45 = 45
    expect(cableAnchor(layoutOf(scene), {}, ref("a"), "output", idApi)).toEqual({ x: 124, y: 45 });
  });

  it("front-facing input: nudged left (0.38)", () => {
    const scene = makeScene({ a: place("s1", "front", "front") });
    expect(cableAnchor(layoutOf(scene), {}, ref("a"), "input", idApi)).toEqual({ x: 76, y: 45 });
  });

  it("back-facing: anchors at the measured socket when its face is the shown (back) face", () => {
    const scene = makeScene({ a: place("s1", "front", "back") });
    const anchors = { "a:output:0": { x: 9, y: 9, face: "back" as const } };
    expect(cableAnchor(layoutOf(scene), anchors, ref("a"), "output", idApi)).toEqual({
      x: 9,
      y: 9,
      face: "back",
    });
  });

  it("back-facing but unmeasured: falls back to the chassis estimate", () => {
    const scene = makeScene({ a: place("s1", "front", "back") });
    expect(cableAnchor(layoutOf(scene), {}, ref("a"), "output", idApi)).toEqual({ x: 124, y: 45 });
  });

  it("front-face jack: anchors at the measured socket when the front face is shown", () => {
    // A faceplate (e.g. the Scarlett) can place a jack on the front face. Front is shown, the jack's
    // measured face matches → anchor precisely at it rather than the mid-chassis estimate.
    const scene = makeScene({ a: place("s1", "front", "front") });
    const anchors = { "a:input:0": { x: 3, y: 4, face: "front" as const } };
    expect(cableAnchor(layoutOf(scene), anchors, ref("a"), "input", idApi)).toEqual({
      x: 3,
      y: 4,
      face: "front",
    });
  });

  it("hidden-face jack: ignores a measured anchor on the away face and estimates instead", () => {
    // Back is shown, but this jack was measured on the front (hidden) face — its centre is mirrored under
    // rotateY(180deg), so it must be ignored and the interior chassis estimate used instead.
    // deviceRect = {x:0,y:0,w:200,h:100}; output wx = 200·0.62 = 124, wy = 100·0.45 = 45
    const scene = makeScene({ a: place("s1", "front", "back") });
    const anchors = { "a:output:0": { x: 9, y: 9, face: "front" as const } };
    expect(cableAnchor(layoutOf(scene), anchors, ref("a"), "output", idApi)).toEqual({
      x: 124,
      y: 45,
    });
  });

  it("rack-mounted: anchors at the measured socket when the *rack* is turned to back (own facing stays front)", () => {
    // The regression: a bolted-in unit keeps facing "front"; only the rack flips. cableAnchor must use
    // effective (rack) facing, else it estimates the front-panel position and the cable floats off the jack.
    const scene = makeScene({
      a: { ...place("s1", "back", "front"), rack: { id: "r1", uSlot: 0 } },
    });
    scene.ui.racks = [
      {
        id: "r1",
        space: "s1",
        wall: "back",
        facing: "back",
        position: { x: 0, y: 0, z: 0 },
        slots: 8,
      },
    ];
    const anchors = { "a:output:0": { x: 9, y: 9, face: "back" as const } };
    expect(cableAnchor(layoutOf(scene, "back"), anchors, ref("a"), "output", idApi)).toEqual({
      x: 9,
      y: 9,
      face: "back",
    });
  });

  it("is null when the device isn't in the shown view (drawn as a portal instead)", () => {
    const scene = makeScene({ a: place("s1", "back") });
    expect(cableAnchor(layoutOf(scene, "front"), {}, ref("a"), "output", idApi)).toBeNull();
  });
});

describe("tipPatchEnd", () => {
  const ref = (device: string, port = 0) => ({ device, port });

  it("is true for a measured front-face jack on a front-shown device (occluded by its panel)", () => {
    const scene = makeScene({ a: place("s1", "front", "front") });
    const anchors = { "a:input:0": { x: 3, y: 4, face: "front" as const } };
    expect(tipPatchEnd(layoutOf(scene), anchors, ref("a"), "input")).toBe(true);
  });

  it("is false on a back-shown device (it already sits below the cables — nothing to patch over)", () => {
    const scene = makeScene({ a: place("s1", "front", "back") });
    const anchors = { "a:output:0": { x: 9, y: 9, face: "back" as const } };
    expect(tipPatchEnd(layoutOf(scene), anchors, ref("a"), "output")).toBe(false);
  });

  it("is false when the jack isn't measured yet (the end anchors to an estimate, no visible socket)", () => {
    const scene = makeScene({ a: place("s1", "front", "front") });
    expect(tipPatchEnd(layoutOf(scene), {}, ref("a"), "input")).toBe(false);
  });

  it("is false for a jack measured on the hidden (away) face of a front-shown device", () => {
    const scene = makeScene({ a: place("s1", "front", "front") });
    const anchors = { "a:input:0": { x: 3, y: 4, face: "back" as const } };
    expect(tipPatchEnd(layoutOf(scene), anchors, ref("a"), "input")).toBe(false);
  });

  it("is false when the device isn't in the shown view", () => {
    const scene = makeScene({ a: place("s1", "back", "front") });
    const anchors = { "a:input:0": { x: 3, y: 4, face: "front" as const } };
    expect(tipPatchEnd(layoutOf(scene, "front"), anchors, ref("a"), "input")).toBe(false);
  });
});

describe("deviceSurfaceRect", () => {
  it("returns the device's chassis rect in surface coords (top-left origin)", () => {
    const scene = makeScene({ a: place("s1", "front", "front") });
    // deviceRect(front) = {x:0,y:0,w:200,h:100}; idApi passes coords through → the same box, y-down.
    expect(deviceSurfaceRect(layoutOf(scene), "a", idApi)).toEqual({
      x: 0,
      y: 0,
      width: 200,
      height: 100,
    });
  });

  it("is null when the device has no rect (not placed / unknown id)", () => {
    const scene = makeScene({ a: place("s1", "front") });
    expect(deviceSurfaceRect(layoutOf(scene), "ghost", idApi)).toBeNull();
  });
});

describe("cableEndpoints — back-shown chassis clamp", () => {
  const conn = (from: string, to: string): Connection => ({
    from: { device: from, port: 0 },
    to: { device: to, port: 0 },
  });
  // Two devices on the front wall, 200×100 each: `dst` at the origin (rect x∈[0,200]), `src` at x=400
  // (rect x∈[400,600]) — non-overlapping, so src's estimate lands outside dst's rect.
  const twoScene = (dstFacing: "front" | "back") =>
    makeScene({
      dst: { ...place("s1", "front", dstFacing), position: { x: 0, y: 0, z: 0 } },
      src: { ...place("s1", "front", "front"), position: { x: 400, y: 0, z: 0 } },
    });

  it("clamps a back-shown device's hidden-face estimate to the chassis edge toward the other end", () => {
    // dst back-shown, unmeasured input → estimate wx=200·0.38=76, wy=100·0.45=45 ⇒ {76,45}, inside dst's
    // rect x∈[0,200] y∈[0,100]. src output estimate = 400+200·0.62=524, y=45 ⇒ {524,45} (the "other" end).
    // Segment {76,45}→{524,45} exits dst's rect at the right edge x=200 (t=124/448), y=45 ⇒ {200,45}.
    const ends = cableEndpoints(layoutOf(twoScene("back")), {}, conn("src", "dst"), idApi);
    expect(ends?.b).toEqual({ x: 200, y: 45 });
    // src is front-shown → sits above the cables, so its estimate end is left unclamped.
    expect(ends?.a).toEqual({ x: 524, y: 45 });
  });

  it("does not clamp a front-shown device's estimate (it sits above the cables, hiding the estimate)", () => {
    // Same layout, dst front-shown: its input estimate {76,45} is kept as-is.
    const ends = cableEndpoints(layoutOf(twoScene("front")), {}, conn("src", "dst"), idApi);
    expect(ends?.b).toEqual({ x: 76, y: 45 });
  });

  it("keeps the estimate when the other end is inside the same rect (degenerate overlap, no crossing)", () => {
    // Both devices at the origin → src output estimate {124,45} lands inside dst's rect x∈[0,200] y∈[0,100],
    // so the segment from dst's input estimate {76,45} toward it never crosses the boundary ⇒ unchanged.
    const scene = makeScene({
      dst: place("s1", "front", "back"),
      src: place("s1", "front", "front"),
    });
    const ends = cableEndpoints(layoutOf(scene), {}, conn("src", "dst"), idApi);
    expect(ends?.b).toEqual({ x: 76, y: 45 });
  });

  it("uses the precise socket (no clamp) when the jack is measured on the shown back face", () => {
    const anchors = { "dst:input:0": { x: 9, y: 9, face: "back" as const } };
    const ends = cableEndpoints(layoutOf(twoScene("back")), anchors, conn("src", "dst"), idApi);
    expect(ends?.b).toEqual({ x: 9, y: 9, face: "back" });
  });

  it("is null when an end is off-view (a portal stub, not a continuous cable)", () => {
    const scene = makeScene({
      dst: place("s1", "front", "back"),
      src: place("s1", "back", "front"),
    });
    expect(cableEndpoints(layoutOf(scene, "front"), {}, conn("src", "dst"), idApi)).toBeNull();
  });
});

// The workbench bench's CableLayout: a flat surface showing *both* faces of every device, no rooms/walls,
// no z-interleave. Every device is in view, either face's measured jack anchors precisely, and there is no
// chassis rect (so the interior estimate never fires) and no clamp/tip-patch.
describe("bench CableLayout (both faces flat)", () => {
  const bench: CableLayout = {
    inView: () => true,
    faceAnchorable: () => true,
    rect: () => null,
    clampsEstimate: () => false,
    frontPatchOver: () => false,
  };
  const ref = (device: string, port = 0) => ({ device, port });
  const conn = (from: string, to: string): Connection => ({
    from: { device: from, port: 0 },
    to: { device: to, port: 0 },
  });

  it("anchors precisely at a measured jack on EITHER face (both faces are shown on the bench)", () => {
    const anchors = {
      "a:output:0": { x: 5, y: 6, face: "front" as const },
      "b:input:0": { x: 7, y: 8, face: "back" as const },
    };
    expect(cableAnchor(bench, anchors, ref("a"), "output", idApi)).toEqual({
      x: 5,
      y: 6,
      face: "front",
    });
    expect(cableAnchor(bench, anchors, ref("b"), "input", idApi)).toEqual({
      x: 7,
      y: 8,
      face: "back",
    });
  });

  it("joins two measured jacks with no clamp (nothing sits below the cables)", () => {
    const anchors = {
      "src:output:0": { x: 10, y: 20, face: "back" as const },
      "dst:input:0": { x: 100, y: 40, face: "back" as const },
    };
    expect(cableEndpoints(bench, anchors, conn("src", "dst"), idApi)).toEqual({
      a: { x: 10, y: 20, face: "back" },
      b: { x: 100, y: 40, face: "back" },
    });
  });

  it("draws nothing until a jack is measured (no chassis rect ⇒ no interior estimate)", () => {
    expect(cableAnchor(bench, {}, ref("a"), "output", idApi)).toBeNull();
    expect(cableEndpoints(bench, {}, conn("src", "dst"), idApi)).toBeNull();
  });

  it("never tip-patches (cables draw above the flat panels)", () => {
    const anchors = { "a:input:0": { x: 3, y: 4, face: "front" as const } };
    expect(tipPatchEnd(bench, anchors, ref("a"), "input")).toBe(false);
  });
});
