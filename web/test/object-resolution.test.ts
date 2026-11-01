// Story 10.5.1: resolved objects, semantic paint layers, and the total physical order.
import { describe, expect, it } from "vitest";
import type { PlacementPreview } from "../src/effective-placement";
import {
  beginFlip,
  createPresentationState,
  type PresentationState,
} from "../src/presentation-state";
import type { LayoutCtx } from "../src/projection";
import type { Wall } from "../src/spatial";
import {
  CHASSIS_RADIUS_MM,
  type ResolvedObject,
  resolveBenchObjects,
  resolveFaces,
  resolveStudioObjects,
  silhouetteOf,
} from "../src/view-resolution";
import { benchScene, CATALOG, layoutFor } from "./fixtures/resolver-contract";

function studio(
  ctx: LayoutCtx,
  presentation: PresentationState = createPresentationState(),
  preview?: PlacementPreview,
): ResolvedObject[] {
  const faces = resolveFaces({
    kind: "studio",
    scene: ctx.scene,
    catalog: ctx.catalog,
    presentation,
    space: ctx.space,
    view: ctx.view,
    preview,
  });
  return resolveStudioObjects({
    ctx,
    faces: new Map(faces.map((face) => [face.objectId, face])),
    preview,
  });
}

const summary = (objects: ResolvedObject[]) =>
  objects.map(({ id, layer, order, parentId }) => ({ id, layer, order, parentId }));

describe("studio paint order", () => {
  it.each<{ view: Wall | "top"; expected: ReturnType<typeof summary> }>([
    {
      // rack-front is furniture; free-front (scene index 0) precedes mounted-front (index 4) in front.
      view: "front",
      expected: [
        { id: "rack-front", layer: "furniture", order: 0, parentId: null },
        { id: "free-front", layer: "front-object", order: 1, parentId: null },
        { id: "mounted-front", layer: "front-object", order: 2, parentId: "rack-front" },
      ],
    },
    {
      // rack-back faces back, so its occupant shows its rear; free-back faces back on its own.
      view: "back",
      expected: [
        { id: "rack-back", layer: "furniture", order: 0, parentId: null },
        { id: "free-back", layer: "rear-object", order: 1, parentId: null },
        { id: "mounted-back", layer: "rear-object", order: 2, parentId: "rack-back" },
      ],
    },
    {
      view: "left",
      expected: [{ id: "free-left", layer: "front-object", order: 0, parentId: null }],
    },
    {
      // Top view presents no faces: rack footprints are furniture, free footprints above them (even
      // free-back), and mounted gear is absent.
      view: "top",
      expected: [
        { id: "rack-front", layer: "furniture", order: 0, parentId: null },
        { id: "rack-back", layer: "furniture", order: 1, parentId: null },
        { id: "free-front", layer: "front-object", order: 2, parentId: null },
        { id: "free-back", layer: "front-object", order: 3, parentId: null },
        { id: "free-left", layer: "front-object", order: 4, parentId: null },
        { id: "free-right", layer: "front-object", order: 5, parentId: null },
      ],
    },
  ])("$view view resolves layers, parentage, and total order", ({ view, expected }) => {
    expect(summary(studio(layoutFor(view)))).toEqual(expected);
  });

  it("keeps every rack below all of its occupants in both rack facings", () => {
    for (const facing of ["front", "back"] as const) {
      const ctx = layoutFor("front");
      const rack = ctx.scene.ui.racks.find((r) => r.id === "rack-front");
      if (!rack) throw new Error("fixture rack missing");
      rack.facing = facing;
      const objects = studio(ctx);
      const rackOrder = objects.find((o) => o.id === "rack-front")?.order ?? Infinity;
      const occupants = objects.filter((o) => o.parentId === "rack-front");
      expect(occupants.length).toBeGreaterThan(0);
      for (const occupant of occupants) expect(occupant.order).toBeGreaterThan(rackOrder);
      // The occupant inherits the rack's side: a turned rack sends it behind the cable layer.
      expect(occupants[0]?.layer).toBe(facing === "back" ? "rear-object" : "front-object");
    }
  });

  it("does not reorder objects under a rack, free, or mount preview", () => {
    const ctx = layoutFor("front");
    const committed = studio(ctx).map((o) => o.id);
    const previews: PlacementPreview[] = [
      {
        kind: "studio-rack",
        rackId: "rack-front",
        space: "studio",
        wall: "front",
        position: { x: 50, y: 0, z: 400 },
      },
      {
        kind: "studio-free",
        deviceId: "free-front",
        space: "studio",
        wall: "front",
        position: { x: 2000, y: 300, z: 400 },
      },
      { kind: "studio-mount", deviceId: "mounted-front", rackId: "rack-front", uSlot: 5 },
    ];
    for (const preview of previews) {
      const objects = studio(ctx, createPresentationState(), preview);
      expect(objects.map((o) => o.id)).toEqual(committed);
    }
    // The rack preview moves the assembly: the occupant's rect follows the frame.
    const moved = studio(ctx, createPresentationState(), previews[0]);
    const before = studio(ctx);
    const dx = (o: ResolvedObject[], id: string) => o.find((x) => x.id === id)?.rect?.x ?? NaN;
    expect(dx(moved, "mounted-front") - dx(before, "mounted-front")).toBeCloseTo(
      dx(moved, "rack-front") - dx(before, "rack-front"),
    );
  });

  it("takes a flipping device's layer from its settled face, not the durable facing", () => {
    const ctx = layoutFor("front");
    const placement = ctx.scene.ui.placements["free-front"];
    if (!placement) throw new Error("fixture placement missing");
    placement.facing = "back"; // a flip writes the durable facing at once
    const started = beginFlip(createPresentationState(), "free-front", "back");
    const mid = studio(ctx, started.state).find((o) => o.id === "free-front");
    const settled = studio(ctx).find((o) => o.id === "free-front");

    expect(mid?.layer).toBe("front-object");
    expect(settled?.layer).toBe("rear-object");
  });

  it("is deterministic for identical input", () => {
    expect(studio(layoutFor("front"))).toEqual(studio(layoutFor("front")));
  });

  it("emits opaque silhouettes from the resolved rect", () => {
    const [rack] = studio(layoutFor("front"));
    if (!rack) throw new Error("no objects");
    expect(silhouetteOf(rack)).toEqual({
      objectId: "rack-front",
      rect: rack.rect,
      radius: CHASSIS_RADIUS_MM,
    });
  });
});

describe("workbench paint order", () => {
  it("keeps the DUT in front and a support device on its settled side", () => {
    const faces = resolveFaces({
      kind: "workbench",
      scene: benchScene,
      catalog: CATALOG,
      presentation: createPresentationState(),
      dutId: "dut",
    });
    const objects = resolveBenchObjects({ scene: benchScene, dutId: "dut", faces });

    expect(summary(objects)).toEqual([
      { id: "support-back", layer: "rear-object", order: 0, parentId: null },
      { id: "dut", layer: "front-object", order: 1, parentId: null },
      { id: "support-front", layer: "front-object", order: 2, parentId: null },
    ]);
    expect(objects.every((o) => o.rect === null && silhouetteOf(o) === null)).toBe(true);
  });
});
