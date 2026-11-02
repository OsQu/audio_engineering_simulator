import { describe, expect, it } from "vitest";
import type { DeviceDescriptor } from "../src/catalog";
import type { Endpoint } from "../src/connections";
import {
  aliasFor,
  cancel,
  endpointFromJackKey,
  type JackHit,
  jackKeyOf,
  type PatchState,
  pointerCancel,
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
    const st = pointerMove(started(), null, { x: 50, y: 50 }, deps);
    expect(st).toMatchObject({ free: { x: 50, y: 50 }, over: false, legal: false, verdict: null });
  });
  it("snaps to a legal target jack and marks it legal", () => {
    const st = pointerMove(started(), sinkHit, { x: 50, y: 50 }, deps);
    expect(st?.over).toBe(true);
    expect(st?.legal).toBe(true);
    expect(st?.free).toEqual({ x: 90, y: 90 }); // snapped to the jack anchor, not the cursor
  });
  it("ignores a hover over the source's own jack", () => {
    const st = pointerMove(started(), srcHit, { x: 50, y: 50 }, deps);
    expect(st).toMatchObject({ over: false, free: { x: 50, y: 50 } });
  });
  it("never caches a source anchor (the held lead re-resolves its logical source)", () => {
    const st = pointerMove(started(), null, { x: 50, y: 50 }, deps);
    expect(st && "srcPoint" in st).toBe(false);
    expect(st?.source).toEqual(OUT);
  });
});

const otherOut: Endpoint = {
  device: "s2",
  port: 0,
  direction: "output",
  domain: "analog",
  channels: 1,
  connector: "quarterInch",
};
// Another device's output jack — output→output is illegal.
const outHit: JackHit = { key: jackKeyOf(otherOut), endpoint: otherOut, anchor: { x: 5, y: 5 } };

describe("pointerUp — drag mode", () => {
  it("commits when released over a legal jack", () => {
    const over = pointerMove(started(), sinkHit, { x: 50, y: 50 }, deps);
    const res = pointerUp(over, sinkHit, false, deps); // moved (a real drag)
    expect(res.state).toBeNull();
    expect(res.commit?.ok).toBe(true);
  });
  it("resolves the release target, not the last move's verdict (no final move needed)", () => {
    // Last move was over empty space; the release lands on the sink → commit.
    const moved = pointerMove(started(), null, { x: 50, y: 50 }, deps);
    expect(pointerUp(moved, sinkHit, false, deps).commit?.ok).toBe(true);
    // Last move was over the legal sink; the release lands on nothing → no stale commit.
    const stale = pointerMove(started(), sinkHit, { x: 50, y: 50 }, deps);
    const res = pointerUp(stale, null, false, deps);
    expect(res.commit).toBeUndefined();
    expect(res.state).toBeNull();
  });
  it("promotes a click (no move) to a pending pick", () => {
    const res = pointerUp(started(), null, true, deps);
    expect(res.commit).toBeUndefined();
    expect(res.state).toMatchObject({ source: OUT, mode: "pending", over: false, verdict: null });
  });
  it("promotes a click on the source itself to a pending pick", () => {
    expect(pointerUp(started(), srcHit, true, deps).state?.mode).toBe("pending");
  });
  it("cancels a real drag released over nothing", () => {
    const moved = pointerMove(started(), null, { x: 50, y: 50 }, deps);
    expect(pointerUp(moved, null, false, deps).state).toBeNull();
  });
  it("turns a drag released on an illegal jack into a pending pick with the reason", () => {
    const res = pointerUp(started(), outHit, false, deps);
    expect(res.commit).toBeUndefined();
    expect(res.state).toMatchObject({
      mode: "pending",
      rejection: { key: "s2:output:0", reason: "connect an output to an input" },
    });
  });
});

describe("pointerUp — click + click and drag + drop agree", () => {
  const pending = (): PatchState => pointerUp(started(), null, true, deps).state;
  it.each([
    ["legal", sinkHit],
    ["illegal", outHit],
  ] as const)("returns the same verdict for a %s target", (_name, hit) => {
    const drag = pointerUp(started(), hit, false, deps);
    const click = pointerUp(pending(), hit, true, deps);
    expect(drag.commit).toEqual(click.commit);
    expect(drag.state).toEqual(click.state);
  });
});

describe("pointerUp — pending mode", () => {
  const pending = (): PatchState => pointerUp(started(), null, true, deps).state; // click → pending

  it("completes a cross-view patch on a click onto a legal jack", () => {
    const res = pointerUp(pending(), sinkHit, true, deps);
    expect(res.state).toBeNull();
    expect(res.commit?.ok).toBe(true);
  });
  it("completes on a press-and-drag released onto a legal jack", () => {
    expect(pointerUp(pending(), sinkHit, false, deps).commit?.ok).toBe(true);
  });
  it("keeps the pending pick through a drag released over nothing", () => {
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
  it("treats any alias of the source's duplex socket as the source", () => {
    const duplexAlias: JackHit = {
      ...sinkHit,
      key: "s1:input:0",
      aliases: ["s1:output:0", "s1:input:0"],
    };
    expect(pointerUp(pending(), duplexAlias, true, deps)).toEqual({ state: null });
  });
  it("keeps the pick with the reason on a click over an illegal jack", () => {
    const res = pointerUp(pending(), outHit, true, deps);
    expect(res.commit).toBeUndefined();
    expect(res.state).toMatchObject({
      mode: "pending",
      source: OUT,
      rejection: { key: "s2:output:0", reason: "connect an output to an input" },
    });
  });
});

describe("rejection feedback", () => {
  it("survives while the pointer stays on the rejected socket, clears when it leaves", () => {
    const rejected = pointerUp(started(), outHit, false, deps).state;
    expect(pointerMove(rejected, outHit, { x: 5, y: 5 }, deps)?.rejection?.key).toBe("s2:output:0");
    expect(pointerMove(rejected, null, { x: 50, y: 50 }, deps)?.rejection).toBeNull();
    expect(pointerMove(rejected, sinkHit, { x: 50, y: 50 }, deps)?.rejection).toBeNull();
  });
});

describe("aliasFor", () => {
  it("picks the duplex alias complementing the held source, else the canonical one", () => {
    const aliases = ["u:output:0", "u:input:0"];
    expect(aliasFor(aliases, OUT)).toBe("u:input:0");
    expect(aliasFor(aliases, IN)).toBe("u:output:0");
    expect(aliasFor(aliases, null)).toBe("u:output:0");
    expect(aliasFor(["x:input:2"], OUT)).toBe("x:input:2");
  });
});

describe("pointerCancel / cancel", () => {
  it("drops a held drag but keeps a pending pick (cancel is never a commit)", () => {
    expect(pointerCancel(started())).toBeNull();
    const p = pointerUp(started(), null, true, deps).state;
    expect(pointerCancel(p)).toBe(p);
  });
  it("cancel drops any in-progress patch", () => {
    expect(cancel()).toBeNull();
  });
});
