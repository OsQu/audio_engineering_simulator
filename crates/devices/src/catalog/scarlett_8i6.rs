use super::descriptor::{ConfigKind, Connector, FormFactor, ParamKind, ParamTaper, PortKind};
use super::spec::{
    BITS, CatalogEntry, ConfigUi, DeviceConfig, GridAxis, GridSpec, InternalEdge, NodeSpec,
    ParamGroup, ParamUi, PortUi, ReadoutSpec, ReadoutUi,
};
use engine::{
    AdConverter, BalancedReceiver, BitDepth, DaConverter, DigitalDemux, DigitalMux, EventThru,
    GainStage, InputZ, Matrix, MicPreamp, Node, NoiseDensity, Ohms, SampleRate, Volts, VuMeter,
};

/// This device's own converter clock — per-device, never a global digital rate.
const SCARLETT_CLOCK_HZ: f64 = 48_000.0;

/// The 8i6 preamp input impedances. Line-level (default) keeps today's 10 kΩ; instrument/hi-Z (INST
/// engaged) is ~1.5 MΩ so a high-output-impedance pickup isn't loaded down. The choice is baked into
/// the loading divider at compile, so it's a structural config (a toggle recompiles), not a param.
const PREAMP_LINE_Z_OHMS: f32 = 10_000.0;
const PREAMP_INST_Z_OHMS: f32 = 1_500_000.0;

/// The 8i6 preamp's input-referred self-noise (EIN) as a white-noise density. An informed
/// approximation of the Scarlett's published ≈ −128 dBu EIN (150 Ω source, max gain): over a 20 kHz
/// band that is `0.775·10^(−128/20) = 0.309 µV` RMS, i.e. `0.309e-6 / √20000 ≈ 2.2 nV/√Hz`. The
/// preamp amplifies this along with the signal, so cranking the gain (as a quiet source forces) lifts
/// the audible hiss — the whole point. A single lumped figure today; see `docs/IMPROVEMENTS.md` for
/// the emergent-thermal follow-up.
const PREAMP_EIN_V_PER_ROOT_HZ: f32 = 2.2e-9;

/// The 8i6's 48V phantom key — **one switch feeding both preamps** (the real 3rd-gen 8i6 has a
/// single global 48V button; contrast the per-channel INST keys). 48V is **structural** like INST:
/// engaging it changes the DC bias topology, so a toggle recompiles rather than smoothing a param
/// (an INST-style structural switch). Acoustically safe — the pedestal cancels at every balanced
/// receiver in both states, so the swap can't click.
pub(super) const PHANTOM_KEY: &str = "phantom";

/// Build one 8i6 preamp — a [`MicPreamp`] whose input impedance is selected by the `inst_key`
/// structural toggle (`>= 0.5` ⇒ instrument/hi-Z, else line), and whose +48 V phantom feed is
/// engaged by the **shared** [`PHANTOM_KEY`] toggle (both preamps read the same key). Defaults
/// (`0.0` = line, phantom off) reproduce the pre-INST/pre-48V behavior. Both faces are **balanced**
/// (a combo jack's XLR and TRS paths both carry the pair); an unbalanced source still seats via the
/// engine's grounding edge, with the *same* loading gain — the differential Z plays exactly the
/// role the unbalanced Z did.
fn scarlett_preamp(cfg: &DeviceConfig, inst_key: &str) -> Box<dyn Node> {
    let z_ohms = if cfg.get_or(inst_key, 0.0) >= 0.5 {
        PREAMP_INST_Z_OHMS
    } else {
        PREAMP_LINE_Z_OHMS
    };
    Box::new(
        // Default to the minimum gain (≈ +8 dB) — a real preamp powers on with the gain pot fully
        // counter-clockwise, and the knob's floor is +8 dB (never an attenuator).
        MicPreamp::new(
            MicPreamp::MIN_GAIN,
            Volts::new(10.0),
            InputZ::balanced(Ohms::new(z_ohms)),
            Ohms::new(150.0),
        )
        .with_phantom(cfg.get_or(PHANTOM_KEY, 0.0) >= 0.5)
        .with_noise(NoiseDensity::new(PREAMP_EIN_V_PER_ROOT_HZ)),
    )
}

