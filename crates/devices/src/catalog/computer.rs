use engine::{BitDepth, DigitalMeter, Matrix, MultitrackRecorder, SampleRate};

use crate::{
    Connector, DeviceConfig, ParamKind, PortKind,
    catalog::{
        BITS, CatalogEntry, FormFactor, GridAxis, GridSpec, HOST_RATE_HZ, InternalEdge, MeterBank,
        PortUi, ReadoutSpec, ReadoutUi,
    },
};

const USB_SENDS: &str = "usb_sends";
const USB_RETURNS: &str = "usb_returns";
const TRACK_COUNT: &str = "track_count";
// Configs are floats.
const DEFAULT_USB_CHANNELS_CONFIG: f32 = 2.0;
const DEFAULT_TRACK_COUNT: f32 = 1.0;

fn usb_channel_count(cfg: &DeviceConfig) -> (usize, usize) {
    let sends = cfg
        .get_or(USB_SENDS, DEFAULT_USB_CHANNELS_CONFIG)
        .round()
        .max(1.0) as usize;

    let returns = cfg
        .get_or(USB_RETURNS, DEFAULT_USB_CHANNELS_CONFIG)
        .round()
        .max(1.0) as usize;

    (sends, returns)
}

/// The DAW's track count — a hidden structural config (host-written, like the USB counts), at least 1.
fn track_count(cfg: &DeviceConfig) -> usize {
    cfg.get_or(TRACK_COUNT, DEFAULT_TRACK_COUNT)
        .round()
        .max(1.0) as usize
}

/// Crossbar routing defaults for the DAW's `n_tracks → m_returns` bus matrix: route **every track →
/// return 0** (master) at unity, everything else muted (row-major `t·m_returns + j`). So out of the
/// box every track channel is heard on master — and the default scene's one monitor track (send 0)
/// carries the mic/synth to the monitors.
fn crossbar_defaults(n_tracks: usize, m_returns: usize) -> Vec<f32> {
    let mut d = vec![0.0; n_tracks * m_returns];
    if m_returns > 0 {
        for t in 0..n_tracks {
            d[t * m_returns] = 1.0; // track t → return 0 (master)
        }
    }
    d
}

