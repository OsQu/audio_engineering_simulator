import { describe, expect, it } from "vitest";
import { resolveConnection, resolveEndpoint } from "../src/cable-resolution";
import { otherEndLabel } from "../src/cable-view";
import type { PlacementPreview } from "../src/effective-placement";
import { createPresentationState } from "../src/presentation-state";
import { effectiveFacing, placedItemsFor } from "../src/projection";
import type { DeviceFacing } from "../src/scene-store";
import type { GeometryEntry } from "../src/socket-geometry";
import type { Wall } from "../src/spatial";
import { resolveFaces, resolveStudioObjects } from "../src/view-resolution";
import { benchCableInput, benchVisibleFaces, studioCableInput } from "../src/view-resolver";
import {
  benchFaceCases,
  benchScene,
  connectionCases,
  faceCases,
  layoutFor,
  socketCases,
  studioScene,
  studioViewCases,
  WALL_LABELS,
} from "./fixtures/resolver-contract";

// The studio cable input for a wall view, with every presented face's sockets registered on `side`
// (or on its presented side when omitted): output 0 at (10, 10), input 0 at (90, 10) of a 100 × 50 face.
function studioInput(
  view: Wall,
  side?: (device: string) => DeviceFacing,
  preview?: PlacementPreview,
) {
  const ctx = layoutFor(view);
  const presentation = createPresentationState();
  const faces = new Map(
    resolveFaces({
      kind: "studio",
      scene: ctx.scene,
      catalog: ctx.catalog,
      presentation,
      space: ctx.space,
      view,
      preview,
    }).map((face) => [face.objectId, face]),
  );
  const entries: Record<string, GeometryEntry> = {};
  for (const face of faces.values()) {
    const on = side?.(face.objectId) ?? face.face;
    entries[`${face.id}|${on}`] = {
      status: "current",
      revision: "r",
      generation: 1,
      geometry: {
        width: 100,
        height: 50,
        sockets: [
          { id: "o", aliases: [`${face.objectId}:output:0`], x: 10, y: 10, r: 2 },
          { id: "i", aliases: [`${face.objectId}:input:0`], x: 90, y: 10, r: 2 },
        ],
      },
    };
  }
  return studioCableInput({
    ctx,
    faces,
    objects: resolveStudioObjects({ ctx, faces, preview }),
    presentation,
    preview,
    geometry: { entries, generation: 1 },
    worldToSurface: (x, y) => ({ x, y: ctx.room.height - y }),
  });
}

describe("view resolver contract matrix", () => {
  it.each(studioViewCases)("$view projects exactly its contract items", ({ view, itemIds }) => {
    expect(
      placedItemsFor(layoutFor(view))
        .map((item) => item.id)
        .sort(),
    ).toEqual([...itemIds].sort());
  });

  it.each(faceCases)("$id resolves to its effective $expected face", ({ id, expected }) => {
    expect(effectiveFacing(studioScene, id)).toBe(expected);
  });

  it.each(socketCases)("$name socket resolves as a $expected", ({
    view,
    device,
    face,
    expected,
  }) => {
    const input = studioInput(view, (id) => (id === device ? face : "front"));
    expect(resolveEndpoint(input, { device, port: 0 }, "output").kind).toBe(expected);
  });

  it.each(connectionCases)("$name has the expected continuous/portal membership", ({
    connection,
    continuous,
    portal,
    otherEnd,
    label,
  }) => {
    const resolved = resolveConnection(studioInput("front"), connection);
    expect(resolved.kind === "full").toBe(continuous);
    expect(resolved.kind === "portal").toBe(portal);
    if (otherEnd) expect(otherEndLabel(studioScene, "studio", WALL_LABELS, otherEnd)).toBe(label);
  });

  it("a placement preview moves the lead with the device (no committed-scene lag)", () => {
    const ref = { device: "free-front", port: 0 };
    const at = (preview?: PlacementPreview) => {
      const end = resolveEndpoint(studioInput("front", undefined, preview), ref, "output");
      if (end.kind !== "socket") throw new Error(end.kind);
      return end.point;
    };
    const committed = at();
    const position = studioScene.ui.placements["free-front"]?.position;
    if (!position) throw new Error("fixture");
    const moved = at({
      kind: "studio-free",
      deviceId: "free-front",
      space: "studio",
      wall: "front",
      position: { ...position, x: position.x + 300 },
    });
    // A front-wall elevation maps world x straight to surface x: +300 mm along the wall.
    expect(moved.x - committed.x).toBeCloseTo(300);
    expect(moved.y).toBeCloseTo(committed.y);
  });

  it.each(benchFaceCases)("bench $id shows $faces", ({ id, faces }) => {
    expect(benchVisibleFaces(benchScene, id, "dut")).toEqual(faces);
  });
});

describe("benchCableInput", () => {
  // A DUT presenting both faces and a supporting device turned front-on, whose only socket is on its
  // hidden back. Faces 100 × 50; the support frame sits at (0, 300), scale 1.
  const scene = structuredClone(benchScene);
  const geometry = (alias: string) => ({
    status: "current" as const,
    revision: "r",
    generation: 1,
    geometry: {
      width: 100,
      height: 50,
      sockets: [{ id: alias, aliases: [alias], x: 90, y: 40, r: 2 }],
    },
  });
  const supportId = scene.patch.devices.find((d) => d.id !== "dut")?.id ?? "";
  const input = benchCableInput({
    scene,
    dutId: "dut",
    objects: [
      { id: "dut", kind: "device", parentId: null, layer: "rear-object", order: 0, rect: null },
      {
        id: supportId,
        kind: "device",
        parentId: null,
        layer: "front-object",
        order: 1,
        rect: null,
      },
    ],
    slots: new Map([
      [
        "dut",
        [
          {
            slot: { face: "workbench:dut-front:dut", side: "front" },
            frame: { x: 0, y: 0, width: 100, height: 50 },
            presented: true,
          },
        ],
      ],
      [
        supportId,
        (["front", "back"] as const).map((side) => ({
          slot: { face: `workbench:support:${supportId}`, side },
          frame: { x: 0, y: 300, width: 100, height: 50 },
          presented: side === "front",
        })),
      ],
    ]),
    presentation: createPresentationState(),
    geometry: {
      entries: { [`workbench:support:${supportId}|back`]: geometry(`${supportId}:output:0`) },
      generation: 1,
    },
  });

  it("gives supporting devices a silhouette and omits the both-faced DUT", () => {
    expect([...input.silhouettes.keys()]).toEqual([supportId]);
    expect(input.membership(supportId)).toBe("local");
    expect(input.membership("ghost")).toBe("unresolved");
  });

  it("exits a supporting device's hidden-side socket instead of a mirrored attachment", () => {
    // Back socket at (90, 40) seen through → x = 100 − 90 = 10, y = 340. Distances: bottom 10, left 10,
    // top 40, right 90 → bottom (tie order bottom before left).
    const end = resolveEndpoint(input, { device: supportId, port: 0 }, "output");
    expect(end).toMatchObject({
      kind: "exit",
      edge: "bottom",
      side: "back",
      point: { x: 10, y: 350 },
    });
  });
});
