import { describe, expect, it } from "vitest";
import {
  canPlaceInRack,
  fitsInRack,
  footprint,
  project,
  RACK_UNIT_MM,
  RACK_WIDTH_MM,
  rackRunsOverlap,
  rectsOverlap,
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
});
