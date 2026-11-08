use super::descriptor::{Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::spec::{CatalogEntry, NodeSpec, ParamUi, PortUi, ReadoutSpec};
use engine::{GainStage, InputZ, Ohms, Volts};

pub(super) const GAIN_STAGE: CatalogEntry = CatalogEntry {
    type_id: "gain_stage",
    name: "Gain Stage",
    form_factor: FormFactor::Rackmount { rack_units: 1 },
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        ))
    })],
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
};
