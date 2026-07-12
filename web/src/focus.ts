// Focusability as pure functions — no DOM, no Svelte — so it stays unit-testable (the vitest runner is
// node-only and can't import components). This module decides *whether* a device opens a focus surface;
// *which component* it draws lives in device-ui.ts (`focusUi`). Deliberately not a catalog flag — the
// engine/devices layer stays free of UI-presentation vocabulary.

import { type DeviceDescriptor, isPlayable } from "./catalog";

/** TypeIds that open a **dedicated** software focus surface — a mixing console, a routing matrix, the
 *  MIDI controller's playable keybed. The matching typeId → component map is `FOCUS_SURFACES` in
 *  device-ui.ts (keep the two in sync). A device *without* a dedicated surface but with an events input is
 *  still focusable (see `isFocusable`) — it just magnifies its physical faceplate; the on-screen keybed is
 *  no longer free, it is composed by the controller's dedicated surface here. */
const DEDICATED_FOCUS_SURFACES: ReadonlySet<string> = new Set([
  "channel_strip",
  "scarlett_8i6",
  "computer",
  "midi_controller",
]);

/** Whether a device opens a focus surface when clicked: a listed type opens its dedicated surface, and a
 *  playable device (an events input) is focusable to be played/operated — no per-type list needed for it. */
export function isFocusable(desc: DeviceDescriptor): boolean {
  return isPlayable(desc) || DEDICATED_FOCUS_SURFACES.has(desc.typeId);
}
