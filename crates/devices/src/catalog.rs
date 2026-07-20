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

use std::borrow::Cow;

use crate::scene::ConfigSetting;
use engine::{
    AdConverter, BitDepth, CondenserMic, DaConverter, DigitalDemux, DigitalMeter, DigitalMux,
    Domain, EqBand, EventThru, GainStage, Graph, InputZ, Matrix, MicPreamp, Node, NodeId, Ohms,
    ParamId, ReadoutId, SampleRate, Speaker, SynthVoice, ThreeBandEq, Volts, VuMeter,
};
use serde::Serialize;

mod computer;

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
    /// (the default face — the type catalog's descriptor is built from it) and by config-free devices.
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
    /// How travel maps onto the value (linear vs dB-linear) — see [`ParamTaper`]. Omitted from the
    /// JS descriptor when `Linear` (the common case reads as `taper?: "log"`).
    #[serde(skip_serializing_if = "is_linear_taper")]
    pub taper: ParamTaper,
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
    /// **Round-trip latency** output: an edge *from* this output carries one block of latency (built
    /// via [`engine::Graph::connect_delayed`]). Its physical meaning is a device that trails its input
    /// by a buffer — a computer/DAW, whose playback is one block behind what it records — so a
    /// monitoring loop *through* it (interface → DAW → interface) can close without a same-block
    /// feedback cycle. Always `false` for inputs and for ordinary outputs.
    pub delayed: bool,
    /// The port on the **other** direction that shares this port's physical connector — a **duplex**
    /// jack (USB-C, Ethernet), which carries data both ways over one connector. For an output it is the
    /// paired input's id; for an input, the paired output's id. `None` for an ordinary one-way jack. A
    /// duplex [`scene::Connection`](crate::scene::Connection) between two duplex jacks expands to the
    /// two directed engine edges; the UI draws one jack and one cable. Omitted from the JS descriptor
    /// for an ordinary jack (so it reads as `duplexPartner?: number`, not `null`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplex_partner: Option<u32>,
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

/// How a continuous control's **rotation/travel maps onto its value** — a UI/control-law hint, not
/// engine truth (the value sent to the engine is unchanged; `min`/`max` still come from the node's
/// `ParamDecl`). A voltage-gain multiplier spans a huge linear range (`0..1000` ≈ 0×→+60 dB), so a
/// **linear** knob crams the whole usable range into the first sliver of travel — quarter-turn is
/// already +48 dB. [`Log`](Self::Log) instead maps travel **dB-linearly** (equal rotation = equal dB
/// step), the way a real gain pot is marked, so the readout is shown in dB and the low end is usable.
/// The web widget owns the exact curve (silence at the very bottom for controls whose `min` is 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ParamTaper {
    /// Value is linear in travel: `value = min + t·(max − min)`. The default for every control.
    #[default]
    Linear,
    /// Value is logarithmic (dB-linear) in travel — for voltage-gain knobs displayed in dB.
    Log,
}

/// Serde skip predicate: a `Linear` taper is the default, omitted from the descriptor JSON.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if signature"
)]
fn is_linear_taper(taper: &ParamTaper) -> bool {
    matches!(taper, ParamTaper::Linear)
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
    /// An optional **generated crosspoint grid** for a routing-matrix node — synthesizes the matrix's
    /// `inputs × outputs` crosspoint UIs (labels, not hand-authored). Its params are the *trailing*
    /// ungrouped exposed params, so the matrix must be the entry's last param-contributing node.
    param_grid: Option<GridSpec>,
    /// One per *exposed* input port (open inputs, in node order).
    inputs: &'static [PortUi],
    /// One per *exposed* output port (open outputs, in node order).
    outputs: &'static [PortUi],
    /// Exposed **output** ids whose edges carry one block of **round-trip latency** (a computer/DAW's
    /// playback trails its input). `build_patch` wires edges from these via
    /// [`engine::Graph::connect_delayed`], letting a monitoring loop *through* the device close without
    /// a cycle. Empty for every ordinary device; only a latency source (the `computer`) lists one.
    delayed_outputs: &'static [u32],
    /// **Duplex jacks**: `(output_id, input_id)` pairs where one physical connector (USB-C, Ethernet)
    /// carries both directions. Each pair surfaces as a `duplex_partner` on both descriptors, and a
    /// duplex connection to it expands to two engine edges. Empty for a device with only one-way jacks.
    duplex_links: &'static [(u32, u32)],
    /// The device's exposed readout labels — one per *exposed* readout (all node readouts, concatenated
    /// in node order). `Static(&[])` for a device that measures nothing.
    readouts: ReadoutSpec,
    /// Structural config toggles the device offers (INST/hi-Z etc.), read by the node builders and
    /// surfaced to the UI via the descriptor. Empty for a device with no structural options.
    configs: &'static [ConfigUi],
}

struct ParamUi {
    label: &'static str,
    unit: &'static str,
    kind: ParamKind,
    /// Rotation→value law (see [`ParamTaper`]). `Linear` for everything but voltage-gain knobs.
    taper: ParamTaper,
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

/// A **generated grid of crosspoint param UIs** — the routing matrix's `inputs × outputs` cells,
/// whose labels (`"{input} → {output}"`) are *synthesized* rather than hand-authored (the full 8i6's
/// matrix has 196 crosspoints — infeasible as static `ParamUi`s). Row-major (input outer, output
/// inner), matching [`Matrix`]'s crosspoint id order. The generated entries are the **last ungrouped**
/// exposed params, so a device using a grid must make its `Matrix` the *last param-contributing node*;
/// the web focus surface renders them as a grid, deriving rows/cols from the `" → "` labels.
struct GridSpec {
    /// Input (row) axis, in matrix input order.
    inputs: GridAxis,
    /// Output (column) axis, in matrix output order.
    outputs: GridAxis,
    kind: ParamKind,
    unit: &'static str,
}

/// A grid axis (a matrix's rows or columns): today only [`Named`](GridAxis::Named) hand-authored
/// labels — one per lane on the axis.
enum GridAxis {
    /// Hand-authored names, one per matrix lane on this axis (the 8i6's fixed 14×14).
    Named(&'static [&'static str]),
    Generated {
        prefix: &'static str,
    },
}

impl GridAxis {
    /// The axis names, in lane order.
    fn names(&self, count: usize) -> Vec<Cow<'static, str>> {
        match self {
            GridAxis::Named(names) => names.iter().map(|&s| Cow::Borrowed(s)).collect(),
            GridAxis::Generated { prefix } => (1..=count)
                .map(|k| Cow::Owned(format!("{prefix} {k}")))
                .collect(),
        }
    }
}

