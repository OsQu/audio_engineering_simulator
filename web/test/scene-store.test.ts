import { describe, expect, it } from "vitest";
import { defaultScene, parseScene, SCHEMA_VERSION, serializeScene } from "../src/scene-store";

describe("scene store", () => {
  it("the default scene places every patch device in the default space", () => {
    const scene = defaultScene();
    const ids = scene.patch.devices.map((d) => d.id);
    for (const id of ids) {
      const placement = scene.ui.placements[id];
      expect(placement).toBeDefined();
      expect(placement.space).toBe(scene.ui.spaces[0].id);
    }
    expect(scene.ui.spaces.length).toBe(1);
  });

  it("the default scene is a rectangular room of front-wall desktop gear (the monitoring loop)", () => {
    const scene = defaultScene();
    const space = scene.ui.spaces[0];
    // A rectangular room, wider than deep → the left/right walls are the shorter sides.
    expect(space.room.width).toBeGreaterThan(space.room.depth);
    // The loop is all desktop gear (8i6 + computer + synth + controller + speaker) — no rack.
    expect(scene.ui.racks).toEqual([]);
    // Every device stands against the front wall (where the window to the live room is).
    for (const place of Object.values(scene.ui.placements)) {
      expect(place.wall).toBe("front");
    }
    // The interface, the computer, and the speaker are all present and placed.
    expect(scene.ui.placements.if.wall).toBe("front");
    expect(scene.ui.placements.computer.wall).toBe("front");
    expect(scene.ui.placements.spk.wall).toBe("front");
  });

  it("round-trips a scene through serialize/parse, placements included", () => {
    const scene = defaultScene();
    const back = parseScene(serializeScene(scene));
    expect(back).toEqual(scene);
  });

  it("discards a save at a different schema version (no migration)", () => {
    const scene = defaultScene();
    const stale = serializeScene({ ...scene, schemaVersion: SCHEMA_VERSION - 1 });
    expect(parseScene(stale)).toBeNull();
  });

  it("returns null on malformed JSON or a missing patch/ui", () => {
    expect(parseScene("not json")).toBeNull();
    expect(parseScene(JSON.stringify({ schemaVersion: SCHEMA_VERSION, ui: {} }))).toBeNull();
    expect(parseScene(JSON.stringify({ schemaVersion: SCHEMA_VERSION, patch: {} }))).toBeNull();
  });
});
