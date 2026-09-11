import type { DeviceDescriptor } from "../../src/catalog";
import type { LayoutCtx } from "../../src/projection";
import type { Connection } from "../../src/scene";
import type { DeviceFacing, Placement, Scene } from "../../src/scene-store";
import type { Room, Wall } from "../../src/spatial";

export const ROOM: Room = { width: 4000, depth: 3000, height: 1400 };
export const WALL_LABELS: Record<Wall, string> = {
  front: "Front",
  back: "Back",
  left: "Left",
  right: "Right",
};

const desktop: DeviceDescriptor = {
  typeId: "desktop",
  name: "Desktop",
  formFactor: { kind: "desktop", widthMm: 200, heightMm: 100, depthMm: 150 },
  params: [],
  ports: [],
  readouts: [],
  configs: [],
};
const rackmount: DeviceDescriptor = {
  typeId: "rackmount",
  name: "Rackmount",
  formFactor: { kind: "rackmount", rackUnits: 1 },
  params: [],
  ports: [],
  readouts: [],
  configs: [],
};
export const CATALOG = [desktop, rackmount];

const place = (space: string, wall: Wall, facing: DeviceFacing = "front"): Placement => ({
  space,
  wall,
  position: { x: 300, y: 0, z: 400 },
  facing,
});

export const studioScene: Scene = {
  schemaVersion: 99,
  ui: {
    spaces: [
      { id: "studio", name: "Studio", room: ROOM },
      { id: "booth", name: "Booth", room: ROOM },
    ],
    racks: [
      {
        id: "rack-front",
        space: "studio",
        wall: "front",
        facing: "front",
        position: { x: 900, y: 0, z: 400 },
        slots: 8,
      },
      {
        id: "rack-back",
        space: "studio",
        wall: "back",
        facing: "back",
        position: { x: 900, y: 0, z: 400 },
        slots: 8,
      },
    ],
    placements: {
      "free-front": place("studio", "front", "front"),
      "free-back": place("studio", "back", "back"),
      "free-left": place("studio", "left"),
      "free-right": place("studio", "right"),
      "mounted-front": {
        ...place("studio", "front", "back"),
        rack: { id: "rack-front", uSlot: 0 },
      },
      "mounted-back": { ...place("studio", "back", "front"), rack: { id: "rack-back", uSlot: 0 } },
      "remote-room": place("booth", "front"),
    },
    portals: {},
  },
  patch: {
    devices: [
      { id: "free-front", typeId: "desktop" },
      { id: "free-back", typeId: "desktop" },
      { id: "free-left", typeId: "desktop" },
      { id: "free-right", typeId: "desktop" },
      { id: "mounted-front", typeId: "rackmount" },
      { id: "mounted-back", typeId: "rackmount" },
      { id: "remote-room", typeId: "desktop" },
    ],
    connections: [],
    output: { device: "free-front", port: 0 },
  },
};

export const studioViewCases: { view: Wall | "top"; itemIds: string[] }[] = [
  { view: "front", itemIds: ["rack-front", "mounted-front", "free-front"] },
  { view: "back", itemIds: ["rack-back", "mounted-back", "free-back"] },
  { view: "left", itemIds: ["free-left"] },
  { view: "right", itemIds: ["free-right"] },
  {
    view: "top",
    itemIds: ["rack-front", "rack-back", "free-front", "free-back", "free-left", "free-right"],
  },
];

export const layoutFor = (view: Wall | "top"): LayoutCtx => ({
  space: "studio",
  view,
  wall: view === "top" ? null : view,
  room: ROOM,
  scene: structuredClone(studioScene),
  catalog: CATALOG,
});

export const faceCases: { id: string; expected: DeviceFacing }[] = [
  { id: "free-front", expected: "front" },
  { id: "free-back", expected: "back" },
  { id: "mounted-front", expected: "front" },
  { id: "mounted-back", expected: "back" },
];

export const socketCases: {
  name: string;
  view: Wall;
  device: string;
  face: DeviceFacing;
  expected: "measured" | "estimated";
}[] = [
  {
    name: "shown free front",
    view: "front",
    device: "free-front",
    face: "front",
    expected: "measured",
  },
  {
    name: "hidden free back",
    view: "front",
    device: "free-front",
    face: "back",
    expected: "estimated",
  },
  {
    name: "shown rack-controlled back",
    view: "back",
    device: "mounted-back",
    face: "back",
    expected: "measured",
  },
  {
    name: "hidden rack-controlled front",
    view: "back",
    device: "mounted-back",
    face: "front",
    expected: "estimated",
  },
];

const connection = (from: string, to: string): Connection => ({
  from: { device: from, port: 0 },
  to: { device: to, port: 0 },
});
export const connectionCases: {
  name: string;
  connection: Connection;
  continuous: boolean;
  portal: boolean;
  otherEnd?: string;
  label?: string;
}[] = [
  {
    name: "local-local",
    connection: connection("free-front", "mounted-front"),
    continuous: true,
    portal: false,
  },
  {
    name: "local-remote-wall",
    connection: connection("free-front", "free-back"),
    continuous: false,
    portal: true,
    otherEnd: "free-back",
    label: "Back",
  },
  {
    name: "local-remote-room",
    connection: connection("free-front", "remote-room"),
    continuous: false,
    portal: true,
    otherEnd: "remote-room",
    label: "Booth",
  },
  {
    name: "remote-remote",
    connection: connection("free-back", "remote-room"),
    continuous: false,
    portal: false,
  },
];

export const benchScene: Scene = {
  schemaVersion: 99,
  ui: {
    spaces: [],
    racks: [],
    placements: {},
    portals: {},
    bench: {
      dut: { x: 0, y: 0, facing: "back" },
      "support-front": { x: 0, y: 0, facing: "front" },
      "support-back": { x: 0, y: 0, facing: "back" },
    },
  },
  patch: {
    devices: [
      { id: "dut", typeId: "desktop" },
      { id: "support-front", typeId: "desktop" },
      { id: "support-back", typeId: "desktop" },
    ],
    connections: [],
    output: { device: "dut", port: 0 },
  },
};
export const benchFaceCases: { id: string; faces: DeviceFacing[] }[] = [
  { id: "dut", faces: ["front", "back"] },
  { id: "support-front", faces: ["front"] },
  { id: "support-back", faces: ["back"] },
];