/// The 8i6's AD/DA/gain building blocks, factored out of the (large) entry. The AD's input impedance
/// is a parameter: preamp-fed ADs present a high `z_in` (the buffered preamp drives them, so the divider
/// stays unity), while a **line input** AD presents a realistic line-level impedance the external
/// source loads against.
fn scarlett_ad(z_in_ohms: f32) -> Box<dyn Node> {
    Box::new(AdConverter::new(
        SampleRate::new(SCARLETT_CLOCK_HZ),
        BitDepth::new(BITS),
        Volts::new(1.0),
        Ohms::new(z_in_ohms),
    ))
}

/// Differential front end for each rear balanced ¼-inch TRS line input (3–6). The receiver presents
/// the specified 44 kΩ input impedance and rejects common mode before feeding the ordinary buffered
/// ADC. TS sources still work through the graph's grounded-cold path.
fn scarlett_line_receiver() -> Box<dyn Node> {
    Box::new(BalancedReceiver::new(Ohms::new(44_000.0), Ohms::new(1.0)))
}
fn scarlett_da() -> Box<dyn Node> {
    Box::new(DaConverter::new(
        SampleRate::new(SCARLETT_CLOCK_HZ),
        BitDepth::new(BITS),
        Volts::new(1.0),
        Ohms::new(150.0),
    ))
}
fn scarlett_gain() -> Box<dyn Node> {
    // The 8i6's monitor and headphone amps are **volume** controls: they attenuate from unity (0 dB,
    // fully open — the default) down to silence, never boost. Cap the GAIN at unity so the knob is a
    // proper level control, not a +60 dB preamp.
    Box::new(
        GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
        .with_gain_range(0.0, 1.0),
    )
}

