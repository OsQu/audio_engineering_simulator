import { describe, expect, it } from "vitest";
import {
  canPlaceInRack,
  fitsInRack,
  footprint,
  nearestFreeSlot,
  project,
  RACK_UNIT_MM,
  RACK_WIDTH_MM,
  type Room,
  rackRunsOverlap,
  rectsOverlap,
  snapToGrid,
  wallProjection,
} from "../src/spatial";

describe("footprint", () => {
  it("derives a rackmount box from the U-count + standard rack width", () => {
    const box = footprint({ kind: "rackmount", rackUnits: 2 });
    expect(box.width).toBe(RACK_WIDTH_MM);
    expect(box.height).toBeCloseTo(2 * RACK_UNIT_MM);
    expect(box.depth).toBeGreaterThan(0);
  });

  it("passes a desktop footprint through verbatim", () => {
    const box = footprint({ kind: "desktop", widthMm: 250, heightMm: 380, depthMm: 300 });
    expect(box).toEqual({ width: 250, height: 380, depth: 300 });
  });
});

describe("project", () => {
  const pos = { x: 10, y: 20, z: 30 };
  const size = { width: 4, height: 5, depth: 6 };

  it("front selects the (x,y) plane", () => {
    expect(project(pos, size, "front")).toEqual({ x: 10, y: 20, width: 4, height: 5 });
  });
  it("top selects the (x,z) plane", () => {
    expect(project(pos, size, "top")).toEqual({ x: 10, y: 30, width: 4, height: 6 });
  });
  it("side selects the (z,y) plane", () => {
    expect(project(pos, size, "side")).toEqual({ x: 30, y: 20, width: 6, height: 5 });
  });
});

describe("wallProjection", () => {
  // A room 4000 wide (x) × 3000 deep (z) × 1400 tall (y). Left/right (spanning depth 3000) are shorter
  // than front/back (spanning width 4000), as a rectangular room should be.
  const room: Room = { width: 4000, depth: 3000, height: 1400 };
  // A box at world (x=500, y=0, z=200), 480 wide × 90 tall × 300 deep.
  const pos = { x: 500, y: 0, z: 200 };
  const size = { width: 480, height: 90, depth: 300 };

  it("front is the identity projection (screen-x = world x), matching the pre-4.6 single view", () => {
    // horizontal = x = 500; width = 480; vertical = y = 0; height = 90.
    expect(wallProjection(pos, size, "front", room)).toEqual({
      x: 500,
      y: 0,
      width: 480,
      height: 90,
    });
  });

  it("back mirrors x about the room width (turning 180° flips left↔right)", () => {
    // mirror [500, 980] about 4000 → [4000-980, 4000-500] = [3020, 3520]; x = 3020, width = 480.
    expect(wallProjection(pos, size, "back", room)).toEqual({
      x: 3020,
      y: 0,
      width: 480,
      height: 90,
    });
  });

  it("left runs along the depth axis (screen-x = world z), so it uses the box depth as its width", () => {
    // horizontal = z = 200; width = depth = 300; vertical/height unchanged.
    expect(wallProjection(pos, size, "left", room)).toEqual({
      x: 200,
      y: 0,
      width: 300,
      height: 90,
    });
  });

  it("right runs along depth and mirrors z about the room depth", () => {
    // mirror [200, 500] about 3000 → [3000-500, 3000-200] = [2500, 2800]; x = 2500, width = 300.
    expect(wallProjection(pos, size, "right", room)).toEqual({
      x: 2500,
      y: 0,
      width: 300,
      height: 90,
    });
  });

  it("keeps a box wholly inside the wall span when the box is wholly inside the room", () => {
    // A device flush in the far corner projects flush against the mirrored wall's far edge.
    const corner = { x: 3520, y: 0, z: 2700 }; // x+width=4000, z+depth=3000 (flush to both far walls)
    // back: mirror [3520,4000] about 4000 → [0,480]; right: mirror [2700,3000] about 3000 → [0,300].
    expect(wallProjection(corner, size, "back", room).x).toBe(0);
    expect(wallProjection(corner, size, "right", room).x).toBe(0);
  });
});

