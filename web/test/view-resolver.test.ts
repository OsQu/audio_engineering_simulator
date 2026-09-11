import { describe, expect, it } from "vitest";
import { bothInView, cableAnchor, jackKey, oneInView, otherEndLabel } from "../src/cable-view";
import { effectiveFacing, placedItemsFor } from "../src/projection";
import { benchCableLayout, benchVisibleFaces, sceneCableLayout } from "../src/view-resolver";
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

const identityApi = {
  worldToSurface: (x: number, y: number) => ({ x, y }),
  clientToSurface: (x: number, y: number) => ({ x, y }),
  measureRoot: () => null,
};

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

  it.each(socketCases)("$name socket is $expected", ({ view, device, face, expected }) => {
    const anchor = { x: 17, y: 23, face };
    const point = cableAnchor(
      sceneCableLayout(layoutFor(view)),
      { [jackKey(device, "output", 0)]: anchor },
      { device, port: 0 },
      "output",
      identityApi,
    );
    if (expected === "measured") expect(point).toEqual(anchor);
    else expect(point).not.toEqual(anchor);
  });

  it.each(connectionCases)("$name has the expected continuous/portal membership", ({
    connection,
    continuous,
    portal,
    otherEnd,
    label,
  }) => {
    const layout = sceneCableLayout(layoutFor("front"));
    expect(bothInView(layout, connection)).toBe(continuous);
    expect(oneInView(layout, connection)).toBe(portal);
    if (otherEnd) expect(otherEndLabel(studioScene, "studio", WALL_LABELS, otherEnd)).toBe(label);
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
