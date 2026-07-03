//! Device catalog + scene assembly — the product/content layer over the [`engine`].
//!
//! The engine provides *primitives* (parameterized node types, the graph, `compile`) but deliberately
//! has no opinion on **what gear exists** — that an "AD Converter" has a 1 MΩ input or that a "Gain
//! Stage" rails at 10 V is a product decision, not a law of physics. This crate is that decision: the
//! `catalog` of named device *types* (each a builder that constructs engine nodes + a UI
//! [`DeviceDescriptor`]), and the `scene` IR — the serializable runnable [`Patch`] the UI builds,
//! saves, and assembles back into an engine graph.
//!
//! It sits **on** the engine and is consumed by both `wasm-bindings` (the browser) and the native
//! `harness` (render scenarios), so none of it is JS-specific. serde lives here (the IR + descriptors
//! serialize); the `JsValue` bridge stays in `wasm-bindings`.

mod build;
mod cables;
mod catalog;
mod scene;

pub use build::{BuildError, BuiltScene, build_patch};
pub use cables::{CableType, cable_types};
pub use catalog::{
    BuiltDevice, Connector, DeviceDescriptor, ParamDescriptor, ParamKind, PortDescriptor,
    PortDirection, PortDomain, PortKind, connectors_compatible, descriptors, instantiate,
};
pub use scene::{CableSpec, Connection, DeviceInstance, ParamSetting, Patch, PortRef};