/// A simplified Focusrite Scarlett 8i6 — the first **mixed-face, multi-I/O** interface.
/// Two `MicPreamp`s (gain + PAD + AIR, INST/hi-Z via structural config) each feed an AD; the two ADs
/// and a "USB return" feed a 3×3 routing **matrix**; the matrix's two "USB send" outputs go to the
/// computer and its third output drives a DA whose analog monitor bus fans out to a line output and a
/// headphone amp; MIDI passes through. The web faceplate splits the exposed face across two chassis
/// faces (front: combo inputs + phones; back: line/USB/MIDI + power), and the routing matrix is
/// driven from the Focusrite Control focus surface. 48V phantom is the shared
/// `PHANTOM_KEY` structural config: one switch engages both preamps' P48 supplies at build.
pub(super) const SCARLETT_8I6: CatalogEntry = CatalogEntry {
    type_id: "scarlett_8i6",
    name: "Scarlett 8i6",
    // The real 2nd-gen 8i6: a half-rack-width desktop box, ~216 × 47 × 173 mm (W×H×D).
    form_factor: FormFactor::Desktop {
        width_mm: 210.0,
        height_mm: 47.5,
        depth_mm: 149.5,
    },
    // Signal-flow layout. Every routing-matrix port is internal (fed by the ADs/demuxes, consumed
    // by the muxes/DAs), so the matrix is placed **last** — its 196 generated crosspoints become the
    // trailing exposed params without disturbing the port face. Node order:
    //   0,1 preamps · 2,3 preamp ADs · 4–7 line receivers (3–6) · 8–11 line-in ADs ·
    //   12 S/PDIF-in demux · 13 USB-return demux · 14 USB-send mux · 15 S/PDIF-out mux ·
    //   16,17 monitor DAs (L/R) · 18,19 monitor amps (Line Out 1/2) · 20,21 line-out DAs (3/4) ·
    //   22,23 phones amps · 24 MIDI thru · 25 matrix · 26,27 input meters (combo 1/2).
    nodes: &[
        NodeSpec::Plain(|cfg| scarlett_preamp(cfg, "inst1")),
        NodeSpec::Plain(|cfg| scarlett_preamp(cfg, "inst2")),
        NodeSpec::Plain(|_cfg| scarlett_ad(1_000_000.0)), // preamp 1 AD (buffered feed ⇒ high Z, unity divider)
        NodeSpec::Plain(|_cfg| scarlett_ad(1_000_000.0)), // preamp 2 AD
        NodeSpec::Plain(|_cfg| scarlett_line_receiver()), // line in 3 differential front end
        NodeSpec::Plain(|_cfg| scarlett_line_receiver()), // line in 4 differential front end
        NodeSpec::Plain(|_cfg| scarlett_line_receiver()), // line in 5 differential front end
        NodeSpec::Plain(|_cfg| scarlett_line_receiver()), // line in 6 differential front end
        NodeSpec::Plain(|_cfg| scarlett_ad(1_000_000.0)), // buffered line in 3 AD
        NodeSpec::Plain(|_cfg| scarlett_ad(1_000_000.0)), // buffered line in 4 AD
        NodeSpec::Plain(|_cfg| scarlett_ad(1_000_000.0)), // buffered line in 5 AD
        NodeSpec::Plain(|_cfg| scarlett_ad(1_000_000.0)), // buffered line in 6 AD
        NodeSpec::Plain(|_cfg| {
            Box::new(DigitalDemux::new(
                SampleRate::new(SCARLETT_CLOCK_HZ),
                BitDepth::new(BITS),
                2,
            ))
        }), // S/PDIF in (2ch)
        NodeSpec::Plain(|_cfg| {
            Box::new(DigitalDemux::new(
                SampleRate::new(SCARLETT_CLOCK_HZ),
                BitDepth::new(BITS),
                6,
            ))
        }), // USB return (6ch)
        NodeSpec::Plain(|_cfg| {
            Box::new(DigitalMux::new(
                SampleRate::new(SCARLETT_CLOCK_HZ),
                BitDepth::new(BITS),
                8,
            ))
        }), // USB send (8ch)
        NodeSpec::Plain(|_cfg| {
            Box::new(DigitalMux::new(
                SampleRate::new(SCARLETT_CLOCK_HZ),
                BitDepth::new(BITS),
                2,
            ))
        }), // S/PDIF out (2ch)
        NodeSpec::Plain(|_cfg| scarlett_da()),            // monitor L DA
        NodeSpec::Plain(|_cfg| scarlett_da()),            // monitor R DA
        NodeSpec::Plain(|_cfg| scarlett_gain()),          // monitor L amp → Line Out 1
        NodeSpec::Plain(|_cfg| scarlett_gain()),          // monitor R amp → Line Out 2
        NodeSpec::Plain(|_cfg| scarlett_da()),            // line out 3 DA
        NodeSpec::Plain(|_cfg| scarlett_da()),            // line out 4 DA
        NodeSpec::Plain(|_cfg| scarlett_gain()),          // phones 1 amp
        NodeSpec::Plain(|_cfg| scarlett_gain()),          // phones 2 amp
        NodeSpec::Plain(|_cfg| Box::new(EventThru::new(64))),
        // The routing matrix (Focusrite Control's mixer, gains only). 14 ins (2 preamp + 4 line +
        // 2 S/PDIF + 6 USB return) × 14 outs (8 USB send + 4 line + 2 S/PDIF). Identity default:
        // input i → output i — hardware inputs to USB sends (record), USB returns to the analog /
        // S-PDIF outs (playback) — the standard interface routing, so behavior is unchanged until
        // the user re-routes in the focus view.
        NodeSpec::Plain(|_cfg| {
            let n = 14;
            let mut d = vec![0.0; n * n];
            for i in 0..n {
                d[i * n + i] = 1.0;
            }
            Box::new(Matrix::new(
                SampleRate::new(SCARLETT_CLOCK_HZ),
                BitDepth::new(BITS),
                n,
                n,
                d,
            ))
        }),
        // 26,27: input meters on combo 1/2, inserted post-preamp (pre-AD). Appended *after* the
        // matrix; a `VuMeter` is a transparent bridging passthrough, so it meters the gained input
        // level (the ring around the gain knob) without altering the record path.
        NodeSpec::Plain(|_cfg| Box::new(VuMeter::new())), // input 1 meter
        NodeSpec::Plain(|_cfg| Box::new(VuMeter::new())), // input 2 meter
    ],
    // The internal chassis wiring. preamp→meter→AD (×2), line receiver→AD (×4), then the
    // 6 ADs + S/PDIF-in demux (2) + USB-return
    // demux (6) feed the matrix's 14 inputs; the matrix's 14 outputs feed the USB-send mux (8), the
    // monitor + line-out DAs (4), and the S/PDIF-out mux (2); each monitor DA fans to its line-out
    // amp and a phones amp.
    internal: &[
        // preamp → input meter → its AD (the meter taps the gained input level for the knob ring)
        InternalEdge {
            from_node: 0,
            from_port: 0,
            to_node: 26,
            to_port: 0,
        },
        InternalEdge {
            from_node: 26,
            from_port: 0,
            to_node: 2,
            to_port: 0,
        },
        InternalEdge {
            from_node: 1,
            from_port: 0,
            to_node: 27,
            to_port: 0,
        },
        InternalEdge {
            from_node: 27,
            from_port: 0,
            to_node: 3,
            to_port: 0,
        },
        // Rear balanced line receivers → their buffered single-ended ADs.
        InternalEdge {
            from_node: 4,
            from_port: 0,
            to_node: 8,
            to_port: 0,
        },
        InternalEdge {
            from_node: 5,
            from_port: 0,
            to_node: 9,
            to_port: 0,
        },
        InternalEdge {
            from_node: 6,
            from_port: 0,
            to_node: 10,
            to_port: 0,
        },
        InternalEdge {
            from_node: 7,
            from_port: 0,
            to_node: 11,
            to_port: 0,
        },
        // ADs (preamp 1/2, line 3–6) → matrix ins 0–5
        InternalEdge {
            from_node: 2,
            from_port: 0,
            to_node: 25,
            to_port: 0,
        },
        InternalEdge {
            from_node: 3,
            from_port: 0,
            to_node: 25,
            to_port: 1,
        },
        InternalEdge {
            from_node: 8,
            from_port: 0,
            to_node: 25,
            to_port: 2,
        },
        InternalEdge {
            from_node: 9,
            from_port: 0,
            to_node: 25,
            to_port: 3,
        },
        InternalEdge {
            from_node: 10,
            from_port: 0,
            to_node: 25,
            to_port: 4,
        },
        InternalEdge {
            from_node: 11,
            from_port: 0,
            to_node: 25,
            to_port: 5,
        },
        // S/PDIF-in demux (2ch) → matrix ins 6,7
        InternalEdge {
            from_node: 12,
            from_port: 0,
            to_node: 25,
            to_port: 6,
        },
        InternalEdge {
            from_node: 12,
            from_port: 1,
            to_node: 25,
            to_port: 7,
        },
        // USB-return demux (6ch) → matrix ins 8–13
        InternalEdge {
            from_node: 13,
            from_port: 0,
            to_node: 25,
            to_port: 8,
        },
        InternalEdge {
            from_node: 13,
            from_port: 1,
            to_node: 25,
            to_port: 9,
        },
        InternalEdge {
            from_node: 13,
            from_port: 2,
            to_node: 25,
            to_port: 10,
        },
        InternalEdge {
            from_node: 13,
            from_port: 3,
            to_node: 25,
            to_port: 11,
        },
        InternalEdge {
            from_node: 13,
            from_port: 4,
            to_node: 25,
            to_port: 12,
        },
        InternalEdge {
            from_node: 13,
            from_port: 5,
            to_node: 25,
            to_port: 13,
        },
        // matrix outs 0–7 → USB-send mux (8ch)
        InternalEdge {
            from_node: 25,
            from_port: 0,
            to_node: 14,
            to_port: 0,
        },
        InternalEdge {
            from_node: 25,
            from_port: 1,
            to_node: 14,
            to_port: 1,
        },
        InternalEdge {
            from_node: 25,
            from_port: 2,
            to_node: 14,
            to_port: 2,
        },
        InternalEdge {
            from_node: 25,
            from_port: 3,
            to_node: 14,
            to_port: 3,
        },
        InternalEdge {
            from_node: 25,
            from_port: 4,
            to_node: 14,
            to_port: 4,
        },
        InternalEdge {
            from_node: 25,
            from_port: 5,
            to_node: 14,
            to_port: 5,
        },
        InternalEdge {
            from_node: 25,
            from_port: 6,
            to_node: 14,
            to_port: 6,
        },
        InternalEdge {
            from_node: 25,
            from_port: 7,
            to_node: 14,
            to_port: 7,
        },
        // matrix outs 8–11 → monitor + line-out DAs
        InternalEdge {
            from_node: 25,
            from_port: 8,
            to_node: 16,
            to_port: 0,
        },
        InternalEdge {
            from_node: 25,
            from_port: 9,
            to_node: 17,
            to_port: 0,
        },
        InternalEdge {
            from_node: 25,
            from_port: 10,
            to_node: 20,
            to_port: 0,
        },
        InternalEdge {
            from_node: 25,
            from_port: 11,
            to_node: 21,
            to_port: 0,
        },
        // matrix outs 12,13 → S/PDIF-out mux (2ch)
        InternalEdge {
            from_node: 25,
            from_port: 12,
            to_node: 15,
            to_port: 0,
        },
        InternalEdge {
            from_node: 25,
            from_port: 13,
            to_node: 15,
            to_port: 1,
        },
        // monitor DAs fan out to their line-out amp and a phones amp
        InternalEdge {
            from_node: 16,
            from_port: 0,
            to_node: 18,
            to_port: 0,
        }, // mon L → Line Out 1
        InternalEdge {
            from_node: 16,
            from_port: 0,
            to_node: 22,
            to_port: 0,
        }, // mon L → Phones 1
        InternalEdge {
            from_node: 17,
            from_port: 0,
            to_node: 19,
            to_port: 0,
        }, // mon R → Line Out 2
        InternalEdge {
            from_node: 17,
            from_port: 0,
            to_node: 23,
            to_port: 0,
        }, // mon R → Phones 2
    ],
    // Ungrouped hand-authored params, in node order (each stage's `powered` is captured by the
    // Power group and hidden): the two preamps' gain/pad/air, then the two phones amps' gain. The
    // matrix's 196 crosspoints are the trailing ungrouped params, **generated** by `param_grid`
    // below (not hand-authored). Monitor is a group (the stereo monitor pair driven by one knob).
    // Exposed ids: 0 Gain1 · 1 Pad1 · 2 Air1 · 3 Gain2 · 4 Pad2 · 5 Air2 · 6 Phones1 · 7 Phones2 ·
    // 8–203 crosspoints · 204 Monitor · 205 Power.
    params: &[
        ParamUi {
            label: "Gain 1",
            unit: "dB",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
        },
        ParamUi {
            label: "Pad 1",
            unit: "",
            kind: ParamKind::Switch,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Air 1",
            unit: "",
            kind: ParamKind::Switch,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Gain 2",
            unit: "dB",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
        },
        ParamUi {
            label: "Pad 2",
            unit: "",
            kind: ParamKind::Switch,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Air 2",
            unit: "",
            kind: ParamKind::Switch,
            taper: ParamTaper::Linear,
        },
        ParamUi {
            label: "Phones 1",
            unit: "dB",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
        },
        ParamUi {
            label: "Phones 2",
            unit: "dB",
            kind: ParamKind::Knob,
            taper: ParamTaper::Log,
        },
    ],
    // The routing matrix's crosspoints (node 25, the last param-contributing node): 14 inputs ×
    // 14 outputs, labels generated as "input → output". Rendered as a grid in Focusrite Control.
    param_grid: Some(GridSpec {
        inputs: GridAxis::Named(&[
            "Pre 1", "Pre 2", "Line 3", "Line 4", "Line 5", "Line 6", "SPDIF L", "SPDIF R",
            "DAW 1", "DAW 2", "DAW 3", "DAW 4", "DAW 5", "DAW 6",
        ]),
        outputs: GridAxis::Named(&[
            "USB 1", "USB 2", "USB 3", "USB 4", "USB 5", "USB 6", "USB 7", "USB 8", "Line 1",
            "Line 2", "Line 3", "Line 4", "SPDIF L", "SPDIF R",
        ]),
        kind: ParamKind::Knob,
        unit: "×",
    }),
    // Two groups. **Monitor** — the single monitor-level knob over the stereo monitor pair (both
    // monitor amps' GAIN). **Power** — the whole unit's power over every stage's `powered`: both
    // preamps (id 1), the 6 ADs (id 0), the 4 DAs (id 0), and the 4 output amps (id 1). The matrix
    // has no power gate — an off device's ADs/DAs are already silenced, so every output goes quiet.
    param_groups: &[
        ParamGroup {
            ui: ParamUi {
                label: "Monitor",
                unit: "dB",
                kind: ParamKind::Knob,
                taper: ParamTaper::Log,
            },
            targets: &[(18, GainStage::GAIN), (19, GainStage::GAIN)],
        },
        ParamGroup {
            ui: ParamUi {
                label: "Power",
                unit: "",
                kind: ParamKind::Switch,
                taper: ParamTaper::Linear,
            },
            targets: &[
                (0, MicPreamp::POWERED),
                (1, MicPreamp::POWERED),
                (2, AdConverter::POWERED),
                (3, AdConverter::POWERED),
                (8, AdConverter::POWERED),
                (9, AdConverter::POWERED),
                (10, AdConverter::POWERED),
                (11, AdConverter::POWERED),
                (16, DaConverter::POWERED),
                (17, DaConverter::POWERED),
                (20, DaConverter::POWERED),
                (21, DaConverter::POWERED),
                (18, GainStage::POWERED),
                (19, GainStage::POWERED),
                (22, GainStage::POWERED),
                (23, GainStage::POWERED),
            ],
        },
    ],
    // Open inputs in node order: 2 combo (front), 4 line ins (rear), S/PDIF in (2ch), USB return
    // (6ch), MIDI in.
    inputs: &[
        PortUi {
            label: "In 1",
            kind: PortKind::Instrument,
            connector: Connector::Combo,
        },
        PortUi {
            label: "In 2",
            kind: PortKind::Instrument,
            connector: Connector::Combo,
        },
        PortUi {
            label: "Line In 3",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "Line In 4",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "Line In 5",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "Line In 6",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "S/PDIF In",
            kind: PortKind::Digital,
            connector: Connector::Spdif,
        },
        PortUi {
            label: "USB In",
            kind: PortKind::Digital,
            connector: Connector::Usb,
        },
        PortUi {
            label: "MIDI In",
            kind: PortKind::Midi,
            connector: Connector::Din5,
        },
    ],
    // Open outputs in node order: USB send (8ch), S/PDIF out (2ch), line outs 1–4, phones 1–2
    // (mirroring the monitor pair, mono), MIDI out.
    outputs: &[
        PortUi {
            label: "USB Out",
            kind: PortKind::Digital,
            connector: Connector::Usb,
        },
        PortUi {
            label: "S/PDIF Out",
            kind: PortKind::Digital,
            connector: Connector::Spdif,
        },
        PortUi {
            label: "Line Out 1",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "Line Out 2",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "Line Out 3",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "Line Out 4",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "Phones 1",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "Phones 2",
            kind: PortKind::Line,
            connector: Connector::QuarterInch,
        },
        PortUi {
            label: "MIDI Out",
            kind: PortKind::Midi,
            connector: Connector::Din5,
        },
    ],
    delayed_outputs: &[],
    // The single USB-C jack is duplex: USB Out (output 0) + USB In (input 7) are one connector.
    duplex_links: &[(0, 7)],
    // Input meters on combo 1/2 (nodes 26,27), each a VuMeter exposing VU + peak-dBu — in node
    // order, so the four exposed readouts are In 1 (VU, Peak) then In 2 (VU, Peak). The web
    // faceplate renders these as the level ring around each preamp's gain knob.
    readouts: ReadoutSpec::Static(&[
        ReadoutUi {
            label: "In 1 VU",
            unit: "VU",
        },
        ReadoutUi {
            label: "In 1 Peak",
            unit: "dBu",
        },
        ReadoutUi {
            label: "In 2 VU",
            unit: "VU",
        },
        ReadoutUi {
            label: "In 2 Peak",
            unit: "dBu",
        },
    ]),
    // INST/hi-Z per preamp: a structural toggle selecting the channel's input impedance (line
    // vs instrument), read by the preamp builders. Default off (line-level), reproducing today's
    // behavior. AIR/PAD are runtime *params* on the preamp, not structural configs.
    //
    // 48V (`PHANTOM_KEY`): the phantom supply, **one switch over both preamps** (the real unit
    // has one global button; contrast the per-channel INST keys). Structural like INST — the DC
    // bias topology changes, so toggling recompiles. Default off:
    // an unfed condenser mic is dead, and non-phantom sources are unaffected either way.
    configs: &[
        ConfigUi {
            key: "inst1",
            label: "Inst 1",
            kind: ConfigKind::Toggle,
            default: 0.0,
            options: &[],
        },
        ConfigUi {
            key: "inst2",
            label: "Inst 2",
            kind: ConfigKind::Toggle,
            default: 0.0,
            options: &[],
        },
        ConfigUi {
            key: PHANTOM_KEY,
            label: "48V",
            kind: ConfigKind::Toggle,
            default: 0.0,
            options: &[],
        },
    ],
};
