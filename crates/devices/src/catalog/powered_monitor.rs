use super::descriptor::{Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::spec::{CatalogEntry, NodeSpec, ParamUi, PortUi, ReadoutSpec};
use engine::{InputZ, Ohms, PoweredMonitor};

/// A powered monitor — the catalog face of `engine::PoweredMonitor`, the **output-side mirror** of a
/// microphone. It takes a line-level drive voltage and **radiates it as sound pressure**
/// (`pressure = drive_volts × sensitivity × volume`, ~1 Pa/V ⇒ 1 V ≈ 94 dB SPL at unity), so a device
/// can now be **mic'd**: patch its Air output into a mic capsule over the ideal-air acoustic edge.
/// Distinct from the `speaker` terminus above — that one stays in volts (the voltage-capture tap).
/// Wire `mic → preamp → monitor → air → mic` and raise the gain to hear howlround emerge: the
/// delayed air edge breaks the loop and the feedback is not special-cased.
pub(super) const POWERED_MONITOR: CatalogEntry = CatalogEntry {
    type_id: "powered_monitor",
    name: "Powered Monitor",
    // A compact desktop studio monitor — same scale as the `speaker` terminus and the 8i6.
    form_factor: FormFactor::Desktop {
        width_mm: 130.0,
        height_mm: 170.0,
        depth_mm: 150.0,
    },
    // ~1 Pa/V (the calibration anchor: 1 V ⇒ 1 Pa ≈ 94 dB SPL at unity volume), on a bridging line
    // input so it doesn't load its source (like the speaker terminus — we model no amplifier power).
    nodes: &[NodeSpec::Plain(|_cfg| {
        Box::new(PoweredMonitor::new(1.0, InputZ::new(Ohms::new(10_000.0))))
    })],
    internal: &[],
    params: &[ParamUi {
        // Volume is a linear level multiplier (an attenuator from unity); the web shows it in dB via
        // the `"dB"` unit + log taper, exactly like a gain/volume fader (0 dB = unity, −∞ at 0).
        label: "Volume",
        unit: "dB",
        kind: ParamKind::Knob,
        taper: ParamTaper::Log,
    }],
    param_groups: &[],
    param_grid: None,
    // A line-level input, fed by an interface line/monitor out over a ¼" jack (like the speaker).
    inputs: &[PortUi {
        label: "In",
        kind: PortKind::Line,
        connector: Connector::QuarterInch,
    }],
    // One acoustic output: the sound radiating into the air (mates only with a mic capsule).
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
