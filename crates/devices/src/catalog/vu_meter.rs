use super::descriptor::{Connector, FormFactor, PortKind};
use super::spec::{CatalogEntry, NodeSpec, PortUi, ReadoutSpec, ReadoutUi};
use engine::VuMeter;

/// A voltage-native VU meter — bridging inline analog meter (unity passthrough). Its two readouts
/// ride the node→host lane: the ballistic VU reading and the block peak in dBu. The analog half of
/// "gain-staging across the AD/DA boundary made visible".
pub(super) const VU_METER: CatalogEntry = CatalogEntry {
    type_id: "vu_meter",
    name: "VU Meter",
    form_factor: FormFactor::Rackmount { rack_units: 1 },
    nodes: &[NodeSpec::Plain(|_cfg| Box::new(VuMeter::new()))],
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
};
