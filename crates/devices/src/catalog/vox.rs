use super::descriptor::{Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::spec::{CatalogEntry, NodeSpec, ParamUi, PortUi, ReadoutSpec};
use engine::{Pascals, SoundSource};

/// A generic acoustic sound source ("vox") — the catalog face of `engine::SoundSource`. A sine at a
/// chosen loudness radiating into the air: patch its acoustic output into a mic's capsule (over the
/// ideal-air acoustic edge) to mic it. Deliberately simple: recognizable, not realistic —
/// a stand-in for whatever makes the sound. The engine param is a linear peak pressure in pascals
/// (like a gain is linear volts); the web shows it as a dB-SPL loudness knob.
pub(super) const VOX: CatalogEntry = CatalogEntry {
    type_id: "vox",
    name: "Vox",
    // A compact desktop stand-in source (was a person-sized 1.7 m tower, which dwarfed the gear and
    // was awkward to patch): a small ~200 mm box, on the scale of a mic or a small monitor.
    form_factor: FormFactor::Desktop {
        width_mm: 200.0,
        height_mm: 200.0,
        depth_mm: 200.0,
    },
    // ~1 Pa peak (≈94 dB SPL) default at 220 Hz — a plausible sung fundamental.
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(SoundSource::new(Pascals::new(1.0)))
    })],
    internal: &[],
    params: &[
        ParamUi {
            // The engine param is a linear peak pressure in pascals; the web shows it as dB SPL (the
            // engineer's loudness unit) via the `"dB SPL"` unit — display only, the stored value stays
            // linear Pa. The log taper's dB-below-max travel is reused as-is.
            label: "Loudness",
            unit: "dB SPL",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
        },
        ParamUi {
            label: "Pitch",
            unit: "Hz",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
        },
    ],
    param_groups: &[],
    param_grid: None,
    inputs: &[],
    // One acoustic output: the sound radiating into the air.
    outputs: &[PortUi {
        label: "Air",
        kind: PortKind::Acoustic,
        connector: Connector::Acoustic,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[]),
    configs: &[],
};
