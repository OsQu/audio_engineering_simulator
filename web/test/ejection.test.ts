// Story 10.5.4 — approved ejection policy U3: nearest legal free lattice position, atomic, preserving the
// rack's effective room/wall/facing. Every expected position is a hand calc (FRAME_MARGIN 14 mm,
// 1U = 44.45 mm, 19" = 482.6 mm, frame 510.6 mm wide, GRID_MM 50).
import { describe, expect, it } from "vitest";
import type { DeviceDescriptor } from "../src/catalog";
import { eject, planEjection } from "../src/ejection";
import type { Placement, Rack, Scene } from "../src/scene-store";
import type { Room, Vec3, Wall } from "../src/spatial";

const ROOM: Room = { width: 4000, depth: 3000, height: 1400 };
const unit = (typeId: string, rackUnits: number): DeviceDescriptor => ({
  typeId,
  name: typeId,
  formFactor: { kind: "rackmount", rackUnits },
  params: [],
  ports: [],
  readouts: [],
  configs: [],
});
const DESKTOP: DeviceDescriptor = {
  ...unit("desk", 1),
  formFactor: { kind: "desktop", widthMm: 300, heightMm: 100, depthMm: 200 },
};
const CATALOG = [unit("u1", 1), unit("u2", 2), DESKTOP];

const rack = (id: string, position: Vec3, extra: Partial<Rack> = {}): Rack => ({
  id,
  space: "s1",
  wall: "front",
  facing: "front",
  position,
  slots: 8,
  ...extra,
});
const mounted = (rackId: string, uSlot: number, wall: Wall = "front"): Placement => ({
  space: "s1",
  wall,
  facing: "front",
  position: { x: 3333, y: 0, z: 3333 }, // a stale stored copy: must never be restored
  rack: { id: rackId, uSlot },
});

function scene(racks: Rack[], devices: [string, string, Placement][], room: Room = ROOM): Scene {
  return {
    schemaVersion: 99,
    ui: {
      spaces: [{ id: "s1", name: "Studio", room }],
      racks,
      placements: Object.fromEntries(devices.map(([id, , p]) => [id, p])),
      portals: {},
    },
    patch: {
      devices: devices.map(([id, typeId]) => ({ id, typeId })),
      connections: [],
      output: { device: "", port: 0 },
    },
  };
}

