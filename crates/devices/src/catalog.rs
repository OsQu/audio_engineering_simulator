//! The device catalog: the registry of device *types* the UI can place.
//!
//! The catalog is **one table** — [`CATALOG`] — with one self-contained [`CatalogEntry`] per device
//! type. Each entry bundles everything that defines a device in a single place (the "easy to add gear"
//! goal): its `type_id`, display name, its **node(s)** (real Rust builders — the black-box transform),
//! the **internal edges** wiring them, and the **UI metadata** for its panel. Adding a device is one
//! entry here.
//!
//! **One chassis → one-or-many nodes (the chassis-group seam).** A device expands into 1..N engine
//! nodes wired by internal edges; a physical multi-I/O box (a preamp + its converter, a channel
//! strip) is several nodes behind one logical device. The device's **exposed face** is derived by
//! convention: an input/output port is exposed when **no internal edge consumes it** (open ports,
//! in node order); all node params are exposed, concatenated in node order.
//! [`instantiate_with_assets`] expands an entry into a `Graph` and returns a [`BuiltDevice`] — the
//! map from device-level ports/params to concrete `(NodeId, …)` `build_patch` uses to remap
//! connections and resolve handles. A single-node device is the trivial case (no internal edges;
//! the one node's whole face is exposed).
//!
//! **Routing seam (extension points, not yet built).** [`instantiate_with_assets`] →
//! [`BuiltDevice`] is the stable boundary: callers never see *how* a device built itself, so richer
//! topologies stay additive behind it. Static [`InternalEdge`](spec::InternalEdge) data covers fixed topology (what
//! exists). *Build-time-parameterized* topology (an N-channel mixer, an interface with N preamps)
//! needs an imperative-builder variant of [`CatalogEntry`] + an optional structural-config field on
//! the scene `DeviceInstance`.
//! *Runtime-switchable* routing (bypass, mid/side, a routing matrix) is **not** a topology change — it
//! lives inside a node behind a control param (or is user-repatching → a graph edit + recompile).
//!
//! **The descriptor is derived where it can be.** A param's id/range/default and a port's domain are
//! *engine truth*, so [`descriptors`] reads them off freshly built nodes — they cannot drift from the
//! engine. Only the UI-only fields (display names, control labels/units, the knob-vs-fader and
//! mic-vs-line *kinds*) are hand-authored in the entry, positionally aligned to the exposed face; a
//! test pins that alignment.
//!
//! **Construction config is fixed per type:** builders bake realistic electrical values; only the
//! nodes' smoothed `params()` are user-facing. Field names are camelCase on the JS side.

mod descriptor;
mod expand;
mod spec;

mod acoustic_guitar;
mod ad_converter;
mod channel_strip;
mod computer;
mod condenser_mic;
mod da_converter;
mod digital_meter;
mod dynamic_mic;
mod electric_guitar;
mod gain_stage;
mod guitar;
mod luma_leveler;
mod midi_controller;
mod powered_monitor;
mod scarlett_8i6;
mod speaker;
mod synth_voice;
mod three_band_eq;
mod vox;
mod vu_meter;

#[cfg(test)]
mod tests;

pub use descriptor::{
    ConfigDescriptor, ConfigKind, ConfigOption, Connector, DeviceDescriptor, ParamDescriptor,
    ParamKind, ParamTaper, PortDescriptor, PortDirection, PortDomain, PortKind,
    connectors_compatible,
};
pub use expand::{BuiltDevice, describe_device, descriptors, instantiate_with_assets};
pub use spec::DeviceConfig;

use spec::CatalogEntry;

/// The device catalog: every type the UI can place, builders + descriptor together. Each entry's
/// `params`/`inputs`/`outputs` lengths must match its exposed face — `catalog_aligns_with_exposed_face`
/// guards it (the `zip` in `describe` would otherwise silently truncate).
const CATALOG: &[CatalogEntry] = &[
    synth_voice::SYNTH_VOICE,
    midi_controller::MIDI_CONTROLLER,
    electric_guitar::ELECTRIC_GUITAR,
    acoustic_guitar::ACOUSTIC_GUITAR,
    vox::VOX,
    condenser_mic::CONDENSER_MIC,
    dynamic_mic::DYNAMIC_MIC,
    gain_stage::GAIN_STAGE,
    three_band_eq::THREE_BAND_EQ,
    ad_converter::AD_CONVERTER,
    da_converter::DA_CONVERTER,
    speaker::SPEAKER,
    powered_monitor::POWERED_MONITOR,
    channel_strip::CHANNEL_STRIP,
    vu_meter::VU_METER,
    luma_leveler::LUMA_LEVELER,
    digital_meter::DIGITAL_METER,
    scarlett_8i6::SCARLETT_8I6,
    computer::COMPUTER,
];
