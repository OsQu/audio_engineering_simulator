import { describe, expect, it } from "vitest";
import { beginFlip, createPresentationState } from "../src/presentation-state";
import { physicalLeadsSuppressed, resolveFaces, transitionOwner } from "../src/view-resolution";
import { benchScene, CATALOG, studioScene } from "./fixtures/resolver-contract";

describe("face presentation resolution", () => {
  it("emits one stable studio slot on the selected wall and no faces in top view", () => {
    const presentation = createPresentationState();
    const front = resolveFaces({
      kind: "studio",
      scene: studioScene,
      catalog: CATALOG,
      presentation,
      space: "studio",
      view: "front",
    });

    expect(front.map(({ id, objectId, face }) => ({ id, objectId, face }))).toEqual([
      { id: "studio:free-front", objectId: "free-front", face: "front" },
      { id: "studio:mounted-front", objectId: "mounted-front", face: "front" },
    ]);
    expect(
      resolveFaces({
        kind: "studio",
        scene: studioScene,
        catalog: CATALOG,
        presentation,
        space: "studio",
        view: "top",
      }),
    ).toEqual([]);
  });

  it("inherits a mounted device's transitioning face and lead suppression from its rack", () => {
    const started = beginFlip(createPresentationState(), "rack-front", "back");
    const faces = resolveFaces({
      kind: "studio",
      scene: studioScene,
      catalog: CATALOG,
      presentation: started.state,
      space: "studio",
      view: "front",
    });
    const mounted = faces.find((face) => face.objectId === "mounted-front");

    expect(mounted).toMatchObject({
      id: "studio:mounted-front",
      face: "front",
      state: "transitioning",
      interaction: "disabled",
      transition: { generation: 1, from: "front", to: "back" },
    });
    expect(physicalLeadsSuppressed(studioScene, started.state, "mounted-front")).toBe(true);
    expect(physicalLeadsSuppressed(studioScene, started.state, "free-front")).toBe(false);
    expect(transitionOwner(studioScene, "mounted-front")).toBe("rack-front");
    expect(transitionOwner(studioScene, "free-front")).toBe("free-front");
  });

  it("gives the DUT independent faces and a support device one stable slot across facing changes", () => {
    const presentation = createPresentationState();
    const before = resolveFaces({
      kind: "workbench",
      scene: benchScene,
      catalog: CATALOG,
      presentation,
      dutId: "dut",
    });
    const turned = structuredClone(benchScene);
    const support = turned.ui.bench?.["support-front"];
    if (!support) throw new Error("support fixture has no bench placement");
    support.facing = "back";
    const after = resolveFaces({
      kind: "workbench",
      scene: turned,
      catalog: CATALOG,
      presentation,
      dutId: "dut",
    });

    expect(before.filter((face) => face.objectId === "dut")).toMatchObject([
      { id: "workbench:dut-front:dut", face: "front" },
      { id: "workbench:dut-back:dut", face: "back" },
    ]);
    expect(before.find((face) => face.objectId === "support-front")).toMatchObject({
      id: "workbench:support:support-front",
      face: "front",
    });
    expect(after.find((face) => face.objectId === "support-front")).toMatchObject({
      id: "workbench:support:support-front",
      face: "back",
    });
  });

  it("isolates duplicate focus windows from each other and disables physical interaction", () => {
    const presentation = createPresentationState();
    const first = resolveFaces({
      kind: "focus",
      scene: studioScene,
      catalog: CATALOG,
      presentation,
      deviceId: "free-front",
      windowId: "window-a",
    });
    const second = resolveFaces({
      kind: "focus",
      scene: studioScene,
      catalog: CATALOG,
      presentation,
      deviceId: "free-front",
      windowId: "window-b",
    });

    expect(first[0]).toMatchObject({
      id: "focus:window-a:free-front",
      interaction: "disabled",
      state: "presented",
    });
    expect(second[0]?.id).toBe("focus:window-b:free-front");
    expect(second[0]?.id).not.toBe(first[0]?.id);
  });

  it("omits unresolved studio placements instead of producing fallback faces", () => {
    const scene = structuredClone(studioScene);
    scene.ui.placements["mounted-front"].rack = { id: "missing", uSlot: 0 };
    const faces = resolveFaces({
      kind: "studio",
      scene,
      catalog: CATALOG,
      presentation: createPresentationState(),
      space: "studio",
      view: "front",
    });
    expect(faces.some((face) => face.objectId === "mounted-front")).toBe(false);
  });
});
