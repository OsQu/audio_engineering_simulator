import { describe, expect, it } from "vitest";
import type { DeviceDescriptor } from "../src/catalog";
import {
  deviceById,
  deviceRect,
  deviceUnits,
  effectiveFacing,
  FRAME_MARGIN,
  isRack,
  type LayoutCtx,
  placedItemsFor,
  rackById,
  rackFrameSize,
  rackRect,
  type ViewCtx,
} from "../src/projection";
import type { Rack, Scene } from "../src/scene-store";
import { RACK_UNIT_MM, RACK_WIDTH_MM, type Room, type Wall } from "../src/spatial";

// A 4 m × 3 m room, 1.4 m of wall shown — matches the default space.
const ROOM: Room = { width: 4000, depth: 3000, height: 1400 };

// Two device descriptors: an 8U rackmount box and a small desktop box.
const RACK_UNIT: DeviceDescriptor = {
  typeId: "comp",
  name: "Compressor",
  formFactor: { kind: "rackmount", rackUnits: 2 },
  params: [],
  ports: [],
  readouts: [],
  configs: [],
};
const DESK_UNIT: DeviceDescriptor = {
  typeId: "synth",
  name: "Synth",
  formFactor: { kind: "desktop", widthMm: 200, heightMm: 100, depthMm: 150 },
  params: [],
  ports: [],
  readouts: [],
  configs: [],
};
const CATALOG = [RACK_UNIT, DESK_UNIT];

// A rack standing against `wall` in space `s1`, U-slot region lower-left-front at (100, 0, 500).
const rack = (id: string, wall: Wall): Rack => ({
  id,
  space: "s1",
  wall,
  facing: "front",
  position: { x: 100, y: 0, z: 500 },
  slots: 8,
});

const viewCtx = (view: Wall | "top"): ViewCtx => ({
  space: "s1",
  view,
  wall: view === "top" ? null : view,
  room: ROOM,
});

// Build a minimal scene from racks + placements; the patch carries only device ids/typeIds.
function makeScene(racks: Rack[], placements: Scene["ui"]["placements"]): Scene {
  return {
    schemaVersion: 99,
    ui: { spaces: [{ id: "s1", name: "Studio", room: ROOM }], racks, placements, portals: {} },
    patch: {
      devices: Object.entries(placements).map(([id]) => ({
        id,
        typeId: id.startsWith("d") ? "synth" : "comp",
      })),
      connections: [],
      output: { device: "", port: 0 },
    },
  };
}

function layout(scene: Scene, view: Wall | "top"): LayoutCtx {
  return { ...viewCtx(view), scene, catalog: CATALOG };
}

describe("rackFrameSize", () => {
  it("is the U-slot column plus a margin on each side, standard rack width", () => {
    // width = 482.6 + 2·14 = 510.6; height = 8·44.45 + 2·14 = 355.6 + 28 = 383.6
    const size = rackFrameSize(rack("r1", "front"));
    expect(size.width).toBeCloseTo(RACK_WIDTH_MM + 2 * FRAME_MARGIN);
    expect(size.width).toBeCloseTo(510.6);
    expect(size.height).toBeCloseTo(8 * RACK_UNIT_MM + 2 * FRAME_MARGIN);
    expect(size.height).toBeCloseTo(383.6);
  });
});

describe("rackRect", () => {
  it("front wall: identity projection at the rack's world (x, y)", () => {
    const r = rackRect(viewCtx("front"), rack("r1", "front"));
    // front: x = pos.x = 100, width = frame width = 510.6
    expect(r).toMatchObject({ x: 100, y: 0 });
    expect(r.width).toBeCloseTo(510.6);
    expect(r.height).toBeCloseTo(383.6);
  });

  it("back wall: x mirrored about the room width", () => {
    const r = rackRect(viewCtx("back"), rack("r1", "back"));
    // back: x = room.width - (pos.x + width) = 4000 - (100 + 510.6) = 3389.4
    expect(r.x).toBeCloseTo(3389.4);
    expect(r.width).toBeCloseTo(510.6);
  });

  it("left wall: x runs along z (frame width preserved via oriented depth)", () => {
    const r = rackRect(viewCtx("left"), rack("r1", "left"));
    // left: screen-x = pos.z = 500; the oriented box turns 90° so the frame width (510.6) still spans the wall
    expect(r.x).toBeCloseTo(500);
    expect(r.width).toBeCloseTo(510.6);
    expect(r.height).toBeCloseTo(383.6);
  });

  it("top view: floor footprint on the (x, z) plane", () => {
    const r = rackRect(viewCtx("top"), rack("r1", "front"));
    // top: x = pos.x = 100, y = pos.z = 500, width = 510.6, height = frame depth = 300
    expect(r).toMatchObject({ x: 100, y: 500 });
    expect(r.width).toBeCloseTo(510.6);
    expect(r.height).toBeCloseTo(300);
  });
});