describe("U3 ejection planner", () => {
  it("removes a rack by allocating its occupants in slot order without overlapping each other", () => {
    // Rack at (900, 0): d2 (2U, slot 0) origin (914, 14) → nearest (900, 0) [dx −14 beats +36; dy −14
    // beats +36]; it then occupies [900, 1382.6] × [0, 88.9]. d1 (1U, slot 2) origin (914, 102.9) →
    // (900, 100) clears d2's top at 88.9. Listed d1 first in the scene to prove slot order, not scene order.
    const s = scene(
      [rack("r1", { x: 900, y: 0, z: 400 }, { facing: "back" })],
      [
        ["d1", "u1", mounted("r1", 2)],
        ["d2", "u2", mounted("r1", 0)],
      ],
    );
    const plan = planEjection(s, CATALOG, { kind: "rack", rackId: "r1" });

    expect(plan).toEqual({
      ok: true,
      placements: {
        d2: { space: "s1", wall: "front", facing: "back", position: { x: 900, y: 0, z: 400 } },
        d1: { space: "s1", wall: "front", facing: "back", position: { x: 900, y: 100, z: 400 } },
      },
    });
  });

  it("breaks equal distances by smaller y, then smaller x", () => {
    // Rack at (911, 11): origin (925, 25) is equidistant (25² + 25²) from (900|950, 0|50) ⇒ (900, 0).
    const s = scene([rack("r1", { x: 911, y: 11, z: 400 })], [["d1", "u1", mounted("r1", 0)]]);
    const plan = planEjection(s, CATALOG, { kind: "rack", rackId: "r1" });
    expect(plan.ok && plan.placements.d1?.position).toEqual({ x: 900, y: 0, z: 400 });
  });

  it("treats other free gear and other racks' occupants as obstacles", () => {
    // Removing r1 (at 900): the 1U origin (914, 14) wants (900, 0), but a free desktop occupies
    // [900, 1200] × [0, 100]. Next nearest clear: (900, 100) — d² = 14² + 86² = 7 592 — vs (1200, 0),
    // d² = 286² + 14² = 81 992 ⇒ (900, 100). A second rack r2 at x=2000 with a unit stays put.
    const s = scene(
      [rack("r1", { x: 900, y: 0, z: 400 }), rack("r2", { x: 2000, y: 0, z: 400 })],
      [
        ["d1", "u1", mounted("r1", 0)],
        [
          "desk",
          "desk",
          { space: "s1", wall: "front", facing: "front", position: { x: 900, y: 0, z: 400 } },
        ],
        ["other", "u1", mounted("r2", 0)],
      ],
    );
    const plan = planEjection(s, CATALOG, { kind: "rack", rackId: "r1" });
    expect(plan.ok && plan.placements).toEqual({
      d1: { space: "s1", wall: "front", facing: "front", position: { x: 900, y: 100, z: 400 } },
    });
  });

  it("preserves a side wall and maps the elevation back to world axes", () => {
    // Left wall: elevation x = world z. Rack at z = 900 ⇒ frame [900, 1410.6] × [0, 383.6] (retained,
    // so it blocks); origin (914, 14). Above it (900, 400): d² = 14² + 386² = 149 192 beats left of it
    // (400, 0): d² = 514² + 14² = 264 392. World position: {x: rack x 400, y: 400, z: elevation x 900}.
    const s = scene(
      [rack("r1", { x: 400, y: 0, z: 900 }, { wall: "left" })],
      [["d1", "u1", mounted("r1", 0, "left")]],
    );
    const plan = planEjection(s, CATALOG, { kind: "device", deviceId: "d1" });
    expect(plan.ok && plan.placements.d1).toEqual({
      space: "s1",
      wall: "left",
      facing: "front",
      position: { x: 400, y: 400, z: 900 },
    });
  });

  it("fails without touching the scene when no legal spot exists in the room", () => {
    // A 600 × 400 wall: the retained 510.6 × 383.6 frame leaves 89.4 mm beside it (< 482.6) and nothing
    // above it (383.6 + 44.45 > 400).
    const small: Room = { width: 600, depth: 600, height: 400 };
    const s = scene([rack("r1", { x: 0, y: 0, z: 0 })], [["d1", "u1", mounted("r1", 0)]], small);
    const before = structuredClone(s);

    expect(eject(s, CATALOG, { kind: "device", deviceId: "d1" })).toEqual({
      ok: false,
      reason: "no-free-space",
      deviceId: "d1",
    });
    expect(s).toEqual(before);
  });

  it("leaves the rack and every occupant in place if any occupant cannot be placed", () => {
    // 2U then 1U in a 520 × 120 wall with the frame removed: the 2U unit fits at (0, 0) (88.9 tall),
    // but the 1U unit then needs 44.45 mm above 88.9 — 133.35 > 120 — and 520 − 482.6 is too narrow.
    const tight: Room = { width: 520, depth: 520, height: 120 };
    const s = scene(
      [rack("r1", { x: 0, y: 0, z: 0 })],
      [
        ["d2", "u2", mounted("r1", 0)],
        ["d1", "u1", mounted("r1", 2)],
      ],
      tight,
    );
    const before = structuredClone(s);
    expect(eject(s, CATALOG, { kind: "rack", rackId: "r1" })).toMatchObject({
      ok: false,
      reason: "no-free-space",
      deviceId: "d1",
    });
    expect(s).toEqual(before);
  });

  it("removes an empty rack, and applies a successful plan with the rack gone after its children", () => {
    const empty = scene([rack("r1", { x: 0, y: 0, z: 0 })], []);
    expect(eject(empty, CATALOG, { kind: "rack", rackId: "r1" })).toEqual({
      ok: true,
      placements: {},
    });
    expect(empty.ui.racks).toEqual([]);

    const s = scene([rack("r1", { x: 900, y: 0, z: 400 })], [["d1", "u1", mounted("r1", 0)]]);
    expect(eject(s, CATALOG, { kind: "rack", rackId: "r1" }).ok).toBe(true);
    expect(s.ui.racks).toEqual([]);
    expect(s.ui.placements.d1?.rack).toBeUndefined();
  });

  it("reports a free device or a missing rack without writing", () => {
    const s = scene(
      [],
      [
        [
          "desk",
          "desk",
          { space: "s1", wall: "front", facing: "front", position: { x: 0, y: 0, z: 0 } },
        ],
      ],
    );
    const before = structuredClone(s);
    expect(eject(s, CATALOG, { kind: "device", deviceId: "desk" })).toMatchObject({
      reason: "not-mounted",
    });
    expect(eject(s, CATALOG, { kind: "rack", rackId: "nope" })).toMatchObject({
      reason: "missing-rack",
    });
    expect(s).toEqual(before);
  });
});
