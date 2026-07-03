//! The device catalog: the registry of device *types* the UI can place.
//!
//! The catalog is **one table** — [`CATALOG`] — with one self-contained [`CatalogEntry`] per device
//! type. Each entry bundles everything that defines a device in a single place (the "easy to add gear"
//! goal): its `type_id`, display name, its **node(s)** (real Rust builders — the black-box transform),
//! the **internal edges** wiring them, and the **UI metadata** for its panel. Adding a device is one
//! entry here.
//!
//! **One chassis → one-or-many nodes (the chassis-group seam).** A device expands into 1..N
//! engine nodes wired by internal edges; a physical multi-I/O box (a preamp + its converter, a channel
//! strip) is several nodes behind one logical device. The device's **exposed face** is derived by
//! convention: an input/output port is exposed when **no internal edge consumes it** (open ports, in
//! node order); all node params are exposed, concatenated in node order. [`instantiate`] expands an
//! entry into a `Graph` and returns a [`BuiltDevice`] — the map from device-level ports/params to
//! concrete `(NodeId, …)` `build_patch` uses to remap connections and resolve handles.
//! A single-node device is the trivial case (no internal edges; the one node's whole face is exposed).
//!
//! **Routing seam (extension points, not yet built).** [`instantiate`] → [`BuiltDevice`] is the stable
//! boundary: callers never see *how* a device built itself, so richer topologies stay additive behind
//! it. Static [`InternalEdge`] data covers fixed topology (what exists). *Build-time-parameterized*
//! topology (an N-channel mixer, an interface with N preamps) needs an imperative-builder variant of
//! [`CatalogEntry`] + an optional structural-config field on the scene `DeviceInstance`.
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

use engine::{
    AdConverter, BitDepth, DaConverter, DigitalMeter, Domain, EqBand, EventThru, GainStage, Graph,
    InputZ, Node, NodeId, Ohms, ParamId, ReadoutId, SampleRate, Speaker, SynthVoice, ThreeBandEq,
    Volts, VuMeter,
};
use serde::Serialize;

/// The fixed converter clock + word length the catalog's digital devices are built at — the same
/// 48 kHz / 16-bit as the canonical patch (`M = 384 kHz / 48 kHz = 8` against the analog rate).
const HOST_RATE_HZ: f64 = 48_000.0;
const BITS: u32 = 16;

/// A device type's full UI descriptor — everything the UI needs to list it in the catalog and draw
/// its panel. Built by [`descriptors`] (numeric/domain fields from the node, labels from the entry).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDescriptor {
    /// Stable catalog type id — the `typeId` a scene's `DeviceInstance` names.
    pub type_id: String,
    /// Human display name for the catalog/panel.
    pub name: String,
    /// Physical form factor + size — the device's intrinsic dimensions, for spatial placement.
    pub form_factor: FormFactor,
    /// The device's smoothed control params, in exposed-id order.
    pub params: Vec<ParamDescriptor>,
    /// The device's ports (exposed inputs then outputs), each id scoped to its direction.
    pub ports: Vec<PortDescriptor>,
    /// The device's scalar readouts (meter values the host reads back), in exposed-id order. Empty
    /// for a device that measures nothing.
    pub readouts: Vec<ReadoutDescriptor>,
}

/// A device's physical form factor and size — intrinsic **content** (as fixed as its impedance),
/// authored per catalog entry and consumed by the UI's spatial world. It governs placement:
/// rackmount gear occupies contiguous U-slots in a rack; desktop gear places freely on a surface.
/// The UI derives the rendered footprint (and the 3-D box) from this; the engine never sees it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FormFactor {
    /// 19"-rack gear: occupies `rack_units` contiguous U-slots (1U ≈ 44.45 mm tall, 482.6 mm wide).
    #[serde(rename_all = "camelCase")]
    Rackmount { rack_units: u32 },
    /// Free-standing desktop/floor gear with an authored footprint box, in millimetres.
    #[serde(rename_all = "camelCase")]
    Desktop {
        width_mm: f32,
        height_mm: f32,
        depth_mm: f32,
    },
}

/// One control param: engine truth (`id`/`min`/`max`/`default`) + UI labels (`label`/`unit`/`kind`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamDescriptor {
    /// Device-level param id (its index in the exposed param list) — what a `ParamSetting` addresses.
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
    /// Device-level port id **within its direction** — exposed inputs are 0..n_in, outputs 0..n_out,
    /// exactly as a scene's `PortRef` and the engine's `connect(from, out_port, to, in_port)` index them.
    pub id: u32,
    /// Display label (e.g. "In", "Out", "MIDI").
    pub label: String,
    /// Input or output.
    pub direction: PortDirection,
    /// Carrier domain (analog voltage / digital audio / events) — from the node's port face.
    pub domain: PortDomain,
    /// Connector kind for the UI (mic/line/instrument/speaker/digital/MIDI jack styling).
    pub kind: PortKind,
    /// Physical connector shape — the hard constraint on what may plug in (see [`Connector`]).
    pub connector: Connector,
}