describe("deviceRect — rack-mounted (elevation)", () => {
  // A 2U device mounted at U-slot 2 of the front-wall rack.
  const mounted = makeScene([rack("r1", "front")], {
    c1: {
      space: "s1",
      wall: "front",
      position: { x: 0, y: 0, z: 0 },
      rack: { id: "r1", uSlot: 2 },
      facing: "front",
    },
  });

  it("front: sits FRAME_MARGIN inside the projected frame, at panel width", () => {
    const r = deviceRect(layout(mounted, "front"), "c1", "comp");
    // frame at x=100,y=0. x = 100 + 14 = 114; y = 0 + 14 + 2·44.45 = 14 + 88.9 = 102.9
    // width = panel width 482.6; height = 2U = 88.9
    expect(r).not.toBeNull();
    expect(r?.x).toBeCloseTo(114);
    expect(r?.y).toBeCloseTo(102.9);
    expect(r?.width).toBeCloseTo(RACK_WIDTH_MM);
    expect(r?.height).toBeCloseTo(88.9);
  });

  it("back: inherits the mirrored frame x, same slot y and panel width", () => {
    const backMounted = makeScene([rack("r1", "back")], {
      c1: {
        space: "s1",
        wall: "back",
        position: { x: 0, y: 0, z: 0 },
        rack: { id: "r1", uSlot: 2 },
        facing: "front",
      },
    });
    const r = deviceRect(layout(backMounted, "back"), "c1", "comp");
    // frame.x = 3389.4 → device x = 3389.4 + 14 = 3403.4; y unchanged at 102.9
    expect(r?.x).toBeCloseTo(3403.4);
    expect(r?.y).toBeCloseTo(102.9);
    expect(r?.width).toBeCloseTo(RACK_WIDTH_MM);
  });

  it("is null when the referenced rack is missing", () => {
    const orphan = makeScene([], {
      c1: {
        space: "s1",
        wall: "front",
        position: { x: 0, y: 0, z: 0 },
        rack: { id: "gone", uSlot: 0 },
        facing: "front",
      },
    });
    expect(deviceRect(layout(orphan, "front"), "c1", "comp")).toBeNull();
  });
});

describe("deviceRect — free-standing", () => {
  // A desktop unit against the right wall at (400, 50, 300).
  const free = makeScene([], {
    d1: { space: "s1", wall: "right", position: { x: 400, y: 50, z: 300 }, facing: "front" },
  });

  it("right-wall elevation: z mirrored, panel width (200) shows", () => {
    const r = deviceRect(layout(free, "right"), "d1", "synth");
    // right: oriented box (w=depth 150, h 100, d=width 200); x = room.depth - (pos.z + oriented.depth)
    //        = 3000 - (300 + 200) = 2500; width = oriented.depth = 200; height = 100; y = pos.y = 50
    expect(r).toEqual({ x: 2500, y: 50, width: 200, height: 100 });
  });

  it("top view: floor footprint the right way round for a side-wall unit", () => {
    const r = deviceRect(layout(free, "top"), "d1", "synth");
    // top of oriented(right) box: x = pos.x = 400, y = pos.z = 300, width = 150, height = depth 200
    expect(r).toEqual({ x: 400, y: 300, width: 150, height: 200 });
  });

  it("is null without a placement", () => {
    expect(deviceRect(layout(makeScene([], {}), "front"), "nope", "synth")).toBeNull();
  });
});

describe("deviceUnits", () => {
  it("returns the U-count for rackmount gear", () => {
    expect(deviceUnits(CATALOG, "comp")).toBe(2);
  });
  it("is 0 for desktop gear (never mounts)", () => {
    expect(deviceUnits(CATALOG, "synth")).toBe(0);
  });
  it("is 0 for an unknown type id", () => {
    expect(deviceUnits(CATALOG, "ghost")).toBe(0);
  });
});

