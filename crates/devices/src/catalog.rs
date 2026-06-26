//! The device catalog (Story 4.1, Task 4.1.2): the registry of device *types* the UI can place.
//!
//! A catalog entry is a **pair** (the epic-wide "data-driven gear is UI-only" rule): a **builder**
//! — real Rust that constructs the engine node(s), the black-box transform — and a **descriptor** —
//! serde data the UI reads to draw the device and its controls. This Task ships **single-node**
//! devices (one entry → one [`Node`]); the chassis-group seam that lets one device expand to several
//! nodes is Task 4.1.3.
//!
//! **The descriptor is derived where it can be.** A param's id/range/default and a port's domain are
//! *engine truth*, so [`descriptors`] reads them straight off a freshly built node — they cannot
//! drift from the engine. Only the genuinely UI-only fields (display names, control labels/units, the
//! knob-vs-fader and mic-vs-line *kinds*) are hand-authored, in `UI_META`, positionally aligned to
//! the node's params/ports; a test pins that alignment.
//!
//! **Construction config is fixed per type** (settled at planning): the builder bakes realistic
//! electrical values (impedances, rails, the converter rate/bit-depth); only the node's smoothed
//! `params()` are user-facing. Field names are camelCase on the JS side, like the patch IR.

use engine::{
    AdConverter, BitDepth, DaConverter, Domain, EqBand, GainStage, InputZ, Node, Ohms, SampleRate,
    Speaker, SynthVoice, ThreeBandEq, Volts,
};
use serde::Serialize;

/// The fixed converter clock + word length the catalog's digital devices are built at — the same
/// 48 kHz / 16-bit as the canonical patch (`M = 384 kHz / 48 kHz = 8` against the analog rate).
const HOST_RATE_HZ: f64 = 48_000.0;
const BITS: u32 = 16;

/// A device type's full UI descriptor — everything the UI needs to list it in the catalog and draw
/// its panel. Built by [`descriptors`] (numeric/domain fields from the node, labels from `UI_META`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDescriptor {
    /// Stable catalog type id — the `typeId` a scene's `DeviceInstance` names.
    pub type_id: String,
    /// Human display name for the catalog/panel.
    pub name: String,
    /// The device's smoothed control params, in id order.
    pub params: Vec<ParamDescriptor>,
    /// The device's ports (inputs then outputs), each id scoped to its direction.
    pub ports: Vec<PortDescriptor>,
}

/// One control param: engine truth (`id`/`min`/`max`/`default`) + UI labels (`label`/`unit`/`kind`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamDescriptor {
    /// Device-local param id (the engine `ParamId`'s value) — what a `ParamSetting` addresses.
    pub id: u32,
    /// Display label (e.g. "Level", "Attack").
    pub label: String,
    /// Unit string for the readout (e.g. "V", "ms", "" for unitless).
    pub unit: String,
    /// Suggested control widget.
    pub kind: ParamKind,
    /// Lower bound (from the node's `ParamDecl`).
    pub min: f32,
    /// Upper bound (from the node's `ParamDecl`).
    pub max: f32,
    /// Default / construction value (from the node's `ParamDecl`).
    pub default: f32,
}

/// One port: direction + carrier domain (engine truth) + a UI label and connector kind.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortDescriptor {
    /// Port id **within its direction** — inputs are 0..n_in, outputs 0..n_out, exactly as the
    /// engine's `connect(from, out_port, to, in_port)` indexes them.
    pub id: u32,
    /// Display label (e.g. "In", "Out", "MIDI").
    pub label: String,
    /// Input or output.
    pub direction: PortDirection,
    /// Carrier domain (analog voltage / digital audio / events) — from the node's port face.
    pub domain: PortDomain,
    /// Connector kind for the UI (mic/line/instrument/speaker/digital/MIDI jack styling).
    pub kind: PortKind,
}

/// Suggested control widget for a param.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ParamKind {
    Knob,
    Fader,
    Switch,
}

/// Whether a port is an input or an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortDirection {
    Input,
    Output,
}

/// A port's carrier domain — the UI mirror of the engine's `Domain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortDomain {
    Analog,
    Digital,
    Events,
}

/// Connector kind, for jack styling and (Story 4.4) connection-legality hints. UI-only — the engine
/// validates by *domain*, not by this tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortKind {
    Mic,
    Line,
    Instrument,
    Speaker,
    Digital,
    Midi,
}

impl From<Domain> for PortDomain {
    fn from(d: Domain) -> Self {
        match d {
            Domain::Analog => Self::Analog,
            Domain::DigitalAudio => Self::Digital,
            Domain::Events => Self::Events,
        }
    }
}

/// The hand-authored UI metadata for one device type, positionally aligned to the node's
/// `params()` / `inputs()` / `outputs()`. Everything numeric is derived from the node, not here.
struct UiMeta {
    type_id: &'static str,
    name: &'static str,
    /// One per `params()`, in id order.
    params: &'static [ParamUi],
    /// One per `inputs()`, in declaration order.
    inputs: &'static [PortUi],
    /// One per `outputs()`, in declaration order.
    outputs: &'static [PortUi],
}

