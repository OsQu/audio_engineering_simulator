import { describe, expect, it } from "vitest";
import type { DeviceDescriptor } from "../src/catalog";
import { canPlace, moveTo, moveToTop, rackSlotAt, wallSpawn } from "../src/placement";
import type { LayoutCtx, PlacedItem } from "../src/projection";
import type { Placement, Rack, Scene } from "../src/scene-store";
import { footprint, type Room, type Wall } from "../src/spatial";

const ROOM: Room = { width: 4000, depth: 3000, height: 1400 };

// A 2U rackmount box and a 200×100×150 desktop box.
const COMP: DeviceDescriptor = {
  typeId: "comp",
  name: "Compressor",
  formFactor: { kind: "rackmount", rackUnits: 2 },
  params: [],
  ports: [],
  readouts: [],
  configs: [],
};
const SYNTH: DeviceDescriptor = {
  typeId: "synth",
  name: "Synth",
  formFactor: { kind: "desktop", widthMm: 200, heightMm: 100, depthMm: 150 },
  params: [],
  ports: [],
  readouts: [],
  configs: [],
};
const CATALOG = [COMP, SYNTH];

function makeScene(racks: Rack[], placements: Record<string, Placement>): Scene {
  return {
    schemaVersion: 99,
    ui: { spaces: [{ id: "s1", name: "Studio", room: ROOM }], racks, placements, portals: {} },
    patch: {
      devices: Object.keys(placements).map((id) => ({
        id,
        typeId: id.startsWith("d") ? "synth" : "comp",
      })),
      connections: [],
      output: { device: "", port: 0 },
    },
  };
}

function ctxOf(scene: Scene, view: Wall | "top"): LayoutCtx {
  return {
    space: "s1",
    view,
    wall: view === "top" ? null : view,
    room: ROOM,
    scene,
    catalog: CATALOG,
  };
}

describe("rackSlotAt", () => {
  // Front-wall rack, U-slot region lower-left-front at (100, 0, 500), 8 slots.
  // rackRect(front) = { x:100, y:0, w:510.6, h:383.6 }; slotOy = 0 + FRAME_MARGIN(14) = 14.
  // Slot column spans x ∈ [100, 610.6], y ∈ [14, 14 + 8·44.45 = 369.6].
  const scene = makeScene(
    [
      {
        id: "r1",
        space: "s1",
        wall: "front",
        facing: "front",
        position: { x: 100, y: 0, z: 500 },
        slots: 8,
      },
    ],
    {},
  );
  const ctx = ctxOf(scene, "front");

  it("snaps to slot 0 for a hit just inside the bottom of the column", () => {
    // y = 20 → desired = floor((20-14)/44.45) = 0
    expect(rackSlotAt(ctx, "x", 300, 20, 2)).toEqual({ rackId: "r1", slot: 0 });
  });

  it("snaps to the slot the pointer is over higher up the column", () => {
    // y = 14 + 2·44.45 + 5 = 108 → desired = floor(94/44.45) = 2
    expect(rackSlotAt(ctx, "x", 300, 108, 2)).toEqual({ rackId: "r1", slot: 2 });
  });

  it("misses when x is left of the frame", () => {
    expect(rackSlotAt(ctx, "x", 50, 20, 2)).toBeNull();
  });

  it("misses when y is below the slot column (in the bottom margin)", () => {
    // y = 5 is below slotOy (14) — inside the drawn frame but not the U-slot region
    expect(rackSlotAt(ctx, "x", 300, 5, 2)).toBeNull();
  });

  it("only considers racks on the current wall", () => {
    const other = ctxOf(scene, "back");
    expect(rackSlotAt(other, "x", 300, 20, 2)).toBeNull();
  });
});

describe("canPlace", () => {
  const scene = makeScene([], {
    d1: { space: "s1", wall: "front", position: { x: 0, y: 0, z: 0 }, facing: "front" },
  });
  // A single other item on the front wall occupying x ∈ [250, 450], y ∈ [0, 100].
  const others: PlacedItem[] = [
    { id: "other", rect: { x: 250, y: 0, width: 200, height: 100 }, z: 3 },
  ];

  it("elevation: rejects a free-standing spot overlapping another item", () => {
    // candidate at (300,0) is 200×100 → overlaps [250,450]
    expect(canPlace(ctxOf(scene, "front"), others, "d1", 300, 0)).toBe(false);
  });

  it("elevation: allows a non-overlapping free-standing spot", () => {
    // candidate at (500,0) clears the other item (starts past its right edge 450)
    expect(canPlace(ctxOf(scene, "front"), others, "d1", 500, 0)).toBe(true);
  });

  it("top view is always legal (free floor layout)", () => {
    expect(canPlace(ctxOf(scene, "top"), others, "d1", 300, 0)).toBe(true);
  });

  it("a rack is always legal (repositions freely)", () => {
    const withRack = makeScene(
      [
        {
          id: "r1",
          space: "s1",
          wall: "front",
          facing: "front",
          position: { x: 0, y: 0, z: 0 },
          slots: 8,
        },
      ],
      {},
    );
    expect(canPlace(ctxOf(withRack, "front"), others, "r1", 300, 0)).toBe(true);
  });
});

