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
import type {
  ConfigDescriptor,
  ParamDescriptor,
  PortDescriptor,
  ReadoutDescriptor,
} from "./catalog";
import type { TransportState } from "./engine";
import type { TrackUi } from "./scene-store";
import type { TakeWaveform } from "./waveform";
import Computer from "./widgets/Computer.svelte";
import ComputerMixer from "./widgets/ComputerMixer.svelte";
import Console from "./widgets/Console.svelte";
import FocusriteControl from "./widgets/FocusriteControl.svelte";
import MidiController from "./widgets/MidiController.svelte";
import MidiControllerFocus from "./widgets/MidiControllerFocus.svelte";
import Panel from "./widgets/Panel.svelte";
import Scarlett8i6 from "./widgets/Scarlett8i6.svelte";
import Speaker from "./widgets/Speaker.svelte";
import SynthVoice from "./widgets/SynthVoice.svelte";

/** The DAW control seam a `computer`'s mixer surface drives (Story 5.11.6): live transport state + the
 *  per-track model, plus the transport/track commands. Wired to the session by `App`/`Workbench` for the
 *  focused DAW device only; other surfaces leave it unset. `transport`/`tracks`/`sends` are reactive
 *  reads (getters), so the surface re-renders as the playhead advances and the model changes. */
export interface DawUi {
  /** Live transport state (playhead + rolling/recording), or undefined before the first tick. */
  readonly transport: TransportState | undefined;
  /** The per-track model (arm/monitor/input/level/name), index = track number. */
  readonly tracks: TrackUi[];
  /** The USB send-lane count — the range of a track's input selector. */
  readonly sends: number;
  /** A track's waveform (thumbnail + sample length), or undefined if it has no take. Display-only. */
  waveform(track: number): TakeWaveform | undefined;
  play(): void;
  stop(): void;
  setRecordEnabled(on: boolean): void;
  seek(pos: number): void;
  setTrackInput(track: number, lane: number): void;
  setTrackArmed(track: number, armed: boolean): void;
  setTrackMonitoring(track: number, on: boolean): void;
  setTrackLevel(track: number, level: number): void;
  setTrackCount(count: number): void;
}

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
  configs?: ConfigDescriptor[];
  /** Whether the back panel faces the operator. */
  flipped?: boolean;
  valueFor: (id: number) => number;
  readingFor?: (id: number) => number;
  onParam: (p: ParamDescriptor, value: number) => void;
  /** Current value of a structural config key (build default if unset). */
  configFor?: (key: string) => number;
  /** Set a structural config key — edits the scene and rebuilds the engine (recompile). */
  onConfig?: (key: string, value: number) => void;
  // --- Note-play seam (focus render only) -------------------------------------------------------------
  // A playable focus surface (the MIDI controller) composes an on-screen `Keybed` from these. The
  // in-world faceplate leaves them unset; `App`/`Workbench` populate them for the *focused* device only.
  /** MIDI notes currently sounding (from mouse/QWERTY/MIDI), for the keybed's key highlight. */
  heldNotes?: number[];
  /** Whether this device's events input is cable-driven (a patched controller performs it) — the keybed
   *  renders inert, since host notes would be a no-op. */
  notesDriven?: boolean;
  /** Play (`on=true`) / release a note on this device — the keybed calls this per key press. */
  onNote?: (on: boolean, note: number) => void;
  // --- DAW seam (the computer's mixer focus surface only) ---------------------------------------------
  /** The transport/track control surface, for a DAW device's mixer. Unset for every other surface. */
  daw?: DawUi;
}

/** In-world faceplates by type; a device without one falls back to the generic `Panel`. */
const FACEPLATES: Record<string, Component<DeviceUiProps>> = {
  synth_voice: SynthVoice,
  scarlett_8i6: Scarlett8i6,
  computer: Computer,
  speaker: Speaker,
  midi_controller: MidiController,
};

/** The faceplate component for a device type — its own, or the generic `Panel`. */
export function deviceUi(typeId: string): Component<DeviceUiProps> {
  return FACEPLATES[typeId] ?? Panel;
}

/** Dedicated focus surfaces by type (shown large in the focus overlay). Keys must match focus.ts's
 *  `DEDICATED_FOCUS_SURFACES` (the focusability authority). A type not listed reuses its faceplate. */
const FOCUS_SURFACES: Record<string, Component<DeviceUiProps>> = {
  channel_strip: Console,
  scarlett_8i6: FocusriteControl,
  computer: ComputerMixer,
  midi_controller: MidiControllerFocus,
};

/** The focus-surface component for a device type — its dedicated surface, else its in-world faceplate. */
export function focusUi(typeId: string): Component<DeviceUiProps> {
  return FOCUS_SURFACES[typeId] ?? deviceUi(typeId);
}

/** Whether a type has a *dedicated* (software) focus surface, vs reusing its physical faceplate. The focus
 *  overlay renders a dedicated surface at its own UI scale, but **magnifies** a physical faceplate (which
 *  is now sized in real mm) so it reads large — a zoomed physical view. */
export function hasFocusSurface(typeId: string): boolean {
  return typeId in FOCUS_SURFACES;
}
