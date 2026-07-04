// Hand-written TS mirror of the Rust device catalog (crates/wasm-bindings/src/catalog.rs).
//
// The catalog is the registry of device *types* the UI can place. The worklet
// exposes it via the wasm `catalog()` export (a structured value, serde-wasm-bindgen); these types
// are what that value deserializes to. Numeric param fields and port domains are engine truth
// (derived from the node on the Rust side); the labels/units/kinds are authored. Field names are
// camelCase to match the Rust `#[serde(rename_all = "camelCase")]`; a Rust test pins the type ids.

/** Full UI descriptor for one device type — everything needed to list it and draw its panel. */
export interface DeviceDescriptor {
  /** Stable catalog type id — the `typeId` a scene's device instance names. */
  typeId: string;
  /** Human display name. */
  name: string;
  /** Physical form factor + size — the device's intrinsic dimensions, for spatial placement. */
  formFactor: FormFactor;
  /** Smoothed control params, in id order. */
  params: ParamDescriptor[];
  /** Ports (inputs then outputs); each id is scoped to its direction. */
  ports: PortDescriptor[];
  /** Scalar readouts (meter values read back over the node→host lane), in id order. Empty for a
   *  device that measures nothing. */
  readouts: ReadoutDescriptor[];
  /** Structural config toggles (e.g. a preamp's INST/hi-Z), which the UI renders and whose change
   *  triggers a rebuild. Empty for a device with no structural options. */
  configs: ConfigDescriptor[];
}

/** One structural config option: a key, a UI label, the control kind, and the value the device builds
 *  with when unset. Unlike a param (a smoothed runtime value), a config selects *how the device is
 *  built* — changing it recompiles the patch. */
export interface ConfigDescriptor {
  /** Structural config key — what a scene's `ConfigSetting` addresses. */
  key: string;
  label: string;
  kind: ConfigKind;
  /** Value the device builds with when the instance leaves this key unset. */
  default: number;
}

/** Suggested control widget for a structural config option. */
export type ConfigKind = "toggle";

/** A device's physical form factor and size (content, authored on the Rust catalog). Internally
 *  tagged by `kind`, matching the Rust `#[serde(tag = "kind")]` enum. Rackmount gear occupies U-slots
 *  in a rack; desktop gear has a free-standing footprint box (millimetres). */
export type FormFactor =
  | { kind: "rackmount"; rackUnits: number }
  | { kind: "desktop"; widthMm: number; heightMm: number; depthMm: number };

/** One control param: engine truth (id/min/max/default) + UI labels. */
export interface ParamDescriptor {
  /** Exposed param id — its position in the device's exposed param list, what a scene's `ParamSetting` addresses. */
  id: number;
  label: string;
  /** Unit string for the readout ("V", "ms", "" for unitless). */
  unit: string;
  kind: ParamKind;
  min: number;
  max: number;
  default: number;
}

/** One port: direction + carrier domain (engine truth) + UI label and connector kind. */
export interface PortDescriptor {
  /** Port id within its direction (inputs 0..n_in, outputs 0..n_out). */
  id: number;
  label: string;
  direction: PortDirection;
  domain: PortDomain;
  /** Lane count — digital **channels** (1 mono, N for a multichannel connector), analog conductors
   *  (1/2), 1 for events. Engine truth (the port's `lane_count`). Jacks with `channels > 1` get a badge. */
  channels: number;
  kind: PortKind;
  /** Physical connector shape — the hard constraint on what may plug in (see {@link Connector}). */
  connector: Connector;
  /** A **round-trip-latency** output: an edge from it carries one block of latency (a computer/DAW's
   *  playback trails its input). The build wires such edges delayed, letting a monitoring loop through
   *  the device close without a cycle. `false` for inputs and ordinary outputs. */
  delayed: boolean;
}

/** One scalar readout: engine truth (id) + UI label/unit. The host reads its live value back by
 *  `(deviceId, id)` and shows it on a meter. */