describe("moveToTop — floor drag + wall re-tag", () => {
  it("re-tags a rack to the nearest wall and drags its mounted gear along", () => {
    const scene = makeScene(
      [
        {
          id: "r1",
          space: "s1",
          wall: "front",
          facing: "front",
          position: { x: 2000, y: 0, z: 1000 },
          slots: 8,
        },
      ],
      {
        c1: {
          space: "s1",
          wall: "front",
          position: { x: 0, y: 0, z: 0 },
          rack: { id: "r1", uSlot: 0 },
          facing: "front",
        },
      },
    );
    // Move the rack near x=50: frame 510.6 wide, 300 deep → centre (305.3, 1150). Nearest wall = left (dLeft 305).
    moveToTop(ctxOf(scene, "top"), "r1", 50, 1000);
    expect(scene.ui.racks[0].position).toEqual({ x: 50, y: 0, z: 1000 });
    expect(scene.ui.racks[0].wall).toBe("left");
    expect(scene.ui.placements.c1.wall).toBe("left"); // mounted gear followed the rack's wall
  });

  it("re-tags a free-standing device and clears any rack mount", () => {
    const scene = makeScene([], {
      d1: { space: "s1", wall: "front", position: { x: 1000, y: 0, z: 2000 }, facing: "front" },
    });
    // Move near z=50 (the back wall): synth centre (x+75, 50+100) → nearest wall = back (dBack 100).
    moveToTop(ctxOf(scene, "top"), "d1", 1000, 50);
    expect(scene.ui.placements.d1.position).toEqual({ x: 1000, y: 0, z: 50 });
    expect(scene.ui.placements.d1.wall).toBe("back");
    expect(scene.ui.placements.d1.rack).toBeUndefined();
  });
});

describe("moveTo — elevation drag mounts into a rack", () => {
  it("mounts a free device dragged over a rack slot, inheriting the rack's space + wall", () => {
    const scene = makeScene(
      [
        {
          id: "r1",
          space: "s1",
          wall: "front",
          facing: "front",
          position: { x: 100, y: 0, z: 500 },
          slots: 8,
        },
      ],
      { c1: { space: "s1", wall: "front", position: { x: 0, y: 0, z: 0 }, facing: "front" } },
    );
    // Drop 2U comp over slot 0 of the front rack (x=300, y=20).
    moveTo(ctxOf(scene, "front"), "c1", 300, 20);
    expect(scene.ui.placements.c1.rack).toEqual({ id: "r1", uSlot: 0 });
    expect(scene.ui.placements.c1.wall).toBe("front");
  });
});

describe("wallSpawn — flush position per wall", () => {
  const size = footprint(SYNTH.formFactor); // { width:200, height:100, depth:150 }
  const at = (wall: Wall | "top") => wallSpawn(ctxOf(makeScene([], {}), wall), size, 100);

  it("front: at elevX along x, flush at the far (z = depth − FLUSH) wall", () => {
    // seed z = 3000 − 400 = 2600; front is identity → position (elevX, 0, 2600)
    expect(at("front")).toEqual({ wall: "front", position: { x: 100, y: 0, z: 2600 } });
  });

  it("back: x mirrored about room width, flush at z = 0", () => {
    // x = 4000 − 200(width) − 100(elevX) = 3700
    expect(at("back")).toEqual({ wall: "back", position: { x: 3700, y: 0, z: 0 } });
  });

  it("right: flush at x = width − FLUSH, z mirrored about room depth", () => {
    // seed x = 4000 − 400 = 3600; z = 3000 − 200(oriented depth) − 100 = 2700
    expect(at("right")).toEqual({ wall: "right", position: { x: 3600, y: 0, z: 2700 } });
  });

  it("left: flush at x = 0, z = elevX", () => {
    expect(at("left")).toEqual({ wall: "left", position: { x: 0, y: 0, z: 100 } });
  });

  it("top view falls back to the front wall", () => {
    expect(at("top")).toEqual({ wall: "front", position: { x: 100, y: 0, z: 2600 } });
  });
});