/// One scalar readout a device exposes for the host to display (a meter value read back over the
/// node→host lane). Engine truth is the `id` (its position in the device's exposed readout list);
/// `label`/`unit` are hand-authored UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadoutDescriptor {
    /// Device-level readout id — its index in the exposed readout list, what the host reads back.
    pub id: u32,
    /// Display label (e.g. "VU", "Peak").
    pub label: String,
    /// Unit string for the reading (e.g. "VU", "dBu", "dBFS").
    pub unit: String,
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

/// Connector kind, for jack styling and connection-legality hints. This is the *signal class*
/// (mic/line/instrument/…) a jack presents, which drives colour/labelling — **not** the physical
/// connector shape. Whether two jacks can actually be joined is governed by [`Connector`], not this.
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

/// The **physical connector shape** a port (or cable end) presents — the hard, mechanical constraint on
/// what can plug into what, distinct from the signal-class [`PortKind`]. Two ports may only be joined
/// when their connectors are [compatible](connectors_compatible); a level/signal-class mismatch (a mic
/// into a line input) is *not* rejected here — it stays emergent from the voltage physics, per the
/// project's "don't flag what should emerge" rule. Only shape incompatibility (an XLR into a ¼" hole)
/// is a genuine impossibility and rejected.
///
/// Deliberately coarse and extensible — only the connectors today's catalog needs. `QuarterInch`
/// unifies TS and TRS: they share the same jack (a TS plug seats in a TRS socket, just unbalanced), so
/// they are one connector here. `Xlr`/`Speakon`/`Digital` are carried forward for gear that lands in
/// Epic 5 (balanced mics, power amps, multi-format digital).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Connector {
    /// ¼" (6.35 mm) phone jack — TS and TRS unified (same hole). Instrument + line gear.
    QuarterInch,
    /// 3-pin XLR — balanced mic / line.
    Xlr,
    /// speakON / binding-post speaker connector.
    Speakon,
    /// 5-pin DIN — MIDI (events domain).
    Din5,
    /// A digital-audio connector (one entry for now; SPDIF/ADAT/AES split lands in Epic 5).
    Digital,
}

