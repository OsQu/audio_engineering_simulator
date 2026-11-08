use super::descriptor::{Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::spec::{CatalogEntry, NodeSpec, ParamUi, PortUi, ReadoutSpec};
use engine::{Ohms, SynthVoice, Volts};

pub(super) const SYNTH_VOICE: CatalogEntry = CatalogEntry {
    type_id: "synth_voice",
    name: "Synth Voice",
    // A compact desktop synth module, sized to sit alongside the 8i6 (a tabletop unit, not a
    // console): fader + ADSR knobs + a small envelope screen fit across ~220 mm.
    form_factor: FormFactor::Desktop {
        width_mm: 220.0,
        height_mm: 62.0,
        depth_mm: 150.0,
    },
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)))
    })],
    internal: &[],
    params: &[
        ParamUi {
            label: "Level",
            unit: "V",
            kind: ParamKind::Fader,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Attack",
            unit: "ms",
            kind: ParamKind::Knob,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Decay",
            unit: "ms",
            kind: ParamKind::Knob,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Sustain",
            unit: "",
            kind: ParamKind::Knob,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Release",
            unit: "ms",
            kind: ParamKind::Knob,
            taper: ParamTaper::Linear,
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
        label: "MIDI",
        kind: PortKind::Midi,
        connector: Connector::Din5,
    }],
    outputs: &[PortUi {
        label: "Out",
        kind: PortKind::Instrument,
        connector: Connector::QuarterInch,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[]),
    configs: &[],
};
