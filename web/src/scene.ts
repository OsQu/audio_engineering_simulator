// Hand-written TS mirror of the Rust patch IR (crates/wasm-bindings/src/scene.rs).
//
// Story 4.1. A *scene* (the whole studio, owned + persisted by this UI as versioned JSON) projects
// down to a runnable `Patch` — the engine-facing subset: devices, their param values, the
// connections, and the output tap. The UI posts a `Patch` to the worklet, where Rust deserializes it
// (serde-wasm-bindgen) to build the graph. These interfaces are the wire contract; the Rust structs
// carry `#[serde(rename_all = "camelCase")]`, so the field names here must match exactly. Keep the two
// in sync (a `wire_format_is_camel_case` test on the Rust side guards the casing).

/** A runnable patch: devices, the connections between them, and the output tap to render. */
export interface Patch {
	devices: DeviceInstance[];
	connections: Connection[];
	output: PortRef;
}

/** One placed device: a catalog `typeId` at a stable instance `id`, with param overrides. */
export interface DeviceInstance {
	/** Stable instance id (UI-assigned); referenced by connections and the output tap. */
	id: string;
	/** Catalog type id — selects the device's descriptor + builder. */
	typeId: string;
	/** Param values to apply after build. Omit for construction defaults. */
	params?: ParamSetting[];
}

/** A value for one of a device's smoothed control params, by device-local param id. */
export interface ParamSetting {
	/** Device-local param id (the engine `ParamId`'s value). */
	id: number;
	/** Target value; clamped to the param's declared range when applied. */
	value: number;
}

/** A connection from one device port to another, optionally through a cable. */
export interface Connection {
	from: PortRef;
	to: PortRef;
	/** Optional cable (series R + shunt C); omit for an ideal wire. */
	cable?: CableSpec;
}

/** A reference to one device-level port: a device instance id + the port index. */
export interface PortRef {
	/** Target device instance id (matches a `DeviceInstance.id`). */
	device: string;
	/** Device-level port index. */
	port: number;
}

/** A cable's electrical spec, in SI units. */
export interface CableSpec {
	resistanceOhms: number;
	capacitanceFarads: number;
}
