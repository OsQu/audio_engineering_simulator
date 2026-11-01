// The durable placement record intentionally retains copied fields for scene compatibility, but only a
// free device may read them as physical truth. Mounted devices inherit from their rack; this module is
// the pure boundary that makes that distinction explicit for every consumer.

import { type DeviceDescriptor, descriptorFor } from "./catalog";
import type { DeviceFacing, Rack, Scene } from "./scene-store";
import type { Room, Vec3, Wall } from "./spatial";

export type PlacementDiagnostic =
  | "missing-device"
  | "missing-descriptor"
  | "missing-placement"
  | "missing-rack"
  | "missing-room"
  | "invalid-rack-slot";

export type EffectivePlacement =
  | {
      kind: "studio-free";
      descriptor: DeviceDescriptor;
      room: Room;
      space: string;
      wall: Wall;
      facing: DeviceFacing;
      position: Vec3;
    }
  | {
      kind: "studio-mounted";
      descriptor: DeviceDescriptor;
      rack: Rack;
      room: Room;
      space: string;
      wall: Wall;
      facing: DeviceFacing;
      uSlot: number;
    }
  | { kind: "workbench"; descriptor: DeviceDescriptor; x: number; y: number; facing: DeviceFacing }
  | { kind: "unresolved"; diagnostic: PlacementDiagnostic };

/** A transient candidate location. It is input to resolution only; committing stays in placement.ts. */
export type PlacementPreview =
  | { kind: "studio-free"; deviceId: string; space: string; wall: Wall; position: Vec3 }
  | { kind: "studio-rack"; rackId: string; space: string; wall: Wall; position: Vec3 }
  | { kind: "studio-mount"; deviceId: string; rackId: string; uSlot: number }
  | { kind: "workbench"; deviceId: string; x: number; y: number };

export function resolveEffectivePlacement(
  scene: Scene,
  catalog: DeviceDescriptor[],
  deviceId: string,
  preview?: PlacementPreview,
): EffectivePlacement {
  const device = scene.patch.devices.find((d) => d.id === deviceId);
  if (!device) return { kind: "unresolved", diagnostic: "missing-device" };
  const descriptor = descriptorFor(catalog, device.typeId);
  if (!descriptor) return { kind: "unresolved", diagnostic: "missing-descriptor" };
  if (preview?.kind === "workbench" && preview.deviceId === deviceId) {
    return {
      kind: "workbench",
      descriptor,
      x: preview.x,
      y: preview.y,
      facing: scene.ui.bench?.[deviceId]?.facing ?? "front",
    };
  }
  if (preview?.kind === "studio-free" && preview.deviceId === deviceId) {
    const room = scene.ui.spaces.find((space) => space.id === preview.space)?.room;
    const facing = scene.ui.placements[deviceId]?.facing ?? "front";
    return room
      ? {
          kind: "studio-free",
          descriptor,
          room,
          space: preview.space,
          wall: preview.wall,
          facing,
          position: preview.position,
        }
      : { kind: "unresolved", diagnostic: "missing-room" };
  }
  const placement = scene.ui.placements[deviceId];
  if (!placement) {
    if (scene.ui.spaces.length > 0) return { kind: "unresolved", diagnostic: "missing-placement" };
    const bench = scene.ui.bench?.[deviceId];
    return {
      kind: "workbench",
      descriptor,
      x: bench?.x ?? 0,
      y: bench?.y ?? 0,
      facing: bench?.facing ?? "front",
    };
  }
  const mount =
    preview?.kind === "studio-mount" && preview.deviceId === deviceId
      ? { id: preview.rackId, uSlot: preview.uSlot }
      : placement.rack;
  if (!mount) {
    const room = scene.ui.spaces.find((space) => space.id === placement.space)?.room;
    return room
      ? {
          kind: "studio-free",
          descriptor,
          room,
          space: placement.space,
          wall: placement.wall,
          facing: placement.facing,
          position: placement.position,
        }
      : { kind: "unresolved", diagnostic: "missing-room" };
  }
  const storedRack = scene.ui.racks.find((candidate) => candidate.id === mount.id);
  const rack =
    storedRack && preview?.kind === "studio-rack" && preview.rackId === storedRack.id
      ? { ...storedRack, space: preview.space, wall: preview.wall, position: preview.position }
      : storedRack;
  if (!rack) return { kind: "unresolved", diagnostic: "missing-rack" };
  const units = descriptor.formFactor.kind === "rackmount" ? descriptor.formFactor.rackUnits : 0;
  if (
    !Number.isInteger(mount.uSlot) ||
    units <= 0 ||
    mount.uSlot < 0 ||
    mount.uSlot + units > rack.slots
  ) {
    return { kind: "unresolved", diagnostic: "invalid-rack-slot" };
  }
  const room = scene.ui.spaces.find((space) => space.id === rack.space)?.room;
  return room
    ? {
        kind: "studio-mounted",
        descriptor,
        rack,
        room,
        space: rack.space,
        wall: rack.wall,
        facing: rack.facing,
        uSlot: mount.uSlot,
      }
    : { kind: "unresolved", diagnostic: "missing-room" };
}