impl GridSpec {
    /// The generated `"{input} → {output}"` labels, row-major — one per crosspoint, in matrix
    /// crosspoint id order.
    fn labels(&self, n_in: usize, m_out: usize) -> Vec<String> {
        self.inputs
            .names(n_in)
            .iter()
            .flat_map(move |i| {
                self.outputs
                    .names(m_out)
                    .iter()
                    .map(move |o| format!("{i} → {o}"))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
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

/// One per-lane meter bank: `"{prefix} {lane} {measure}"` for each lane × each `per` measure. A
/// metering node (a `DigitalMeter`) contributes one bank; its lane count is *derived* from that
/// node's exposed-readout count ÷ `per.len()`, so a config-driven meter (whose lane count isn't a
/// port total — e.g. the DAW's per-*track* meter) labels correctly without a separate count source.
struct MeterBank {
    prefix: &'static str,
    per: &'static [ReadoutUi],
}

/// How a device's exposed readouts are labeled.
enum ReadoutSpec {
    /// Hand-authored labels, one per exposed readout, in exposed order.
    Static(&'static [ReadoutUi]),
    /// One **meter bank per metering node**, in node order (matching the exposed readouts' node
    /// order). Each bank labels `"{prefix} {lane} {measure}"`; a device with a send meter, a per-track
    /// meter, and a return meter lists three banks (`Send`s, `Track`s, `Return`s).
    PerNode(&'static [MeterBank]),
}

impl ReadoutSpec {
    /// The readout UIs, in exposed order. `node_readout_counts` is the number of exposed readouts each
    /// **metering node** contributes, in node order — a [`PerNode`](ReadoutSpec::PerNode) bank pairs
    /// with the node at its position and sizes itself from that count ÷ `per.len()`.
    fn label(&self, node_readout_counts: &[usize]) -> Vec<(String, String)> {
        match self {
            ReadoutSpec::Static(uis) => uis
                .iter()
                .map(|ui| (ui.label.into(), ui.unit.into()))
                .collect(),
            ReadoutSpec::PerNode(banks) => banks
                .iter()
                .zip(node_readout_counts)
                .flat_map(|(bank, &node_count)| {
                    let lanes = node_count / bank.per.len().max(1);
                    (1..=lanes).flat_map(move |lane| {
                        bank.per.iter().map(move |t| {
                            (
                                format!("{} {} {}", bank.prefix, lane, t.label),
                                t.unit.into(),
                            )
                        })
                    })
                })
                .collect(),
        }
    }
}

/// The 8i6 preamp input impedances. Line-level (default) keeps today's 10 kΩ; instrument/hi-Z (INST
/// engaged) is ~1.5 MΩ so a high-output-impedance pickup isn't loaded down. The choice is baked into
/// the loading divider at compile, so it's a structural config (a toggle recompiles), not a param.
const PREAMP_LINE_Z_OHMS: f32 = 10_000.0;
const PREAMP_INST_Z_OHMS: f32 = 1_500_000.0;

/// The 8i6's 48V phantom key — **one switch feeding both preamps** (the real 3rd-gen 8i6 has a
/// single global 48V button; contrast the per-channel INST keys). 48V is **structural** like INST:
/// engaging it changes the DC bias topology, so a toggle recompiles rather than smoothing a param
/// (the Story 5.8 design note). Acoustically safe — the pedestal cancels at every balanced
/// receiver in both states, so the swap can't click.
const PHANTOM_KEY: &str = "phantom";

/// Build one 8i6 preamp — a [`MicPreamp`] whose input impedance is selected by the `inst_key`
/// structural toggle (`>= 0.5` ⇒ instrument/hi-Z, else line), and whose +48 V phantom feed is
/// engaged by the **shared** [`PHANTOM_KEY`] toggle (both preamps read the same key). Defaults
/// (`0.0` = line, phantom off) reproduce the pre-INST/pre-48V behavior. Both faces are **balanced**
/// (a combo jack's XLR and TRS paths both carry the pair); an unbalanced source still seats via the
/// engine's grounding edge, with the *same* loading gain — the differential Z plays exactly the
/// role the unbalanced Z did.
fn scarlett_preamp(cfg: &DeviceConfig, inst_key: &str) -> Box<dyn Node> {
    let z_ohms = if cfg.get_or(inst_key, 0.0) >= 0.5 {
        PREAMP_INST_Z_OHMS
    } else {
        PREAMP_LINE_Z_OHMS
    };
    Box::new(
        // Default to the minimum gain (≈ +8 dB) — a real preamp powers on with the gain pot fully
        // counter-clockwise, and the knob's floor is +8 dB (never an attenuator).
        MicPreamp::new(
            MicPreamp::MIN_GAIN,
            Volts::new(10.0),
            InputZ::balanced(Ohms::new(z_ohms)),
            Ohms::new(150.0),
        )
        .with_phantom(cfg.get_or(PHANTOM_KEY, 0.0) >= 0.5),
    )
}

/// The 8i6's AD/DA/gain building blocks, factored out of the (large) entry. The AD's input impedance
/// is a parameter: preamp-fed ADs present a high `z_in` (the buffered preamp drives them, so the divider
/// stays unity), while a **line input** AD presents a realistic line-level impedance the external
/// source loads against.
fn scarlett_ad(z_in_ohms: f32) -> Box<dyn Node> {
    Box::new(AdConverter::new(
        SampleRate::new(HOST_RATE_HZ),
        BitDepth::new(BITS),
        Volts::new(1.0),
        Ohms::new(z_in_ohms),
    ))
}
fn scarlett_da() -> Box<dyn Node> {
    Box::new(DaConverter::new(
        SampleRate::new(HOST_RATE_HZ),
        BitDepth::new(BITS),
        Volts::new(1.0),
        Ohms::new(150.0),
    ))
}
fn scarlett_gain() -> Box<dyn Node> {
    // The 8i6's monitor and headphone amps are **volume** controls: they attenuate from unity (0 dB,
    // fully open — the default) down to silence, never boost. Cap the GAIN at unity so the knob is a
    // proper level control, not a +60 dB preamp.
    Box::new(
        GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
        .with_gain_range(0.0, 1.0),
    )
}

/// The device catalog: every type the UI can place, builders + descriptor together. Each entry's
/// `params`/`inputs`/`outputs` lengths must match its exposed face — `catalog_aligns_with_exposed_face`
/// guards it (the `zip` in `describe` would otherwise silently truncate).
const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        type_id: "synth_voice",
        name: "Synth Voice",
        // A compact desktop synth module, sized to sit alongside the 8i6 (a tabletop unit, not a
        // console): fader + ADSR knobs + a small envelope screen fit across ~220 mm.
        form_factor: FormFactor::Desktop {
            width_mm: 220.0,
            height_mm: 62.0,
            depth_mm: 150.0,
        },
        nodes: &[|_cfg| Box::new(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)))],
        internal: &[],
        params: &[
            ParamUi {
                label: "Level",
                unit: "V",
                kind: ParamKind::Fader,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Attack",
                unit: "ms",
                kind: ParamKind::Knob,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Decay",
                unit: "ms",
                kind: ParamKind::Knob,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Sustain",
                unit: "",
                kind: ParamKind::Knob,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Release",
                unit: "ms",
                kind: ParamKind::Knob,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Power",
                unit: "",
                kind: ParamKind::Switch,
                taper: ParamTaper::Linear,
            },
        ],
        param_groups: &[],
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[]),
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
        // A compact controller/thru box (played via the focus keybed), 8i6-width so the stock set reads
        // as one tidy scale — the true-to-life 49-key (~800 mm) variant is a later, realistic addition.
        form_factor: FormFactor::Desktop {
            width_mm: 210.0,
            height_mm: 42.0,
            depth_mm: 110.0,
        },
        nodes: &[|_cfg| Box::new(EventThru::new(64))],
        internal: &[],
        params: &[],
        param_groups: &[],
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[]),
        configs: &[],
    },
    // A condenser microphone — the catalog face of `engine::CondenserMic` (Story 5.8). A balanced
    // XLR mic-level source that is **dead until phantom-fed**: the node declares its P48 DC load on
    // its output port and `compile` resolves the operating point against whatever engaged supply
    // faces it (the 8i6's 48V config) — power arrives from the patch, never a flag. Acoustics are
    // out of scope until the deferred "air link" story, so the capsule is a declared boundary
    // stand-in — a deterministic sine — and the params are labelled honestly as that capsule tone.
    CatalogEntry {
        type_id: "condenser_mic",
        name: "Condenser Mic",
        // A large-diaphragm studio condenser stood on the desk: LDC bodies run ~45–60 mm across
        // (the classic U 87 is 56 mm) and ~200 mm long including the grille; the footprint is the
        // body cylinder, so depth = width.
        form_factor: FormFactor::Desktop {
            width_mm: 50.0,
            height_mm: 200.0,
            depth_mm: 50.0,
        },
        // 10 mV capsule tone (typical mic level) from a 150 Ω balanced source (the classic mic
        // output impedance).
        nodes: &[|_cfg| Box::new(CondenserMic::new(Volts::new(0.01), Ohms::new(150.0)))],
        internal: &[],
        params: &[
            ParamUi {
                label: "Tone Level",
                unit: "V",
                kind: ParamKind::Knob,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Tone Freq",
                unit: "Hz",
                kind: ParamKind::Knob,
                taper: ParamTaper::Linear,
            },
        ],
        param_groups: &[],
        param_grid: None,
        inputs: &[],
        outputs: &[PortUi {
            label: "Out",
            kind: PortKind::Mic,
            connector: Connector::Xlr,
        }],
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[]),
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
                unit: "dB",
                kind: ParamKind::Knob,
                taper: ParamTaper::Log,
            },
            ParamUi {
                label: "Power",
                unit: "",
                kind: ParamKind::Switch,
                taper: ParamTaper::Linear,
            },
        ],
        param_groups: &[],
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[]),
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
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[]),
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
            taper: ParamTaper::Linear,
        }],
        param_groups: &[],
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[]),
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
            taper: ParamTaper::Linear,
        }],
        param_groups: &[],
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[]),
        configs: &[],
    },
    CatalogEntry {
        type_id: "speaker",
        name: "Speaker",
        // A compact desktop monitor — kept small so it sits with the 8i6 rather than towering over the
        // bench; a full-size main monitor is a later realistic variant.
        form_factor: FormFactor::Desktop {
            width_mm: 130.0,
            height_mm: 170.0,
            depth_mm: 150.0,
        },
        nodes: &[|_cfg| Box::new(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))))],
        internal: &[],
        params: &[],
        param_groups: &[],
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[]),
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
                unit: "dB",
                kind: ParamKind::Knob,
                taper: ParamTaper::Log,
            },
            ParamUi {
                label: "Input Power",
                unit: "",
                kind: ParamKind::Switch,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Output Gain",
                unit: "dB",
                kind: ParamKind::Knob,
                taper: ParamTaper::Log,
            },
            ParamUi {
                label: "Output Power",
                unit: "",
                kind: ParamKind::Switch,
                taper: ParamTaper::Linear,
            },
        ],
        param_groups: &[],
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[]),
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
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[
            ReadoutUi {
                label: "VU",
                unit: "VU",
            },
            ReadoutUi {
                label: "Peak",
                unit: "dBu",
            },
        ]),
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
                1,
            ))
        }],
        internal: &[],
        params: &[],
        param_groups: &[],
        param_grid: None,
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
        delayed_outputs: &[],
        duplex_links: &[],
        readouts: ReadoutSpec::Static(&[
            ReadoutUi {
                label: "Peak",
                unit: "dBFS",
            },
            ReadoutUi {
                label: "RMS",
                unit: "dBFS",
            },
        ]),
        configs: &[],
    },
    // A simplified Focusrite Scarlett 8i6 — the first **mixed-face, multi-I/O** interface (Story 5.7).
    // Two `MicPreamp`s (gain + PAD + AIR, INST/hi-Z via structural config) each feed an AD; the two ADs
    // and a "USB return" feed a 3×3 routing **matrix**; the matrix's two "USB send" outputs go to the
    // computer and its third output drives a DA whose analog monitor bus fans out to a line output and a
    // headphone amp; MIDI passes through. The web faceplate splits the exposed face across two chassis
    // faces (front: combo inputs + phones; back: line/USB/MIDI + power), and the routing matrix is
    // driven from the Focusrite Control focus surface. 48V phantom (Story 5.8) is the shared
    // `PHANTOM_KEY` structural config: one switch engages both preamps' P48 supplies at build.
    CatalogEntry {
        type_id: "scarlett_8i6",
        name: "Scarlett 8i6",
        // The real 2nd-gen 8i6: a half-rack-width desktop box, ~216 × 47 × 173 mm (W×H×D).
        form_factor: FormFactor::Desktop {
            width_mm: 210.0,
            height_mm: 47.5,
            depth_mm: 149.5,
        },
        // Signal-flow layout. Every routing-matrix port is internal (fed by the ADs/demuxes, consumed
        // by the muxes/DAs), so the matrix is placed **last** — its 196 generated crosspoints become the
        // trailing exposed params without disturbing the port face. Node order:
        //   0,1 preamps · 2,3 preamp ADs · 4–7 line-in ADs (3–6) · 8 S/PDIF-in demux · 9 USB-return
        //   demux · 10 USB-send mux · 11 S/PDIF-out mux · 12,13 monitor DAs (L/R) · 14,15 monitor amps
        //   (Line Out 1/2) · 16,17 line-out DAs (3/4) · 18,19 phones amps · 20 MIDI thru · 21 matrix ·
        //   22,23 input meters (combo 1/2). The meters are appended last so adding them left every
        //   earlier node index — and thus every param id and internal edge — unchanged.
        nodes: &[
            |cfg| scarlett_preamp(cfg, "inst1"),
            |cfg| scarlett_preamp(cfg, "inst2"),
            |_cfg| scarlett_ad(1_000_000.0), // preamp 1 AD (buffered feed ⇒ high Z, unity divider)
            |_cfg| scarlett_ad(1_000_000.0), // preamp 2 AD
            |_cfg| scarlett_ad(10_000.0),    // line in 3 (line-level input impedance)
            |_cfg| scarlett_ad(10_000.0),    // line in 4
            |_cfg| scarlett_ad(10_000.0),    // line in 5
            |_cfg| scarlett_ad(10_000.0),    // line in 6
            |_cfg| {
                Box::new(DigitalDemux::new(
                    SampleRate::new(HOST_RATE_HZ),
                    BitDepth::new(BITS),
                    2,
                ))
            }, // S/PDIF in (2ch)
            |_cfg| {
                Box::new(DigitalDemux::new(
                    SampleRate::new(HOST_RATE_HZ),
                    BitDepth::new(BITS),
                    6,
                ))
            }, // USB return (6ch)
            |_cfg| {
                Box::new(DigitalMux::new(
                    SampleRate::new(HOST_RATE_HZ),
                    BitDepth::new(BITS),
                    8,
                ))
            }, // USB send (8ch)
            |_cfg| {
                Box::new(DigitalMux::new(
                    SampleRate::new(HOST_RATE_HZ),
                    BitDepth::new(BITS),
                    2,
                ))
            }, // S/PDIF out (2ch)
            |_cfg| scarlett_da(),            // monitor L DA
            |_cfg| scarlett_da(),            // monitor R DA
            |_cfg| scarlett_gain(),          // monitor L amp → Line Out 1
            |_cfg| scarlett_gain(),          // monitor R amp → Line Out 2
            |_cfg| scarlett_da(),            // line out 3 DA
            |_cfg| scarlett_da(),            // line out 4 DA
            |_cfg| scarlett_gain(),          // phones 1 amp
            |_cfg| scarlett_gain(),          // phones 2 amp
            |_cfg| Box::new(EventThru::new(64)),
            // The routing matrix (Focusrite Control's mixer, gains only). 14 ins (2 preamp + 4 line +
            // 2 S/PDIF + 6 USB return) × 14 outs (8 USB send + 4 line + 2 S/PDIF). Identity default:
            // input i → output i — hardware inputs to USB sends (record), USB returns to the analog /
            // S-PDIF outs (playback) — the standard interface routing, so behavior is unchanged until
            // the user re-routes in the focus view.
            |_cfg| {
                let n = 14;
                let mut d = vec![0.0; n * n];
                for i in 0..n {
                    d[i * n + i] = 1.0;
                }
                Box::new(Matrix::new(
                    SampleRate::new(HOST_RATE_HZ),
                    BitDepth::new(BITS),
                    n,
                    n,
                    d,
                ))
            },
            // 22,23: input meters on combo 1/2, inserted post-preamp (pre-AD). Appended *after* the
            // matrix so every existing node index (and thus every param id + the internal wiring) is
            // unchanged; a `VuMeter` is a transparent bridging passthrough, so it meters the gained
            // input level (the ring around the gain knob) without altering the record path.
            |_cfg| Box::new(VuMeter::new()), // input 1 meter
            |_cfg| Box::new(VuMeter::new()), // input 2 meter
        ],
        // The internal chassis wiring. preamp→meter→AD (×2, the meters tap combo 1/2's input level);
        // the 6 ADs + S/PDIF-in demux (2) + USB-return
        // demux (6) feed the matrix's 14 inputs; the matrix's 14 outputs feed the USB-send mux (8), the
        // monitor + line-out DAs (4), and the S/PDIF-out mux (2); each monitor DA fans to its line-out
        // amp and a phones amp.
        internal: &[
            // preamp → input meter → its AD (the meter taps the gained input level for the knob ring)
            InternalEdge {
                from_node: 0,
                from_port: 0,
                to_node: 22,
                to_port: 0,
            },
            InternalEdge {
                from_node: 22,
                from_port: 0,
                to_node: 2,
                to_port: 0,
            },
            InternalEdge {
                from_node: 1,
                from_port: 0,
                to_node: 23,
                to_port: 0,
            },
            InternalEdge {
                from_node: 23,
                from_port: 0,
                to_node: 3,
                to_port: 0,
            },
            // ADs (preamp 1/2, line 3–6) → matrix ins 0–5
            InternalEdge {
                from_node: 2,
                from_port: 0,
                to_node: 21,
                to_port: 0,
            },
            InternalEdge {
                from_node: 3,
                from_port: 0,
                to_node: 21,
                to_port: 1,
            },
            InternalEdge {
                from_node: 4,
                from_port: 0,
                to_node: 21,
                to_port: 2,
            },
            InternalEdge {
                from_node: 5,
                from_port: 0,
                to_node: 21,
                to_port: 3,
            },
            InternalEdge {
                from_node: 6,
                from_port: 0,
                to_node: 21,
                to_port: 4,
            },
            InternalEdge {
                from_node: 7,
                from_port: 0,
                to_node: 21,
                to_port: 5,
            },
            // S/PDIF-in demux (2ch) → matrix ins 6,7
            InternalEdge {
                from_node: 8,
                from_port: 0,
                to_node: 21,
                to_port: 6,
            },
            InternalEdge {
                from_node: 8,
                from_port: 1,
                to_node: 21,
                to_port: 7,
            },
            // USB-return demux (6ch) → matrix ins 8–13
            InternalEdge {
                from_node: 9,
                from_port: 0,
                to_node: 21,
                to_port: 8,
            },
            InternalEdge {
                from_node: 9,
                from_port: 1,
                to_node: 21,
                to_port: 9,
            },
            InternalEdge {
                from_node: 9,
                from_port: 2,
                to_node: 21,
                to_port: 10,
            },
            InternalEdge {
                from_node: 9,
                from_port: 3,
                to_node: 21,
                to_port: 11,
            },
            InternalEdge {
                from_node: 9,
                from_port: 4,
                to_node: 21,
                to_port: 12,
            },
            InternalEdge {
                from_node: 9,
                from_port: 5,
                to_node: 21,
                to_port: 13,
            },
            // matrix outs 0–7 → USB-send mux (8ch)
            InternalEdge {
                from_node: 21,
                from_port: 0,
                to_node: 10,
                to_port: 0,
            },
            InternalEdge {
                from_node: 21,
                from_port: 1,
                to_node: 10,
                to_port: 1,
            },
            InternalEdge {
                from_node: 21,
                from_port: 2,
                to_node: 10,
                to_port: 2,
            },
            InternalEdge {
                from_node: 21,
                from_port: 3,
                to_node: 10,
                to_port: 3,
            },
            InternalEdge {
                from_node: 21,
                from_port: 4,
                to_node: 10,
                to_port: 4,
            },
            InternalEdge {
                from_node: 21,
                from_port: 5,
                to_node: 10,
                to_port: 5,
            },
            InternalEdge {
                from_node: 21,
                from_port: 6,
                to_node: 10,
                to_port: 6,
            },
            InternalEdge {
                from_node: 21,
                from_port: 7,
                to_node: 10,
                to_port: 7,
            },
            // matrix outs 8–11 → monitor + line-out DAs
            InternalEdge {
                from_node: 21,
                from_port: 8,
                to_node: 12,
                to_port: 0,
            },
            InternalEdge {
                from_node: 21,
                from_port: 9,
                to_node: 13,
                to_port: 0,
            },
            InternalEdge {
                from_node: 21,
                from_port: 10,
                to_node: 16,
                to_port: 0,
            },
            InternalEdge {
                from_node: 21,
                from_port: 11,
                to_node: 17,
                to_port: 0,
            },
            // matrix outs 12,13 → S/PDIF-out mux (2ch)
            InternalEdge {
                from_node: 21,
                from_port: 12,
                to_node: 11,
                to_port: 0,
            },
            InternalEdge {
                from_node: 21,
                from_port: 13,
                to_node: 11,
                to_port: 1,
            },
            // monitor DAs fan out to their line-out amp and a phones amp
            InternalEdge {
                from_node: 12,
                from_port: 0,
                to_node: 14,
                to_port: 0,
            }, // mon L → Line Out 1
            InternalEdge {
                from_node: 12,
                from_port: 0,
                to_node: 18,
                to_port: 0,
            }, // mon L → Phones 1
            InternalEdge {
                from_node: 13,
                from_port: 0,
                to_node: 15,
                to_port: 0,
            }, // mon R → Line Out 2
            InternalEdge {
                from_node: 13,
                from_port: 0,
                to_node: 19,
                to_port: 0,
            }, // mon R → Phones 2
        ],
        // Ungrouped hand-authored params, in node order (each stage's `powered` is captured by the
        // Power group and hidden): the two preamps' gain/pad/air, then the two phones amps' gain. The
        // matrix's 196 crosspoints are the trailing ungrouped params, **generated** by `param_grid`
        // below (not hand-authored). Monitor is a group (the stereo monitor pair driven by one knob).
        // Exposed ids: 0 Gain1 · 1 Pad1 · 2 Air1 · 3 Gain2 · 4 Pad2 · 5 Air2 · 6 Phones1 · 7 Phones2 ·
        // 8–203 crosspoints · 204 Monitor · 205 Power.
        params: &[
            ParamUi {
                label: "Gain 1",
                unit: "dB",
                kind: ParamKind::Knob,
                taper: ParamTaper::Log,
            },
            ParamUi {
                label: "Pad 1",
                unit: "",
                kind: ParamKind::Switch,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Air 1",
                unit: "",
                kind: ParamKind::Switch,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Gain 2",
                unit: "dB",
                kind: ParamKind::Knob,
                taper: ParamTaper::Log,
            },
            ParamUi {
                label: "Pad 2",
                unit: "",
                kind: ParamKind::Switch,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Air 2",
                unit: "",
                kind: ParamKind::Switch,
                taper: ParamTaper::Linear,
            },
            ParamUi {
                label: "Phones 1",
                unit: "dB",
                kind: ParamKind::Knob,
                taper: ParamTaper::Log,
            },
            ParamUi {
                label: "Phones 2",
                unit: "dB",
                kind: ParamKind::Knob,
                taper: ParamTaper::Log,
            },
        ],
        // The routing matrix's crosspoints (node 21, the last param-contributing node): 14 inputs ×
        // 14 outputs, labels generated as "input → output". Rendered as a grid in Focusrite Control.
        param_grid: Some(GridSpec {
            inputs: GridAxis::Named(&[
                "Pre 1", "Pre 2", "Line 3", "Line 4", "Line 5", "Line 6", "SPDIF L", "SPDIF R",
                "DAW 1", "DAW 2", "DAW 3", "DAW 4", "DAW 5", "DAW 6",
            ]),
            outputs: GridAxis::Named(&[
                "USB 1", "USB 2", "USB 3", "USB 4", "USB 5", "USB 6", "USB 7", "USB 8", "Line 1",
                "Line 2", "Line 3", "Line 4", "SPDIF L", "SPDIF R",
            ]),
            kind: ParamKind::Knob,
            unit: "×",
        }),
        // Two groups. **Monitor** — the single monitor-level knob over the stereo monitor pair (both
        // monitor amps' GAIN). **Power** — the whole unit's power over every stage's `powered`: both
        // preamps (id 1), the 6 ADs (id 0), the 4 DAs (id 0), and the 4 output amps (id 1). The matrix
        // has no power gate — an off device's ADs/DAs are already silenced, so every output goes quiet.
        param_groups: &[
            ParamGroup {
                ui: ParamUi {
                    label: "Monitor",
                    unit: "dB",
                    kind: ParamKind::Knob,
                    taper: ParamTaper::Log,
                },
                targets: &[(14, GainStage::GAIN), (15, GainStage::GAIN)],
            },
            ParamGroup {
                ui: ParamUi {
                    label: "Power",
                    unit: "",
                    kind: ParamKind::Switch,
                    taper: ParamTaper::Linear,
                },
                targets: &[
                    (0, MicPreamp::POWERED),
                    (1, MicPreamp::POWERED),
                    (2, AdConverter::POWERED),
                    (3, AdConverter::POWERED),
                    (4, AdConverter::POWERED),
                    (5, AdConverter::POWERED),
                    (6, AdConverter::POWERED),
                    (7, AdConverter::POWERED),
                    (12, DaConverter::POWERED),
                    (13, DaConverter::POWERED),
                    (16, DaConverter::POWERED),
                    (17, DaConverter::POWERED),
                    (14, GainStage::POWERED),
                    (15, GainStage::POWERED),
                    (18, GainStage::POWERED),
                    (19, GainStage::POWERED),
                ],
            },
        ],
        // Open inputs in node order: 2 combo (front), 4 line ins (rear), S/PDIF in (2ch), USB return
        // (6ch), MIDI in.
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
                label: "Line In 3",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "Line In 4",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "Line In 5",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "Line In 6",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "S/PDIF In",
                kind: PortKind::Digital,
                connector: Connector::Spdif,
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
        // Open outputs in node order: USB send (8ch), S/PDIF out (2ch), line outs 1–4, phones 1–2
        // (mirroring the monitor pair, mono), MIDI out.
        outputs: &[
            PortUi {
                label: "USB Out",
                kind: PortKind::Digital,
                connector: Connector::Usb,
            },
            PortUi {
                label: "S/PDIF Out",
                kind: PortKind::Digital,
                connector: Connector::Spdif,
            },
            PortUi {
                label: "Line Out 1",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "Line Out 2",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "Line Out 3",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "Line Out 4",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "Phones 1",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "Phones 2",
                kind: PortKind::Line,
                connector: Connector::QuarterInch,
            },
            PortUi {
                label: "MIDI Out",
                kind: PortKind::Midi,
                connector: Connector::Din5,
            },
        ],
        delayed_outputs: &[],
        // The single USB-C jack is duplex: USB Out (output 0) + USB In (input 7) are one connector.
        duplex_links: &[(0, 7)],
        // Input meters on combo 1/2 (nodes 22,23), each a VuMeter exposing VU + peak-dBu — in node
        // order, so the four exposed readouts are In 1 (VU, Peak) then In 2 (VU, Peak). The web
        // faceplate renders these as the level ring around each preamp's gain knob.
        readouts: ReadoutSpec::Static(&[
            ReadoutUi {
                label: "In 1 VU",
                unit: "VU",
            },
            ReadoutUi {
                label: "In 1 Peak",
                unit: "dBu",
            },
            ReadoutUi {
                label: "In 2 VU",
                unit: "VU",
            },
            ReadoutUi {
                label: "In 2 Peak",
                unit: "dBu",
            },
        ]),
        // INST/hi-Z per preamp: a structural toggle selecting the channel's input impedance (line
        // vs instrument), read by the preamp builders. Default off (line-level), reproducing today's
        // behavior. AIR/PAD are runtime *params* on the preamp, not structural configs.
        //
        // 48V (`PHANTOM_KEY`): the phantom supply, **one switch over both preamps** (the real unit
        // has one global button; contrast the per-channel INST keys). Structural like INST — the DC
        // bias topology changes, so toggling recompiles (the Story 5.8 design note). Default off:
        // an unfed condenser mic is dead, and non-phantom sources are unaffected either way.
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
            ConfigUi {
                key: PHANTOM_KEY,
                label: "48V",
                kind: ConfigKind::Toggle,
                default: 0.0,
            },
        ],
    },
    computer::COMPUTER,
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
/// Most devices' exposed face is config-independent (a config changes baked values, not which
/// ports/params exist), but a **channel-count config** can resize it — the `computer`'s
/// `usb_sends`/`usb_returns` grow its port lanes, crosspoints, and readouts — so the returned map is
/// built for *this* config, not a canonical one.
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
    CATALOG
        .iter()
        .map(|entry| describe(entry, &DeviceConfig::EMPTY))
        .collect()
}