describe("lookups", () => {
  const scene = makeScene([rack("r1", "front")], {
    d1: { space: "s1", wall: "front", position: { x: 0, y: 0, z: 0 }, facing: "front" },
  });
  it("deviceById / rackById resolve by id, isRack distinguishes them", () => {
    expect(deviceById(scene, "d1")?.id).toBe("d1");
    expect(rackById(scene, "r1")?.id).toBe("r1");
    expect(isRack(scene, "r1")).toBe(true);
    expect(isRack(scene, "d1")).toBe(false);
    expect(deviceById(scene, "r1")).toBeUndefined();
  });
});

describe("placedItemsFor", () => {
  // One front-wall rack; a device mounted in it (front); a free device on front facing back; a free
  // device on the back wall; a device in another space's placement (excluded everywhere here).
  const scene = makeScene([rack("r1", "front")], {
    c1: {
      space: "s1",
      wall: "front",
      position: { x: 0, y: 0, z: 0 },
      rack: { id: "r1", uSlot: 0 },
      facing: "front",
    },
    d1: { space: "s1", wall: "front", position: { x: 300, y: 0, z: 200 }, facing: "back" },
    d2: { space: "s1", wall: "back", position: { x: 900, y: 0, z: 200 }, facing: "front" },
  });

  it("elevation: filters to the current wall, racks behind (z 0), z by facing", () => {
    const items = placedItemsFor(layout(scene, "front"));
    const ids = items.map((i) => i.id);
    // rack r1 (front) + mounted c1 + free d1 (front). d2 is on the back wall → excluded.
    expect(ids.sort()).toEqual(["c1", "d1", "r1"]);
    expect(items.find((i) => i.id === "r1")).toMatchObject({ background: true, z: 0 });
    // d1 faces "back" ⇒ z 1 (behind the cable layer); c1 faces "front" ⇒ z 3.
    expect(items.find((i) => i.id === "d1")?.z).toBe(1);
    expect(items.find((i) => i.id === "c1")?.z).toBe(3);
  });

  it("elevation: switching wall shows only that wall's items", () => {
    const ids = placedItemsFor(layout(scene, "back")).map((i) => i.id);
    // Only d2 stands against the back wall; the front rack + its gear are gone.
    expect(ids).toEqual(["d2"]);
  });

  it("top: whole room's racks + free gear, mounted gear hidden", () => {
    const ids = placedItemsFor(layout(scene, "top")).map((i) => i.id);
    // rack r1 (background) + free d1, d2 (any wall). Mounted c1 lives inside its rack box → hidden.
    expect(ids.sort()).toEqual(["d1", "d2", "r1"]);
    const items = placedItemsFor(layout(scene, "top"));
    expect(items.find((i) => i.id === "r1")).toMatchObject({ background: true, z: 0 });
    expect(items.find((i) => i.id === "d1")?.z).toBe(3);
  });

  it("elevation: a flipped rack sends its mounted gear to the back z-order", () => {
    // Same scene but the rack is turned around: its mounted c1 now shows its back ⇒ z 1, not 3.
    const flipped = makeScene([{ ...rack("r1", "front"), facing: "back" }], {
      c1: {
        space: "s1",
        wall: "front",
        position: { x: 0, y: 0, z: 0 },
        rack: { id: "r1", uSlot: 0 },
        facing: "front",
      },
    });
    expect(placedItemsFor(layout(flipped, "front")).find((i) => i.id === "c1")?.z).toBe(1);
  });
});

describe("effectiveFacing", () => {
  it("a free-standing device follows its own facing", () => {
    const scene = makeScene([], {
      d1: { space: "s1", wall: "front", position: { x: 0, y: 0, z: 0 }, facing: "back" },
    });
    expect(effectiveFacing(scene, "d1")).toBe("back");
  });

  it("mounted gear follows the rack's facing, ignoring its own", () => {
    const scene = makeScene([{ ...rack("r1", "front"), facing: "back" }], {
      // its own facing is "front", but it's bolted into a rack turned to "back"
      c1: {
        space: "s1",
        wall: "front",
        position: { x: 0, y: 0, z: 0 },
        rack: { id: "r1", uSlot: 0 },
        facing: "front",
      },
    });
    expect(effectiveFacing(scene, "c1")).toBe("back");
  });

  it("defaults to front for an unknown device", () => {
    expect(effectiveFacing(makeScene([], {}), "nope")).toBe("front");
  });
});
