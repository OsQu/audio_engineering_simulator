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

use crate::scene::ConfigSetting;
use engine::{
    AdConverter, BitDepth, DaConverter, DigitalMeter, Domain, EqBand, EventThru, GainStage, Graph,
    InputZ, MicPreamp, Node, NodeId, Ohms, ParamId, ReadoutId, SampleRate, Speaker, SynthVoice,
    ThreeBandEq, Volts, VuMeter,
};
use serde::Serialize;

/// A device's **structural config** at build time — the `(key → scalar)` view a catalog node builder
/// reads to select which node/impedance to construct (e.g. a preamp's `"inst1"` hi-Z toggle). Wraps
/// the scene's [`ConfigSetting`]s; unknown keys fall back to the builder's default. Unlike a param
/// (smoothed at runtime), config is consumed once, at build — changing it recompiles.
pub struct DeviceConfig<'a> {
    settings: &'a [ConfigSetting],
}

impl<'a> DeviceConfig<'a> {
    /// A view over a device instance's config settings.
    #[must_use]
    pub fn new(settings: &'a [ConfigSetting]) -> Self {
        Self { settings }
    }

    /// The empty config — every key falls back to its default. Used to build a device's descriptor
    /// (the exposed face is config-independent) and by config-free devices.
    pub const EMPTY: DeviceConfig<'static> = DeviceConfig { settings: &[] };

    /// The value for `key`, or `default` if the instance didn't set it.
    #[must_use]
    pub fn get_or(&self, key: &str, default: f32) -> f32 {
        self.settings
            .iter()
            .find(|c| c.key == key)
            .map_or(default, |c| c.value)
    }
}

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
    /// The device's **structural** config toggles (e.g. a preamp's INST/hi-Z), which the UI renders
    /// and whose change triggers a rebuild. Empty for a device with no structural options.
    pub configs: Vec<ConfigDescriptor>,
}

/// One structural config option: its key, a UI label, the control kind, and the default value the
/// device builds with when unset. Hand-authored per catalog entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDescriptor {
    /// Structural config key — what a `ConfigSetting` addresses.
    pub key: String,
    /// Display label (e.g. "Inst 1").
    pub label: String,
    /// Suggested control widget for the structural toggle.
    pub kind: ConfigKind,
    /// Value the device builds with when the instance leaves this key unset.
    pub default: f32,
}

/// Suggested control widget for a structural config option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigKind {
    /// A two-state structural switch (`0.0` / `1.0`) — e.g. a preamp's INST/hi-Z.
    Toggle,
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
    /// Lane count — the port's [`lane_count`](engine::InputPort::lane_count): digital **channels**
    /// (1 mono, N for a multichannel connector), analog conductors (1 unbalanced, 2 balanced), 1 for
    /// events. Engine truth (derived, can't drift). The UI badges a digital jack with `channels > 1`.
    pub channels: u16,
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
/// they are one connector here. `Combo` is the XLR+TRS combo jack real interfaces use for their front
/// inputs — it physically accepts an XLR *or* a ¼" plug, so it's compatible with both. `Digital` stays
/// the generic digital connector for abstract lab converters; `Usb`/`Spdif` are the specific physical
/// digital connectors an interface presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Connector {
    /// ¼" (6.35 mm) phone jack — TS and TRS unified (same hole). Instrument + line gear.
    QuarterInch,
    /// 3-pin XLR — balanced mic / line.
    Xlr,
    /// XLR+TRS **combo** jack — accepts an XLR or a ¼" plug (an interface's front input).
    Combo,
    /// speakON / binding-post speaker connector.
    Speakon,
    /// 5-pin DIN — MIDI (events domain).
    Din5,
    /// A generic digital-audio connector (abstract lab converters/EQ/meters).
    Digital,
    /// USB — the interface↔computer multichannel digital link.
    Usb,
    /// S/PDIF (RCA coax) — a stereo digital in/out on an interface.
    Spdif,
}

