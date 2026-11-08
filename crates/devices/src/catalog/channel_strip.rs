use super::descriptor::{Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::spec::{CatalogEntry, InternalEdge, NodeSpec, ParamUi, PortUi, ReadoutSpec};
use engine::{GainStage, InputZ, Ohms, Volts};

/// The minimal **multi-node** device, proving the chassis seam: two analog gain stages in series
/// (input gain → output gain) behind one logical device. The internal edge hides stage 0's output
/// and stage 1's input; the exposed face is stage 0's input, stage 1's output, and *both* gains'
/// params (so device param 1 maps to the second node — a non-trivial remap). A strip with
/// EQ/dynamics would need an internal AD (analog can't enter a digital port), so two analog stages
/// is the smallest electrically valid multi-node device.
pub(super) const CHANNEL_STRIP: CatalogEntry = CatalogEntry {
    type_id: "channel_strip",
    name: "Channel Strip",
    form_factor: FormFactor::Rackmount { rack_units: 2 },
    nodes: &[
        NodeSpec::Plain(|_cfg| {
            Box::new(GainStage::new(
                1.0,
                Volts::new(10.0),
                InputZ::new(Ohms::new(10_000.0)),
                Ohms::new(150.0),
            ))
        }),
        NodeSpec::Plain(|_cfg| {
            Box::new(GainStage::new(
                1.0,
                Volts::new(10.0),
                InputZ::new(Ohms::new(10_000.0)),
                Ohms::new(150.0),
            ))
        }),
    ],
    internal: &[InternalEdge {
        from_node: 0,
        from_port: 0,
        to_node: 1,
        to_port: 0,
    }],
    // Params are exposed in node order — both of stage 0's, then both of stage 1's — so the
    // interleave is gain, power, gain, power (each stage carries its own power switch).
    params: &[
        ParamUi {
            label: "Input Gain",
            unit: "dB",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
        },
        ParamUi {
            label: "Input Power",
            unit: "",
            kind: ParamKind::Switch,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Output Gain",
            unit: "dB",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
        },
        ParamUi {
            label: "Output Power",
            unit: "",
            kind: ParamKind::Switch,
            taper: ParamTaper::Linear,
        },
    ],
    param_groups: &[],
    param_grid: None,
    inputs: &[PortUi {
        label: "In",
        kind: PortKind::Line,
        connector: Connector::QuarterInch,
    }],
    outputs: &[PortUi {
        label: "Out",
        kind: PortKind::Line,
        connector: Connector::QuarterInch,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[]),
    configs: &[],
};