/// Build one descriptor: numeric param fields + port domains from the exposed face, labels from the entry.
fn describe(entry: &CatalogEntry, config: &DeviceConfig) -> DeviceDescriptor {
    // The type-catalog descriptor is built with the EMPTY config — the device's *default* face (2×2
    // for the config-driven computer). A per-instance, config-aware descriptor is a later story.
    let face = expand(entry, config);
    // The grid's column (return) count — the summed exposed output lanes.
    let m_out = face.outputs.iter().map(|o| usize::from(o.channels)).sum();

    // Params, exposed (position) order — the id is the position, matching how the host addresses a
    // param (`BuiltScene::param(device, id)` indexes the exposed handle vec), not the node-local
    // `ParamId`. For a multi-node device the two differ: `channel_strip`'s stages both expose
    // `ParamId` 0/1, so node-local ids would collide at `[0,1,0,1]` and misaddress the second stage.
    // The UI list is, in the same order `expand` lays out the exposed face (ungrouped in node order,
    // then groups): the hand-authored ungrouped labels, then the **generated matrix crosspoints** (the
    // matrix is the last param-contributing node, so its params trail the ungrouped face), then the
    // group labels. Each entry is `(label, unit, kind)`.
    let mut param_ui: Vec<(String, String, ParamKind, ParamTaper)> = entry
        .params
        .iter()
        .map(|ui| (ui.label.to_owned(), ui.unit.to_owned(), ui.kind, ui.taper))
        .collect();
    if let Some(grid) = &entry.param_grid {
        // The grid's columns are the matrix outputs (the return lanes, `m_out`); its rows are the
        // matrix *inputs*, which for a crossbar mixer exceed the device's input ports (it folds in
        // track playbacks as well as the live sends). So size the rows from the matrix's own
        // crosspoint count — the trailing ungrouped params — over `m_out`, not from the input face.
        // (`GridAxis::Named` ignores these counts, so the 8i6's hand-named 14×14 is unaffected.)
        let grid_crosspoints = face
            .params
            .len()
            .saturating_sub(entry.params.len())
            .saturating_sub(entry.param_groups.len());
        let grid_rows = grid_crosspoints.checked_div(m_out).unwrap_or(0);
        // Crosspoints are routing/mixer sends (0 = mute, small makeup range) — always linear.
        param_ui.extend(
            grid.labels(grid_rows, m_out)
                .into_iter()
                .map(|label| (label, grid.unit.to_owned(), grid.kind, ParamTaper::Linear)),
        );
    }
    param_ui.extend(entry.param_groups.iter().map(|g| {
        (
            g.ui.label.to_owned(),
            g.ui.unit.to_owned(),
            g.ui.kind,
            g.ui.taper,
        )
    }));
    let params = face
        .params
        .iter()
        .zip(param_ui)
        .enumerate()
        .map(|(i, (p, (label, unit, kind, taper)))| ParamDescriptor {
            id: i as u32,
            label,
            unit,
            kind,
            taper,
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
            delayed: false, // inputs are never a latency source
            // Paired output, if this input is half of a duplex jack.
            duplex_partner: entry
                .duplex_links
                .iter()
                .find(|(_, in_id)| *in_id == i as u32)
                .map(|(out_id, _)| *out_id),
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
            delayed: entry.delayed_outputs.contains(&(i as u32)),
            // Paired input, if this output is half of a duplex jack.
            duplex_partner: entry
                .duplex_links
                .iter()
                .find(|(out_id, _)| *out_id == i as u32)
                .map(|(_, in_id)| *in_id),
        });
    let ports = inputs.chain(outputs).collect();

    // Readouts, exposed (position) order — the id is the position, matching how the host addresses a
    // reading (`BuiltScene::readout(device, id)`), not the node-local `ReadoutId`.
    // Per-metering-node readout counts, in node order (readouts are already grouped by node) — a
    // `PerNode` bank pairs with the node at its position and sizes itself from this count.
    let mut node_readout_counts: Vec<usize> = Vec::new();
    let mut last_node: Option<usize> = None;
    for r in &face.readouts {
        if last_node != Some(r.node) {
            node_readout_counts.push(0);
            last_node = Some(r.node);
        }
        if let Some(c) = node_readout_counts.last_mut() {
            *c += 1;
        }
    }
    let readouts = face
        .readouts
        .iter()
        .zip(entry.readouts.label(&node_readout_counts))
        .enumerate()
        .map(|(i, (_, (label, unit)))| ReadoutDescriptor {
            id: i as u32,
            label,
            unit,
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

pub fn describe_device(type_id: &str, config: &DeviceConfig) -> Option<DeviceDescriptor> {
    entry(type_id).map(|entry| describe(entry, config))
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
            // Generated labels size to the built face, so materialize the grid with the same counts
            // `describe` uses: columns = exposed return lanes, rows = the matrix's own crosspoints over
            // the columns (so a crossbar whose inputs exceed the input ports still aligns). `Named`
            // axes ignore the counts and self-size, so this still checks the 8i6's hand-named 14×14.
            let m_out: usize = face.outputs.iter().map(|p| usize::from(p.channels)).sum();
            let grid_crosspoints = face
                .params
                .len()
                .saturating_sub(entry.params.len())
                .saturating_sub(entry.param_groups.len());
            let grid_rows = grid_crosspoints.checked_div(m_out).unwrap_or(0);
            // The exposed param face is the ungrouped UIs ++ the generated matrix crosspoints ++ one
            // entry per group.
            let grid = entry
                .param_grid
                .as_ref()
                .map_or(0, |g| g.labels(grid_rows, m_out).len());
            assert_eq!(
                entry.params.len() + grid + entry.param_groups.len(),
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
            // Per-metering-node readout counts, in node order (mirrors `describe`).
            let mut node_readout_counts: Vec<usize> = Vec::new();
            let mut last_node: Option<usize> = None;
            for r in &face.readouts {
                if last_node != Some(r.node) {
                    node_readout_counts.push(0);
                    last_node = Some(r.node);
                }
                if let Some(c) = node_readout_counts.last_mut() {
                    *c += 1;
                }
            }
            assert_eq!(
                entry.readouts.label(&node_readout_counts).len(),
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
            let desc = describe(entry, &DeviceConfig::EMPTY);

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

    /// The condenser mic's port/param remap (the single-node source case, like the speaker's but
    /// with no inputs at all): one node, no internal edges, the balanced output exposed as device
    /// output 0, and the two capsule-tone params (node-local LEVEL/FREQ) as device params 0/1.
    #[test]
    fn condenser_mic_expands_and_maps() {
        let mut g = Graph::new();
        let mic = instantiate("condenser_mic", &DeviceConfig::EMPTY, &mut g)
            .expect("condenser_mic is in the catalog");

        assert_eq!(mic.nodes.len(), 1, "one internal node");
        assert_eq!(g.connection_count(), 0, "no internal edges");
        assert!(mic.inputs.is_empty(), "a mic has no inputs");
        assert_eq!(mic.outputs, vec![(mic.nodes[0], 0)]);
        assert_eq!(
            mic.params,
            vec![
                vec![(mic.nodes[0], CondenserMic::LEVEL)],
                vec![(mic.nodes[0], CondenserMic::FREQ)],
            ]
        );
        assert!(mic.readouts.is_empty(), "the mic measures nothing");
    }

    /// The condenser mic's descriptor face: a balanced (2-conductor) analog XLR output styled as a
    /// mic jack, no inputs, no configs — and the capsule-tone params carrying the engine's construction
    /// truth (10 mV level default, 1 kHz frequency default).
    #[test]
    fn condenser_mic_descriptor_face() {
        let mic = descriptors()
            .into_iter()
            .find(|d| d.type_id == "condenser_mic")
            .expect("condenser_mic is in the catalog");

        assert_eq!(mic.ports.len(), 1, "one port: the XLR out");
        let out = &mic.ports[0];
        assert_eq!(out.direction, PortDirection::Output);
        assert_eq!(out.domain, PortDomain::Analog);
        assert_eq!(out.channels, 2, "balanced = two conductors");
        assert_eq!(out.kind, PortKind::Mic);
        assert_eq!(out.connector, Connector::Xlr);

        // The capsule tone: level (constructed 10 mV) and frequency (1 kHz default), engine truth.
        assert_eq!(mic.params.len(), 2);
        assert_eq!(mic.params[0].label, "Tone Level");
        assert_eq!(mic.params[0].unit, "V");
        assert_eq!(mic.params[0].default, 0.01);
        assert_eq!(mic.params[1].label, "Tone Freq");
        assert_eq!(mic.params[1].unit, "Hz");
        assert_eq!(mic.params[1].default, 1_000.0);

        assert!(mic.configs.is_empty(), "no structural options on the mic");
        assert!(mic.readouts.is_empty());
    }

    /// The 8i6 exposes the shared 48V toggle: **one** `phantom` key (both preamps read it — the real
    /// unit's single global button), structural like the per-channel INST keys, default off.
    #[test]
    fn scarlett_8i6_exposes_the_shared_phantom_config() {
        let dev = descriptors()
            .into_iter()
            .find(|d| d.type_id == "scarlett_8i6")
            .expect("scarlett_8i6 is in the catalog");

        let phantom = dev
            .configs
            .iter()
            .find(|c| c.key == "phantom")
            .expect("the 8i6 offers the 48V config");
        assert_eq!(phantom.label, "48V");
        assert_eq!(phantom.kind, ConfigKind::Toggle);
        assert_eq!(phantom.default, 0.0, "48V boots off");
        assert_eq!(
            dev.configs.iter().filter(|c| c.key == "phantom").count(),
            1,
            "one shared key, not one per preamp"
        );
    }

    /// The full Scarlett 8i6 expands into its 24 internal nodes wired by 36 internal edges, and its
    /// exposed face maps to the right `(NodeId, …)`. This pins the big remap: 9 inputs (2 combo, 4 line,
    /// S/PDIF, USB return, MIDI-in), 9 outputs (USB send, S/PDIF, 4 line, 2 phones, MIDI-out), the
    /// param face — the preamp + phones controls, the 196 matrix crosspoints, the Monitor group, and the
    /// Power group over all 16 powered stages — and the 4 input-meter readouts. The param map is
    /// spot-checked at its boundaries (the full enumeration would be 206 entries).
    #[test]
    fn scarlett_8i6_expands_with_mixed_face_io() {
        let mut g = Graph::new();
        let dev = instantiate("scarlett_8i6", &DeviceConfig::EMPTY, &mut g)
            .expect("scarlett_8i6 is in the catalog");
        let n = &dev.nodes;

        // 22 signal/routing nodes + 2 input meters (appended last, so earlier indices are unchanged).
        assert_eq!(n.len(), 24, "24 internal nodes");
        // 34 original edges + 2: each preamp→AD became preamp→meter→AD (one extra edge per meter).
        assert_eq!(g.connection_count(), 36, "36 internal edges");

        // Inputs: 2 preamps, 4 line-in ADs, S/PDIF-in demux, USB-return demux, MIDI-in (in node order).
        assert_eq!(
            dev.inputs,
            vec![
                (n[0], 0),
                (n[1], 0),
                (n[4], 0),
                (n[5], 0),
                (n[6], 0),
                (n[7], 0),
                (n[8], 0),
                (n[9], 0),
                (n[20], 0),
            ]
        );
        // Outputs: USB-send mux, S/PDIF-out mux, monitor amps (Line 1/2), line-out DAs (3/4), phones
        // amps, MIDI-out.
        assert_eq!(
            dev.outputs,
            vec![
                (n[10], 0),
                (n[11], 0),
                (n[14], 0),
                (n[15], 0),
                (n[16], 0),
                (n[17], 0),
                (n[18], 0),
                (n[19], 0),
                (n[20], 0),
            ]
        );
        // Param face boundaries: 8 hand-authored ungrouped + 196 crosspoints + Monitor + Power = 206.
        assert_eq!(dev.params.len(), 206);
        assert_eq!(
            dev.params[0],
            vec![(n[0], ParamId(0))],
            "id 0 = preamp 1 Gain"
        );
        assert_eq!(
            dev.params[6],
            vec![(n[18], ParamId(0))],
            "id 6 = phones 1 amp Gain"
        );
        assert_eq!(
            dev.params[7],
            vec![(n[19], ParamId(0))],
            "id 7 = phones 2 amp Gain"
        );
        // Crosspoints 8..=203 are all on the matrix (node 21), ParamId 0..=195, in order.
        assert_eq!(dev.params[8], vec![(n[21], ParamId(0))], "first crosspoint");
        assert_eq!(
            dev.params[203],
            vec![(n[21], ParamId(195))],
            "last crosspoint"
        );
        // Monitor group (id 204): the two monitor amps' GAIN. Power group (id 205): all 16 `powered`.
        assert_eq!(
            dev.params[204],
            vec![(n[14], ParamId(0)), (n[15], ParamId(0))],
            "Monitor group over the monitor pair"
        );
        assert_eq!(dev.params[205].len(), 16, "Power group over all 16 stages");

        // The two input meters (nodes 22,23) each expose VU + peak-dBu, in node order → 4 readouts,
        // which the faceplate binds as the level ring around gain knobs 1 and 2.
        assert_eq!(
            dev.readouts,
            vec![
                (n[22], ReadoutId(0)),
                (n[22], ReadoutId(1)),
                (n[23], ReadoutId(0)),
                (n[23], ReadoutId(1)),
            ],
            "In 1/In 2 VU + peak readouts"
        );
    }

    /// The 8i6's routing matrix defaults to the **identity** — input i → output i (hardware inputs to
    /// USB sends, USB returns to the analog / S-PDIF outs) — reproducing the standard fixed routing, so
    /// behavior is unchanged until the user re-routes. The 196 crosspoints are exposed params 8..=203
    /// (14×14, after the 8 hand-authored params); the diagonal cells default to 1.0, the rest to 0.0.
    #[test]
    fn scarlett_8i6_matrix_defaults_to_fixed_routing() {
        let dev = descriptors()
            .into_iter()
            .find(|d| d.type_id == "scarlett_8i6")
            .expect("scarlett_8i6 is in the catalog");
        let n = 14;
        for i in 0..n {
            for j in 0..n {
                let id = 8 + i * n + j; // crosspoint (i, j) exposed id
                let expected = if i == j { 1.0 } else { 0.0 };
                assert_eq!(
                    dev.params[id].default, expected,
                    "crosspoint ({i} → {j}) at id {id}"
                );
            }
        }
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
            "computer",
            "condenser_mic",
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
        // Structural config toggles serialize (the 8i6's INST + shared 48V keys) — the web renders
        // them.
        assert!(json.contains("configs"));
        assert!(json.contains("inst1"));
        assert!(json.contains("phantom"));
        // The mic's XLR connector serializes for the jack-fit check.
        assert!(json.contains(r#""connector":"xlr""#));
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
