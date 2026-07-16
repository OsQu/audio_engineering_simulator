import { describe, expect, it } from "vitest";
import type { CableType, DeviceDescriptor } from "../src/catalog";
import type { ConnectVerdict } from "../src/connections";
import type { LayoutCtx } from "../src/projection";
import type { Connection } from "../src/scene";
import {
  addBenchDevice,
  addDevice,
  addSpace,
  cablesFor,
  commitCable,
  connectionDomain,
  connKey,
  disconnect,
  moveDeviceToSpace,
  removeDevice,
  removeRack,
  setCableType,
  toggleFlip,
  toggleRackFlip,
  unmount,
} from "../src/scene-ops";
import type { Placement, Rack, Scene } from "../src/scene-store";
import type { Room, Wall } from "../src/spatial";

const ROOM: Room = { width: 4000, depth: 3000, height: 1400 };

// An amp: analog line I/O. A MIDI source: an events output. A desktop synth for placement.
const AMP: DeviceDescriptor = {
  typeId: "amp",
  name: "Amp",
  formFactor: { kind: "rackmount", rackUnits: 1 },
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
const MIDI: DeviceDescriptor = {
  typeId: "midi",
  name: "MIDI",
  formFactor: { kind: "desktop", widthMm: 200, heightMm: 100, depthMm: 150 },
  params: [],
  ports: [
    {
      id: 0,
      label: "out",
      direction: "output",
      domain: "events",
      channels: 1,
      kind: "midi",
      connector: "din5",
      delayed: false,
    },
  ],
  readouts: [],
  configs: [],
};
// An interface with a single duplex USB-C jack (one connector, both directions), like the 8i6/computer.
const IFACE: DeviceDescriptor = {
  typeId: "iface",
  name: "Interface",
  formFactor: { kind: "rackmount", rackUnits: 1 },
  params: [],
  ports: [
    {
      id: 0,
      label: "USB",
      direction: "output",
      domain: "digital",
      channels: 8,
      kind: "digital",
      connector: "usb",
      delayed: false,
      duplexPartner: 0,
    },
    {
      id: 0,
      label: "USB",
      direction: "input",
      domain: "digital",
      channels: 8,
      kind: "digital",
      connector: "usb",
      delayed: false,
      duplexPartner: 0,
    },
  ],
  readouts: [],
  configs: [],
};
const CATALOG = [AMP, MIDI, IFACE];

const CABLES: CableType[] = [
  {
    typeId: "trs",
    label: "TRS 6 m",
    kind: "line",
    connector: "quarterInch",
    lengthM: 6,
    resistanceOhms: 50,
    capacitanceFarads: 1e-9,
  },
  {
    typeId: "long",
    label: "TRS 30 m",
    kind: "line",
    connector: "quarterInch",
    lengthM: 30,
    resistanceOhms: 250,
    capacitanceFarads: 5e-9,
  },
];

const place = (space: string, wall: Wall, rack?: { id: string; uSlot: number }): Placement => ({
  space,
  wall,
  position: { x: 0, y: 0, z: 0 },
  facing: "front",
  ...(rack ? { rack } : {}),
});

function makeScene(opts: {
  devices?: { id: string; typeId: string }[];
  connections?: Connection[];
  placements?: Record<string, Placement>;
  racks?: Rack[];
  output?: { device: string; port: number };
}): Scene {
  return {
    schemaVersion: 99,
    ui: {
      spaces: [{ id: "s1", name: "Studio", room: ROOM }],
      racks: opts.racks ?? [],
      placements: opts.placements ?? {},
      portals: {},
    },
    patch: {
      devices: opts.devices ?? [],
      connections: opts.connections ?? [],
      output: opts.output ?? { device: "", port: 0 },
    },
  };
}

function ctxOf(scene: Scene, view: Wall | "top" = "front"): LayoutCtx {
  return {
    space: "s1",
    view,
    wall: view === "top" ? null : view,
    room: ROOM,
    scene,
    catalog: CATALOG,
  };
}

describe("connKey", () => {
  it("encodes both endpoints stably", () => {
    expect(connKey({ from: { device: "a", port: 0 }, to: { device: "b", port: 1 } })).toBe(
      "a:0->b:1",
    );
  });
});

describe("commitCable", () => {
  it("gives a fresh analog connection the default (first) cable", () => {
    const scene = makeScene({
      devices: [
        { id: "amp1", typeId: "amp" },
        { id: "amp2", typeId: "amp" },
      ],
    });
    const v: ConnectVerdict = {
      ok: true,
      connection: { from: { device: "amp1", port: 0 }, to: { device: "amp2", port: 0 } },
      replaces: null,
    };
    commitCable(scene, CATALOG, CABLES, v);
    expect(scene.patch.connections).toHaveLength(1);
    // analog ⇒ carries the first preset's R·C
    expect(scene.patch.connections[0].cable).toEqual({
      resistanceOhms: 50,
      capacitanceFarads: 1e-9,
    });
  });

  it("leaves an events connection ideal (no cable)", () => {
    const scene = makeScene({
      devices: [
        { id: "m1", typeId: "midi" },
        { id: "amp2", typeId: "amp" },
      ],
    });
    // sanity: the source port is an events domain
    const conn = { from: { device: "m1", port: 0 }, to: { device: "amp2", port: 0 } };
    expect(connectionDomain(scene, CATALOG, conn)).toBe("events");
    commitCable(scene, CATALOG, CABLES, { ok: true, connection: conn, replaces: null });
    expect(scene.patch.connections[0].cable).toBeUndefined();
  });

  it("preserves the duplex flag on a USB-C link (and adds no cable — it's digital)", () => {
    // Regression: commitCable rebuilt the connection from from/to alone, dropping `duplex`, so a
    // USB-C cable committed as a one-way link (no return leg → silence). It must survive commit.
    const scene = makeScene({
      devices: [
        { id: "if1", typeId: "iface" },
        { id: "pc1", typeId: "iface" },
      ],
    });
    const v: ConnectVerdict = {
      ok: true,
      connection: {
        from: { device: "if1", port: 0 },
        to: { device: "pc1", port: 0 },
        duplex: true,
      },
      replaces: null,
    };
    commitCable(scene, CATALOG, CABLES, v);
    expect(scene.patch.connections).toHaveLength(1);
    expect(scene.patch.connections[0].duplex).toBe(true);
    expect(scene.patch.connections[0].cable).toBeUndefined();
  });

  it("replaces the fan-in edge it supersedes", () => {
    const existing: Connection = {
      from: { device: "amp1", port: 0 },
      to: { device: "amp2", port: 0 },
    };
    const scene = makeScene({
      devices: [
        { id: "amp1", typeId: "amp" },
        { id: "amp2", typeId: "amp" },
        { id: "amp3", typeId: "amp" },
      ],
      connections: [existing],
    });
    const v: ConnectVerdict = {
      ok: true,
      connection: { from: { device: "amp3", port: 0 }, to: { device: "amp2", port: 0 } },
      replaces: existing,
    };
    commitCable(scene, CATALOG, CABLES, v);
    // old amp1→amp2 gone, new amp3→amp2 in
    expect(scene.patch.connections.map(connKey)).toEqual(["amp3:0->amp2:0"]);
  });
});

describe("connector-aware cable selection", () => {
  const analogConn: Connection = {
    from: { device: "amp1", port: 0 },
    to: { device: "amp2", port: 0 },
  };
  // An XLR cable listed *first*, ahead of the two ¼" presets — it must be skipped for a ¼" connection.
  const mixedCables: CableType[] = [
    {
      typeId: "xlr",
      label: "XLR 3 m",
      kind: "mic",
      connector: "xlr",
      lengthM: 3,
      resistanceOhms: 1,
      capacitanceFarads: 1e-10,
    },
    ...CABLES,
  ];
  const twoAmps = () =>
    makeScene({
      devices: [
        { id: "amp1", typeId: "amp" },
        { id: "amp2", typeId: "amp" },
      ],
    });

  it("cablesFor keeps only presets whose connector matches the connection's ports (¼\")", () => {
    const fit = cablesFor(twoAmps(), CATALOG, mixedCables, analogConn);
    expect(fit.map((c) => c.typeId)).toEqual(["trs", "long"]); // the XLR cable is excluded
  });

  it("commitCable assigns the first *matching* preset, not the first overall", () => {
    const scene = twoAmps();
    commitCable(scene, CATALOG, mixedCables, { ok: true, connection: analogConn, replaces: null });
    // trs (first ¼" preset), not the XLR that leads the list.
    expect(scene.patch.connections[0].cable).toEqual({
      resistanceOhms: 50,
      capacitanceFarads: 1e-9,
    });
  });
});

describe("disconnect", () => {
  it("removes exactly the matching connection", () => {
    const a: Connection = { from: { device: "amp1", port: 0 }, to: { device: "amp2", port: 0 } };
    const b: Connection = { from: { device: "amp2", port: 0 }, to: { device: "amp3", port: 0 } };
    const scene = makeScene({ connections: [a, b] });
    disconnect(scene, a);
    expect(scene.patch.connections.map(connKey)).toEqual(["amp2:0->amp3:0"]);
  });
});

describe("setCableType", () => {
  const base = (): Scene =>
    makeScene({
      connections: [
        {
          from: { device: "amp1", port: 0 },
          to: { device: "amp2", port: 0 },
          cable: { resistanceOhms: 50, capacitanceFarads: 1e-9 },
        },
      ],
    });

  it("swaps in the selected preset's R·C", () => {
    const scene = base();
    setCableType(scene, CABLES, scene.patch.connections[0], "long");
    expect(scene.patch.connections[0].cable).toEqual({
      resistanceOhms: 250,
      capacitanceFarads: 5e-9,
    });
  });

  it("clears to an ideal wire on the empty type id", () => {
    const scene = base();
    setCableType(scene, CABLES, scene.patch.connections[0], "");
    expect(scene.patch.connections[0].cable).toBeUndefined();
  });
});

describe("removeDevice", () => {
  it("cascades: drops the device, its connections (either end), and its placement", () => {
    const scene = makeScene({
      devices: [
        { id: "amp1", typeId: "amp" },
        { id: "amp2", typeId: "amp" },
        { id: "amp3", typeId: "amp" },
      ],
      connections: [
        { from: { device: "amp1", port: 0 }, to: { device: "amp2", port: 0 } }, // amp2 as sink
        { from: { device: "amp2", port: 0 }, to: { device: "amp3", port: 0 } }, // amp2 as source
        { from: { device: "amp1", port: 0 }, to: { device: "amp3", port: 0 } }, // unrelated
      ],
      placements: { amp2: place("s1", "front") },
      output: { device: "amp3", port: 0 },
    });
    removeDevice(scene, "amp2");
    expect(scene.patch.devices.map((d) => d.id)).toEqual(["amp1", "amp3"]);
    expect(scene.patch.connections.map(connKey)).toEqual(["amp1:0->amp3:0"]);
    expect(scene.ui.placements.amp2).toBeUndefined();
  });

  it("refuses to remove the output tap", () => {
    const scene = makeScene({
      devices: [{ id: "amp1", typeId: "amp" }],
      output: { device: "amp1", port: 0 },
    });
    removeDevice(scene, "amp1");
    expect(scene.patch.devices.map((d) => d.id)).toEqual(["amp1"]); // untouched
  });

  it("also drops the device's bench offset (workbench cleanup)", () => {
    const scene = makeScene({
      devices: [
        { id: "amp1", typeId: "amp" },
        { id: "amp2", typeId: "amp" },
      ],
      output: { device: "amp1", port: 0 },
    });
    scene.ui.bench = { amp1: { x: 1, y: 2 }, amp2: { x: 3, y: 4 } };
    removeDevice(scene, "amp2");
    expect(scene.ui.bench.amp2).toBeUndefined();
    expect(scene.ui.bench.amp1).toEqual({ x: 1, y: 2 }); // survivor untouched
  });
});

describe("addBenchDevice", () => {
  it("appends an unwired instance and returns a unique id", () => {
    const scene = makeScene({ devices: [{ id: "dev", typeId: "amp" }] });
    const id = addBenchDevice(scene, "amp");
    expect(id).toBe("amp-1");
    expect(scene.patch.devices).toContainEqual({ id: "amp-1", typeId: "amp" });
    expect(scene.patch.connections).toEqual([]); // unwired
  });

  it("bumps the suffix past an existing same-type instance", () => {
    const scene = makeScene({
      devices: [
        { id: "amp-1", typeId: "amp" },
        { id: "amp-2", typeId: "amp" },
      ],
    });
    expect(addBenchDevice(scene, "amp")).toBe("amp-3");
  });
});

describe("removeRack", () => {
  it("un-mounts its gear (keeping positions) and drops the rack", () => {
    const mounted = place("s1", "front", { id: "r1", uSlot: 3 });
    mounted.position = { x: 111, y: 0, z: 222 };
    const scene = makeScene({
      devices: [{ id: "d1", typeId: "amp" }],
      placements: { d1: mounted },
      racks: [
        {
          id: "r1",
          space: "s1",
          wall: "front",
          facing: "front",
          position: { x: 0, y: 0, z: 0 },
          slots: 8,
        },
      ],
    });
    removeRack(scene, "r1");
    expect(scene.ui.racks).toHaveLength(0);
    expect(scene.ui.placements.d1.rack).toBeUndefined(); // un-mounted
    expect(scene.ui.placements.d1.position).toEqual({ x: 111, y: 0, z: 222 }); // position kept
  });
});

describe("space + flip furniture", () => {
  it("addSpace appends a uniquely-named space and returns its id", () => {
    const scene = makeScene({});
    const id = addSpace(scene);
    expect(id).toBe("space-2"); // one space already exists (s1); counter is length+1
    expect(scene.ui.spaces.map((s) => s.id)).toContain("space-2");
  });

  it("moveDeviceToSpace re-homes a device to the floor origin, un-mounted", () => {
    const scene = makeScene({
      placements: { d1: place("s1", "front", { id: "r1", uSlot: 0 }) },
    });
    moveDeviceToSpace(scene, "d1", "s2");
    expect(scene.ui.placements.d1.space).toBe("s2");
    expect(scene.ui.placements.d1.rack).toBeUndefined();
    expect(scene.ui.placements.d1.position).toEqual({ x: 0, y: 0, z: 0 });
  });

  it("toggleFlip flips a free-standing device's facing both ways", () => {
    const scene = makeScene({ placements: { d1: place("s1", "front") } });
    toggleFlip(scene, "d1");
    expect(scene.ui.placements.d1.facing).toBe("back");
    toggleFlip(scene, "d1");
    expect(scene.ui.placements.d1.facing).toBe("front");
  });

  it("toggleFlip is a no-op for rack-mounted gear (it's bolted in — turn the rack instead)", () => {
    const scene = makeScene({
      placements: { d1: place("s1", "front", { id: "r1", uSlot: 0 }) },
    });
    toggleFlip(scene, "d1");
    expect(scene.ui.placements.d1.facing).toBe("front"); // unchanged
  });

  it("toggleRackFlip turns the whole rack around both ways", () => {
    const scene = makeScene({
      racks: [
        {
          id: "r1",
          space: "s1",
          wall: "back",
          facing: "front",
          position: { x: 0, y: 0, z: 0 },
          slots: 8,
        },
      ],
    });
    toggleRackFlip(scene, "r1");
    expect(scene.ui.racks[0].facing).toBe("back");
    toggleRackFlip(scene, "r1");
    expect(scene.ui.racks[0].facing).toBe("front");
  });

  it("unmount ejects a mounted device to free-standing, keeping its position", () => {
    const mounted = place("s1", "front", { id: "r1", uSlot: 0 });
    mounted.position = { x: 111, y: 0, z: 222 };
    const scene = makeScene({ placements: { d1: mounted } });
    unmount(scene, "d1");
    expect(scene.ui.placements.d1.rack).toBeUndefined();
    expect(scene.ui.placements.d1.position).toEqual({ x: 111, y: 0, z: 222 });
  });

  it("unmount is a no-op for already free-standing gear", () => {
    const scene = makeScene({ placements: { d1: place("s1", "front") } });
    unmount(scene, "d1");
    expect(scene.ui.placements.d1.rack).toBeUndefined();
  });
});

describe("addDevice", () => {
  it("adds a uniquely-id'd instance with a placement in the current space", () => {
    const scene = makeScene({ devices: [{ id: "amp-1", typeId: "amp" }] });
    addDevice(ctxOf(scene), [], "amp");
    // amp-1 exists ⇒ next free id is amp-2
    expect(scene.patch.devices.map((d) => d.id)).toEqual(["amp-1", "amp-2"]);
    expect(scene.ui.placements["amp-2"].space).toBe("s1");
    expect(scene.ui.placements["amp-2"].facing).toBe("front");
  });
});