/// Whether two connectors can be physically joined. Same-connector is always fine; the one asymmetric
/// case is the **combo** jack, which physically accepts an XLR *or* a ¼" plug — so `Combo` is
/// compatible with [`Xlr`](Connector::Xlr) and [`QuarterInch`](Connector::QuarterInch) (and itself).
/// Everything else is equality (TS/TRS already unified under `QuarterInch`; other adapters/hybrid leads
/// are out of scope). Domain + digital channel-count compatibility are *separate* checks.
#[must_use]
pub fn connectors_compatible(a: Connector, b: Connector) -> bool {
    // A combo jack mates with an XLR or a ¼" plug, in either direction.
    let combo_mates = |x: Connector, y: Connector| {
        x == Connector::Combo && matches!(y, Connector::Xlr | Connector::QuarterInch)
    };
    a == b || combo_mates(a, b) || combo_mates(b, a)
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

/// A node builder: constructs one engine node from the device's structural [`DeviceConfig`]. Most
/// ignore the config; a config-driven builder reads a key to pick an impedance or topology.
type NodeBuilder = fn(&DeviceConfig) -> Box<dyn Node>;

/// One device type in the catalog — the single place a device is defined. Bundles its identity, its
/// **node builders** + **internal edges** (the chassis), and the **UI metadata** for its panel. The
/// metadata is positionally aligned to the *exposed* face (open ports in node order; all params
/// concatenated); everything numeric (ranges, domains) is *derived* from the nodes by [`descriptors`].
struct CatalogEntry {
    type_id: &'static str,
    name: &'static str,
    /// Physical form factor + size — intrinsic content, hand-authored like the labels.
    form_factor: FormFactor,
    /// The internal node(s), in order; each is a [`NodeBuilder`] constructing one engine node from the
    /// device's structural config. Length 1 is the single-node case.
    nodes: &'static [NodeBuilder],
    /// Edges wiring the internal nodes. Empty for a single-node device.
    internal: &'static [InternalEdge],
    /// One per **ungrouped** exposed param (node params not captured by a group, concatenated in node
    /// order). Grouped params are hidden from this positional walk; the groups' UIs live in
    /// [`param_groups`](Self::param_groups). The exposed param face is `params` ++ `param_groups`.
    params: &'static [ParamUi],
    /// Device-level param **groups**: each binds one exposed control to N node params driven together
    /// (e.g. a single power switch over every stage's `powered`). Appended to the exposed param face
    /// after the ungrouped params, in declaration order. Empty for a device with no grouped controls.
    param_groups: &'static [ParamGroup],
    /// One per *exposed* input port (open inputs, in node order).
    inputs: &'static [PortUi],
    /// One per *exposed* output port (open outputs, in node order).
    outputs: &'static [PortUi],
    /// One per *exposed* readout (all node readouts, concatenated in node order). Empty for a device
    /// that measures nothing.
    readouts: &'static [ReadoutUi],
    /// Structural config toggles the device offers (INST/hi-Z etc.), read by the node builders and
    /// surfaced to the UI via the descriptor. Empty for a device with no structural options.
    configs: &'static [ConfigUi],
}

struct ParamUi {
    label: &'static str,
    unit: &'static str,
    kind: ParamKind,
}

/// A hand-authored structural config option (the entry's counterpart to a [`ConfigDescriptor`]).
struct ConfigUi {
    key: &'static str,
    label: &'static str,
    kind: ConfigKind,
    /// Value the device builds with when the instance leaves this key unset — must match what the
    /// node builder passes to [`DeviceConfig::get_or`] for this key.
    default: f32,
}

/// A **device-level param group**: one exposed control bound to N node params that are driven
/// together as a single value. The device-crate concept sitting above the strictly-per-node engine
/// (`node.rs`'s layering doc keeps the engine per-node) — a real interface's single bus-power state
/// maps to a `powered` gate on every stage. The bound node params are **hidden** from the positional
/// param walk (the same convention as a port consumed by an [`InternalEdge`]); the group appears once
/// on the exposed face. Every target must carry an **identical** decl (range/default/smooth), from
/// which the descriptor's range/default derive — `catalog_group_targets_carry_identical_decls` guards
/// it.
struct ParamGroup {
    /// The single exposed control's UI (label/unit/kind).
    ui: ParamUi,
    /// The node params it binds: `(node index in `nodes`, node-local `ParamId`)`.
    targets: &'static [(usize, ParamId)],
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

/// The 8i6 preamp input impedances. Line-level (default) keeps today's 10 kΩ; instrument/hi-Z (INST
/// engaged) is ~1.5 MΩ so a high-output-impedance pickup isn't loaded down. The choice is baked into
/// the loading divider at compile, so it's a structural config (a toggle recompiles), not a param.
const PREAMP_LINE_Z_OHMS: f32 = 10_000.0;
const PREAMP_INST_Z_OHMS: f32 = 1_500_000.0;

/// Build one 8i6 preamp — a [`MicPreamp`] whose input impedance is selected by the `inst_key`
/// structural toggle (`>= 0.5` ⇒ instrument/hi-Z, else line). Its default (`0.0` = line) reproduces
/// the pre-INST 10 kΩ behavior.
fn scarlett_preamp(cfg: &DeviceConfig, inst_key: &str) -> Box<dyn Node> {
    let z_ohms = if cfg.get_or(inst_key, 0.0) >= 0.5 {
        PREAMP_INST_Z_OHMS
    } else {
        PREAMP_LINE_Z_OHMS
    };
    Box::new(MicPreamp::new(
        1.0,
        Volts::new(10.0),
        InputZ::new(Ohms::new(z_ohms)),
        Ohms::new(150.0),
    ))
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
        nodes: &[|_cfg| Box::new(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)))],
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
        param_groups: &[],
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
        configs: &[],
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
        nodes: &[|_cfg| Box::new(EventThru::new(64))],
        internal: &[],
        params: &[],
        param_groups: &[],
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
        configs: &[],
    },
    CatalogEntry {
        type_id: "gain_stage",
        name: "Gain Stage",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|_cfg| {
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
        param_groups: &[],
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
        configs: &[],
    },
    CatalogEntry {
        type_id: "three_band_eq",
        name: "3-Band EQ",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|_cfg| {
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
        param_groups: &[],
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
        configs: &[],
    },
    CatalogEntry {
        type_id: "ad_converter",
        name: "AD Converter",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|_cfg| {
            Box::new(AdConverter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                Volts::new(1.0),
                Ohms::new(1_000_000.0),
            ))
        }],
        internal: &[],
        // The converter's `powered` gate — a standalone AD exposes it as one power switch.
        params: &[ParamUi {
            label: "Power",
            unit: "",
            kind: ParamKind::Switch,
        }],
        param_groups: &[],
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
        configs: &[],
    },
    CatalogEntry {
        type_id: "da_converter",
        name: "DA Converter",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|_cfg| {
            Box::new(DaConverter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                Volts::new(1.0),
                Ohms::new(150.0),
            ))
        }],
        internal: &[],
        // The converter's `powered` gate — a standalone DA exposes it as one power switch.
        params: &[ParamUi {
            label: "Power",
            unit: "",
            kind: ParamKind::Switch,
        }],
        param_groups: &[],
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
        configs: &[],
    },
    CatalogEntry {
        type_id: "speaker",
        name: "Speaker",
        form_factor: FormFactor::Desktop {
            width_mm: 250.0,
            height_mm: 380.0,
            depth_mm: 300.0,
        },
        nodes: &[|_cfg| Box::new(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))))],
        internal: &[],
        params: &[],
        param_groups: &[],
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
        configs: &[],
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
            |_cfg| {
                Box::new(GainStage::new(
                    1.0,
                    Volts::new(10.0),
                    InputZ::new(Ohms::new(10_000.0)),
                    Ohms::new(150.0),
                ))
            },
            |_cfg| {
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
        param_groups: &[],
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
        configs: &[],
    },
    // A voltage-native VU meter — bridging inline analog meter (unity passthrough). Its two readouts
    // ride the node→host lane: the ballistic VU reading and the block peak in dBu. The analog half of
    // "gain-staging across the AD/DA boundary made visible".
    CatalogEntry {
        type_id: "vu_meter",
        name: "VU Meter",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|_cfg| Box::new(VuMeter::new())],
        internal: &[],
        params: &[],
        param_groups: &[],
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
        configs: &[],
    },
    // A digital level meter — inline passthrough on a digital channel, reporting peak and RMS in
    // dBFS. Placed after the AD, it's the digital half of the across-converter gain-staging story.
    CatalogEntry {
        type_id: "digital_meter",
        name: "Digital Meter",
        form_factor: FormFactor::Rackmount { rack_units: 1 },
        nodes: &[|_cfg| {
            Box::new(DigitalMeter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
            ))
        }],
        internal: &[],
        params: &[],
        param_groups: &[],
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
        configs: &[],
    },
    // A simplified Focusrite Scarlett 8i6 — the first **mixed-face, multi-I/O** interface (Story 5.7),
    // built entirely from existing nodes (no new engine node). Two mic/instrument preamps each feed an
    // AD converter (the digital "USB send"); a digital "USB return" drives a DA whose analog monitor bus
    // fans out to a line output and a headphone amp; MIDI passes through. The device's exposed face is
    // deliberately split across two chassis faces by the web faceplate: the **front** carries the two
    // combo inputs + the headphone out, the **back** carries line/USB/MIDI (and, as params, power).
    // INST/AIR/PAD/48V are intentionally **absent** — none is honestly modelable in today's engine
    // (see `docs/IMPROVEMENTS.md` and the Story 5.7 design notes); the faceplate omits, never fakes, them.
    CatalogEntry {
        type_id: "scarlett_8i6",
        name: "Scarlett 8i6",
        form_factor: FormFactor::Desktop {
            width_mm: 810.0,
            height_mm: 150.0,
            depth_mm: 150.0,
        },
        // Node order fixes the exposed-face order (open ports + concatenated params, in node order):
        // 0,1 preamps · 2,3 their ADs · 4 the DA · 5,6 monitor + headphone amps · 7 MIDI thru.
        nodes: &[
            // The two mic/instrument preamps: `MicPreamp`s whose INST/hi-Z input impedance is picked
            // from the device config (`inst1`/`inst2`). PAD + AIR ride as their runtime params.
            |cfg| scarlett_preamp(cfg, "inst1"),
            |cfg| scarlett_preamp(cfg, "inst2"),
            |_cfg| {
                Box::new(AdConverter::new(
                    SampleRate::new(HOST_RATE_HZ),
                    BitDepth::new(BITS),
                    Volts::new(1.0),
                    Ohms::new(1_000_000.0),
                ))
            },
            |_cfg| {
                Box::new(AdConverter::new(
                    SampleRate::new(HOST_RATE_HZ),
                    BitDepth::new(BITS),
                    Volts::new(1.0),
                    Ohms::new(1_000_000.0),
                ))
            },
            |_cfg| {
                Box::new(DaConverter::new(
                    SampleRate::new(HOST_RATE_HZ),
                    BitDepth::new(BITS),
                    Volts::new(1.0),
                    Ohms::new(150.0),
                ))
            },
            |_cfg| {
                Box::new(GainStage::new(
                    1.0,
                    Volts::new(10.0),
                    InputZ::new(Ohms::new(10_000.0)),
                    Ohms::new(150.0),
                ))
            },
            |_cfg| {
                Box::new(GainStage::new(
                    1.0,
                    Volts::new(10.0),
                    InputZ::new(Ohms::new(10_000.0)),
                    Ohms::new(150.0),
                ))
            },
            |_cfg| Box::new(EventThru::new(64)),
        ],
        // preamp→AD (×2), then the DA fans out to both the monitor and the headphone amp.
        internal: &[
            InternalEdge {
                from_node: 0,
                from_port: 0,
                to_node: 2,
                to_port: 0,
            },
            InternalEdge {
                from_node: 1,
                from_port: 0,
                to_node: 3,
                to_port: 0,
            },
            InternalEdge {
                from_node: 4,
                from_port: 0,
                to_node: 5,
                to_port: 0,
            },
            InternalEdge {
                from_node: 4,
                from_port: 0,
                to_node: 6,
                to_port: 0,
            },
        ],
        // Ungrouped params, exposed in node order (each stage's `powered` is captured by the Power
        // group below and hidden from this walk). The two preamps each expose gain + PAD + AIR
        // switches (INST/hi-Z is a structural config, not here); the monitor + phones amps expose gain.
        // Exposed param ids: 0 Gain 1 · 1 Pad 1 · 2 Air 1 · 3 Gain 2 · 4 Pad 2 · 5 Air 2 · 6 Monitor ·
        // 7 Phones · 8 Power.
        params: &[
            ParamUi {
                label: "Gain 1",
                unit: "×",
                kind: ParamKind::Knob,
            },
            ParamUi {
                label: "Pad 1",
                unit: "",
                kind: ParamKind::Switch,
            },
            ParamUi {
                label: "Air 1",
                unit: "",
                kind: ParamKind::Switch,
            },
            ParamUi {
                label: "Gain 2",
                unit: "×",
                kind: ParamKind::Knob,
            },
            ParamUi {
                label: "Pad 2",
                unit: "",
                kind: ParamKind::Switch,
            },
            ParamUi {
                label: "Air 2",
                unit: "",
                kind: ParamKind::Switch,
            },
            ParamUi {
                label: "Monitor",
                unit: "×",
                kind: ParamKind::Knob,
            },
            ParamUi {
                label: "Phones",
                unit: "×",
                kind: ParamKind::Knob,
            },
        ],
        // One device-level power switch — a real 8i6 is a single powered unit. It gates every stage's
        // `powered`: both preamps (MicPreamp id 1), both ADs and the DA (id 0), and the monitor +
        // phones amps (GainStage id 1). An "off" device is silent on *both* analog outs and the USB
        // sends (the AD gate).
        param_groups: &[ParamGroup {
            ui: ParamUi {
                label: "Power",
                unit: "",
                kind: ParamKind::Switch,
            },
            targets: &[
                (0, MicPreamp::POWERED),
                (1, MicPreamp::POWERED),
                (2, AdConverter::POWERED),
                (3, AdConverter::POWERED),
                (4, DaConverter::POWERED),
                (5, GainStage::POWERED),
                (6, GainStage::POWERED),
            ],
        }],
        // Open inputs in node order: the two preamp inputs (front combo jacks), the DA's digital
        // "USB return", and the MIDI in.
        inputs: &[
            PortUi {
                label: "In 1",
                kind: PortKind::Instrument,
                connector: Connector::Combo,
            },
            PortUi {
                label: "In 2",
                kind: PortKind::Instrument,
                connector: Connector::Combo,
            },
            PortUi {
                label: "USB In",
                kind: PortKind::Digital,
                connector: Connector::Usb,
            },
            PortUi {
                label: "MIDI In",
                kind: PortKind::Midi,
                connector: Connector::Din5,
            },
        ],
        // Open outputs in node order: the two AD "USB sends", the monitor line out, the headphone
        // out (drawn on the front), and the MIDI out.
        outputs: &[
            PortUi {
                label: "USB 1",
                kind: PortKind::Digital,
                connector: Connector::Usb,
            },
            PortUi {
                label: "USB 2",
                kind: PortKind::Digital,
                connector: Connector::Usb,
            },
            PortUi {
                label: "Line Out",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "Phones",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "MIDI Out",
                kind: PortKind::Midi,
                connector: Connector::Din5,
            },
        ],
        readouts: &[],
        // INST/hi-Z per preamp: a structural toggle selecting the channel's input impedance (line
        // vs instrument), read by the preamp builders. Default off (line-level), reproducing today's
        // behavior. AIR/PAD are runtime *params* on the preamp, not structural configs.
        configs: &[
            ConfigUi {
                key: "inst1",
                label: "Inst 1",
                kind: ConfigKind::Toggle,
                default: 0.0,
            },
            ConfigUi {
                key: "inst2",
                label: "Inst 2",
                kind: ConfigKind::Toggle,
                default: 0.0,
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
    /// Device param id → the `(node, node ParamId)` target(s) it drives. One target for an ungrouped
    /// param; N for a device-level param group (e.g. a power switch over every stage) — the host fans
    /// one value out to all of them.
    pub params: Vec<Vec<(NodeId, ParamId)>>,
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
    /// The port's lane count (digital channels / analog conductors / 1 for events), from the face.
    channels: u16,
}

/// One exposed param: the `(node index, ParamId)` target(s) it drives, plus the decl's range/default
/// (engine truth, copied so the descriptor needn't re-introspect). An ungrouped param has exactly one
/// target; a [`ParamGroup`] has N (all carrying the same decl, so one range/default describes them).
struct ExposedParam {
    targets: Vec<(usize, ParamId)>,
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
fn expand(entry: &CatalogEntry, config: &DeviceConfig) -> Expansion {
    let nodes: Vec<Box<dyn Node>> = entry.nodes.iter().map(|build| build(config)).collect();
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
                    channels: face.lane_count() as u16,
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
                    channels: face.lane_count() as u16,
                });
            }
        }
        for decl in node.params() {
            // A param captured by a group is hidden from the positional walk (like a port an
            // internal edge consumes); it's driven via the appended group entry instead.
            let grouped = entry
                .param_groups
                .iter()
                .any(|g| g.targets.contains(&(ni, decl.id)));
            if !grouped {
                params.push(ExposedParam {
                    targets: vec![(ni, decl.id)],
                    min: decl.min,
                    max: decl.max,
                    default: decl.default,
                });
            }
        }
        for decl in node.readouts() {
            readouts.push(ExposedReadout {
                node: ni,
                id: decl.id,
            });
        }
    }

    // Append one exposed param per group, after the ungrouped walk, in declaration order. Its
    // range/default derive from the group's first target's decl — every target must carry an
    // identical decl (`catalog_group_targets_carry_identical_decls` proves it), so any target agrees.
    for group in entry.param_groups {
        let (ni, id) = group.targets[0];
        let decl = nodes[ni]
            .params()
            .iter()
            .find(|d| d.id == id)
            .expect("a param group targets a param its node declares");
        params.push(ExposedParam {
            targets: group.targets.to_vec(),
            min: decl.min,
            max: decl.max,
            default: decl.default,
        });
    }

    Expansion {
        nodes,
        inputs,
        outputs,
        params,
        readouts,
    }
}

