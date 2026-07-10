import { describe, expect, it } from "vitest";
import { matchesQuery, type Watchable, watchables } from "../src/bench-watch";
import type { DeviceDescriptor } from "../src/catalog";
import { isWatched, toggleWatch, watchKey } from "../src/scene-ops";
import type { BenchWatch, Scene } from "../src/scene-store";

// A device exposing one of each watchable lane: a param, a structural config, and a readout.
const IFACE: DeviceDescriptor = {
  typeId: "iface",
  name: "Interface",
  formFactor: { kind: "rackmount", rackUnits: 1 },
  params: [
    { id: 0, label: "Gain", unit: "dB", kind: "knob", min: 0, max: 60, default: 20 },
    { id: 1, label: "Monitor", unit: "", kind: "fader", min: 0, max: 1, default: 0.5 },
  ],
  ports: [],
  readouts: [{ id: 0, label: "Output", unit: "dBFS" }],
  configs: [{ key: "inst1", label: "Instrument 1", kind: "toggle", default: 0 }],
};

const scene = (): Scene => ({
  schemaVersion: 1,
  ui: { spaces: [], racks: [], placements: {}, portals: {} },
  patch: {
    devices: [{ id: "if", typeId: "iface" }],
    connections: [],
    output: { device: "if", port: 0 },
  },
});

describe("watchables", () => {
  it("flattens a device's params, configs, then readouts", () => {
    const list = watchables(scene(), [IFACE]);
    expect(list.map((w) => `${w.kind}:${w.id}`)).toEqual([
      "param:0",
      "param:1",
      "config:inst1",
      "readout:0",
    ]);
  });

  it("tags configs as recompile and carries label/unit + device name", () => {
    const list = watchables(scene(), [IFACE]);
    const config = list.find((w) => w.kind === "config") as Watchable;
    expect(config).toMatchObject({
      device: "if",
      deviceName: "Interface",
      label: "Instrument 1",
      recompile: true,
    });
    const param = list.find((w) => w.id === "0" && w.kind === "param") as Watchable;
    expect(param).toMatchObject({ unit: "dB", recompile: false });
  });

  it("skips devices whose type isn't in the catalog (a stale pin)", () => {
    expect(watchables(scene(), [])).toEqual([]);
  });
});

describe("matchesQuery", () => {
  const list = watchables(scene(), [IFACE]);
  const byLabel = (label: string) => list.find((w) => w.label === label) as Watchable;

  it("matches nothing for an empty/whitespace query (filter-to-pin, not a dump)", () => {
    expect(list.some((w) => matchesQuery(w, ""))).toBe(false);
    expect(list.some((w) => matchesQuery(w, "   "))).toBe(false);
  });

  it("matches case-insensitively across device name, label, kind, and id", () => {
    expect(matchesQuery(byLabel("Gain"), "gain")).toBe(true);
    expect(matchesQuery(byLabel("Gain"), "interface")).toBe(true); // device name
    expect(matchesQuery(byLabel("Output"), "readout")).toBe(true); // kind
    expect(matchesQuery(byLabel("Instrument 1"), "inst1")).toBe(true); // config key = id
    expect(matchesQuery(byLabel("Gain"), "monitor")).toBe(false);
  });
});

describe("watch-list pins (scene-ops)", () => {
  const item: BenchWatch = { device: "if", kind: "param", id: "0" };

  it("toggleWatch adds then removes; isWatched reflects it", () => {
    const s = scene();
    expect(isWatched(s, item)).toBe(false);
    toggleWatch(s, item);
    expect(isWatched(s, item)).toBe(true);
    expect(s.ui.benchWatch).toEqual([item]);
    toggleWatch(s, item);
    expect(isWatched(s, item)).toBe(false);
    expect(s.ui.benchWatch).toEqual([]); // list stays (empty), for a stable URL shape
  });

  it("distinguishes items by device, kind, and id via watchKey", () => {
    const s = scene();
    toggleWatch(s, { device: "if", kind: "param", id: "0" });
    // Same id, different kind — a distinct pin (no collision between param 0 and readout 0).
    expect(isWatched(s, { device: "if", kind: "readout", id: "0" })).toBe(false);
    toggleWatch(s, { device: "if", kind: "readout", id: "0" });
    expect(s.ui.benchWatch).toHaveLength(2);
    expect(watchKey({ device: "if", kind: "param", id: "0" })).not.toBe(
      watchKey({ device: "if", kind: "readout", id: "0" }),
    );
  });
});
