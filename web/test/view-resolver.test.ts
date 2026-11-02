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
import { benchCableLayout, benchVisibleFaces, studioCableInput } from "../src/view-resolver";
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

  it("the workbench anchors either measured face without scene-only behavior", () => {
    expect(benchCableLayout.inView("anything")).toBe(true);
    expect(benchCableLayout.faceAnchorable("support-back", "front")).toBe(true);
    expect(benchCableLayout.faceAnchorable("support-back", "back")).toBe(true);
    expect(benchCableLayout.rect("support-back")).toBeNull();
    expect(benchCableLayout.clampsEstimate("support-back")).toBe(false);
    expect(benchCableLayout.frontPatchOver("support-back")).toBe(false);
  });
});
