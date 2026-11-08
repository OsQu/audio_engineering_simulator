use super::descriptor::{Connector, FormFactor, PortKind};
use super::spec::{CatalogEntry, NodeSpec, PortUi, ReadoutSpec};
use engine::{DynamicMic, Ohms};

/// A dynamic microphone — the catalog face of `engine::DynamicMic`. A **passive** balanced XLR mic:
/// a moving coil that generates its own EMF, so it draws no phantom (no DC load declared) and is
/// always alive regardless of the 8i6's 48V toggle. It is an **acoustic→analog transducer** like
/// the condenser, only **less sensitive** (~2 mV/Pa vs ~10 mV/Pa — a factor of 5, ~14 dB), so it
/// leans harder on preamp gain — emergent from the transducer efficiency, not an asserted level.
pub(super) const DYNAMIC_MIC: CatalogEntry = CatalogEntry {
    type_id: "dynamic_mic",
    name: "Dynamic Mic",
    // A handheld/cardioid dynamic laid in its stand: a stubby body ~35 mm across the barrel and
    // ~160 mm long including the ball grille; footprint is the body cylinder, so depth = width.
    form_factor: FormFactor::Desktop {
        width_mm: 35.0,
        height_mm: 160.0,
        depth_mm: 35.0,
    },
    // ~2 mV/Pa sensitivity (a quieter, less-efficient moving coil) from a 200 Ω balanced source
    // (typical moving-coil output impedance).
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(DynamicMic::new(0.002, Ohms::new(200.0)))
    })],
    internal: &[],
    params: &[],
    param_groups: &[],
    param_grid: None,
    // The capsule: an acoustic input you patch a sound source into over the ideal-air edge.
    inputs: &[PortUi {
        label: "Capsule",
        kind: PortKind::Acoustic,
        connector: Connector::Acoustic,
    }],
    outputs: &[PortUi {
        label: "Out",
        kind: PortKind::Mic,
        connector: Connector::Xlr,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[]),
    configs: &[],
};
