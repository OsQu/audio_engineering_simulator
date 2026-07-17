use engine::{BitDepth, DigitalMeter, Matrix, SampleRate};

use crate::{
    Connector, DeviceConfig, ParamKind, PortKind,
    catalog::{
        BITS, CatalogEntry, FormFactor, GridAxis, GridSpec, HOST_RATE_HZ, InternalEdge, PortUi,
        ReadoutSpec, ReadoutUi,
    },
};

const USB_SENDS: &str = "usb_sends";
const USB_RETURNS: &str = "usb_returns";
// Configs are floats
const DEFAULT_USB_CHANNELS_CONFIG: f32 = 2.0;

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

/// Diagonal loopback crosspoint defaults for an `n_in`→`m_out` matrix: send k → return k at unity for
/// k < min(n_in, m_out), silent elsewhere (row-major `i·m_out + j`). Every return carries signal out
/// of the box — the 8i6 matrix's identity-default philosophy.
fn loopback_defaults(n_in: usize, m_out: usize) -> Vec<f32> {
    let mut d = vec![0.0; n_in * m_out];
    for k in 0..n_in.min(m_out) {
        d[k * m_out + k] = 1.0;
    }
    d
}

// The `computer` — the 8i6's USB peer, without which a multichannel USB port has no legal partner
// and the 8i6 can't be played end-to-end. Minimal but faithful: it presents the mirror of the 8i6's
// USB cluster — an **8-lane input** (the "USB send" the interface records into the DAW) and a
// **6-lane output** (the "USB return" the DAW plays back). Behaviour v1 is **loopback + meters**:
//   * every send lane is metered per-lane (a `DigitalMeter` each → Peak/RMS readouts, the DAW's
//     input meters);
//   * the sends feed an 8→6 routing `Matrix` whose default is a **loopback** — send 1 → return 1,
//     send 2 → return 2, the rest silent — so the classic playable loop closes: mic/synth → preamp
//     → AD → USB → computer → USB return → DA → monitor.
// The `Matrix` (not a fixed mux) is what cleanly absorbs the 8→6 asymmetry with **no dangling
// ports** — it consumes all 8 metered lanes and emits exactly the 6 returns — and, as a bonus, makes
// the loopback runtime-routable (the seam a real DAW-mixer focus surface will drive later; that
// surface is future work). Node order: 0 USB-in demux (8ch) · 1–8 per-lane send meters · 9 the 8→6
// matrix · 10 USB-out mux (6ch). A real DAW is far more than this; the "correct-enough, never false"
// line keeps the signal path and levels honest and leaves the application layer out of scope.
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
        |cfg| {
            let (sends, _returns) = usb_channel_count(cfg);
            Box::new(DigitalMeter::new(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                sends as u16,
            ))
        },
        |cfg| {
            let (sends, returns) = usb_channel_count(cfg);
            Box::new(Matrix::new_single_ports(
                SampleRate::new(HOST_RATE_HZ),
                BitDepth::new(BITS),
                sends,
                returns,
                loopback_defaults(sends, returns),
            ))
        },
    ],
    internal: &[InternalEdge {
        // DigitalMeter -> Matrix
        from_node: 0,
        from_port: 0,
        to_node: 1,
        to_port: 0,
    }],
    // All exposed params are the matrix's 48 crosspoints (the meters have none), generated below.
    params: &[],
    // The 8×6 routing matrix (node 9, the only param-contributing node). Sends (rows) × returns
    // (cols); rendered as a grid, driven at runtime, loopback by default.
    param_grid: Some(GridSpec {
        inputs: GridAxis::Named(&[
            "Send 1", "Send 2", "Send 3", "Send 4", "Send 5", "Send 6", "Send 7", "Send 8",
        ]),
        outputs: GridAxis::Named(&["Ret 1", "Ret 2", "Ret 3", "Ret 4", "Ret 5", "Ret 6"]),
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
    readouts: ReadoutSpec::Static(&[
        ReadoutUi {
            label: "Send 1 Peak",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 1 RMS",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 2 Peak",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 2 RMS",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 3 Peak",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 3 RMS",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 4 Peak",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 4 RMS",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 5 Peak",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 5 RMS",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 6 Peak",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 6 RMS",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 7 Peak",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 7 RMS",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 8 Peak",
            unit: "dBFS",
        },
        ReadoutUi {
            label: "Send 8 RMS",
            unit: "dBFS",
        },
    ]),
    configs: &[],
};

#[cfg(test)]
mod tests {
    use engine::Graph;

    use crate::{DeviceConfig, descriptors, instantiate};

    /// The `computer` peer expands into the metered-loopback chassis: an 8-lane USB input, a 6-lane USB
    /// output, 8 per-lane send meters (16 readouts), and a single exposed face of 48 matrix crosspoints.
    /// The chassis has no dangling ports — the matrix consumes all 8 metered lanes and emits exactly the
    /// 6 returns.
    #[test]
    fn computer_expands_to_metered_loopback() {
        let mut g = Graph::new();
        let dev = instantiate("computer", &DeviceConfig::EMPTY, &mut g)
            .expect("computer is in the catalog");

        // 1 demux + 8 meters + 1 matrix + 1 mux = 11 nodes, wired by 8 + 8 + 6 = 22 internal edges.
        assert_eq!(dev.nodes.len(), 11, "11 internal nodes");
        assert_eq!(g.connection_count(), 22, "22 internal edges");

        // One exposed input (the 8-lane USB-in demux) and one exposed output (the 6-lane USB-out mux);
        // every other port is consumed internally (no dangling face).
        assert_eq!(dev.inputs, vec![(dev.nodes[0], 0)], "USB in = demux input");
        assert_eq!(
            dev.outputs,
            vec![(dev.nodes[10], 0)],
            "USB out = mux output"
        );

        // The exposed param face is the matrix's 48 crosspoints (8 sends × 6 returns), all on node 9.
        assert_eq!(dev.params.len(), 48, "8×6 crosspoints");
        assert!(
            dev.params
                .iter()
                .all(|t| t.len() == 1 && t[0].0 == dev.nodes[9]),
            "every exposed param is a single matrix crosspoint"
        );

        // Two readouts per send meter, in node order → 16 readouts, all on the 8 meter nodes (1–8).
        assert_eq!(dev.readouts.len(), 16, "8 meters × (peak, rms)");
        for (i, r) in dev.readouts.iter().enumerate() {
            assert_eq!(r.0, dev.nodes[1 + i / 2], "readout {i} on its meter node");
        }
    }

    /// The `computer`'s routing matrix defaults to the **loopback**: send 1 → return 1 and send 2 →
    /// return 2 at unity, every other crosspoint muted — so the playable monitoring path closes out of
    /// the box before any DAW routing is touched.
    #[test]
    fn computer_matrix_defaults_to_loopback() {
        let dev = descriptors()
            .into_iter()
            .find(|d| d.type_id == "computer")
            .expect("computer is in the catalog");

        // Crosspoints are the whole param face, row-major (send i, return j) → id i·6 + j.
        assert_eq!(dev.params.len(), 48);
        for (id, p) in dev.params.iter().enumerate() {
            // Loopback crosspoints: (send 1, ret 1) = id 0 and (send 2, ret 2) = id 1·6 + 1 = 7.
            let expected = if id == 0 || id == 7 { 1.0 } else { 0.0 };
            assert_eq!(p.default, expected, "crosspoint {id} default");
        }
    }
}
