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