struct ParamUi {
    label: &'static str,
    unit: &'static str,
    kind: ParamKind,
}

struct PortUi {
    label: &'static str,
    kind: PortKind,
}

/// The catalog's UI metadata. Each entry's `params`/`inputs`/`outputs` lengths must match the node
/// [`build_node`] makes for the same `type_id` — `ui_meta_aligns_with_nodes` guards it.
const UI_META: &[UiMeta] = &[
    UiMeta {
        type_id: "synth_voice",
        name: "Synth Voice",
        params: &[
            ParamUi {
                label: "Level",
                unit: "V",
                kind: ParamKind::Fader,
            },
            ParamUi {
                label: "Attack",
                unit: "ms",
                kind: ParamKind::Knob,
            },
            ParamUi {
                label: "Decay",
                unit: "ms",
                kind: ParamKind::Knob,
            },
            ParamUi {
                label: "Sustain",
                unit: "",
                kind: ParamKind::Knob,
            },
            ParamUi {
                label: "Release",
                unit: "ms",
                kind: ParamKind::Knob,
            },
        ],
        inputs: &[PortUi {
            label: "MIDI",
            kind: PortKind::Midi,
        }],
        outputs: &[PortUi {
            label: "Out",
            kind: PortKind::Instrument,
        }],
    },
    UiMeta {
        type_id: "gain_stage",
        name: "Gain Stage",
        params: &[ParamUi {
            label: "Gain",
            unit: "×",
            kind: ParamKind::Knob,
        }],
        inputs: &[PortUi {
            label: "In",
            kind: PortKind::Line,
        }],
        outputs: &[PortUi {
            label: "Out",
            kind: PortKind::Line,
        }],
    },
    UiMeta {
        type_id: "three_band_eq",
        name: "3-Band EQ",
        params: &[],
        inputs: &[PortUi {
            label: "In",
            kind: PortKind::Digital,
        }],
        outputs: &[PortUi {
            label: "Out",
            kind: PortKind::Digital,
        }],
    },
    UiMeta {
        type_id: "ad_converter",
        name: "AD Converter",
        params: &[],
        inputs: &[PortUi {
            label: "Analog In",
            kind: PortKind::Line,
        }],
        outputs: &[PortUi {
            label: "Digital Out",
            kind: PortKind::Digital,
        }],
    },
    UiMeta {
        type_id: "da_converter",
        name: "DA Converter",
        params: &[],
        inputs: &[PortUi {
            label: "Digital In",
            kind: PortKind::Digital,
        }],
        outputs: &[PortUi {
            label: "Analog Out",
            kind: PortKind::Line,
        }],
    },
    UiMeta {
        type_id: "speaker",
        name: "Speaker",
        params: &[],
        inputs: &[PortUi {
            label: "In",
            kind: PortKind::Speaker,
        }],
        outputs: &[PortUi {
            label: "Tap",
            kind: PortKind::Speaker,
        }],
    },
];

/// Build the engine node for `type_id` with its fixed construction config, or `None` if unknown.
///
/// Single-node devices (Task 4.1.2): one `type_id` → one boxed [`Node`]. Returns `Box<dyn Node>` so
/// the one construction site serves both graph insertion (`Graph::add_boxed`, Task 4.1.4) and
/// descriptor introspection here. The electrical values are the realistic, fixed-per-type config.
#[must_use]
pub fn build_node(type_id: &str) -> Option<Box<dyn Node>> {
    let host = SampleRate::new(HOST_RATE_HZ);
    let bits = BitDepth::new(BITS);
    Some(match type_id {
        "synth_voice" => Box::new(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0))),
        "gain_stage" => Box::new(GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )),
        "three_band_eq" => Box::new(ThreeBandEq::new(
            host,
            bits,
            // Transparent default (all 0 dB): the UI sets bands later. Frequencies are the usual
            // low-shelf / mid-peak / high-shelf split.
            EqBand::new(100.0, 0.7, 0.0),
            EqBand::new(1_000.0, 0.7, 0.0),
            EqBand::new(8_000.0, 0.7, 0.0),
        )),
        "ad_converter" => Box::new(AdConverter::new(
            host,
            bits,
            Volts::new(1.0),
            Ohms::new(1_000_000.0),
        )),
        "da_converter" => Box::new(DaConverter::new(
            host,
            bits,
            Volts::new(1.0),
            Ohms::new(150.0),
        )),
        "speaker" => Box::new(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0)))),
        _ => return None,
    })
}

/// The full catalog of device descriptors, one per `UI_META` entry. Each is built by reading a
/// fresh node's params/ports (engine truth) and zipping the UI labels onto them. Cold path (called
/// once at UI startup); the node-building cost is negligible.
#[must_use]
pub fn descriptors() -> Vec<DeviceDescriptor> {
    UI_META.iter().map(describe).collect()
}

