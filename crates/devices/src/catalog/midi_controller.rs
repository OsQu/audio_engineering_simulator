use super::descriptor::{Connector, FormFactor, PortKind};
use super::spec::{CatalogEntry, NodeSpec, PortUi, ReadoutSpec};
use engine::EventThru;

/// A standalone MIDI controller: a keybed with no sound of its own that *produces* a performance
/// and forwards it to MIDI-OUT (an `EventThru` — the identity event processor). Its MIDI-IN is the
/// open, host-fed input a human plays via the focus surface (or another controller patches into);
/// the cable from MIDI-OUT to a synth's MIDI-IN is the events connection the UI drives. No sound,
/// no params — pure event plumbing.
pub(super) const MIDI_CONTROLLER: CatalogEntry = CatalogEntry {
    type_id: "midi_controller",
    name: "MIDI Controller",
    // A compact controller/thru box (played via the focus keybed), 8i6-width so the stock set reads
    // as one tidy scale — the true-to-life 49-key (~800 mm) variant is a later, realistic addition.
    form_factor: FormFactor::Desktop {
        width_mm: 210.0,
        height_mm: 42.0,
        depth_mm: 110.0,
    },
    nodes: &[NodeSpec::Plain(|_cfg| Box::new(EventThru::new(64)))],
    internal: &[],
    params: &[],
    param_groups: &[],
    param_grid: None,
    inputs: &[PortUi {
        label: "MIDI In",
        kind: PortKind::Midi,
        connector: Connector::Din5,
    }],
    outputs: &[PortUi {
        label: "MIDI Out",
        kind: PortKind::Midi,
        connector: Connector::Din5,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[]),
    configs: &[],
};
