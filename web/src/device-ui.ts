// The device-UI registry: which Svelte component draws a given device type — its bespoke faceplate, or
// the generic `Panel` fallback (Story 5.7). This is the *look & feel* seam that mirrors `skin.ts`: the
// Rust catalog owns which params/ports a device has (by id); a registered component owns how they're
// arranged and styled, composing the shared bound widgets (Control/Socket/Reading) which bind by id.
//
// Two lookups, one prop shape:
//   • `deviceUi(typeId)`  — the in-world faceplate (the panel you see in the rack / on the desk).
//   • `focusUi(typeId)`   — the surface shown large in the focus overlay; a device may register a
//                           richer/alternate one (a mixing console), else it reuses its faceplate.
// Whether a device is *focusable at all* is decided by focus.ts (pure, unit-tested); this module only
// maps a focusable type to its surface component.

import type { Component } from "svelte";
import type { ParamDescriptor, PortDescriptor, ReadoutDescriptor } from "./catalog";
import Console from "./widgets/Console.svelte";
import Panel from "./widgets/Panel.svelte";
import SynthVoice from "./widgets/SynthVoice.svelte";

/** The props every faceplate/focus-surface component accepts — the same shape `App` feeds `Panel`
 *  today (the generic `Panel` additionally accepts an optional `children` embellishment, so it remains
 *  assignable here). Values/readings/edits flow through the per-device closures `App` builds. */
export interface DeviceUiProps {
  /** Device instance id — tags jacks so the cable layer can locate them. */
  device: string;
  /** Catalog type id — selects the skin. */
  typeId: string;
  name: string;
  params: ParamDescriptor[];
  ports: PortDescriptor[];
  readouts?: ReadoutDescriptor[];
  /** Whether the back panel faces the operator. */
  flipped?: boolean;
  valueFor: (id: number) => number;
  readingFor?: (id: number) => number;
  onParam: (p: ParamDescriptor, value: number) => void;
}

/** In-world faceplates by type; a device without one falls back to the generic `Panel`. */
const FACEPLATES: Record<string, Component<DeviceUiProps>> = {
  synth_voice: SynthVoice,
};

/** The faceplate component for a device type — its own, or the generic `Panel`. */
export function deviceUi(typeId: string): Component<DeviceUiProps> {
  return FACEPLATES[typeId] ?? Panel;
}

/** Dedicated focus surfaces by type (shown large in the focus overlay). Keys must match focus.ts's
 *  `DEDICATED_FOCUS_SURFACES` (the focusability authority). A type not listed reuses its faceplate. */
const FOCUS_SURFACES: Record<string, Component<DeviceUiProps>> = {
  channel_strip: Console,
};

/** The focus-surface component for a device type — its dedicated surface, else its in-world faceplate. */
export function focusUi(typeId: string): Component<DeviceUiProps> {
  return FOCUS_SURFACES[typeId] ?? deviceUi(typeId);
}
