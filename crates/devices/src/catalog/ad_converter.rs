use super::descriptor::{Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::spec::{BITS, CatalogEntry, NodeSpec, ParamUi, PortUi, ReadoutSpec};
use engine::{AdConverter, BitDepth, Ohms, SampleRate, Volts};

/// This device's own converter clock — per-device, never a global digital rate.
const AD_CLOCK_HZ: f64 = 48_000.0;

pub(super) const AD_CONVERTER: CatalogEntry = CatalogEntry {
    type_id: "ad_converter",
    name: "AD Converter",
    form_factor: FormFactor::Rackmount { rack_units: 1 },
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(AdConverter::new(
            SampleRate::new(AD_CLOCK_HZ),
            BitDepth::new(BITS),
            Volts::new(1.0),
            Ohms::new(1_000_000.0),
        ))
    })],
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
};
