// The scene-walking helpers resolve each device through a per-instance `Describe`, not the type catalog:
// a config-driven computer (sized by `usb_sends` / `track_count`) has more params and wider ports than
// its type's default face, and those extras must be seeded, pushed, watchable and patchable.

import { describe, expect, it } from "vitest";
import { watchables } from "../src/bench-watch";
import {
  type Describe,
  type DeviceDescriptor,
  type ParamDescriptor,
  type PortDescriptor,
  typeDescriber,
} from "../src/catalog";
import type { ControlMessage } from "../src/engine";
import { pushParams, seedParamValues } from "../src/params";
import { endpointFromJackKey } from "../src/patching";
import { connectionConnectors } from "../src/scene-ops";
import type { Scene } from "../src/scene-store";

// One crossbar crosspoint param per (send, track) pair — the shape that grows with the config.
const crosspoints = (sends: number, tracks: number): ParamDescriptor[] =>
  Array.from({ length: sends * tracks }, (_, id) => ({
    id,
    label: `Send ${Math.floor(id / tracks) + 1} → Track ${(id % tracks) + 1}`,
    unit: "",
    kind: "switch",
    min: 0,
    max: 1,
    default: 0,
  }));

// A computer described against a given (sends, tracks): its USB input carries `sends` lanes, and each
// track gets an analog monitor output (port id = track index).
function computer(sends: number, tracks: number): DeviceDescriptor {
  const outs: PortDescriptor[] = Array.from({ length: tracks }, (_, id) => ({
    id,
    direction: "output",
    domain: "analog",
    kind: "line",
    connector: "quarterInch",
    channels: 1,
    label: `Track ${id + 1}`,
    delayed: false,
  }));
  return {
    typeId: "computer",
    name: "Computer",
    formFactor: { kind: "rackmount", rackUnits: 1 },
    params: crosspoints(sends, tracks),
    ports: [
      {
        id: 0,
        direction: "input",
        domain: "digital",
        kind: "digital",
        connector: "usb",
        channels: sends,
        label: "USB",
        delayed: false,
      },
      ...outs,
    ],
    readouts: [],
    configs: [
      { key: "usb_sends", label: "USB sends", kind: "toggle", default: 2 },
      { key: "track_count", label: "Tracks", kind: "toggle", default: 1 },
    ],
  } as DeviceDescriptor;
}

// The type catalog only knows the default face (2 sends × 1 track); the engine reports the configured
// instance as 8 sends × 4 tracks.
const TYPE_FACE = computer(2, 1);
const INSTANCE_FACE = computer(8, 4);

const scene = (): Scene => ({
  schemaVersion: 1,
  ui: { spaces: [], racks: [], placements: {}, portals: {} },
  patch: {
    devices: [
      {
        id: "pc",
        typeId: "computer",
        config: [
          { key: "usb_sends", value: 8 },
          { key: "track_count", value: 4 },
        ],
        params: [{ id: 31, value: 1 }], // the last crosspoint — only exists on the configured face
      },
    ],
    connections: [{ from: { device: "pc", port: 3 }, to: { device: "pc", port: 0 } }],
    output: { device: "pc", port: 0 },
  },
});

// The session's resolution: the engine-reported per-instance descriptor, else the type's.
const perInstance =
  (s: Scene): Describe =>
  (id) =>
    id === "pc" ? INSTANCE_FACE : typeDescriber(s, [TYPE_FACE])(id);

describe("per-instance descriptors for a configured computer", () => {
  it("seeds every crosspoint of the configured face, keeping saved values", () => {
    const s = scene();
    const values = seedParamValues(s, perInstance(s));
    expect(Object.keys(values)).toHaveLength(32); // 8 sends × 4 tracks
    expect(values["pc:31"]).toBe(1); // saved
    expect(values["pc:30"]).toBe(0); // defaulted
    // The type face alone would seed only 2 crosspoints and drop the saved one.
    expect(Object.keys(seedParamValues(s, typeDescriber(s, [TYPE_FACE])))).toHaveLength(2);
  });

  it("pushes every crosspoint of the configured face to the engine", () => {
    const s = scene();
    const describeFn = perInstance(s);
    const sent: number[] = [];
    pushParams(
      (m: ControlMessage) => {
        if (m.type === "param") sent.push(m.paramId);
      },
      s,
      describeFn,
      seedParamValues(s, describeFn),
    );
    expect(sent).toHaveLength(32);
    expect(sent).toContain(31);
  });

  it("lists the configured face's params as watchables (debug pins resolve)", () => {
    const s = scene();
    const ids = watchables(s, perInstance(s))
      .filter((w) => w.kind === "param")
      .map((w) => w.id);
    expect(ids).toHaveLength(32);
    expect(ids).toContain("31");
  });

  it("resolves a jack that only exists on the configured face", () => {
    const s = scene();
    expect(endpointFromJackKey(perInstance(s), "pc:output:3")).toMatchObject({
      device: "pc",
      port: 3,
      domain: "analog",
    });
    expect(endpointFromJackKey(typeDescriber(s, [TYPE_FACE]), "pc:output:3")).toBeNull();
  });

  it("resolves a connection's connectors against the configured face", () => {
    const s = scene();
    expect(connectionConnectors(perInstance(s), s.patch.connections[0])).toEqual([
      "quarterInch",
      "usb",
    ]);
  });
});
