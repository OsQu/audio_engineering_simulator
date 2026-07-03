// The focus-surface registry as pure functions — no DOM, no Svelte. Decides, per device, whether it
// warrants a large "sit down at it" focus surface (Story 4.8) and which one to draw. Kept rendering-free
// so the focusability/which-surface logic is unit-testable, mirroring the synth_voice → Screen.svelte
// (ADSR) precedent — a UI-side registry, deliberately *not* a catalog flag (the engine/devices layer
// stays free of UI-presentation vocabulary).

import { type DeviceDescriptor, isPlayable } from "./catalog";

/** Which large focus surface a device opens when clicked. `instrument` = a keybed (+ any voice
 *  controls); `console` = a full channel-strip mixing view. A device with no meaningful deep-control
 *  surface (a converter, a speaker) is not focusable and has none. */
export type FocusSurface = "instrument" | "console";

/** Per-`typeId` surface overrides for devices that warrant a focus surface but are **not** instruments
 *  (so they aren't picked up by the derived `isPlayable` rule below). The instrument surface stays
 *  *derived* — any device with an events input gets a keybed — so this table only names the exceptions. */
const SURFACE_BY_TYPE: Record<string, FocusSurface> = {
  channel_strip: "console",
};

/** The focus surface a device opens, or `null` if it isn't focusable.
 *
 *  An events input ⇒ an `instrument` keybed (derived from {@link isPlayable}, so no per-type list to
 *  keep in sync); otherwise an explicit {@link SURFACE_BY_TYPE} override; otherwise not focusable. */
export function focusSurfaceFor(desc: DeviceDescriptor): FocusSurface | null {
  if (isPlayable(desc)) return "instrument";
  return SURFACE_BY_TYPE[desc.typeId] ?? null;
}

/** Whether a device opens a focus surface when clicked (it has one to show). */
export function isFocusable(desc: DeviceDescriptor): boolean {
  return focusSurfaceFor(desc) !== null;
}
