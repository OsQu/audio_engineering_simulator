//! Expanding a catalog entry: deriving its exposed face from its nodes, building it into a graph
//! (`instantiate_with_assets`), and describing it for the UI (`descriptors` / `describe_device`).

use super::CATALOG;
use super::descriptor::{
    ConfigDescriptor, ConfigOption, DeviceDescriptor, ParamDescriptor, ParamKind, ParamTaper,
    PortDescriptor, PortDirection, ReadoutDescriptor,
};
#[cfg(test)]
use super::spec::AssetNodeBuilder;
use super::spec::{CatalogEntry, DeviceConfig, NodeFace, NodeSpec};
use crate::assets::{AssetBuildError, BuildContext};
use engine::{Domain, Graph, Node, NodeId, ParamId, ReadoutId};

/// Build an asset-backed node with the explicit context supplied by patch assembly.
///
/// The fallible counterpart to the catalog's ordinary node builders; tests use it to drive an
/// [`AssetNodeBuilder`] directly.
#[cfg(test)]
pub(crate) fn build_asset_node(
    builder: AssetNodeBuilder,
    config: &DeviceConfig,
    context: &BuildContext,
) -> Result<Box<dyn Node>, AssetBuildError> {
    builder(config, context)
}

/// A built device's footprint in a graph: its engine nodes and the resolved maps from device-level
/// ports/params to concrete `(NodeId, …)`. Built by `instantiate_with_assets`; consumed by
/// `build_patch` to remap inter-device connections to graph edges and to resolve control handles.
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
pub(super) struct ExposedPort {
    pub(super) node: usize,
    pub(super) port: usize,
    pub(super) domain: Domain,
    /// The port's lane count (digital channels / analog conductors / 1 for events), from the face.
    pub(super) channels: u16,
}

/// One exposed param: the `(node index, ParamId)` target(s) it drives, plus the decl's range/default
/// (engine truth, copied so the descriptor needn't re-introspect). An ungrouped param has exactly one
/// target; a [`ParamGroup`](super::spec::ParamGroup) has N (all carrying the same decl, so one range/default describes them).
pub(super) struct ExposedParam {
    pub(super) targets: Vec<(usize, ParamId)>,
    pub(super) min: f32,
    pub(super) max: f32,
    pub(super) default: f32,
}

/// One exposed readout: which internal node + `ReadoutId` it is (no range — a readout is a plain
/// scalar output).
pub(super) struct ExposedReadout {
    pub(super) node: usize,
    pub(super) id: ReadoutId,
}

/// A device's built nodes plus its exposed face (open ports + all params + all readouts),
/// node-index-based. Shared by [`instantiate_with_assets`] (maps node indices → `NodeId`) and
/// [`describe`] (reads domains + UI labels).
pub(super) struct Expansion {
    pub(super) nodes: Vec<Box<dyn Node>>,
    pub(super) face: FaceExpansion,
}

/// The exposed ports, params, and readouts computed from node declarations alone.
pub(super) struct FaceExpansion {
    pub(super) inputs: Vec<ExposedPort>,
    pub(super) outputs: Vec<ExposedPort>,
    pub(super) params: Vec<ExposedParam>,
    pub(super) readouts: Vec<ExposedReadout>,
}

/// The catalog entry for `type_id`, or `None` if unknown.
pub(super) fn entry(type_id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.type_id == type_id)
}

