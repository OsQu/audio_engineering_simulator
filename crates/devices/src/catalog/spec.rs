//! The authoring types a catalog entry is written in: the entry itself, its node specs and internal
//! edges, and the hand-authored UI metadata (params, configs, ports, readouts, grids).

use std::borrow::Cow;

use super::descriptor::{ConfigKind, Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use crate::assets::{AssetBuildError, BuildContext};
use crate::scene::ConfigSetting;
use engine::{InputPort, Node, OutputPort, ParamId};

/// A device's **structural config** at build time — the `(key → scalar)` view a catalog node builder
/// reads to select which node/impedance to construct (e.g. a preamp's `"inst1"` hi-Z toggle). Wraps
/// the scene's [`ConfigSetting`]s; unknown keys fall back to the builder's default. Unlike a param
/// (smoothed at runtime), config is consumed once, at build — changing it recompiles.
pub struct DeviceConfig<'a> {
    pub(super) settings: &'a [ConfigSetting],
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

/// The word length the catalog's digital devices are built at.
pub(super) const BITS: u32 = 16;

/// An edge between two of a device's internal nodes (by their index in [`CatalogEntry::nodes`]):
/// `from_node`'s output port `from_port` → `to_node`'s input port `to_port`. An ideal internal wire
/// (PCB trace, no cable). A port touched by an internal edge is **hidden** from the device's face.
pub(super) struct InternalEdge {
    pub(super) from_node: usize,
    pub(super) from_port: usize,
    pub(super) to_node: usize,
    pub(super) to_port: usize,
}

/// A node builder: constructs one engine node from the device's structural [`DeviceConfig`]. Most
/// ignore the config; a config-driven builder reads a key to pick an impedance or topology.
pub(super) type NodeBuilder = fn(&DeviceConfig) -> Box<dyn Node>;

/// A cold-path builder for a node that needs a host-supplied parsed asset. The normal catalog keeps
/// using [`NodeBuilder`]; this separate signature prevents asset-free entries from gaining needless
/// error handling while giving a future instrument entry the explicit construction context it needs.
pub(crate) type AssetNodeBuilder =
    fn(&DeviceConfig, &BuildContext) -> Result<Box<dyn Node>, AssetBuildError>;

/// A constructor-independent node declaration used solely to describe an asset-backed face.
pub(super) struct NodeFace {
    pub(super) inputs: Vec<InputPort>,
    pub(super) outputs: Vec<OutputPort>,
    pub(super) params: Vec<engine::ParamDecl>,
    pub(super) readouts: Vec<engine::ReadoutDecl>,
}

pub(super) type NodeFaceBuilder = fn(&DeviceConfig) -> NodeFace;

/// A catalog node is either ordinary or needs a parsed host asset to construct. Both variants
/// still provide an exposed face before the graph is built.
pub(super) enum NodeSpec {
    Plain(NodeBuilder),
    Asset {
        build: AssetNodeBuilder,
        face: NodeFaceBuilder,
    },
}

/// One device type in the catalog — the single place a device is defined. Bundles its identity, its
/// **node builders** + **internal edges** (the chassis), and the **UI metadata** for its panel. The
/// metadata is positionally aligned to the *exposed* face (open ports in node order; all params
/// concatenated); everything numeric (ranges, domains) is *derived* from the nodes by [`descriptors`](super::descriptors).
pub(super) struct CatalogEntry {
    pub(super) type_id: &'static str,
    pub(super) name: &'static str,
    /// Physical form factor + size — intrinsic content, hand-authored like the labels.
    pub(super) form_factor: FormFactor,
    /// The internal node(s), in order. Asset nodes have an explicit, construction-independent face
    /// so catalog listing never parses an asset or constructs a placeholder synth.
    pub(super) nodes: &'static [NodeSpec],
    /// Edges wiring the internal nodes. Empty for a single-node device.
    pub(super) internal: &'static [InternalEdge],
    /// One per **ungrouped** exposed param (node params not captured by a group, concatenated in node
    /// order). Grouped params are hidden from this positional walk; the groups' UIs live in
    /// [`param_groups`](Self::param_groups). The exposed param face is `params` ++ `param_groups`.
    pub(super) params: &'static [ParamUi],
    /// Device-level param **groups**: each binds one exposed control to N node params driven together
    /// (e.g. a single power switch over every stage's `powered`). Appended to the exposed param face
    /// after the ungrouped params, in declaration order. Empty for a device with no grouped controls.
    pub(super) param_groups: &'static [ParamGroup],
    /// An optional **generated crosspoint grid** for a routing-matrix node — synthesizes the matrix's
    /// `inputs × outputs` crosspoint UIs (labels, not hand-authored). Its params are the *trailing*
    /// ungrouped exposed params, so the matrix must be the entry's last param-contributing node.
    pub(super) param_grid: Option<GridSpec>,
    /// One per *exposed* input port (open inputs, in node order).
    pub(super) inputs: &'static [PortUi],
    /// One per *exposed* output port (open outputs, in node order).
    pub(super) outputs: &'static [PortUi],
    /// Exposed **output** ids whose edges carry one block of **round-trip latency** (a computer/DAW's
    /// playback trails its input). `build_patch` wires edges from these via
    /// [`engine::Graph::connect_delayed`], letting a monitoring loop *through* the device close without
    /// a cycle. Empty for every ordinary device; only a latency source (the `computer`) lists one.
    pub(super) delayed_outputs: &'static [u32],
    /// **Duplex jacks**: `(output_id, input_id)` pairs where one physical connector (USB-C, Ethernet)
    /// carries both directions. Each pair surfaces as a `duplex_partner` on both descriptors, and a
    /// duplex connection to it expands to two engine edges. Empty for a device with only one-way jacks.
    pub(super) duplex_links: &'static [(u32, u32)],
    /// The device's exposed readout labels — one per *exposed* readout (all node readouts, concatenated
    /// in node order). `Static(&[])` for a device that measures nothing.
    pub(super) readouts: ReadoutSpec,
    /// Structural config toggles the device offers (INST/hi-Z etc.), read by the node builders and
    /// surfaced to the UI via the descriptor. Empty for a device with no structural options.
    pub(super) configs: &'static [ConfigUi],
}

pub(super) struct ParamUi {
    pub(super) label: &'static str,
    pub(super) unit: &'static str,
    pub(super) kind: ParamKind,
    /// Rotation→value law (see [`ParamTaper`]). `Linear` for everything but voltage-gain knobs.
    pub(super) taper: ParamTaper,
}

/// A hand-authored structural config option (the entry's counterpart to a [`ConfigDescriptor`](super::ConfigDescriptor)).
pub(super) struct ConfigUi {
    pub(super) key: &'static str,
    pub(super) label: &'static str,
    pub(super) kind: ConfigKind,
    /// Value the device builds with when the instance leaves this key unset — must match what the
    /// node builder passes to [`DeviceConfig::get_or`] for this key.
    pub(super) default: f32,
    pub(super) options: &'static [ConfigOptionUi],
}

pub(super) struct ConfigOptionUi {
    pub(super) label: &'static str,
    pub(super) value: f32,
}

/// A **device-level param group**: one exposed control bound to N node params that are driven
/// together as a single value. The device-crate concept sitting above the strictly-per-node engine
/// (`node.rs`'s layering doc keeps the engine per-node) — a real interface's single bus-power state
/// maps to a `powered` gate on every stage. The bound node params are **hidden** from the positional
/// param walk (the same convention as a port consumed by an [`InternalEdge`]); the group appears once
/// on the exposed face. Every target must carry an **identical** decl (range/default/smooth), from
/// which the descriptor's range/default derive — `catalog_group_targets_carry_identical_decls` guards
/// it.
pub(super) struct ParamGroup {
    /// The single exposed control's UI (label/unit/kind).
    pub(super) ui: ParamUi,
    /// The node params it binds: `(node index in `nodes`, node-local `ParamId`)`.
    pub(super) targets: &'static [(usize, ParamId)],
}

/// A **generated grid of crosspoint param UIs** — the routing matrix's `inputs × outputs` cells,
/// whose labels (`"{input} → {output}"`) are *synthesized* rather than hand-authored (the full 8i6's
/// matrix has 196 crosspoints — infeasible as static `ParamUi`s). Row-major (input outer, output
/// inner), matching [`Matrix`](engine::Matrix)'s crosspoint id order. The generated entries are the **last ungrouped**
/// exposed params, so a device using a grid must make its `Matrix` the *last param-contributing node*;
/// the web focus surface renders them as a grid, deriving rows/cols from the `" → "` labels.
pub(super) struct GridSpec {
    /// Input (row) axis, in matrix input order.
    pub(super) inputs: GridAxis,
    /// Output (column) axis, in matrix output order.
    pub(super) outputs: GridAxis,
    pub(super) kind: ParamKind,
    pub(super) unit: &'static str,
}

/// A grid axis (a matrix's rows or columns): today only [`Named`](GridAxis::Named) hand-authored
/// labels — one per lane on the axis.
pub(super) enum GridAxis {
    /// Hand-authored names, one per matrix lane on this axis (the 8i6's fixed 14×14).
    Named(&'static [&'static str]),
    Generated {
        prefix: &'static str,
    },
}

impl GridAxis {
    /// The axis names, in lane order.
    pub(super) fn names(&self, count: usize) -> Vec<Cow<'static, str>> {
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
    pub(super) fn labels(&self, n_in: usize, m_out: usize) -> Vec<String> {
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

pub(super) struct PortUi {
    pub(super) label: &'static str,
    pub(super) kind: PortKind,
    pub(super) connector: Connector,
}

pub(super) struct ReadoutUi {
    pub(super) label: &'static str,
    pub(super) unit: &'static str,
}

/// One per-lane meter bank: `"{prefix} {lane} {measure}"` for each lane × each `per` measure. A
/// metering node (a `DigitalMeter`) contributes one bank; its lane count is *derived* from that
/// node's exposed-readout count ÷ `per.len()`, so a config-driven meter (whose lane count isn't a
/// port total — e.g. the DAW's per-*track* meter) labels correctly without a separate count source.
pub(super) struct MeterBank {
    pub(super) prefix: &'static str,
    pub(super) per: &'static [ReadoutUi],
}

/// How a device's exposed readouts are labeled.
pub(super) enum ReadoutSpec {
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
    pub(super) fn label(&self, node_readout_counts: &[usize]) -> Vec<(String, String)> {
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
