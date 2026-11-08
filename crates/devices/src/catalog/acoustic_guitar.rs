use super::descriptor::{ConfigKind, Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::guitar::{
    ACOUSTIC_GUITAR_PRESET, ACOUSTIC_GUITAR_PRESET_OPTIONS, GUITAR_PRESET_KEY, acoustic_guitar_da,
    acoustic_guitar_monitor, acoustic_guitar_source, guitar_source_face,
};
use super::spec::{CatalogEntry, ConfigUi, InternalEdge, NodeSpec, ParamUi, PortUi, ReadoutSpec};

/// The acoustic sibling uses the same SoundFont source and DA, then radiates through the shipped
/// voltage→pressure transducer. Only air leaves its face, so it must be mic'd to enter an analog chain.
pub(super) const ACOUSTIC_GUITAR: CatalogEntry = CatalogEntry {
    type_id: "acoustic_guitar",
    name: "Acoustic Guitar",
    form_factor: FormFactor::Desktop {
        width_mm: 330.0,
        height_mm: 45.0,
        depth_mm: 110.0,
    },
    nodes: &[
        NodeSpec::Asset {
            build: acoustic_guitar_source,
            face: guitar_source_face,
        },
        NodeSpec::Plain(acoustic_guitar_da),
        NodeSpec::Plain(acoustic_guitar_monitor),
    ],
    internal: &[
        InternalEdge {
            from_node: 0,
            from_port: 0,
            to_node: 1,
            to_port: 0,
        },
        InternalEdge {
            from_node: 1,
            from_port: 0,
            to_node: 2,
            to_port: 0,
        },
    ],
    params: &[
        ParamUi {
            label: "Power",
            unit: "",
            kind: ParamKind::Switch,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Volume",
            unit: "dB",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
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
        label: "Air",
        kind: PortKind::Acoustic,
        connector: Connector::Acoustic,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[]),
    configs: &[ConfigUi {
        key: GUITAR_PRESET_KEY,
        label: "Preset",
        kind: ConfigKind::Select,
        default: ACOUSTIC_GUITAR_PRESET,
        options: ACOUSTIC_GUITAR_PRESET_OPTIONS,
    }],
};