/// Build a device's nodes and compute its exposed face by convention: an input/output port is exposed
/// when no internal edge consumes it (open ports, in node order); every node param is exposed,
/// concatenated in node order. Cold path; the node-building cost is negligible.
pub(super) fn expose(entry: &CatalogEntry, nodes: &[NodeFace]) -> FaceExpansion {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut params = Vec::new();
    let mut readouts = Vec::new();

    for (ni, node) in nodes.iter().enumerate() {
        for (port, face) in node.inputs.iter().enumerate() {
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
        for (port, face) in node.outputs.iter().enumerate() {
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
        for decl in &node.params {
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
        for decl in &node.readouts {
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
            .params
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

    FaceExpansion {
        inputs,
        outputs,
        params,
        readouts,
    }
}

pub(super) fn node_face(node: &dyn Node) -> NodeFace {
    NodeFace {
        inputs: node.inputs().to_vec(),
        outputs: node.outputs().to_vec(),
        params: node.params().to_vec(),
        readouts: node.readouts().to_vec(),
    }
}

pub(super) fn describe_expansion(entry: &CatalogEntry, config: &DeviceConfig) -> FaceExpansion {
    let nodes: Vec<NodeFace> = entry
        .nodes
        .iter()
        .map(|spec| match spec {
            NodeSpec::Plain(build) => node_face(&*build(config)),
            NodeSpec::Asset { face, .. } => face(config),
        })
        .collect();
    expose(entry, &nodes)
}

pub(super) fn build_expansion(
    entry: &CatalogEntry,
    config: &DeviceConfig,
    context: &BuildContext,
) -> Result<Expansion, AssetBuildError> {
    let nodes: Vec<Box<dyn Node>> = entry
        .nodes
        .iter()
        .map(|spec| match spec {
            NodeSpec::Plain(build) => Ok(build(config)),
            NodeSpec::Asset { build, .. } => build(config, context),
        })
        .collect::<Result<_, _>>()?;
    let faces: Vec<NodeFace> = nodes.iter().map(|node| node_face(&**node)).collect();
    Ok(Expansion {
        face: expose(entry, &faces),
        nodes,
    })
}

/// [`instantiate_with_assets`] with an empty asset library, for tests of asset-free entries. `None`
/// if the type is unknown or an asset failed to build.
#[cfg(test)]
pub(crate) fn instantiate(
    type_id: &str,
    config: &DeviceConfig,
    g: &mut Graph,
) -> Option<BuiltDevice> {
    let assets = crate::assets::SoundFontBankLibrary::default();
    instantiate_with_assets(type_id, config, g, &BuildContext::new(&assets))
        .ok()
        .flatten()
}

/// Expand the device type `type_id` (built with structural `config`) into `g`: add its node(s), wire
/// its internal edges, and return the instance map (device-level ports/params → concrete
/// `(NodeId, …)`).
///
/// The chassis-seam primitive: `build_patch` calls this per device with the instance's config, then
/// uses the returned [`BuiltDevice`] to remap inter-device connections and resolve control handles.
/// Most devices' exposed face is config-independent (a config changes baked values, not which
/// ports/params exist), but a **channel-count config** can resize it — the `computer`'s
/// `usb_sends`/`usb_returns` grow its port lanes, crosspoints, and readouts — so the returned map is
/// built for *this* config, not a canonical one.
///
/// Ordinary catalog entries ignore `context`; an asset-backed entry (the guitars) resolves the
/// SoundFont bank it needs from it. `Ok(None)` means the type is unknown; `Err` preserves an
/// asset-construction failure for patch assembly.
pub fn instantiate_with_assets(
    type_id: &str,
    config: &DeviceConfig,
    g: &mut Graph,
    context: &BuildContext,
) -> Result<Option<BuiltDevice>, AssetBuildError> {
    let Some(entry) = entry(type_id) else {
        return Ok(None);
    };
    let Expansion {
        nodes,
        face:
            FaceExpansion {
                inputs,
                outputs,
                params,
                readouts,
            },
    } = build_expansion(entry, config, context)?;

    let node_ids: Vec<NodeId> = nodes.into_iter().map(|node| g.add_boxed(node)).collect();
    for edge in entry.internal {
        g.connect_ideal(
            node_ids[edge.from_node],
            edge.from_port,
            node_ids[edge.to_node],
            edge.to_port,
        );
    }

    Ok(Some(BuiltDevice {
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
    }))
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
pub(super) fn describe(entry: &CatalogEntry, config: &DeviceConfig) -> DeviceDescriptor {
    // The type-catalog descriptor is built with the EMPTY config — the device's *default* face (2×2
    // for the config-driven computer). A per-instance, config-aware descriptor is a later story.
    let face = describe_expansion(entry, config);
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
            options: c
                .options
                .iter()
                .map(|option| ConfigOption {
                    label: option.label.to_owned(),
                    value: option.value,
                })
                .collect(),
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

/// The descriptor of device type `type_id` built with structural `config` — the same face
/// [`descriptors`] lists, but sized for *this* config (a channel-count config such as the
/// `computer`'s `usb_sends` resizes ports, params and readouts). `None` if the type is unknown.
/// Describes without building: asset-backed entries never parse their asset here.
#[must_use]
pub fn describe_device(type_id: &str, config: &DeviceConfig) -> Option<DeviceDescriptor> {
    entry(type_id).map(|entry| describe(entry, config))
}