describe("snapToGrid", () => {
  it("snaps to the nearest multiple, rounding at the half-step boundary", () => {
    expect(snapToGrid(63, 50)).toBe(50); // 63/50 = 1.26 → round 1 → 50
    expect(snapToGrid(80, 50)).toBe(100); // 80/50 = 1.6 → round 2 → 100
    expect(snapToGrid(75, 50)).toBe(100); // exact half rounds up (Math.round)
    expect(snapToGrid(-63, 50)).toBe(-50); // -1.26 → round -1 → -50
  });
  it("leaves exact multiples untouched", () => {
    expect(snapToGrid(150, 50)).toBe(150);
    expect(snapToGrid(0, 50)).toBe(0);
  });
  it("is a no-op for a non-positive step (snapping disabled)", () => {
    expect(snapToGrid(63, 0)).toBe(63);
    expect(snapToGrid(63, -50)).toBe(63);
  });
});

describe("rectsOverlap", () => {
  const a = { x: 0, y: 0, width: 10, height: 10 };
  it("detects an overlap", () => {
    expect(rectsOverlap(a, { x: 5, y: 5, width: 10, height: 10 })).toBe(true);
  });
  it("treats touching edges as non-overlapping", () => {
    expect(rectsOverlap(a, { x: 10, y: 0, width: 5, height: 10 })).toBe(false);
  });
  it("detects a clear separation", () => {
    expect(rectsOverlap(a, { x: 20, y: 20, width: 5, height: 5 })).toBe(false);
  });
});

describe("rack U-slot legality", () => {
  const rack = { slots: 8 };

  it("a device within bounds fits", () => {
    expect(fitsInRack(rack, { startSlot: 0, rackUnits: 2 })).toBe(true);
    expect(fitsInRack(rack, { startSlot: 6, rackUnits: 2 })).toBe(true);
  });
  it("rejects out-of-bounds or zero-height runs", () => {
    expect(fitsInRack(rack, { startSlot: 7, rackUnits: 2 })).toBe(false); // 7 + 2 > 8
    expect(fitsInRack(rack, { startSlot: -1, rackUnits: 1 })).toBe(false);
    expect(fitsInRack(rack, { startSlot: 0, rackUnits: 0 })).toBe(false);
  });
  it("detects overlapping U-runs", () => {
    expect(rackRunsOverlap({ startSlot: 0, rackUnits: 2 }, { startSlot: 1, rackUnits: 2 })).toBe(
      true,
    );
    expect(rackRunsOverlap({ startSlot: 0, rackUnits: 2 }, { startSlot: 2, rackUnits: 1 })).toBe(
      false,
    );
  });
  it("canPlaceInRack rejects collisions and out-of-bounds, accepts a free run", () => {
    const existing = [{ startSlot: 0, rackUnits: 2 }];
    expect(canPlaceInRack(rack, { startSlot: 1, rackUnits: 1 }, existing)).toBe(false); // collides
    expect(canPlaceInRack(rack, { startSlot: 2, rackUnits: 2 }, existing)).toBe(true); // free
    expect(canPlaceInRack(rack, { startSlot: 7, rackUnits: 2 }, existing)).toBe(false); // out of bounds
  });

  it("nearestFreeSlot returns the desired slot when free, else the closest free one", () => {
    const existing = [{ startSlot: 2, rackUnits: 2 }]; // occupies slots 2,3
    expect(nearestFreeSlot(rack, existing, 1, 0)).toBe(0); // desired free
    expect(nearestFreeSlot(rack, existing, 1, 2)).toBe(1); // desired taken → nearest free is 1
    expect(nearestFreeSlot(rack, existing, 1, 3)).toBe(4); // desired taken → 4 is the closest free
  });

  it("nearestFreeSlot returns null when nothing fits", () => {
    const full = [{ startSlot: 0, rackUnits: 8 }]; // whole 8U rack taken
    expect(nearestFreeSlot(rack, full, 1, 0)).toBeNull();
    expect(nearestFreeSlot(rack, [], 9, 0)).toBeNull(); // taller than the rack
  });
});