// The `computer` — the interface's USB peer, and a minimal **DAW**. A real computer has no channel
// count of its own; it adapts to whatever the attached interface's driver publishes. So its shape is
// **config-driven**: hidden `usb_sends` / `usb_returns` keys size the USB bus and `track_count` sizes
// the DAW's tracks. Unattached it defaults to **2×2** (the built-in sound card) with **1** track.
//
// It is a **five-node, lane-bundled chassis** — the channel-strip mixer topology:
//   * node 0 — `DigitalMeter(N)`: the DAW's **send input meters** (pre-fader, for record levels);
//   * node 1 — `MultitrackRecorder(N → T)`: T **track channels**, each `(playback + monitored send)
//     × per-track fader`; records armed sends; owns the transport;
//   * node 2 — `DigitalMeter(T)`: the **per-track after-fader meters**;
//   * node 3 — `Matrix(T → M)`: the **bus crossbar**, routing/summing track channels to the returns
//     (default: every track → return 0 = master), fan-out/aux by adding crosspoints;
//   * node 4 — `DigitalMeter(M)`: the **return / bus meters**.
// Four internal edges chain them. Faders live on the *tracks* (a real desk trims live inputs at the
// preamp, not the DAW); routing lives in the `Matrix`; so track count is independent of the
// interface's channel count (30 tracks fold to a 2-lane master; a track fans out to an aux return).
// The transport, arm, monitor, input-assign, track faders, and record/playback file streams are all
// driven over the wasm control seam, not as params.
pub(super) const COMPUTER: CatalogEntry = CatalogEntry {
    type_id: "computer",
    name: "Computer",
    // A compact laptop-ish DAW host, trimmed to sit with the 8i6; tall enough to show the USB send
    // meters on the front. A true 15" footprint is a later realistic variant.
    form_factor: FormFactor::Desktop {
        width_mm: 240.0,
        height_mm: 28.0,
        depth_mm: 175.0,
    },
    nodes: &[
        // 0 — send input meters (pre-fader), all N send lanes behind one port.
        |cfg| {
            let (sends, _returns) = usb_channel_count(cfg);
            Box::new(DigitalMeter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                sends as u16,
            ))
        },
        // 1 — the track channels: N sends in → T post-fader channels out.
        |cfg| {
            let (sends, _returns) = usb_channel_count(cfg);
            let tracks = track_count(cfg);
            Box::new(MultitrackRecorder::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                sends,
                tracks,
            ))
        },
        // 2 — per-track after-fader meters.
        |cfg| {
            let tracks = track_count(cfg);
            Box::new(DigitalMeter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                tracks as u16,
            ))
        },
        // 3 — the bus crossbar: T track channels → M returns.
        |cfg| {
            let (_sends, returns) = usb_channel_count(cfg);
            let tracks = track_count(cfg);
            Box::new(Matrix::new_single_ports(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                tracks,
                returns,
                crossbar_defaults(tracks, returns),
            ))
        },
        // 4 — return / bus meters.
        |cfg| {
            let (_sends, returns) = usb_channel_count(cfg);
            Box::new(DigitalMeter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                returns as u16,
            ))
        },
    ],
    internal: &[
        // send meter -> recorder -> track meter -> crossbar -> return meter
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
        InternalEdge {
            from_node: 2,
            from_port: 0,
            to_node: 3,
            to_port: 0,
        },
        InternalEdge {
            from_node: 3,
            from_port: 0,
            to_node: 4,
            to_port: 0,
        },
    ],
    // All exposed params are the crossbar's `T·M` crosspoints (the meters and recorder have none),
    // generated by the grid.
    params: &[],
    // The bus crossbar (node 3, the only param-contributing node). Rows = the T track channels, cols
    // = the M returns; the row count is derived from the matrix's crosspoints (see `describe`).
    param_grid: Some(GridSpec {
        inputs: GridAxis::Generated { prefix: "Track" },
        outputs: GridAxis::Generated { prefix: "Return" },
        kind: ParamKind::Knob,
        unit: "×",
    }),
    param_groups: &[],
    inputs: &[PortUi {
        label: "USB In",
        kind: PortKind::Digital,
        connector: Connector::Usb,
    }],
    outputs: &[PortUi {
        label: "USB Out",
        kind: PortKind::Digital,
        connector: Connector::Usb,
    }],
    // The USB return (output 0) is the DAW's playback — one block behind what it records — so edges
    // from it are **delayed**: `build_patch` wires them with round-trip latency, letting the
    // interface → computer → interface monitoring loop close without a same-block cycle.
    delayed_outputs: &[0],
    // The single USB-C jack is duplex: USB Out (0) + USB In (0) are one physical connector.
    duplex_links: &[(0, 0)],
    // Three meter banks, in node order: send inputs (node 0), per-track after-fader (node 2), and
    // return/bus (node 4). Each bank's lane count derives from its meter node's readout count.
    readouts: ReadoutSpec::PerNode(&[
        MeterBank {
            prefix: "Send",
            per: &[
                ReadoutUi {
                    label: "Peak",
                    unit: "dBFS",
                },
                ReadoutUi {
                    label: "RMS",
                    unit: "dBFS",
                },
            ],
        },
        MeterBank {
            prefix: "Track",
            per: &[
                ReadoutUi {
                    label: "Peak",
                    unit: "dBFS",
                },
                ReadoutUi {
                    label: "RMS",
                    unit: "dBFS",
                },
            ],
        },
        MeterBank {
            prefix: "Return",
            per: &[
                ReadoutUi {
                    label: "Peak",
                    unit: "dBFS",
                },
                ReadoutUi {
                    label: "RMS",
                    unit: "dBFS",
                },
            ],
        },
    ]),
    configs: &[],
};

#[cfg(test)]
mod tests {
    use engine::Graph;

    use super::crossbar_defaults;
    use crate::scene::ConfigSetting;
    use crate::{DeviceConfig, PortDirection, describe_device, descriptors, instantiate};

