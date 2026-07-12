import { describe, expect, it } from "vitest";
import type { DeviceDescriptor } from "../src/catalog";
import type { Endpoint } from "../src/connections";
import {
  cancel,
  endpointFromJackKey,
  type JackHit,
  jackKeyOf,
  type PatchState,
  pointerDown,
  pointerMove,
  pointerUp,
} from "../src/patching";
import type { Connection } from "../src/scene";
import type { Scene } from "../src/scene-store";

// Two devices: a source with an analog line output (port 0) and a sink with an analog line input.
const SRC: DeviceDescriptor = {
  typeId: "src",
  name: "Source",
  formFactor: { kind: "desktop", widthMm: 100, heightMm: 100, depthMm: 100 },
  params: [],
  ports: [
    {
      id: 0,
      label: "out",
      direction: "output",
      domain: "analog",
      channels: 1,
      kind: "line",
      connector: "quarterInch",
      delayed: false,
    },
  ],
  readouts: [],
  configs: [],
};
const SINK: DeviceDescriptor = {
  typeId: "sink",
  name: "Sink",
  formFactor: { kind: "desktop", widthMm: 100, heightMm: 100, depthMm: 100 },
  params: [],
  ports: [
    {
      id: 0,
      label: "in",
      direction: "input",
      domain: "analog",
      channels: 1,
      kind: "line",
      connector: "quarterInch",
      delayed: false,
    },
  ],
  readouts: [],
  configs: [],
};
const CATALOG = [SRC, SINK];

const scene: Scene = {
  schemaVersion: 99,
  ui: { spaces: [], racks: [], placements: {}, portals: {} },
  patch: {
    devices: [
      { id: "s1", typeId: "src" },
      { id: "d1", typeId: "sink" },
    ],
    connections: [],
    output: { device: "d1", port: 0 },
  },
};

const OUT: Endpoint = {
  device: "s1",
  port: 0,
  direction: "output",
  domain: "analog",
  channels: 1,
  connector: "quarterInch",
};
const IN: Endpoint = {
  device: "d1",
  port: 0,
  direction: "input",
  domain: "analog",
  channels: 1,
  connector: "quarterInch",
};
const srcHit: JackHit = { key: jackKeyOf(OUT), endpoint: OUT, anchor: { x: 10, y: 10 } };
const sinkHit: JackHit = { key: jackKeyOf(IN), endpoint: IN, anchor: { x: 90, y: 90 } };
const deps = { connections: [] as Connection[] };

// A drag started from the source jack — the common precondition for the move/up tests.
const started = (): PatchState => pointerDown(null, srcHit).state;

describe("endpointFromJackKey / jackKeyOf", () => {
  it("round-trips a valid jack key to an Endpoint with its domain", () => {
    expect(endpointFromJackKey(scene, CATALOG, "s1:output:0")).toEqual(OUT);
    expect(jackKeyOf(OUT)).toBe("s1:output:0");
  });
  it("rejects a malformed direction or an unknown port", () => {
    expect(endpointFromJackKey(scene, CATALOG, "s1:sideways:0")).toBeNull();
    expect(endpointFromJackKey(scene, CATALOG, "s1:output:9")).toBeNull();
    expect(endpointFromJackKey(scene, CATALOG, "ghost:output:0")).toBeNull();
  });
});

describe("pointerDown", () => {
  it("starts a drag from a jack with a measured anchor", () => {
    const st = pointerDown(null, srcHit).state;
    expect(st).toMatchObject({
      source: OUT,
      srcPoint: { x: 10, y: 10 },
      free: { x: 10, y: 10 },
      mode: "drag",
    });
  });
  it("does not start without an anchor", () => {
    expect(pointerDown(null, { ...srcHit, anchor: null }).state).toBeNull();
    expect(pointerDown(null, null).state).toBeNull();
  });
  it("leaves a pending cable untouched (its second press resolves on up)", () => {
    const pending: PatchState = { ...started(), mode: "pending" } as PatchState;
    expect(pointerDown(pending, sinkHit).state).toBe(pending);
  });
});

