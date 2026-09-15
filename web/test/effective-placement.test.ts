import { describe, expect, it } from "vitest";
import { resolveEffectivePlacement } from "../src/effective-placement";
import { CATALOG, studioScene } from "./fixtures/resolver-contract";

describe("effective placement", () => {
  it("uses a free device's own durable placement", () => {
    expect(resolveEffectivePlacement(studioScene, CATALOG, "free-front")).toMatchObject({
      kind: "studio-free",
      space: "studio",
      wall: "front",
      facing: "front",
      position: { x: 300, y: 0, z: 400 },
    });
  });

  it("mounts from the rack, never its stale copied space, wall, or facing", () => {
    const scene = structuredClone(studioScene);
    const placement = scene.ui.placements["mounted-front"];
    placement.space = "booth";
    placement.wall = "back";
    placement.facing = "back";
    expect(resolveEffectivePlacement(scene, CATALOG, "mounted-front")).toMatchObject({
      kind: "studio-mounted",
      space: "studio",
      wall: "front",
      facing: "front",
      uSlot: 0,
    });
  });

  it("diagnoses missing parents and invalid U-slot spans without falling back to copied fields", () => {
    const missingRack = structuredClone(studioScene);
    missingRack.ui.placements["mounted-front"].rack = { id: "gone", uSlot: 0 };
    expect(resolveEffectivePlacement(missingRack, CATALOG, "mounted-front")).toEqual({
      kind: "unresolved",
      diagnostic: "missing-rack",
    });

    const invalidSlot = structuredClone(studioScene);
    invalidSlot.ui.placements["mounted-front"].rack = { id: "rack-front", uSlot: 8 };
    expect(resolveEffectivePlacement(invalidSlot, CATALOG, "mounted-front")).toEqual({
      kind: "unresolved",
      diagnostic: "invalid-rack-slot",
    });
  });

  it("resolves a room-less scene through its flat workbench placement", () => {
    const scene = structuredClone(studioScene);
    scene.ui.spaces = [];
    scene.ui.racks = [];
    scene.ui.placements = {};
    scene.ui.bench = { "free-front": { x: 123, y: 456, facing: "back" } };
    expect(resolveEffectivePlacement(scene, CATALOG, "free-front")).toMatchObject({
      kind: "workbench",
      x: 123,
      y: 456,
      facing: "back",
    });
  });
});