/// Whether two connectors can be physically joined. The whole rule: they must be the *same* connector
/// (TS/TRS are already unified under [`Connector::QuarterInch`]). Adapters/hybrid leads (XLR↔TRS) are
/// out of scope. Domain compatibility is a *separate* check the engine owns at compile.
#[must_use]
pub fn connectors_compatible(a: Connector, b: Connector) -> bool {
    a == b
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

/// An edge between two of a device's internal nodes (by their index in [`CatalogEntry::nodes`]):
/// `from_node`'s output port `from_port` → `to_node`'s input port `to_port`. An ideal internal wire
/// (PCB trace, no cable). A port touched by an internal edge is **hidden** from the device's face.
struct InternalEdge {
    from_node: usize,
    from_port: usize,
    to_node: usize,
    to_port: usize,
}

/// One device type in the catalog — the single place a device is defined. Bundles its identity, its
/// **node builders** + **internal edges** (the chassis), and the **UI metadata** for its panel. The
/// metadata is positionally aligned to the *exposed* face (open ports in node order; all params
/// concatenated); everything numeric (ranges, domains) is *derived* from the nodes by [`descriptors`].
struct CatalogEntry {
    type_id: &'static str,
    name: &'static str,
    /// Physical form factor + size — intrinsic content, hand-authored like the labels.
    form_factor: FormFactor,
    /// The internal node(s), in order; each builds one engine node with fixed config. Length 1 is the
    /// single-node case.
    nodes: &'static [fn() -> Box<dyn Node>],
    /// Edges wiring the internal nodes. Empty for a single-node device.
    internal: &'static [InternalEdge],
    /// One per *exposed* param (all node params, concatenated in node order).
    params: &'static [ParamUi],
    /// One per *exposed* input port (open inputs, in node order).
    inputs: &'static [PortUi],
    /// One per *exposed* output port (open outputs, in node order).
    outputs: &'static [PortUi],
    /// One per *exposed* readout (all node readouts, concatenated in node order). Empty for a device
    /// that measures nothing.
    readouts: &'static [ReadoutUi],
}

struct ParamUi {
    label: &'static str,
    unit: &'static str,
    kind: ParamKind,
}

struct PortUi {
    label: &'static str,
    kind: PortKind,
    connector: Connector,
}

struct ReadoutUi {
    label: &'static str,
    unit: &'static str,
}

/// The device catalog: every type the UI can place, builders + descriptor together. Each entry's
/// `params`/`inputs`/`outputs` lengths must match its exposed face — `catalog_aligns_with_exposed_face`
/// guards it (the `zip` in `describe` would otherwise silently truncate).
const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        type_id: "synth_voice",
        name: "Synth Voice",
        form_factor: FormFactor::Desktop {
            width_mm: 600.0,
            height_mm: 90.0,
            depth_mm: 300.0,
        },
        nodes: &[|| Box::new(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)))],
        internal: &[],
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
            ParamUi {
                label: "Power",
                unit: "",
                kind: ParamKind::Switch,
            },
        ],
        inputs: &[PortUi {
            label: "MIDI",
            kind: PortKind::Midi,
            connector: Connector::Din5,
        }],
        outputs: &[PortUi {
            label: "Out",
            kind: PortKind::Instrument,
            connector: Connector::QuarterInch,
        }],
        readouts: &[],
    },
    // A standalone MIDI controller: a keybed with no sound of its own that *produces* a performance
    // and forwards it to MIDI-OUT (an `EventThru` — the identity event processor). Its MIDI-IN is the
    // open, host-fed input a human plays via the focus surface (or another controller patches into);
    // the cable from MIDI-OUT to a synth's MIDI-IN is the events connection the UI drives. No sound,
    // no params — pure event plumbing.
    CatalogEntry {
        type_id: "midi_controller",
        name: "MIDI Controller",
        form_factor: FormFactor::Desktop {
            width_mm: 800.0,
            height_mm: 80.0,
            depth_mm: 250.0,
        },
        nodes: &[|| Box::new(EventThru::new(64))],
        internal: &[],
        params: &[],
        inputs: &[PortUi {
            label: "MIDI In",
            kind: PortKind::Midi,
            connector: Connector::Din5,
        }],
        outputs: &[PortUi {
            label: "MIDI Out",
            kind: PortKind::Midi,
            connector: Connector::Din5,
        }],
        readouts: &[],
    },
    CatalogEntry {
        type_id: "gain_stage",
        name: "Gain Stage",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|| {
            Box::new(GainStage::new(
                1.0,
                Volts::new(10.0),
                InputZ::new(Ohms::new(10_000.0)),
                Ohms::new(150.0),
            ))
        }],
        internal: &[],
        params: &[
            ParamUi {
                label: "Gain",
                unit: "×",
                kind: ParamKind::Knob,
            },
            ParamUi {
                label: "Power",
                unit: "",
                kind: ParamKind::Switch,
            },
        ],
        inputs: &[PortUi {
            label: "In",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        }],
        outputs: &[PortUi {
            label: "Out",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        }],
        readouts: &[],
    },
    CatalogEntry {
        type_id: "three_band_eq",
        name: "3-Band EQ",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|| {
            Box::new(ThreeBandEq::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                // Transparent default (all 0 dB): the UI sets bands later. Frequencies are the usual
                // low-shelf / mid-peak / high-shelf split.
                EqBand::new(100.0, 0.7, 0.0),
                EqBand::new(1_000.0, 0.7, 0.0),
                EqBand::new(8_000.0, 0.7, 0.0),
            ))
        }],
        internal: &[],
        params: &[],
        inputs: &[PortUi {
            label: "In",
            kind: PortKind::Digital,
            connector: Connector::Digital,
        }],
        outputs: &[PortUi {
            label: "Out",
            kind: PortKind::Digital,
            connector: Connector::Digital,
        }],
        readouts: &[],
    },
    CatalogEntry {
        type_id: "ad_converter",
        name: "AD Converter",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|| {
            Box::new(AdConverter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                Volts::new(1.0),
                Ohms::new(1_000_000.0),
            ))
        }],
        internal: &[],
        params: &[],
        inputs: &[PortUi {
            label: "Analog In",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        }],
        outputs: &[PortUi {
            label: "Digital Out",
            kind: PortKind::Digital,
            connector: Connector::Digital,
        }],
        readouts: &[],
    },
    CatalogEntry {
        type_id: "da_converter",
        name: "DA Converter",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|| {
            Box::new(DaConverter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                Volts::new(1.0),
                Ohms::new(150.0),
            ))
        }],
        internal: &[],
        params: &[],
        inputs: &[PortUi {
            label: "Digital In",
            kind: PortKind::Digital,
            connector: Connector::Digital,
        }],
        outputs: &[PortUi {
            label: "Analog Out",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        }],
        readouts: &[],
    },
    CatalogEntry {
        type_id: "speaker",
        name: "Speaker",
        form_factor: FormFactor::Desktop {
            width_mm: 250.0,
            height_mm: 380.0,
            depth_mm: 300.0,
        },
        nodes: &[|| Box::new(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))))],
        internal: &[],
        params: &[],
        inputs: &[PortUi {
            label: "In",
            kind: PortKind::Speaker,
            // The speaker is a simplified powered-monitor terminus fed by the line-level DA, so its
            // input is a ¼" line jack today — this keeps the default `da→spk` connection legal. A
            // Speakon input arrives with an Epic-5 power amp + passive speaker.
            connector: Connector::QuarterInch,
        }],
        outputs: &[PortUi {
            label: "Tap",
            kind: PortKind::Speaker,
            connector: Connector::QuarterInch,
        }],
        readouts: &[],
    },
    // The minimal **multi-node** device, proving the chassis seam: two analog gain stages in series
    // (input gain → output gain) behind one logical device. The internal edge hides stage 0's output
    // and stage 1's input; the exposed face is stage 0's input, stage 1's output, and *both* gains'
    // params (so device param 1 maps to the second node — a non-trivial remap). A strip with
    // EQ/dynamics would need an internal AD (analog can't enter a digital port), so two analog stages
    // is the smallest electrically valid multi-node device.
    CatalogEntry {
        type_id: "channel_strip",
        name: "Channel Strip",
        form_factor: FormFactor::Rackmount { rack_units: 2 },
        nodes: &[
            || {
                Box::new(GainStage::new(
                    1.0,
                    Volts::new(10.0),
                    InputZ::new(Ohms::new(10_000.0)),
                    Ohms::new(150.0),
                ))
            },
            || {
                Box::new(GainStage::new(
                    1.0,
                    Volts::new(10.0),
                    InputZ::new(Ohms::new(10_000.0)),
                    Ohms::new(150.0),
                ))
            },
        ],
        internal: &[InternalEdge {
            from_node: 0,
            from_port: 0,
            to_node: 1,
            to_port: 0,
        }],
        // Params are exposed in node order — both of stage 0's, then both of stage 1's — so the
        // interleave is gain, power, gain, power (each stage carries its own power switch).
        params: &[
            ParamUi {
                label: "Input Gain",
                unit: "×",
                kind: ParamKind::Knob,
            },
            ParamUi {
                label: "Input Power",
                unit: "",
                kind: ParamKind::Switch,
            },
            ParamUi {
                label: "Output Gain",
                unit: "×",
                kind: ParamKind::Knob,
            },
            ParamUi {
                label: "Output Power",
                unit: "",
                kind: ParamKind::Switch,
            },
        ],
        inputs: &[PortUi {
            label: "In",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        }],
        outputs: &[PortUi {
            label: "Out",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        }],
        readouts: &[],
    },
    // A voltage-native VU meter — bridging inline analog meter (unity passthrough). Its two readouts
    // ride the node→host lane: the ballistic VU reading and the block peak in dBu. The analog half of
    // "gain-staging across the AD/DA boundary made visible".
    CatalogEntry {
        type_id: "vu_meter",
        name: "VU Meter",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|| Box::new(VuMeter::new())],
        internal: &[],
        params: &[],
        inputs: &[PortUi {
            label: "In",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        }],
        outputs: &[PortUi {
            label: "Thru",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        }],
        readouts: &[
            ReadoutUi {
                label: "VU",
                unit: "VU",
            },
            ReadoutUi {
                label: "Peak",
                unit: "dBu",
            },
        ],
    },
    // A digital level meter — inline passthrough on a digital channel, reporting peak and RMS in
    // dBFS. Placed after the AD, it's the digital half of the across-converter gain-staging story.
    CatalogEntry {
        type_id: "digital_meter",
        name: "Digital Meter",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|| {
            Box::new(DigitalMeter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
            ))
        }],
        internal: &[],
        params: &[],
        inputs: &[PortUi {
            label: "In",
            kind: PortKind::Digital,
            connector: Connector::Digital,
        }],
        outputs: &[PortUi {
            label: "Thru",
            kind: PortKind::Digital,
            connector: Connector::Digital,
        }],
        readouts: &[
            ReadoutUi {
                label: "Peak",
                unit: "dBFS",
            },
            ReadoutUi {
                label: "RMS",
                unit: "dBFS",
            },
        ],
    },
];

