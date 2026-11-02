import { describe, expect, it } from "vitest";
import { jackKey, otherEndLabel, portalKey } from "../src/cable-view";
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

describe("otherEndLabel", () => {
  const scene = makeScene({ b: place("s1", "back"), c: place("s2", "front") });
  it("names the room when the other end is in a different space", () => {
    expect(otherEndLabel(scene, "s1", WALL_LABELS, "c")).toBe("Booth");
  });
  it("names the wall when the other end is a different wall of this room", () => {
    expect(otherEndLabel(scene, "s1", WALL_LABELS, "b")).toBe("Back");
  });
  it("is '?' for an unplaced device", () => {
    expect(otherEndLabel(scene, "s1", WALL_LABELS, "ghost")).toBe("?");
  });
});

describe("portalKey", () => {
  const conn: Connection = { from: { device: "a", port: 0 }, to: { device: "b", port: 1 } };
  it("keys per connection + end", () => {
    expect(portalKey(conn, true)).toBe("a:0->b:1|from");
    expect(portalKey(conn, false)).toBe("a:0->b:1|to");
  });
});

describe("jackKey", () => {
  it("names a socket alias device:direction:port", () => {
    expect(jackKey("amp", "output", 2)).toBe("amp:output:2");
  });
});
