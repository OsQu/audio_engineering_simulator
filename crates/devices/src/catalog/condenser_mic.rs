use super::descriptor::{Connector, FormFactor, PortKind};
use super::spec::{CatalogEntry, NodeSpec, PortUi, ReadoutSpec};
use engine::{CondenserMic, Ohms};

/// A condenser microphone — the catalog face of `engine::CondenserMic`. A balanced XLR mic that is
/// **dead until phantom-fed**: the node declares its P48 DC load on its output port and `compile`
/// resolves the operating point against whatever engaged supply faces it (the 8i6's 48V config) —
/// power arrives from the patch, never a flag. It is a true **acoustic→analog transducer**: its
/// capsule takes sound pressure in (from a patched source over the ideal-air acoustic edge) and
/// converts it at a fixed sensitivity (~10 mV/Pa) — no tone params, no self-sine.
pub(super) const CONDENSER_MIC: CatalogEntry = CatalogEntry {
    type_id: "condenser_mic",
    name: "Condenser Mic",
    // A large-diaphragm studio condenser stood on the desk: LDC bodies run ~45–60 mm across
    // (the classic U 87 is 56 mm) and ~200 mm long including the grille; the footprint is the
    // body cylinder, so depth = width.
    form_factor: FormFactor::Desktop {
        width_mm: 50.0,
        height_mm: 200.0,
        depth_mm: 50.0,
    },
    // ~10 mV/Pa sensitivity (a typical studio condenser) from a 150 Ω balanced source (the
    // classic mic output impedance).
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(CondenserMic::new(0.01, Ohms::new(150.0)))
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