/// A built device's footprint in a graph: its engine nodes and the resolved maps from device-level
/// ports/params to concrete `(NodeId, …)`. Built by [`instantiate`]; consumed by `build_patch` to
/// remap inter-device connections to graph edges and to resolve control handles.
///
/// `inputs`/`outputs` are indexed by **device-level port id** (the same index a scene's `PortRef`
/// uses); `params` by **device-level param id**. An event input is just an input port whose node port
/// is `Events`-domain — resolve it to an `EventInputId` via `Schedule::event_input(node, port)`.
#[derive(Debug, Clone)]
pub struct BuiltDevice {
    /// The engine nodes this device expanded into, in entry order.
    pub nodes: Vec<NodeId>,
    /// Device input port id → `(node, node input port)`.
    pub inputs: Vec<(NodeId, usize)>,
    /// Device output port id → `(node, node output port)`.
    pub outputs: Vec<(NodeId, usize)>,
    /// Device param id → `(node, node ParamId)`.
    pub params: Vec<(NodeId, ParamId)>,
    /// Device readout id → `(node, node ReadoutId)`, in exposed (position) order — the node→host
    /// mirror of `params`, resolved to `ReadoutHandle`s via `Schedule::readout(node, id)`.
    pub readouts: Vec<(NodeId, ReadoutId)>,
}

