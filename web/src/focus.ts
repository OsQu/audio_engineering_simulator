// Focusability as pure functions — no DOM, no Svelte — so it stays unit-testable (the vitest runner is
// node-only and can't import components). This module decides *whether* a device opens a focus surface;
// *which component* it draws lives in device-ui.ts (`focusUi`). Deliberately not a catalog flag — the
// engine/devices layer stays free of UI-presentation vocabulary.

import { type DeviceDescriptor, isPlayable } from "./catalog";

/** TypeIds that open a **dedicated** (non-keybed) focus surface — e.g. a mixing console. This is the
 *  focusability authority for non-playable devices; the matching typeId → component map is
 *  `FOCUS_SURFACES` in device-ui.ts (keep the two in sync). A playable device (an events input) already
 *  earns a keybed surface for free, so this table lists only the exceptions. */
const DEDICATED_FOCUS_SURFACES: ReadonlySet<string> = new Set(["channel_strip"]);

/** Whether a device opens a focus surface when clicked: a playable device gets a keybed (derived from
 *  its events input, so no per-type list to maintain), and a listed type opens its dedicated surface. */
export function isFocusable(desc: DeviceDescriptor): boolean {
  return isPlayable(desc) || DEDICATED_FOCUS_SURFACES.has(desc.typeId);
}