    /// The crossbar default routes every track → return 0 (master), muting everything else
    /// (row-major `t·m + j`).
    #[test]
    fn crossbar_defaults_route_every_track_to_master() {
        // 1 track × 2 returns: track 0 → return 0 = id 0.
        assert_eq!(crossbar_defaults(1, 2), vec![1.0, 0.0]);
        // 3 tracks × 2 returns: (0,0)=id 0, (1,0)=id 2, (2,0)=id 4 at unity.
        assert_eq!(crossbar_defaults(3, 2), vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
    }

    /// Unattached, the `computer` is the **built-in 2×2 / 1-track DAW**: a five-node chassis — send
    /// meter(2) → recorder(2→1) → track meter(1) → crossbar(1→2) → return meter(2) — wired by four
    /// internal edges. USB in exposes the send meter; USB out the return meter; 2 crosspoints (1
    /// track × 2 returns) and 10 readouts (2 sends + 1 track + 2 returns, each Peak+RMS).
    #[test]
    fn default_computer_is_the_2x2_one_track_daw() {
        let mut g = Graph::new();
        let dev = instantiate("computer", &DeviceConfig::EMPTY, &mut g)
            .expect("computer is in the catalog");

        assert_eq!(
            dev.nodes.len(),
            5,
            "send meter + recorder + track meter + crossbar + return meter"
        );
        assert_eq!(g.connection_count(), 4, "four lane-bundled internal edges");

        assert_eq!(dev.inputs, vec![(dev.nodes[0], 0)], "USB in = send meter");
        assert_eq!(
            dev.outputs,
            vec![(dev.nodes[4], 0)],
            "USB out = return meter"
        );

        // Crossbar (1 track × 2 returns) = 2 crosspoints, all on the matrix (node 3).
        assert_eq!(dev.params.len(), 2, "1 track × 2 returns = 2 crosspoints");
        assert!(
            dev.params
                .iter()
                .all(|t| t.len() == 1 && t[0].0 == dev.nodes[3]),
            "every exposed param is a crossbar crosspoint"
        );

        // Readouts: 2 sends + 1 track + 2 returns, each (Peak, RMS) → 2·(2+1+2) = 10.
        assert_eq!(dev.readouts.len(), 2 * (2 + 1 + 2));
        // Banks land on nodes 0 (sends), 2 (tracks), 4 (returns).
        assert_eq!(
            dev.readouts[0].0, dev.nodes[0],
            "first readout on the send meter"
        );
        assert_eq!(
            dev.readouts[4].0, dev.nodes[2],
            "track readout on the track meter"
        );
        assert_eq!(
            dev.readouts[6].0, dev.nodes[4],
            "return readout on the return meter"
        );
    }

    /// The type-catalog descriptor (EMPTY config) advertises the default 2×2 / 1-track face: USB ports
    /// carry 2 lanes each, the crossbar is 1 track × 2 returns with generated labels, and the three
    /// meter banks label Send/Track/Return.
    #[test]
    fn default_computer_descriptor_is_2x2_one_track() {
        let dev = descriptors()
            .into_iter()
            .find(|d| d.type_id == "computer")
            .expect("computer is in the catalog");

        let usb_in = dev
            .ports
            .iter()
            .find(|p| p.direction == PortDirection::Input)
            .expect("a USB input");
        let usb_out = dev
            .ports
            .iter()
            .find(|p| p.direction == PortDirection::Output)
            .expect("a USB output");
        assert_eq!(usb_in.channels, 2, "USB in = 2 send lanes");
        assert_eq!(usb_out.channels, 2, "USB out = 2 return lanes");

        // 1 track × 2 returns = 2 crosspoints, labels "Track i → Return j".
        assert_eq!(dev.params.len(), 2);
        assert_eq!(dev.params[0].label, "Track 1 → Return 1");
        assert_eq!(dev.params[1].label, "Track 1 → Return 2");
        // Default routes track 0 → return 0 (id 0) at unity; track 0 → return 1 muted.
        assert_eq!(dev.params[0].default, 1.0);
        assert_eq!(dev.params[1].default, 0.0);

        // Readouts: Send 1/2, Track 1, Return 1/2, each Peak+RMS (10 total).
        assert_eq!(dev.readouts.len(), 10);
        assert_eq!(dev.readouts[0].label, "Send 1 Peak");
        assert_eq!(dev.readouts[3].label, "Send 2 RMS");
        assert_eq!(dev.readouts[4].label, "Track 1 Peak");
        assert_eq!(dev.readouts[6].label, "Return 1 Peak");
        assert_eq!(dev.readouts[9].label, "Return 2 RMS");
    }

    /// Attaching an 8×6 interface (default 1 track): send meter(8) → recorder(8→1) → track meter(1) →
    /// crossbar(1→6) → return meter(6). 6 crosspoints; 2·(8+1+6) = 30 readouts.
    #[test]
    fn configured_computer_expands_to_8x6() {
        let settings = [
            ConfigSetting {
                key: "usb_sends".into(),
                value: 8.0,
            },
            ConfigSetting {
                key: "usb_returns".into(),
                value: 6.0,
            },
        ];
        let config = DeviceConfig::new(&settings);
        let mut g = Graph::new();
        let dev = instantiate("computer", &config, &mut g).expect("computer is in the catalog");

        assert_eq!(dev.nodes.len(), 5);
        assert_eq!(dev.params.len(), 6, "1 track × 6 returns = 6 crosspoints");
        assert_eq!(
            dev.readouts.len(),
            2 * (8 + 1 + 6),
            "8 sends + 1 track + 6 returns, each ×2"
        );
    }

    /// `track_count` sizes the DAW independently of the USB channels: 8 sends / 6 returns / 4 tracks →
    /// a recorder emitting 4 channels, a 4×6 crossbar (24 crosspoints), and a 4-lane track meter.
    #[test]
    fn track_count_config_sizes_the_daw() {
        let settings = [
            ConfigSetting {
                key: "usb_sends".into(),
                value: 8.0,
            },
            ConfigSetting {
                key: "usb_returns".into(),
                value: 6.0,
            },
            ConfigSetting {
                key: "track_count".into(),
                value: 4.0,
            },
        ];
        let mut g = Graph::new();
        let dev = instantiate("computer", &DeviceConfig::new(&settings), &mut g)
            .expect("computer is in the catalog");
        assert_eq!(dev.nodes.len(), 5);
        assert_eq!(
            dev.params.len(),
            4 * 6,
            "4 tracks × 6 returns = 24 crosspoints"
        );
        assert_eq!(
            dev.readouts.len(),
            2 * (8 + 4 + 6),
            "8 sends + 4 tracks + 6 returns, each ×2"
        );
    }

    /// The **per-instance** descriptor scales to an 8×6 / 4-track interface: 8-ch / 6-ch USB faces, a
    /// 4×6 crossbar (24 crosspoints, "Track i → Return j"), and Send/Track/Return meter banks.
    #[test]
    fn configured_computer_descriptor_is_8x6_four_tracks() {
        let settings = [
            ConfigSetting {
                key: "usb_sends".into(),
                value: 8.0,
            },
            ConfigSetting {
                key: "usb_returns".into(),
                value: 6.0,
            },
            ConfigSetting {
                key: "track_count".into(),
                value: 4.0,
            },
        ];
        let dev = describe_device("computer", &DeviceConfig::new(&settings))
            .expect("computer is in the catalog");

        let usb_in = dev
            .ports
            .iter()
            .find(|p| p.direction == PortDirection::Input)
            .expect("a USB input");
        let usb_out = dev
            .ports
            .iter()
            .find(|p| p.direction == PortDirection::Output)
            .expect("a USB output");
        assert_eq!(usb_in.channels, 8);
        assert_eq!(usb_out.channels, 6);

        // 4 tracks × 6 returns = 24 crosspoints; last id 23 = 3·6 + 5 = "Track 4 → Return 6".
        assert_eq!(dev.params.len(), 24);
        assert_eq!(dev.params[0].label, "Track 1 → Return 1");
        assert_eq!(dev.params[23].label, "Track 4 → Return 6");

        // Readouts: 2·(8 sends + 4 tracks + 6 returns) = 36.
        assert_eq!(dev.readouts.len(), 36);
        assert_eq!(dev.readouts[0].label, "Send 1 Peak");
        assert_eq!(dev.readouts[16].label, "Track 1 Peak"); // after 8 sends × 2
        assert_eq!(dev.readouts[24].label, "Return 1 Peak"); // after +4 tracks × 2
        assert_eq!(dev.readouts[35].label, "Return 6 RMS");
    }
}
