use super::descriptor::{ConfigKind, Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::guitar::{
    ELECTRIC_GUITAR_PRESET, ELECTRIC_GUITAR_PRESET_OPTIONS, GUITAR_PRESET_KEY, electric_guitar_da,
    electric_guitar_source, guitar_source_face,
};
use super::spec::{CatalogEntry, ConfigUi, InternalEdge, NodeSpec, ParamUi, PortUi, ReadoutSpec};

/// A digital guitar module with a pickup-like electrical face: SoundFont playback crosses one
/// internal DA, then presents a deliberately high source impedance to make cable/loading lessons
/// observable at an existing instrument input.
pub(super) const ELECTRIC_GUITAR: CatalogEntry = CatalogEntry {
    type_id: "electric_guitar",
    name: "Electric Guitar",
    form_factor: FormFactor::Desktop {
        width_mm: 330.0,
        height_mm: 45.0,
        depth_mm: 110.0,
    },
    nodes: &[
        NodeSpec::Asset {
            build: electric_guitar_source,
            face: guitar_source_face,
        },
        NodeSpec::Plain(electric_guitar_da),
    ],
    internal: &[InternalEdge {
        from_node: 0,
        from_port: 0,
        to_node: 1,
        to_port: 0,
    }],
    params: &[ParamUi {
        label: "Power",
        unit: "",
        kind: ParamKind::Switch,
        taper: ParamTaper::Linear,
    }],
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
    configs: &[ConfigUi {
        key: GUITAR_PRESET_KEY,
        label: "Preset",
        kind: ConfigKind::Select,
        default: ELECTRIC_GUITAR_PRESET,
        options: ELECTRIC_GUITAR_PRESET_OPTIONS,
    }],
};
