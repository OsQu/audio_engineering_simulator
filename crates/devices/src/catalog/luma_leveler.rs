use super::descriptor::{Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::spec::{CatalogEntry, NodeSpec, ParamUi, PortUi, ReadoutSpec, ReadoutUi};
use engine::LumaLeveler;

/// A fictional 3U electro-optical-style leveler: one balanced analog node owns the
/// difference-first detector, downward curve, post-reduction gain, power glide, and the two
/// readings its physical meter will select between. It stays voltage-native end to end.
pub(super) const LUMA_LEVELER: CatalogEntry = CatalogEntry {
    type_id: "luma_leveler",
    name: "Luma Leveler",
    form_factor: FormFactor::Rackmount { rack_units: 3 },
    nodes: &[NodeSpec::Plain(|_cfg| Box::new(LumaLeveler::new()))],
    internal: &[],
    params: &[
        ParamUi {
            label: "Peak Reduction",
            unit: "",
            kind: ParamKind::Knob,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Gain",
            unit: "dB",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
        },
        ParamUi {
            label: "Limit",
            unit: "",
            kind: ParamKind::Switch,
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
        label: "Line In",
        kind: PortKind::Line,
        connector: Connector::Xlr,
    }],
    outputs: &[PortUi {
        label: "Line Out",
        kind: PortKind::Line,
        connector: Connector::Xlr,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[
        ReadoutUi {
            label: "Gain Reduction",
            unit: "dB",
        },
        ReadoutUi {
            label: "Output",
            unit: "dBu",
        },
    ]),
    configs: &[],
};
