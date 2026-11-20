import { describe, expect, it } from "vitest";
import {
  defaultScene,
  parseScene,
  parseSpaces,
  SCHEMA_VERSION,
  serializeScene,
  studioSpaceList,
} from "../src/scene-store";
import studioSpaces from "../src/studio-spaces.json";

describe("scene store", () => {
  it("the default scene places every patch device in the first (control) room", () => {
    const scene = defaultScene();
    const ids = scene.patch.devices.map((d) => d.id);
    for (const id of ids) {
      const placement = scene.ui.placements[id];
      expect(placement).toBeDefined();
      expect(placement.space).toBe(scene.ui.spaces[0].id);
    }
    expect(scene.ui.spaces[0].id).toBe("control-room");
  });

  it("the default scene carries the authored studio spaces from the data file", () => {
    const scene = defaultScene();
    expect(scene.ui.spaces).toEqual(studioSpaces);
    expect(scene.ui.spaces.map((s) => s.name)).toEqual(["Control Room", "Live Room"]);
  });

  it("each scene gets its own copy of the authored spaces", () => {
    const a = defaultScene();
    a.ui.spaces[0].room.width = 1;
    a.ui.spaces[0].name = "edited";
    expect(defaultScene().ui.spaces).toEqual(studioSpaces);
    expect(studioSpaceList()).toEqual(studioSpaces);
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

  it("the default scene gives the computer one DAW track (monitors send 0 → master at unity)", () => {
    const scene = defaultScene();
    expect(scene.ui.tracks?.computer).toEqual([
      { input: 0, armed: false, monitoring: true, level: 1 },
    ]);
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

describe("authored spaces", () => {
  const room = { width: 4000, depth: 3000, height: 1400 };

  it("accepts a well-formed list", () => {
    const spaces = [{ id: "a", name: "A", room }];
    expect(parseSpaces(spaces)).toEqual(spaces);
  });

  it("rejects an empty or non-array list", () => {
    expect(() => parseSpaces([])).toThrow(/non-empty/);
    expect(() => parseSpaces({})).toThrow(/non-empty/);
  });

  it("rejects duplicate ids", () => {
    const dup = [
      { id: "a", name: "A", room },
      { id: "a", name: "B", room },
    ];
    expect(() => parseSpaces(dup)).toThrow(/duplicate id "a"/);
  });

  it("rejects a missing id or name", () => {
    expect(() => parseSpaces([{ name: "A", room }])).toThrow(/missing id/);
    expect(() => parseSpaces([{ id: "a", room }])).toThrow(/missing name/);
  });

  it("rejects missing or non-positive room dimensions", () => {
    expect(() => parseSpaces([{ id: "a", name: "A" }])).toThrow(/positive/);
    expect(() => parseSpaces([{ id: "a", name: "A", room: { ...room, depth: 0 } }])).toThrow(
      /positive/,
    );
    expect(() => parseSpaces([{ id: "a", name: "A", room: { ...room, height: "1" } }])).toThrow(
      /positive/,
    );
  });
});
