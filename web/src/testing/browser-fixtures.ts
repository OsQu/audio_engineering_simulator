// Deterministic browser fixtures deliberately use plain scene/catalog records. They are unsaved, never
// start the engine, and live beside the browser adapter rather than becoming a second view-model source.

import type { CableType, DeviceDescriptor } from "../catalog";
import type { Scene } from "../scene-store";

export type BrowserFixtureId = "rack-hover";

export interface BrowserFixture {
  id: BrowserFixtureId;
  scene: Scene;
  catalog: DeviceDescriptor[];
  cables: CableType[];
}

const fixtureCompressor: DeviceDescriptor = {
  typeId: "fixture_compressor",
  name: "Fixture Compressor",
  formFactor: { kind: "rackmount", rackUnits: 2 },
  params: [],
  ports: [],
  readouts: [],
  configs: [],
};

const rackHover: BrowserFixture = {
  id: "rack-hover",
  catalog: [fixtureCompressor],
  cables: [],
  scene: {
    schemaVersion: 18,
    ui: {
      spaces: [
        {
          id: "fixture-room",
          name: "Fixture Room",
          room: { width: 4000, depth: 3000, height: 1400 },
        },
      ],
      racks: [
        {
          id: "fixture-rack",
          space: "fixture-room",
          wall: "front",
          facing: "front",
          position: { x: 900, y: 0, z: 2600 },
          slots: 8,
        },
      ],
      placements: {
        "fixture-compressor": {
          space: "fixture-room",
          wall: "front",
          position: { x: 0, y: 0, z: 0 },
          rack: { id: "fixture-rack", uSlot: 2 },
          facing: "front",
        },
      },
      portals: {},
    },
    patch: {
      devices: [{ id: "fixture-compressor", typeId: "fixture_compressor" }],
      connections: [],
      output: { device: "fixture-compressor", port: 0 },
    },
  },
};

export function browserFixture(id: string): BrowserFixture | undefined {
  return id === rackHover.id ? structuredClone(rackHover) : undefined;
}
