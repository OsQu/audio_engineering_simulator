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

  it("the default scene is a rectangular room with wall-tagged gear (rack on the back wall)", () => {
    const scene = defaultScene();
    const space = scene.ui.spaces[0];
    // A rectangular room, wider than deep → the left/right walls are the shorter sides.
    expect(space.room.width).toBeGreaterThan(space.room.depth);
    const rack = scene.ui.racks[0];
    expect(rack.wall).toBe("back");
    expect(rack.facing).toBe("front"); // the rack starts front-out; turn it around for the rear I/O
    // Mounted gear inherits its rack's wall (as it inherits the rack's space).
    for (const place of Object.values(scene.ui.placements)) {
      if (place.rack?.id === rack.id) expect(place.wall).toBe(rack.wall);
    }
    // The synth + speaker stand against the front wall (where the window to the live room is).
    expect(scene.ui.placements.synth.wall).toBe("front");
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
