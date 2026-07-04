// ============================================================================
// Device skins — the *aesthetic* layer, owned entirely by the UI.
// ----------------------------------------------------------------------------
// The Rust `devices` catalog declares a device's *capabilities* (its params,
// ports, form factor, dimensions) — the truth. It says nothing about how the
// device *looks*. This file is the other half: it maps a catalog `typeId` to a
// visual skin (faceplate finish + knob cap finish), so the UI can dress each
// device without the engine ever knowing about colour.
//
// Finish/cap keys are derived from the token definitions (tokens.ts) so a skin
// can only name a finish/cap that actually exists.
// ============================================================================

import type { KNOB_CAP, PANEL_FINISH } from "./tokens";

/** A device faceplate finish (see PANEL_FINISH in tokens.ts): grey | slate | black. */
export type Finish = keyof typeof PANEL_FINISH;
/** A knob cap finish (see KNOB_CAP in tokens.ts): dark | chrome | red | blue | cream. */
export type CapFinish = keyof typeof KNOB_CAP;

export interface DeviceSkin {
  /** Faceplate finish for the front panel. */
  finish: Finish;
  /** Default knob cap finish for this device's controls. */
  cap: CapFinish;
  /** Optional per-param cap override, keyed by the device-local param id. */
  caps?: Record<number, CapFinish>;
  /** Optional brand accent (a CSS colour) — the chassis outline in every view (the faceplate border and
   *  the top-down floor-plan tile). Undefined ⇒ the neutral chassis edge. */
  accent?: string;
}

/** Fallback for any device type without an explicit skin. */
const DEFAULT_SKIN: DeviceSkin = { finish: "slate", cap: "dark" };

/** Skins keyed by catalog `typeId`. Add an entry when a new device type lands. */
const SKINS: Record<string, DeviceSkin> = {
  synth_voice: { finish: "black", cap: "cream" },
  gain_stage: { finish: "slate", cap: "chrome" },
  channel_strip: { finish: "slate", cap: "dark" },
  three_band_eq: { finish: "grey", cap: "dark" },
  ad_converter: { finish: "black", cap: "chrome" },
  da_converter: { finish: "black", cap: "chrome" },
  // The Focusrite Scarlett look: a black faceplate with the signature red chassis (border + top-view tile).
  scarlett_8i6: { finish: "black", cap: "dark", accent: "#e6362b" },
  // The computer — a neutral slate box (it's a plain USB peer, not a piece of studio metal).
  computer: { finish: "slate", cap: "dark" },
};

/** The skin for a device type (falls back to a neutral slate skin). */
export function skinFor(typeId: string): DeviceSkin {
  return SKINS[typeId] ?? DEFAULT_SKIN;
}

/** The cap finish for one param — a per-param override if present, else the device default. */
export function capFor(skin: DeviceSkin, paramId: number): CapFinish {
  return skin.caps?.[paramId] ?? skin.cap;
}
