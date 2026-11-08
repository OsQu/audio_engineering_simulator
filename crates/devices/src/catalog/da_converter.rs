use super::descriptor::{Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::spec::{BITS, CatalogEntry, NodeSpec, ParamUi, PortUi, ReadoutSpec};
use engine::{BitDepth, DaConverter, Ohms, SampleRate, Volts};

/// This device's own converter clock — per-device, never a global digital rate.
const DA_CLOCK_HZ: f64 = 48_000.0;

pub(super) const DA_CONVERTER: CatalogEntry = CatalogEntry {
    type_id: "da_converter",
    name: "DA Converter",
    form_factor: FormFactor::Rackmount { rack_units: 1 },
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(DaConverter::new(
            SampleRate::new(DA_CLOCK_HZ),
            BitDepth::new(BITS),
            Volts::new(1.0),
            Ohms::new(150.0),
        ))
    })],
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
};