/// Expand the device type `type_id` (built with structural `config`) into `g`: add its node(s), wire
/// its internal edges, and return the instance map (device-level ports/params → concrete
/// `(NodeId, …)`). `None` if the type is unknown.
///
/// The chassis-seam primitive: `build_patch` calls this per device with the instance's config, then
/// uses the returned [`BuiltDevice`] to remap inter-device connections and resolve control handles.
/// The exposed face is config-independent (a config changes baked values/topology, not which
/// ports/params exist), so the returned map is stable across config choices.
pub fn instantiate(type_id: &str, config: &DeviceConfig, g: &mut Graph) -> Option<BuiltDevice> {
    let entry = entry(type_id)?;
    let Expansion {
        nodes,
        inputs,
        outputs,
        params,
        readouts,
    } = expand(entry, config);

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
        params: params
            .iter()
            .map(|p| {
                p.targets
                    .iter()
                    .map(|&(ni, id)| (node_ids[ni], id))
                    .collect()
            })
            .collect(),
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
    // The exposed face is config-independent, so the descriptor is built with the empty config.
    let face = expand(entry, &DeviceConfig::EMPTY);

    // Params, exposed (position) order — the id is the position, matching how the host addresses a
    // param (`BuiltScene::param(device, id)` indexes the exposed handle vec), not the node-local
    // `ParamId`. For a multi-node device the two differ: `channel_strip`'s stages both expose
    // `ParamId` 0/1, so node-local ids would collide at `[0,1,0,1]` and misaddress the second stage.
    // The UI list is the ungrouped labels ++ the group labels, in the same order `expand` lays out
    // the exposed face (ungrouped in node order, then groups in declaration order).
    let param_ui = entry
        .params
        .iter()
        .chain(entry.param_groups.iter().map(|g| &g.ui));
    let params = face
        .params
        .iter()
        .zip(param_ui)
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
            channels: p.channels,
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
            channels: p.channels,
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

    // Structural config toggles, straight from the entry (hand-authored; no engine truth to derive).
    let configs = entry
        .configs
        .iter()
        .map(|c| ConfigDescriptor {
            key: c.key.to_owned(),
            label: c.label.to_owned(),
            kind: c.kind,
            default: c.default,
        })
        .collect();

    DeviceDescriptor {
        type_id: entry.type_id.to_owned(),
        name: entry.name.to_owned(),
        form_factor: entry.form_factor,
        params,
        ports,
        readouts,
        configs,
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
            let face = expand(entry, &DeviceConfig::EMPTY);
            // The exposed param face is the ungrouped UIs ++ one entry per group.
            assert_eq!(
                entry.params.len() + entry.param_groups.len(),
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

    /// Every target of a param group must carry an **identical** decl (range/default/smooth) — the
    /// descriptor derives the group's exposed range/default from the first target, so a mismatch would
    /// silently misreport the others. An authoring guard (e.g. the 8i6's Power group over `GainStage`,
    /// `AdConverter`, and `DaConverter` `powered` params, which are deliberately declared identical).
    #[test]
    fn catalog_group_targets_carry_identical_decls() {
        for entry in CATALOG {
            let nodes: Vec<Box<dyn Node>> = entry
                .nodes
                .iter()
                .map(|build| build(&DeviceConfig::EMPTY))
                .collect();
            for group in entry.param_groups {
                let decl_of = |&(ni, id): &(usize, ParamId)| {
                    *nodes[ni]
                        .params()
                        .iter()
                        .find(|d| d.id == id)
                        .unwrap_or_else(|| {
                            panic!("{} group targets a missing param", entry.type_id)
                        })
                };
                let first = decl_of(&group.targets[0]);
                for target in group.targets {
                    let d = decl_of(target);
                    assert_eq!(
                        d.min.to_bits(),
                        first.min.to_bits(),
                        "{} min",
                        entry.type_id
                    );
                    assert_eq!(
                        d.max.to_bits(),
                        first.max.to_bits(),
                        "{} max",
                        entry.type_id
                    );
                    assert_eq!(
                        d.default.to_bits(),
                        first.default.to_bits(),
                        "{} default",
                        entry.type_id
                    );
                    assert_eq!(
                        d.smooth_ms.to_bits(),
                        first.smooth_ms.to_bits(),
                        "{} smooth_ms",
                        entry.type_id
                    );
                }
            }
        }
    }

    /// Each descriptor carries the exposed param id (its **position** in the exposed face — what the
    /// host addresses, matching ports/readouts, *not* the node-local `ParamId`) plus the node's real
    /// ranges/defaults (bit-exact, derived not retyped) and real port domains — so the UI can never
    /// misaddress a param, show a stale range, or wire a wrong-domain port.
    #[test]
    fn descriptors_carry_engine_truth() {
        for entry in CATALOG {
            let face = expand(entry, &DeviceConfig::EMPTY);
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
                // Channels are the port's lane count — engine truth, derived not retyped.
                assert_eq!(pd.channels, ep.channels, "{} input channels", entry.type_id);
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
        let strip = instantiate("channel_strip", &DeviceConfig::EMPTY, &mut g)
            .expect("channel_strip is in the catalog");

        assert_eq!(strip.nodes.len(), 2, "two internal nodes");
        assert_eq!(g.connection_count(), 1, "one internal edge wired");

        // Exposed input is stage 0's input; exposed output is stage 1's output.
        assert_eq!(strip.inputs, vec![(strip.nodes[0], 0)]);
        assert_eq!(strip.outputs, vec![(strip.nodes[1], 0)]);

        // Each stage's gain (ParamId 0) + power (ParamId 1) exposed, in node order — device params
        // 2/3 map to the *second* node. No groups, so each exposed param is a single target.
        assert_eq!(
            strip.params,
            vec![
                vec![(strip.nodes[0], ParamId(0))],
                vec![(strip.nodes[0], ParamId(1))],
                vec![(strip.nodes[1], ParamId(0))],
                vec![(strip.nodes[1], ParamId(1))],
            ]
        );
    }

    /// A single-node device is the trivial case: one node, no internal edges, and the node's own face
    /// exposed as-is.
    #[test]
    fn single_node_device_is_identity() {
        let mut g = Graph::new();
        let spk = instantiate("speaker", &DeviceConfig::EMPTY, &mut g)
            .expect("speaker is in the catalog");

        assert_eq!(spk.nodes.len(), 1);
        assert_eq!(g.connection_count(), 0, "no internal edges");
        assert_eq!(spk.inputs, vec![(spk.nodes[0], 0)]);
        assert_eq!(spk.outputs, vec![(spk.nodes[0], 0)]);
    }

    /// The Scarlett 8i6 — the mixed-face interface — expands into its eight internal nodes wired by
    /// four internal edges (two preamp→AD, plus the DA **fanning out** to the monitor and headphone
    /// amps), and its exposed face maps to the right `(NodeId, …)` in node order. This pins the
    /// non-trivial remap the faceplate relies on: device inputs are preamp 1/2, the DA's digital
    /// return, and MIDI-in; device outputs are the two AD sends, the monitor + headphone analog outs,
    /// and MIDI-out; and the exposed params are, in node order, each `MicPreamp`'s gain/pad/air, then
    /// the monitor + phones gains, then one **Power group** that fans out to every stage's `powered`
    /// — both preamps, both ADs, the DA, and the monitor + phones amps (seven targets from one control).
    #[test]
    fn scarlett_8i6_expands_with_mixed_face_io() {
        let mut g = Graph::new();
        let dev = instantiate("scarlett_8i6", &DeviceConfig::EMPTY, &mut g)
            .expect("scarlett_8i6 is in the catalog");

        assert_eq!(dev.nodes.len(), 8, "eight internal nodes");
        assert_eq!(
            g.connection_count(),
            4,
            "four internal edges (2 preamp→AD, DA fanned to monitor + phones)"
        );

        // Inputs, in node order: preamp 1/2 inputs, the DA's digital return, MIDI-in.
        assert_eq!(
            dev.inputs,
            vec![
                (dev.nodes[0], 0),
                (dev.nodes[1], 0),
                (dev.nodes[4], 0),
                (dev.nodes[7], 0),
            ]
        );
        // Outputs, in node order: the two AD sends, the monitor + headphone analog outs, MIDI-out —
        // the monitor and phones being *distinct* exposed outputs is the DA fan-out made visible.
        assert_eq!(
            dev.outputs,
            vec![
                (dev.nodes[2], 0),
                (dev.nodes[3], 0),
                (dev.nodes[5], 0),
                (dev.nodes[6], 0),
                (dev.nodes[7], 0),
            ]
        );
        // Exposed params, ungrouped in node order then the group. Each preamp (MicPreamp) exposes its
        // GAIN (0), PAD (2), AIR (3) — POWERED (1) is captured by the group; the monitor + phones
        // GainStages expose GAIN (0). Then the Power group's seven targets: preamps' POWERED (id 1),
        // both ADs + the DA (id 0), monitor + phones POWERED (id 1).
        assert_eq!(
            dev.params,
            vec![
                vec![(dev.nodes[0], ParamId(0))], // Gain 1
                vec![(dev.nodes[0], ParamId(2))], // Pad 1
                vec![(dev.nodes[0], ParamId(3))], // Air 1
                vec![(dev.nodes[1], ParamId(0))], // Gain 2
                vec![(dev.nodes[1], ParamId(2))], // Pad 2
                vec![(dev.nodes[1], ParamId(3))], // Air 2
                vec![(dev.nodes[5], ParamId(0))], // Monitor
                vec![(dev.nodes[6], ParamId(0))], // Phones
                vec![
                    (dev.nodes[0], ParamId(1)),
                    (dev.nodes[1], ParamId(1)),
                    (dev.nodes[2], ParamId(0)),
                    (dev.nodes[3], ParamId(0)),
                    (dev.nodes[4], ParamId(0)),
                    (dev.nodes[5], ParamId(1)),
                    (dev.nodes[6], ParamId(1)),
                ], // Power
            ]
        );
    }

    /// An unknown type id has no entry — `instantiate` returns `None` (no nodes added), the lookup
    /// `build_patch` relies on to reject a bad `typeId` cleanly.
    #[test]
    fn unknown_type_does_not_instantiate() {
        let mut g = Graph::new();
        assert!(instantiate("does_not_exist", &DeviceConfig::EMPTY, &mut g).is_none());
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
            "scarlett_8i6",
        ] {
            assert!(json.contains(type_id), "catalog missing {type_id}");
        }
        // camelCase field names are the wire contract (matches the TS mirror).
        assert!(json.contains("typeId"));
        assert!(json.contains("readouts"));
        // Ports carry their physical connector + channel count, serialized camelCase.
        assert!(json.contains("connector"));
        assert!(json.contains("quarterInch"));
        assert!(json.contains("channels"));
        // The 8i6's front inputs are combo jacks; its USB ports use the USB connector.
        assert!(json.contains("combo"));
        assert!(json.contains("usb"));
        // Structural config toggles serialize (the 8i6's INST keys) — the web renders them.
        assert!(json.contains("configs"));
        assert!(json.contains("inst1"));
        assert!(json.contains("toggle"), "ConfigKind serializes camelCase");
    }

    /// Connector compatibility is same-connector, plus the **combo** jack mating with an XLR or a ¼"
    /// plug (either direction). A signal-class difference isn't what's checked here — only physical fit.
    #[test]
    fn connectors_compatible_matrix() {
        // Same connector always fits.
        assert!(connectors_compatible(
            Connector::QuarterInch,
            Connector::QuarterInch
        ));
        assert!(connectors_compatible(Connector::Xlr, Connector::Xlr));
        assert!(connectors_compatible(Connector::Combo, Connector::Combo));
        // Combo accepts XLR and ¼", symmetrically.
        assert!(connectors_compatible(Connector::Combo, Connector::Xlr));
        assert!(connectors_compatible(Connector::Xlr, Connector::Combo));
        assert!(connectors_compatible(
            Connector::Combo,
            Connector::QuarterInch
        ));
        assert!(connectors_compatible(
            Connector::QuarterInch,
            Connector::Combo
        ));
        // Non-combo mismatches still reject; the digital split is equality (USB ≠ S/PDIF ≠ generic).
        assert!(!connectors_compatible(
            Connector::QuarterInch,
            Connector::Xlr
        ));
        assert!(!connectors_compatible(
            Connector::Speakon,
            Connector::QuarterInch
        ));
        assert!(!connectors_compatible(Connector::Usb, Connector::Spdif));
        assert!(!connectors_compatible(Connector::Usb, Connector::Digital));
        assert!(!connectors_compatible(Connector::Combo, Connector::Usb));
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
        let vu = instantiate("vu_meter", &DeviceConfig::EMPTY, &mut g)
            .expect("vu_meter is in the catalog");
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
