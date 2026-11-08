use super::descriptor::{Connector, FormFactor, PortKind};
use super::spec::{BITS, CatalogEntry, NodeSpec, PortUi, ReadoutSpec};
use engine::{BitDepth, EqBand, SampleRate, ThreeBandEq};

/// This device's own converter clock — per-device, never a global digital rate.
const EQ_CLOCK_HZ: f64 = 48_000.0;

pub(super) const THREE_BAND_EQ: CatalogEntry = CatalogEntry {
    type_id: "three_band_eq",
    name: "3-Band EQ",
    form_factor: FormFactor::Rackmount { rack_units: 1 },
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(ThreeBandEq::new(
            SampleRate::new(EQ_CLOCK_HZ),
            BitDepth::new(BITS),
            // Transparent default (all 0 dB): the UI sets bands later. Frequencies are the usual
            // low-shelf / mid-peak / high-shelf split.
            EqBand::new(100.0, 0.7, 0.0),
            EqBand::new(1_000.0, 0.7, 0.0),
            EqBand::new(8_000.0, 0.7, 0.0),
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
        label: "Out",
        kind: PortKind::Digital,
        connector: Connector::Digital,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[]),
    configs: &[],
};
