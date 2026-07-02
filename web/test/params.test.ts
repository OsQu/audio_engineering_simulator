import { describe, expect, it, vi } from "vitest";
import type { DeviceDescriptor } from "../src/catalog";
import { key, paramValue, pushParams, seedParamValues } from "../src/params";
import type { Scene } from "../src/scene-store";

// A device with two params: gain (id 0, default -6) and tone (id 1, default 0).
const DESC: DeviceDescriptor = {
  typeId: "amp",
  name: "Amp",
  formFactor: { kind: "rackmount", rackUnits: 1 },
  params: [
    { id: 0, label: "Gain", unit: "dB", kind: "knob", min: -60, max: 12, default: -6 },
    { id: 1, label: "Tone", unit: "", kind: "knob", min: 0, max: 1, default: 0 },
  ],
  ports: [],
  readouts: [],
};
const CATALOG = [DESC];

// A scene whose device carries a *saved* value for param 0 (but not param 1).
const scene: Scene = {
  schemaVersion: 99,
  ui: { spaces: [], racks: [], placements: {}, portals: {} },
  patch: {
    devices: [{ id: "amp1", typeId: "amp", params: [{ id: 0, value: 3 }] }],
    connections: [],
    output: { device: "amp1", port: 0 },
  },
};

describe("key", () => {
  it("joins device + param id", () => {
    expect(key("amp1", 0)).toBe("amp1:0");
  });
});

describe("seedParamValues", () => {
  it("prefers a saved value over the descriptor default, else uses the default", () => {
    const values = seedParamValues(scene, CATALOG);
    expect(values).toEqual({ "amp1:0": 3, "amp1:1": 0 }); // 0 saved (3), 1 defaulted (0)
  });

  it("skips devices whose type isn't in the catalog", () => {
    const orphan: Scene = {
      ...scene,
      patch: { ...scene.patch, devices: [{ id: "x", typeId: "ghost" }] },
    };
    expect(seedParamValues(orphan, CATALOG)).toEqual({});
  });
});

describe("paramValue", () => {
  it("returns the live override when present", () => {
    expect(paramValue({ "amp1:0": 9 }, "amp1", DESC, 0)).toBe(9);
  });
  it("falls back to the descriptor default when unset", () => {
    expect(paramValue({}, "amp1", DESC, 0)).toBe(-6);
  });
  it("is 0 for an unknown param id", () => {
    expect(paramValue({}, "amp1", DESC, 99)).toBe(0);
  });
});

describe("pushParams", () => {
  it("emits exactly one param message per device param, using the current values", () => {
    const sent: Array<{ device: string; paramId: number; value: number }> = [];
    const sendFn = vi.fn((m: { type: string; device: string; paramId: number; value: number }) => {
      sent.push({ device: m.device, paramId: m.paramId, value: m.value });
    });
    pushParams(sendFn, scene, CATALOG, { "amp1:0": 3, "amp1:1": 0 });
    expect(sendFn).toHaveBeenCalledTimes(2); // two params, one message each
    expect(sent).toEqual([
      { device: "amp1", paramId: 0, value: 3 },
      { device: "amp1", paramId: 1, value: 0 },
    ]);
  });
});