/// One exposed input/output port of a device, resolved against the built nodes: which internal node
/// + port it is, and its carrier domain.
struct ExposedPort {
    node: usize,
    port: usize,
    domain: Domain,
}

/// One exposed param: which internal node + `ParamId` it is, plus the decl's range/default (engine
/// truth, copied so the descriptor needn't re-introspect).
struct ExposedParam {
    node: usize,
    id: ParamId,
    min: f32,
    max: f32,
    default: f32,
}

/// One exposed readout: which internal node + `ReadoutId` it is (no range — a readout is a plain
/// scalar output).
struct ExposedReadout {
    node: usize,
    id: ReadoutId,
}

/// A device's built nodes plus its exposed face (open ports + all params + all readouts),
/// node-index-based. Shared by [`instantiate`] (maps node indices → `NodeId`) and [`describe`]
/// (reads domains + UI labels).
struct Expansion {
    nodes: Vec<Box<dyn Node>>,
    inputs: Vec<ExposedPort>,
    outputs: Vec<ExposedPort>,
    params: Vec<ExposedParam>,
    readouts: Vec<ExposedReadout>,
}

/// The catalog entry for `type_id`, or `None` if unknown.
fn entry(type_id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.type_id == type_id)
}

/// Build a device's nodes and compute its exposed face by convention: an input/output port is exposed
/// when no internal edge consumes it (open ports, in node order); every node param is exposed,
/// concatenated in node order. Cold path; the node-building cost is negligible.
fn expand(entry: &CatalogEntry) -> Expansion {
    let nodes: Vec<Box<dyn Node>> = entry.nodes.iter().map(|build| build()).collect();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut params = Vec::new();
    let mut readouts = Vec::new();

    for (ni, node) in nodes.iter().enumerate() {
        for (port, face) in node.inputs().iter().enumerate() {
            let consumed = entry
                .internal
                .iter()
                .any(|e| e.to_node == ni && e.to_port == port);
            if !consumed {
                inputs.push(ExposedPort {
                    node: ni,
                    port,
                    domain: face.domain(),
                });
            }
        }
        for (port, face) in node.outputs().iter().enumerate() {
            let consumed = entry
                .internal
                .iter()
                .any(|e| e.from_node == ni && e.from_port == port);
            if !consumed {
                outputs.push(ExposedPort {
                    node: ni,
                    port,
                    domain: face.domain(),
                });
            }
        }
        for decl in node.params() {
            params.push(ExposedParam {
                node: ni,
                id: decl.id,
                min: decl.min,
                max: decl.max,
                default: decl.default,
            });
        }
        for decl in node.readouts() {
            readouts.push(ExposedReadout {
                node: ni,
                id: decl.id,
            });
        }
    }

    Expansion {
        nodes,
        inputs,
        outputs,
        params,
        readouts,
    }
}

/// Expand the device type `type_id` into `g`: add its node(s), wire its internal edges, and return the
/// instance map (device-level ports/params → concrete `(NodeId, …)`). `None` if the type is unknown.
///
/// The chassis-seam primitive: `build_patch` calls this per device, then uses the returned
/// [`BuiltDevice`] to remap inter-device connections and resolve control handles.
pub fn instantiate(type_id: &str, g: &mut Graph) -> Option<BuiltDevice> {
    let entry = entry(type_id)?;
    let Expansion {
        nodes,
        inputs,
        outputs,
        params,
        readouts,
    } = expand(entry);

    let node_ids: Vec<NodeId> = nodes.into_iter().map(|node| g.add_boxed(node)).collect();
    for edge in entry.internal {
        g.connect_ideal(
            node_ids[edge.from_node],
            edge.from_port,
            node_ids[edge.to_node],
            edge.to_port,
        );
    }

    Some(BuiltDevice {
        inputs: inputs.iter().map(|p| (node_ids[p.node], p.port)).collect(),
        outputs: outputs.iter().map(|p| (node_ids[p.node], p.port)).collect(),
        params: params.iter().map(|p| (node_ids[p.node], p.id)).collect(),
        readouts: readouts.iter().map(|r| (node_ids[r.node], r.id)).collect(),
        nodes: node_ids,
    })
}

