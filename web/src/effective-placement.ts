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

export function resolveEffectivePlacement(
  scene: Scene,
  catalog: DeviceDescriptor[],
  deviceId: string,
): EffectivePlacement {
  const device = scene.patch.devices.find((d) => d.id === deviceId);
  if (!device) return { kind: "unresolved", diagnostic: "missing-device" };
  const descriptor = descriptorFor(catalog, device.typeId);
  if (!descriptor) return { kind: "unresolved", diagnostic: "missing-descriptor" };
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
  if (!placement.rack) {
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
  const rack = scene.ui.racks.find((candidate) => candidate.id === placement.rack?.id);
  if (!rack) return { kind: "unresolved", diagnostic: "missing-rack" };
  const units = descriptor.formFactor.kind === "rackmount" ? descriptor.formFactor.rackUnits : 0;
  if (
    !Number.isInteger(placement.rack.uSlot) ||
    units <= 0 ||
    placement.rack.uSlot < 0 ||
    placement.rack.uSlot + units > rack.slots
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
        uSlot: placement.rack.uSlot,
      }
    : { kind: "unresolved", diagnostic: "missing-room" };
}
