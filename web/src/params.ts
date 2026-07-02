// Control-param helpers: the live-value map keying, seeding it from the scene, and pushing it to the
// engine. Pure given their inputs (no Svelte). App owns the `paramValues` $state and `onParamInput`
// (which touches all three param lanes at once); these are the read/seed/push helpers around it.

import { type DeviceDescriptor, descriptorFor } from "./catalog";
import type { ControlMessage } from "./engine";
import type { Scene } from "./scene-store";

// A device-local param's map key: "device:paramId".
export const key = (device: string, paramId: number): string => `${device}:${paramId}`;

// The current value of a device-local param: the live override in `values` if any, else the descriptor
// default.
export function paramValue(
  values: Record<string, number>,
  deviceId: string,
  desc: DeviceDescriptor,
  id: number,
): number {
  const v = values[key(deviceId, id)];
  return v !== undefined ? v : (desc.params.find((p) => p.id === id)?.default ?? 0);
}

// Build the live param map from the scene: each device's saved value if present, else the descriptor
// default. Returned (not assigned) so the caller owns the reactive state.
export function seedParamValues(scene: Scene, catalog: DeviceDescriptor[]): Record<string, number> {
  const values: Record<string, number> = {};
  for (const device of scene.patch.devices) {
    const desc = descriptorFor(catalog, device.typeId);
    if (!desc) continue;
    for (const p of desc.params) {
      const saved = device.params?.find((s) => s.id === p.id)?.value;
      values[key(device.id, p.id)] = saved ?? p.default;
    }
  }
  return values;
}

// Push every device's current param values to the engine — after a (re)build the host re-applies the
// scene's control values over the queue (they'd glide from the node defaults otherwise).
export function pushParams(
  sendFn: (msg: ControlMessage) => void,
  scene: Scene,
  catalog: DeviceDescriptor[],
  values: Record<string, number>,
): void {
  for (const device of scene.patch.devices) {
    const desc = descriptorFor(catalog, device.typeId);
    if (!desc) continue;
    for (const p of desc.params) {
      sendFn({
        type: "param",
        device: device.id,
        paramId: p.id,
        value: paramValue(values, device.id, desc, p.id),
      });
    }
  }
}
