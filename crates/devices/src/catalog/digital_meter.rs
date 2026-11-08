use super::descriptor::{Connector, FormFactor, PortKind};
use super::spec::{BITS, CatalogEntry, NodeSpec, PortUi, ReadoutSpec, ReadoutUi};
use engine::{BitDepth, DigitalMeter, SampleRate};

/// This device's own converter clock — per-device, never a global digital rate.
const DIGITAL_METER_CLOCK_HZ: f64 = 48_000.0;

/// A digital level meter — inline passthrough on a digital channel, reporting peak and RMS in
/// dBFS. Placed after the AD, it's the digital half of the across-converter gain-staging story.
pub(super) const DIGITAL_METER: CatalogEntry = CatalogEntry {
    type_id: "digital_meter",
    name: "Digital Meter",
    form_factor: FormFactor::Rackmount { rack_units: 1 },
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(DigitalMeter::new(
            SampleRate::new(DIGITAL_METER_CLOCK_HZ),
            BitDepth::new(BITS),
            1,
        ))
    })],
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
};