/// Build one descriptor: numeric param fields + port domains from the node, labels from `meta`.
fn describe(meta: &UiMeta) -> DeviceDescriptor {
    let node = build_node(meta.type_id).expect("every UI_META type has a builder");

    let params = node
        .params()
        .iter()
        .zip(meta.params)
        .map(|(decl, ui)| ParamDescriptor {
            id: decl.id.0,
            label: ui.label.to_owned(),
            unit: ui.unit.to_owned(),
            kind: ui.kind,
            min: decl.min,
            max: decl.max,
            default: decl.default,
        })
        .collect();

    let inputs = node
        .inputs()
        .iter()
        .zip(meta.inputs)
        .enumerate()
        .map(|(i, (port, ui))| PortDescriptor {
            id: i as u32,
            label: ui.label.to_owned(),
            direction: PortDirection::Input,
            domain: port.domain().into(),
            kind: ui.kind,
        });
    let outputs = node
        .outputs()
        .iter()
        .zip(meta.outputs)
        .enumerate()
        .map(|(i, (port, ui))| PortDescriptor {
            id: i as u32,
            label: ui.label.to_owned(),
            direction: PortDirection::Output,
            domain: port.domain().into(),
            kind: ui.kind,
        });
    let ports = inputs.chain(outputs).collect();

    DeviceDescriptor {
        type_id: meta.type_id.to_owned(),
        name: meta.name.to_owned(),
        params,
        ports,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hand-authored UI metadata lines up, position-for-position, with the node the builder
    /// makes — equal counts of params, inputs, and outputs. This is the guard that a node gaining
    /// or losing a param/port without the catalog following is caught (the `zip` in `describe`
    /// would otherwise silently truncate).
    #[test]
    fn ui_meta_aligns_with_nodes() {
        for meta in UI_META {
            let node = build_node(meta.type_id).expect("UI_META type builds");
            assert_eq!(
                meta.params.len(),
                node.params().len(),
                "{} param count",
                meta.type_id
            );
            assert_eq!(
                meta.inputs.len(),
                node.inputs().len(),
                "{} input count",
                meta.type_id
            );
            assert_eq!(
                meta.outputs.len(),
                node.outputs().len(),
                "{} output count",
                meta.type_id
            );
        }
    }

    /// Each descriptor carries the node's real param ids/ranges/defaults (bit-exact, derived not
    /// retyped) and the node's real port domains — so the UI can never show a stale range or wire a
    /// wrong-domain port.
    #[test]
    fn descriptors_carry_engine_truth() {
        for desc in descriptors() {
            let node = build_node(&desc.type_id).expect("descriptor type builds");

            for (pd, decl) in desc.params.iter().zip(node.params()) {
                assert_eq!(pd.id, decl.id.0, "{} param id", desc.type_id);
                // Bit-exact: these are derived from the decl, not hand-retyped, so identity holds.
                assert_eq!(pd.min.to_bits(), decl.min.to_bits(), "{} min", desc.type_id);
                assert_eq!(pd.max.to_bits(), decl.max.to_bits(), "{} max", desc.type_id);
                assert_eq!(
                    pd.default.to_bits(),
                    decl.default.to_bits(),
                    "{} default",
                    desc.type_id
                );
            }

            let n_in = desc
                .ports
                .iter()
                .filter(|p| p.direction == PortDirection::Input)
                .count();
            let n_out = desc
                .ports
                .iter()
                .filter(|p| p.direction == PortDirection::Output)
                .count();
            assert_eq!(n_in, node.inputs().len(), "{} inputs", desc.type_id);
            assert_eq!(n_out, node.outputs().len(), "{} outputs", desc.type_id);

            for (pd, port) in desc
                .ports
                .iter()
                .filter(|p| p.direction == PortDirection::Input)
                .zip(node.inputs())
            {
                assert_eq!(
                    pd.domain,
                    port.domain().into(),
                    "{} input domain",
                    desc.type_id
                );
            }
        }
    }

    /// An unknown type id has no builder (and so no descriptor) — the lookup the scene builder
    /// (Task 4.1.4) relies on to reject a bad `typeId` cleanly.
    #[test]
    fn unknown_type_has_no_builder() {
        assert!(build_node("does_not_exist").is_none());
    }

    /// The catalog serializes (via JSON natively; the wasm bridge uses serde-wasm-bindgen) and
    /// exposes the expected type ids in camelCase — the contract the TS `DeviceDescriptor` mirror
    /// consumes.
    #[test]
    fn catalog_serializes_with_expected_types() {
        let json = serde_json::to_string(&descriptors()).expect("descriptors serialize");
        for type_id in [
            "synth_voice",
            "gain_stage",
            "three_band_eq",
            "ad_converter",
            "da_converter",
            "speaker",
        ] {
            assert!(json.contains(type_id), "catalog missing {type_id}");
        }
        // camelCase field names are the wire contract (matches the TS mirror).
        assert!(json.contains("typeId"));
    }
}
