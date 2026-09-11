// Deterministic browser fixtures deliberately use plain scene/catalog records. They are unsaved, never
// start the engine, and live beside the browser adapter rather than becoming a second view-model source.

import type { CableType, DeviceDescriptor, PortDescriptor } from "../catalog";
import type { Placement, Scene } from "../scene-store";

export type BrowserFixtureId = "rack-hover" | "cable-hover" | "patch-gestures" | "rack-drag";

export interface BrowserFixture {
  id: BrowserFixtureId;
  scene: Scene;
  catalog: DeviceDescriptor[];
  cables: CableType[];
}

const room = { width: 4000, depth: 3000, height: 1400 };

const analogOutput: PortDescriptor = {
  id: 0,
  label: "Out",
  direction: "output",
  domain: "analog",
  channels: 1,
  kind: "line",
  connector: "quarterInch",
  delayed: false,
};
const analogInput: PortDescriptor = { ...analogOutput, label: "In", direction: "input" };

const fixtureCompressor: DeviceDescriptor = {
  typeId: "fixture_compressor",
  name: "Fixture Compressor",
  formFactor: { kind: "rackmount", rackUnits: 2 },
  params: [],
  ports: [],
  readouts: [],
  configs: [],
};
const fixtureSource: DeviceDescriptor = {
  typeId: "fixture_source",
  name: "Fixture Source",
  formFactor: { kind: "desktop", widthMm: 220, heightMm: 120, depthMm: 160 },
  params: [],
  ports: [analogOutput],
  readouts: [],
  configs: [],
};
const fixtureSink: DeviceDescriptor = {
  typeId: "fixture_sink",
  name: "Fixture Sink",
  formFactor: { kind: "desktop", widthMm: 220, heightMm: 120, depthMm: 160 },
  params: [],
  ports: [analogInput],
  readouts: [],
  configs: [],
};

const free = (x: number, facing: "front" | "back" = "front"): Placement => ({
  space: "fixture-room",
  wall: "front",
  position: { x, y: 0, z: 2600 },
  facing,
});

function scene(
  devices: Scene["patch"]["devices"],
  placements: Scene["ui"]["placements"],
  connections: Scene["patch"]["connections"] = [],
  racks: Scene["ui"]["racks"] = [],
): Scene {
  return {
    schemaVersion: 18,
    ui: {
      spaces: [{ id: "fixture-room", name: "Fixture Room", room }],
      racks,
      placements,
      portals: {},
    },
    patch: { devices, connections, output: { device: devices[0]?.id ?? "", port: 0 } },
  };
}

const rackScene = (): Scene =>
  scene(
    [{ id: "fixture-compressor", typeId: "fixture_compressor" }],
    {
      "fixture-compressor": {
        ...free(0),
        rack: { id: "fixture-rack", uSlot: 2 },
      },
    },
    [],
    [
      {
        id: "fixture-rack",
        space: "fixture-room",
        wall: "front",
        facing: "front",
        position: { x: 900, y: 0, z: 2600 },
        slots: 8,
      },
    ],
  );

const rackHover: BrowserFixture = {
  id: "rack-hover",
  catalog: [fixtureCompressor],
  cables: [],
  scene: rackScene(),
};

const rackDrag: BrowserFixture = {
  id: "rack-drag",
  catalog: [fixtureCompressor],
  cables: [],
  scene: rackScene(),
};

const cableHover: BrowserFixture = {
  id: "cable-hover",
  catalog: [fixtureSource, fixtureSink],
  cables: [],
  scene: scene(
    [
      { id: "rear-source", typeId: "fixture_source" },
      { id: "rear-sink", typeId: "fixture_sink" },
      { id: "front-source", typeId: "fixture_source" },
      { id: "front-sink", typeId: "fixture_sink" },
    ],
    {
      "rear-source": free(450, "back"),
      "rear-sink": free(900, "back"),
      "front-source": free(2100, "front"),
      "front-sink": free(2550, "front"),
    },
    [
      { from: { device: "rear-source", port: 0 }, to: { device: "rear-sink", port: 0 } },
      { from: { device: "front-source", port: 0 }, to: { device: "front-sink", port: 0 } },
    ],
  ),
};

const patchGestures: BrowserFixture = {
  id: "patch-gestures",
  catalog: [fixtureSource, fixtureSink],
  cables: [],
  scene: scene(
    [
      { id: "gesture-source", typeId: "fixture_source" },
      { id: "gesture-sink", typeId: "fixture_sink" },
    ],
    {
      "gesture-source": free(900, "back"),
      "gesture-sink": free(1700, "back"),
    },
  ),
};

const fixtures: Readonly<Record<BrowserFixtureId, BrowserFixture>> = {
  "rack-hover": rackHover,
  "cable-hover": cableHover,
  "patch-gestures": patchGestures,
  "rack-drag": rackDrag,
};

export function browserFixture(id: string): BrowserFixture | undefined {
  const fixture = fixtures[id as BrowserFixtureId];
  return fixture ? structuredClone(fixture) : undefined;
}
