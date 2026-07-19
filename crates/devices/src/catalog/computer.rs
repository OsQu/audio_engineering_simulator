use engine::{BitDepth, DigitalMeter, Matrix, MultitrackRecorder, SampleRate};

use crate::{
    Connector, DeviceConfig, ParamKind, PortKind,
    catalog::{
        BITS, CatalogEntry, FormFactor, GridAxis, GridSpec, HOST_RATE_HZ, InternalEdge, PortUi,
        ReadoutSpec, ReadoutUi,
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

/// Crossbar routing defaults for the computer's `(n_sends + n_tracks) → m_returns` mixer: monitor
/// **send 0 → return 0** (master) and route **every track playback → return 0**, everything else
/// muted (row-major `i·m_returns + j`; rows `0..n_sends` are the live sends, `n_sends..` the track
/// playbacks). So out of the box the default scene's mic/synth (send 0) is heard on master, and any
/// recorded track plays back to master — the crossbar replacement for the old diagonal loopback.
fn crossbar_defaults(n_sends: usize, n_tracks: usize, m_returns: usize) -> Vec<f32> {
    let n_in = n_sends + n_tracks;
    let mut d = vec![0.0; n_in * m_returns];
    if m_returns > 0 {
        d[0] = 1.0; // send 0 → return 0
        for t in 0..n_tracks {
            d[(n_sends + t) * m_returns] = 1.0; // track t playback → return 0
        }
    }
    d
}

// The `computer` — the interface's USB peer, and a minimal **DAW**. A real computer has no channel
// count of its own; it adapts to whatever the attached interface's driver publishes. So its shape is
// **config-driven**: hidden `usb_sends` / `usb_returns` keys (written by web-side enumeration on USB
// connect) size the USB bus, and a hidden `track_count` key sizes the DAW's tracks. Unattached it
// defaults to **2×2** (the built-in sound card) with **1** track.
//
// It is a **three-node, lane-bundled chassis**, the crossbar-router + record/playback split:
//   * node 0 — a `DigitalMeter(N)` metering every send lane (the DAW's input meters);
//   * node 1 — a `MultitrackRecorder(N → N+T)`: it records armed sends to files and plays track
//     files back, owning the transport; its output bus is the N sends **passed through** (for the
//     mixer to monitor) followed by T track **playbacks**;
//   * node 2 — a `Matrix((N+T) → M)` crossbar: the "simple mixer", routing/leveling any source
//     (live send or track playback) to any return via per-crosspoint gains. Its default is the
//     crossbar loopback (send 0 → return 0, playbacks → return 0), so the classic monitoring loop
//     closes out of the box: mic/synth → preamp → AD → USB → computer → USB return → DA → monitor.
// Two internal edges chain them (meter → recorder → matrix). Routing lives in the `Matrix` (not the
// recorder) so track count is independent of the interface's channel count (30 tracks fold to a
// 2-lane master; a track can fan out to an aux return) — the honest mixer+multitrack topology. The
// transport, arm, and record/playback file streams are driven over the wasm seam, not as params.
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
        // 0 — send meters (the DAW's input meters), all N lanes behind one port.
        |cfg| {
            let (sends, _returns) = usb_channel_count(cfg);
            Box::new(DigitalMeter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                sends as u16,
            ))
        },
        // 1 — the multitrack recorder: N sends in → N+T lanes out (sends passed through + playbacks).
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
        // 2 — the crossbar mixer: (N sends + T playbacks) → M returns.
        |cfg| {
            let (sends, returns) = usb_channel_count(cfg);
            let tracks = track_count(cfg);
            Box::new(Matrix::new_single_ports(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                sends + tracks,
                returns,
                crossbar_defaults(sends, tracks, returns),
            ))
        },
    ],
    internal: &[
        // DigitalMeter -> MultitrackRecorder
        InternalEdge {
            from_node: 0,
            from_port: 0,
            to_node: 1,
            to_port: 0,
        },
        // MultitrackRecorder -> Matrix
        InternalEdge {
            from_node: 1,
            from_port: 0,
            to_node: 2,
            to_port: 0,
        },
    ],
    // All exposed params are the crossbar's `(N+T)·M` crosspoints (the meter and recorder have none),
    // generated by the grid; the recorder is a no-param node.
    params: &[],
    // The crossbar (node 2, the only param-contributing node). Rows = the N+T mixer inputs (sends
    // then track playbacks), cols = the M returns; labels generated from the built face's counts (the
    // row count is derived from the matrix's crosspoints, not the USB-In face — see `describe`).
    param_grid: Some(GridSpec {
        inputs: GridAxis::Generated { prefix: "In" },
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
    // Per-lane send meters, in node order: each meter contributes (Peak, RMS) in decl order.
    readouts: ReadoutSpec::PerLane {
        lane_prefix: "Send",
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
    configs: &[],
};

#[cfg(test)]
mod tests {
    use engine::Graph;

    use super::crossbar_defaults;
    use crate::scene::ConfigSetting;
    use crate::{DeviceConfig, PortDirection, describe_device, descriptors, instantiate};

    /// The crossbar default monitors send 0 → return 0 and routes every track playback → return 0,
    /// muting everything else (row-major `i·m + j`; rows `0..n_sends` = sends, `n_sends..` = tracks).
    #[test]
    fn crossbar_defaults_route_send0_and_playbacks_to_master() {
        // 2 sends + 1 track → 3 rows × 2 cols. Unity at (send0,ret0)=id 0 and (playback0,ret0)=id
        // (2+0)·2 = 4; everything else muted.
        assert_eq!(
            crossbar_defaults(2, 1, 2),
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        );

        // 2 sends + 2 tracks → 4 rows × 2 cols: (send0,ret0)=id 0, (pb0,ret0)=id 4, (pb1,ret0)=id 6.
        assert_eq!(
            crossbar_defaults(2, 2, 2),
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0]
        );
    }

    /// Unattached, the `computer` is the **built-in 2×2 sound card** with **1** track: a three-node
    /// chassis — meter(2) → recorder(2→3) → crossbar(3→2) — wired by two lane-bundled internal edges.
    /// USB in exposes the meter; USB out the crossbar; 6 crosspoints (3 inputs × 2 returns) and 4
    /// (Peak, RMS) send readouts, no dangling ports.
    #[test]
    fn default_computer_is_the_2x2_one_track_daw() {
        let mut g = Graph::new();
        let dev = instantiate("computer", &DeviceConfig::EMPTY, &mut g)
            .expect("computer is in the catalog");

        assert_eq!(dev.nodes.len(), 3, "meter + recorder + crossbar");
        assert_eq!(g.connection_count(), 2, "two lane-bundled internal edges");

        assert_eq!(dev.inputs, vec![(dev.nodes[0], 0)], "USB in = meter");
        assert_eq!(dev.outputs, vec![(dev.nodes[2], 0)], "USB out = crossbar");

        // Crossbar (2 sends + 1 track) × 2 returns = 6 crosspoints, all on the matrix (node 2).
        assert_eq!(dev.params.len(), (2 + 1) * 2, "3×2 crosspoints");
        assert!(
            dev.params
                .iter()
                .all(|t| t.len() == 1 && t[0].0 == dev.nodes[2]),
            "every exposed param is a crossbar crosspoint"
        );

        // 2 send lanes × (Peak, RMS) → 4 readouts, all on the meter (node 0).
        assert_eq!(dev.readouts.len(), 4, "2 send lanes × (peak, rms)");
        assert!(
            dev.readouts.iter().all(|r| r.0 == dev.nodes[0]),
            "every readout is on the meter"
        );
    }

    /// The type-catalog descriptor (EMPTY config) advertises the default 2×2 / 1-track face: USB ports
    /// carry 2 lanes each, the crossbar is 3 inputs × 2 returns with generated labels, defaulting to
    /// the crossbar loopback.
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

        // 3 inputs (2 sends + 1 track playback) × 2 returns = 6 crosspoints, labels "In i → Return j".
        assert_eq!(dev.params.len(), 6);
        assert_eq!(dev.params[0].label, "In 1 → Return 1");
        assert_eq!(dev.params[5].label, "In 3 → Return 2");

        // 2 send lanes × (Peak, RMS) → 4 readouts.
        assert_eq!(dev.readouts.len(), 4);
        assert_eq!(dev.readouts[0].label, "Send 1 Peak");
        assert_eq!(dev.readouts[3].label, "Send 2 RMS");

        // Crossbar loopback default: (send0,ret0)=id 0 and (track0-playback,ret0)=id 2·2 = 4 at unity.
        for (id, p) in dev.params.iter().enumerate() {
            let expected = if id == 0 || id == 4 { 1.0 } else { 0.0 };
            assert_eq!(p.default, expected, "crosspoint {id} default");
        }
    }

    /// Attaching an interface writes `usb_sends`/`usb_returns` config; with the default 1 track the
    /// computer re-sizes to a meter(8) → recorder(8→9) → crossbar(9→6) — 54 crosspoints, 16 readouts.
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

        assert_eq!(dev.nodes.len(), 3, "still meter + recorder + crossbar");
        assert_eq!(
            dev.params.len(),
            (8 + 1) * 6,
            "(8 sends + 1 track) × 6 returns = 54"
        );
        assert_eq!(dev.readouts.len(), 16, "8 send lanes × (peak, rms)");
    }

    /// `track_count` sizes the DAW independently of the USB channels: 8 sends / 6 returns / 4 tracks →
    /// a recorder emitting 8+4 lanes and a (8+4)×6 crossbar (72 crosspoints).
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
        assert_eq!(dev.nodes.len(), 3);
        assert_eq!(
            dev.params.len(),
            (8 + 4) * 6,
            "(8 sends + 4 tracks) × 6 returns = 72"
        );
    }

    /// The **per-instance** descriptor scales to an 8×6 / 1-track interface: 8-ch / 6-ch USB faces, a
    /// 9×6 crossbar (54 crosspoints), 16 readouts, and the crossbar loopback default.
    #[test]
    fn configured_computer_descriptor_is_8x6() {
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
        assert_eq!(usb_in.channels, 8, "8 send lanes");
        assert_eq!(usb_out.channels, 6, "6 return lanes");

        // (8 sends + 1 track) × 6 returns = 54 crosspoints; last id 53 = 8·6 + 5 = "In 9 → Return 6".
        assert_eq!(dev.params.len(), 54);
        assert_eq!(dev.params[0].label, "In 1 → Return 1");
        assert_eq!(dev.params[53].label, "In 9 → Return 6");

        // 8 send lanes × (Peak, RMS) → 16 readouts.
        assert_eq!(dev.readouts.len(), 16);
        assert_eq!(dev.readouts[15].label, "Send 8 RMS");

        // Crossbar loopback default: (send0,ret0)=id 0 and (track0-playback,ret0)=id 8·6 = 48 at unity.
        for (id, p) in dev.params.iter().enumerate() {
            let expected = if id == 0 || id == 48 { 1.0 } else { 0.0 };
            assert_eq!(p.default, expected, "crosspoint {id} default");
        }
    }
}