export interface ReadoutDescriptor {
  /** Device-local readout id — its position in the exposed readout list. */
  id: number;
  label: string;
  /** Unit string for the reading ("VU", "dBu", "dBFS"). */
  unit: string;
}

/** Suggested control widget. */
export type ParamKind = "knob" | "fader" | "switch";

/** Whether a port is an input or an output. */
export type PortDirection = "input" | "output";

/** A port's carrier domain. */
export type PortDomain = "analog" | "digital" | "events";

/** Signal-class of a jack, for styling/labelling (mic/line/instrument/…). Distinct from the physical
 *  {@link Connector} shape that governs whether two jacks can actually be joined. */
export type PortKind = "mic" | "line" | "instrument" | "speaker" | "digital" | "midi";

/** The **physical connector shape** a port (or cable end) presents — the hard constraint on what can
 *  plug into what (mirrors the Rust `Connector`; `quarterInch` unifies TS/TRS). `combo` is the XLR+TRS
 *  combo jack on an interface's front input; `usb`/`spdif` are specific digital connectors, `digital`
 *  the generic one. Two ports may only be joined when their connectors {@link connectorsCompatible
 *  match}; a signal-class/level mismatch is *not* rejected (that stays emergent from the voltage
 *  physics). Authoritatively enforced in `build_patch`; mirrored here for live patching feedback. */
export type Connector =
  | "quarterInch"
  | "xlr"
  | "combo"
  | "speakon"
  | "din5"
  | "digital"
  | "usb"
  | "spdif";

/** Whether two connectors can be physically joined. Same-connector always fits; the one asymmetric
 *  case is the **combo** jack, which accepts an XLR or a ¼" plug (either direction). Everything else is
 *  equality. Mirrors the Rust `connectors_compatible`. */
export function connectorsCompatible(a: Connector, b: Connector): boolean {
  const comboMates = (x: Connector, y: Connector): boolean =>
    x === "combo" && (y === "xlr" || y === "quarterInch");
  return a === b || comboMates(a, b) || comboMates(b, a);
}

/** A cable type the UI offers when wiring an analog connection — a realistic R·C preset (physical
 *  content authored on the Rust side, `crates/devices/src/cables.rs`) plus a connector kind for styling.
 *  Fetched via the wasm `cable_catalog()` export. The chosen R·C rides the connection as a `CableSpec`;
 *  the engine's loading loss + treble rolloff emerge from it. Realistic cables into today's low-Z sources
 *  are inaudible by design (the effect is a numeric oracle; audible payoff arrives with Epic 5's high-Z
 *  sources). Field names are camelCase, matching the Rust `#[serde(rename_all = "camelCase")]`. */
export interface CableType {
  /** Stable catalog id — what a cable picker selects. */
  typeId: string;
  /** Human display name (e.g. "Instrument Cable (6 m)"). */
  label: string;
  /** Connector kind, for cable/jack styling. */
  kind: PortKind;
  /** Physical connector shape — which jacks this cable can plug into (the picker filters on it). */
  connector: Connector;
  /** Nominal length in metres the R·C was authored at (display + length-scaling seam). */
  lengthM: number;
  /** Series resistance, ohms (the loading-divider term). */
  resistanceOhms: number;
  /** Shunt capacitance, farads (forms the treble-rolloff one-pole). */
  capacitanceFarads: number;
}

/** The descriptor for a `typeId`, or `undefined` if the catalog has no such type. */
export function descriptorFor(
  catalog: DeviceDescriptor[],
  typeId: string,
): DeviceDescriptor | undefined {
  return catalog.find((d) => d.typeId === typeId);
}

/** Whether a descriptor presents an event (MIDI/note) input — i.e. it's a playable instrument. */
export function isPlayable(desc: DeviceDescriptor): boolean {
  return desc.ports.some((p) => p.domain === "events" && p.direction === "input");
}

/** The build-time default for a structural config key, or `0` if the device has no such config. */
export function configDefault(desc: DeviceDescriptor, key: string): number {
  return desc.configs.find((c) => c.key === key)?.default ?? 0;
}
