use super::descriptor::{Connector, FormFactor, PortKind};
use super::spec::{CatalogEntry, NodeSpec, PortUi, ReadoutSpec};
use engine::{InputZ, Ohms, Speaker};

pub(super) const SPEAKER: CatalogEntry = CatalogEntry {
    type_id: "speaker",
    name: "Speaker",
    // A compact desktop monitor — kept small so it sits with the 8i6 rather than towering over the
    // bench; a full-size main monitor is a later realistic variant.
    form_factor: FormFactor::Desktop {
        width_mm: 130.0,
        height_mm: 170.0,
        depth_mm: 150.0,
    },
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))))
    })],
    internal: &[],
    params: &[],
    param_groups: &[],
    param_grid: None,
    inputs: &[PortUi {
        label: "In",
        kind: PortKind::Speaker,
        // The speaker is a simplified powered-monitor terminus fed by the line-level DA, so its
        // input is a ¼" line jack today — this keeps the default `da→spk` connection legal. A
        // Speakon input belongs to a power amp + passive speaker pair.
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
};
