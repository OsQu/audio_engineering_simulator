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
  /** Smoothed control params, in id order. */
  params: ParamDescriptor[];
  /** Ports (inputs then outputs); each id is scoped to its direction. */
  ports: PortDescriptor[];
}

/** One control param: engine truth (id/min/max/default) + UI labels. */
export interface ParamDescriptor {
  /** Device-local param id — what a scene's `ParamSetting` addresses. */
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
  kind: PortKind;
}

/** Suggested control widget. */
export type ParamKind = "knob" | "fader" | "switch";

/** Whether a port is an input or an output. */
export type PortDirection = "input" | "output";

/** A port's carrier domain. */
export type PortDomain = "analog" | "digital" | "events";

/** Connector kind, for jack styling and connection-legality hints (UI-only; engine validates by domain). */
export type PortKind = "mic" | "line" | "instrument" | "speaker" | "digital" | "midi";

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
