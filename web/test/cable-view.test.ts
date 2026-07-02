import { describe, expect, it } from "vitest";
import {
  bothInView,
  cableAnchor,
  inView,
  oneInView,
  otherEndLabel,
  PORTAL_LEN,
  portalKey,
  portalOffset,
} from "../src/cable-view";
import type { DeviceDescriptor } from "../src/catalog";
import type { LayoutCtx } from "../src/projection";
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

describe("inView / bothInView / oneInView", () => {
  const scene = makeScene({
    a: place("s1", "front"),
    b: place("s1", "back"),
    c: place("s2", "front"),
  });
  const ctx = ctxOf(scene, "front");
  const conn = (from: string, to: string): Connection => ({
    from: { device: from, port: 0 },
    to: { device: to, port: 0 },
  });

  it("inView is true only for the shown space + wall", () => {
    expect(inView(ctx, "a")).toBe(true); // s1/front — shown
    expect(inView(ctx, "b")).toBe(false); // s1/back — other wall
    expect(inView(ctx, "c")).toBe(false); // s2 — other space
  });
  it("bothInView needs both ends shown; oneInView needs exactly one", () => {
    expect(bothInView(ctx, conn("a", "a"))).toBe(true);
    expect(bothInView(ctx, conn("a", "b"))).toBe(false);
    expect(oneInView(ctx, conn("a", "b"))).toBe(true); // a shown, b not
    expect(oneInView(ctx, conn("b", "c"))).toBe(false); // neither shown
  });
});

describe("otherEndLabel", () => {
  const scene = makeScene({ b: place("s1", "back"), c: place("s2", "front") });
  const ctx = ctxOf(scene, "front");
  it("names the room when the other end is in a different space", () => {
    expect(otherEndLabel(ctx, WALL_LABELS, "c")).toBe("Booth");
  });
  it("names the wall when the other end is a different wall of this room", () => {
    expect(otherEndLabel(ctx, WALL_LABELS, "b")).toBe("Back");
  });
  it("is '?' for an unplaced device", () => {
    expect(otherEndLabel(ctx, WALL_LABELS, "ghost")).toBe("?");
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
    expect(cableAnchor(ctxOf(scene), {}, ref("a"), "output", idApi)).toEqual({ x: 124, y: 45 });
  });

  it("front-facing input: nudged left (0.38)", () => {
    const scene = makeScene({ a: place("s1", "front", "front") });
    expect(cableAnchor(ctxOf(scene), {}, ref("a"), "input", idApi)).toEqual({ x: 76, y: 45 });
  });

  it("back-facing: anchors at the measured socket when available", () => {
    const scene = makeScene({ a: place("s1", "front", "back") });
    const anchors = { "a:output:0": { x: 9, y: 9 } };
    expect(cableAnchor(ctxOf(scene), anchors, ref("a"), "output", idApi)).toEqual({ x: 9, y: 9 });
  });

  it("back-facing but unmeasured: falls back to the chassis estimate", () => {
    const scene = makeScene({ a: place("s1", "front", "back") });
    expect(cableAnchor(ctxOf(scene), {}, ref("a"), "output", idApi)).toEqual({ x: 124, y: 45 });
  });

  it("is null when the device isn't in the shown view (drawn as a portal instead)", () => {
    const scene = makeScene({ a: place("s1", "back") });
    expect(cableAnchor(ctxOf(scene, "front"), {}, ref("a"), "output", idApi)).toBeNull();
  });
});