/// The full catalog of device descriptors, one per `CATALOG` entry. Each is built by reading freshly
/// built nodes' exposed face (engine truth) and zipping the entry's UI labels onto it. Cold path
/// (called once at UI startup).
#[must_use]
pub fn descriptors() -> Vec<DeviceDescriptor> {
    CATALOG.iter().map(describe).collect()
}

/// Build one descriptor: numeric param fields + port domains from the exposed face, labels from the entry.
fn describe(entry: &CatalogEntry) -> DeviceDescriptor {
    let face = expand(entry);

    // Params, exposed (position) order — the id is the position, matching how the host addresses a
    // param (`BuiltScene::param(device, id)` indexes the exposed handle vec), not the node-local
    // `ParamId`. For a multi-node device the two differ: `channel_strip`'s stages both expose
    // `ParamId` 0/1, so node-local ids would collide at `[0,1,0,1]` and misaddress the second stage.
    let params = face
        .params
        .iter()
        .zip(entry.params)
        .enumerate()
        .map(|(i, (p, ui))| ParamDescriptor {
            id: i as u32,
            label: ui.label.to_owned(),
            unit: ui.unit.to_owned(),
            kind: ui.kind,
            min: p.min,
            max: p.max,
            default: p.default,
        })
        .collect();

    let inputs = face
        .inputs
        .iter()
        .zip(entry.inputs)
        .enumerate()
        .map(|(i, (p, ui))| PortDescriptor {
            id: i as u32,
            label: ui.label.to_owned(),
            direction: PortDirection::Input,
            domain: p.domain.into(),
            kind: ui.kind,
            connector: ui.connector,
        });
    let outputs = face
        .outputs
        .iter()
        .zip(entry.outputs)
        .enumerate()
        .map(|(i, (p, ui))| PortDescriptor {
            id: i as u32,
            label: ui.label.to_owned(),
            direction: PortDirection::Output,
            domain: p.domain.into(),
            kind: ui.kind,
            connector: ui.connector,
        });
    let ports = inputs.chain(outputs).collect();

    // Readouts, exposed (position) order — the id is the position, matching how the host addresses a
    // reading (`BuiltScene::readout(device, id)`), not the node-local `ReadoutId`.
    let readouts = face
        .readouts
        .iter()
        .zip(entry.readouts)
        .enumerate()
        .map(|(i, (_, ui))| ReadoutDescriptor {
            id: i as u32,
            label: ui.label.to_owned(),
            unit: ui.unit.to_owned(),
        })
        .collect();

    DeviceDescriptor {
        type_id: entry.type_id.to_owned(),
        name: entry.name.to_owned(),
        form_factor: entry.form_factor,
        params,
        ports,
        readouts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each entry's hand-authored UI metadata lines up, position-for-position, with its *exposed*
    /// face — the open ports + concatenated params its nodes actually present. The guard against a node
    /// (or an internal edge) changing what's exposed without the catalog following; the `zip` in
    /// `describe` would otherwise silently truncate.
    #[test]
    fn catalog_aligns_with_exposed_face() {
        for entry in CATALOG {
            let face = expand(entry);
            assert_eq!(
                entry.params.len(),
                face.params.len(),
                "{} params",
                entry.type_id
            );
            assert_eq!(
                entry.inputs.len(),
                face.inputs.len(),
                "{} inputs",
                entry.type_id
            );
            assert_eq!(
                entry.outputs.len(),
                face.outputs.len(),
                "{} outputs",
                entry.type_id
            );
            assert_eq!(
                entry.readouts.len(),
                face.readouts.len(),
                "{} readouts",
                entry.type_id
            );
        }
    }

    /// Each descriptor carries the exposed param id (its **position** in the exposed face — what the
    /// host addresses, matching ports/readouts, *not* the node-local `ParamId`) plus the node's real
    /// ranges/defaults (bit-exact, derived not retyped) and real port domains — so the UI can never
    /// misaddress a param, show a stale range, or wire a wrong-domain port.
    #[test]
    fn descriptors_carry_engine_truth() {
        for entry in CATALOG {
            let face = expand(entry);
            let desc = describe(entry);

            for (i, (pd, ep)) in desc.params.iter().zip(&face.params).enumerate() {
                assert_eq!(pd.id, i as u32, "{} param id", entry.type_id);
                // Bit-exact: derived from the decl, not hand-retyped, so identity holds.
                assert_eq!(pd.min.to_bits(), ep.min.to_bits(), "{} min", entry.type_id);
                assert_eq!(pd.max.to_bits(), ep.max.to_bits(), "{} max", entry.type_id);
                assert_eq!(
                    pd.default.to_bits(),
                    ep.default.to_bits(),
                    "{} default",
                    entry.type_id
                );
            }

            let inputs: Vec<_> = desc
                .ports
                .iter()
                .filter(|p| p.direction == PortDirection::Input)
                .collect();
            assert_eq!(inputs.len(), face.inputs.len(), "{} inputs", entry.type_id);
            for (pd, ep) in inputs.iter().zip(&face.inputs) {
                assert_eq!(
                    pd.domain,
                    ep.domain.into(),
                    "{} input domain",
                    entry.type_id
                );
            }
        }
    }

    /// A multi-node device's descriptor param ids are the **exposed positions** (`0..n`), not the
    /// node-local `ParamId`s — which would collide. The two-stage `channel_strip` exposes both stages'
    /// gain (`ParamId(0)`) + power (`ParamId(1)`); node-local ids would be `[0,1,0,1]`, misaddressing
    /// the second stage (`BuiltScene::param` indexes positionally) and colliding as UI keys. The
    /// descriptor must instead expose `[0,1,2,3]`.
    #[test]
    fn multi_node_descriptor_param_ids_are_exposed_positions() {
        let strip = descriptors()
            .into_iter()
            .find(|d| d.type_id == "channel_strip")
            .expect("channel_strip is in the catalog");

        let ids: Vec<u32> = strip.params.iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            vec![0, 1, 2, 3],
            "exposed positions, not node-local ids"
        );
    }

    /// The chassis seam: a multi-node device expands into several nodes wired by its internal edge,
    /// and its exposed face maps to the right `(NodeId, …)`. The two-stage channel strip exposes
    /// stage 0's input and stage 1's output (input and output on *different* nodes), and each stage's
    /// gain + power params — concatenated in node order, so device params **2/3** resolve to the
    /// **second** node (a non-trivial remap, the case a naive "everything is node 0" impl would get
    /// wrong).
    #[test]
    fn multi_node_device_expands_and_maps() {
        let mut g = Graph::new();
        let strip = instantiate("channel_strip", &mut g).expect("channel_strip is in the catalog");

        assert_eq!(strip.nodes.len(), 2, "two internal nodes");
        assert_eq!(g.connection_count(), 1, "one internal edge wired");

        // Exposed input is stage 0's input; exposed output is stage 1's output.
        assert_eq!(strip.inputs, vec![(strip.nodes[0], 0)]);
        assert_eq!(strip.outputs, vec![(strip.nodes[1], 0)]);

        // Each stage's gain (ParamId 0) + power (ParamId 1) exposed, in node order — device params
        // 2/3 map to the *second* node.
        assert_eq!(
            strip.params,
            vec![
                (strip.nodes[0], ParamId(0)),
                (strip.nodes[0], ParamId(1)),
                (strip.nodes[1], ParamId(0)),
                (strip.nodes[1], ParamId(1)),
            ]
        );
    }

    /// A single-node device is the trivial case: one node, no internal edges, and the node's own face
    /// exposed as-is.
    #[test]
    fn single_node_device_is_identity() {
        let mut g = Graph::new();
        let spk = instantiate("speaker", &mut g).expect("speaker is in the catalog");

        assert_eq!(spk.nodes.len(), 1);
        assert_eq!(g.connection_count(), 0, "no internal edges");
        assert_eq!(spk.inputs, vec![(spk.nodes[0], 0)]);
        assert_eq!(spk.outputs, vec![(spk.nodes[0], 0)]);
    }

    /// An unknown type id has no entry — `instantiate` returns `None` (no nodes added), the lookup
    /// `build_patch` relies on to reject a bad `typeId` cleanly.
    #[test]
    fn unknown_type_does_not_instantiate() {
        let mut g = Graph::new();
        assert!(instantiate("does_not_exist", &mut g).is_none());
        assert_eq!(g.node_count(), 0, "nothing added for an unknown type");
    }

    /// The catalog serializes (via JSON natively; the wasm bridge uses serde-wasm-bindgen) and exposes
    /// the expected type ids in camelCase — the contract the TS `DeviceDescriptor` mirror consumes.
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
            "channel_strip",
            "vu_meter",
            "digital_meter",
        ] {
            assert!(json.contains(type_id), "catalog missing {type_id}");
        }
        // camelCase field names are the wire contract (matches the TS mirror).
        assert!(json.contains("typeId"));
        assert!(json.contains("readouts"));
        // Ports carry their physical connector, serialized camelCase.
        assert!(json.contains("connector"));
        assert!(json.contains("quarterInch"));
    }

    /// Connector compatibility is same-connector only (TS/TRS already unified as `QuarterInch`); a
    /// signal-class difference is *not* what's checked here — only the physical shape.
    #[test]
    fn connectors_compatible_is_same_connector_only() {
        assert!(connectors_compatible(
            Connector::QuarterInch,
            Connector::QuarterInch
        ));
        assert!(connectors_compatible(Connector::Xlr, Connector::Xlr));
        assert!(!connectors_compatible(
            Connector::QuarterInch,
            Connector::Xlr
        ));
        assert!(!connectors_compatible(
            Connector::Speakon,
            Connector::QuarterInch
        ));
    }

    /// Ports carry an authored physical connector, distinct from their signal-class `kind`: the synth's
    /// instrument out and the gain stage's line in are both ¼" (so they interconnect despite differing
    /// kinds), the AD's digital out is a digital connector, the synth's MIDI in is a 5-pin DIN, and the
    /// speaker terminus takes a ¼" line feed today (keeping the default `da→spk` connection legal).
    #[test]
    fn ports_carry_authored_connectors() {
        let all = descriptors();
        let connector = |type_id: &str, dir: PortDirection, id: u32| {
            all.iter()
                .find(|d| d.type_id == type_id)
                .unwrap_or_else(|| panic!("no device {type_id}"))
                .ports
                .iter()
                .find(|p| p.direction == dir && p.id == id)
                .unwrap_or_else(|| panic!("no {type_id} port {id}"))
                .connector
        };
        assert_eq!(
            connector("synth_voice", PortDirection::Output, 0),
            Connector::QuarterInch
        );
        assert_eq!(
            connector("synth_voice", PortDirection::Input, 0),
            Connector::Din5
        );
        assert_eq!(
            connector("gain_stage", PortDirection::Input, 0),
            Connector::QuarterInch
        );
        assert_eq!(
            connector("ad_converter", PortDirection::Output, 0),
            Connector::Digital
        );
        assert_eq!(
            connector("speaker", PortDirection::Input, 0),
            Connector::QuarterInch
        );
    }

    /// The meter devices expose their node readouts as descriptors: the VU meter's VU + peak, the
    /// digital meter's peak + RMS, ids in exposed (position) order — the surface the UI meters read.
    #[test]
    fn meter_devices_expose_their_readouts() {
        let all = descriptors();
        let vu = all
            .iter()
            .find(|d| d.type_id == "vu_meter")
            .expect("vu_meter");
        assert_eq!(vu.readouts.len(), 2);
        assert_eq!(vu.readouts[0].id, 0);
        assert_eq!(vu.readouts[0].label, "VU");
        assert_eq!(vu.readouts[1].id, 1);
        assert_eq!(vu.readouts[1].unit, "dBu");
        // The VU meter measures, so it exposes no control params.
        assert!(vu.params.is_empty());

        let dig = all
            .iter()
            .find(|d| d.type_id == "digital_meter")
            .expect("digital_meter");
        assert_eq!(dig.readouts.len(), 2);
        assert_eq!(dig.readouts[1].label, "RMS");
        assert_eq!(dig.readouts[1].unit, "dBFS");

        // A non-meter device exposes no readouts.
        let spk = all
            .iter()
            .find(|d| d.type_id == "speaker")
            .expect("speaker");
        assert!(spk.readouts.is_empty());
    }

    /// The chassis seam maps readouts to concrete `(NodeId, ReadoutId)`, in exposed order — the map
    /// `build_patch` resolves to `ReadoutHandle`s. A single-node meter's two readouts map to its one
    /// node with node-local ids 0 and 1.
    #[test]
    fn meter_readouts_map_to_nodes() {
        let mut g = Graph::new();
        let vu = instantiate("vu_meter", &mut g).expect("vu_meter is in the catalog");
        assert_eq!(vu.nodes.len(), 1);
        assert_eq!(
            vu.readouts,
            vec![(vu.nodes[0], ReadoutId(0)), (vu.nodes[0], ReadoutId(1))]
        );
    }

    /// Every device carries a sane physical form factor (content for the spatial world): a rackmount
    /// unit spans at least 1U; a desktop unit has a positive footprint box. Guards against a 0-U or
    /// zero-size device that the placement model couldn't lay out.
    #[test]
    fn catalog_carries_sane_form_factors() {
        for entry in CATALOG {
            match entry.form_factor {
                FormFactor::Rackmount { rack_units } => {
                    assert!(rack_units >= 1, "{} rack_units", entry.type_id);
                }
                FormFactor::Desktop {
                    width_mm,
                    height_mm,
                    depth_mm,
                } => {
                    assert!(
                        width_mm > 0.0 && height_mm > 0.0 && depth_mm > 0.0,
                        "{} desktop footprint",
                        entry.type_id
                    );
                }
            }
        }
    }

    /// The form factor serializes in the camelCase, internally-tagged shape the TS `FormFactor` mirror
    /// consumes: a `kind` discriminant plus camelCase variant fields. Pins the wire contract.
    #[test]
    fn form_factor_serializes_as_tagged_camel_case() {
        let json = serde_json::to_string(&descriptors()).expect("descriptors serialize");
        assert!(json.contains("formFactor"));
        assert!(json.contains(r#""kind":"rackmount""#));
        assert!(json.contains("rackUnits"));
        assert!(json.contains(r#""kind":"desktop""#));
        assert!(json.contains("widthMm"));
    }
}