describe("pointerMove", () => {
  it("follows the cursor over empty space (not over a jack)", () => {
    const st = pointerMove(started(), null, { x: 50, y: 50 }, null, deps);
    expect(st).toMatchObject({ free: { x: 50, y: 50 }, over: false, legal: false, verdict: null });
  });
  it("snaps to a legal target jack and marks it legal", () => {
    const st = pointerMove(started(), sinkHit, { x: 50, y: 50 }, null, deps);
    expect(st?.over).toBe(true);
    expect(st?.legal).toBe(true);
    expect(st?.free).toEqual({ x: 90, y: 90 }); // snapped to the jack anchor, not the cursor
  });
  it("ignores a hover over the source's own jack", () => {
    const st = pointerMove(started(), srcHit, { x: 50, y: 50 }, null, deps);
    expect(st).toMatchObject({ over: false, free: { x: 50, y: 50 } });
  });
  it("uses the live source anchor when the source is back in view", () => {
    const st = pointerMove(started(), null, { x: 50, y: 50 }, { x: 12, y: 13 }, deps);
    expect(st?.srcPoint).toEqual({ x: 12, y: 13 });
  });
  it("keeps the pick-time source point when the source is off-view (null anchor)", () => {
    const st = pointerMove(started(), null, { x: 50, y: 50 }, null, deps);
    expect(st?.srcPoint).toEqual({ x: 10, y: 10 });
  });
});

describe("pointerUp — drag mode", () => {
  it("commits when released over a legal jack", () => {
    const over = pointerMove(started(), sinkHit, { x: 50, y: 50 }, null, deps);
    const res = pointerUp(over, null, false, deps); // moved (a real drag)
    expect(res.state).toBeNull();
    expect(res.commit?.ok).toBe(true);
  });
  it("promotes a click (no move) to a pending pick", () => {
    const res = pointerUp(started(), null, true, deps);
    expect(res.commit).toBeUndefined();
    expect(res.state).toMatchObject({ source: OUT, mode: "pending", over: false, verdict: null });
  });
  it("cancels a real drag released over nothing", () => {
    const moved = pointerMove(started(), null, { x: 50, y: 50 }, null, deps);
    expect(pointerUp(moved, null, false, deps).state).toBeNull();
  });
});

describe("pointerUp — pending mode", () => {
  const pending = (): PatchState => pointerUp(started(), null, true, deps).state; // click → pending

  it("completes a cross-view patch on a click onto a legal jack", () => {
    const res = pointerUp(pending(), sinkHit, true, deps);
    expect(res.state).toBeNull();
    expect(res.commit?.ok).toBe(true);
  });
  it("keeps the pending pick through a pan (press-and-drag)", () => {
    const p = pending();
    expect(pointerUp(p, null, false, deps).state).toBe(p); // unchanged
  });
  it("keeps the pick through a non-jack click (the view/space switcher — how cross-view completes)", () => {
    const p = pending();
    const res = pointerUp(p, null, true, deps); // click on empty space / a switcher button
    expect(res.state).toBe(p); // survives the switch
    expect(res.commit).toBeUndefined();
  });
  it("cancels on a click back on the source jack", () => {
    expect(pointerUp(pending(), srcHit, true, deps)).toEqual({ state: null });
  });
  it("keeps the pick (no commit) on a click over an illegal jack", () => {
    // Another device's output jack — output→output is illegal, so the pick stays for another try.
    const otherOut: Endpoint = {
      device: "s2",
      port: 0,
      direction: "output",
      domain: "analog",
      channels: 1,
      connector: "quarterInch",
    };
    const outHit: JackHit = {
      key: jackKeyOf(otherOut),
      endpoint: otherOut,
      anchor: { x: 5, y: 5 },
    };
    const p = pending();
    const res = pointerUp(p, outHit, true, deps);
    expect(res.state).toBe(p);
    expect(res.commit).toBeUndefined();
  });
});

describe("cancel", () => {
  it("drops any in-progress patch", () => {
    expect(cancel()).toBeNull();
  });
});
