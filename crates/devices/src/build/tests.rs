use super::*;
use crate::scene::{Connection, DeviceInstance};
use engine::{
    AdConverter, BitDepth, DaConverter, EventMessage, EventQueue, Graph, InputZ, MicPreamp, Ohms,
    ParamQueue, SampleRate, Speaker, SynthVoice, VoltageBuffer, Volts, compile, spl_to_pascals,
};

const BLOCK_LEN: usize = 384; // 384 / M=8 = 48 host samples; divisible, as compile requires.
const NOTE: u8 = 69; // A4

fn rate() -> AnalogRate {
    AnalogRate::new(384_000.0)
}

fn conn(from: &str, from_port: u32, to: &str, to_port: u32) -> Connection {
    Connection {
        from: PortRef {
            device: from.into(),
            port: from_port,
        },
        to: PortRef {
            device: to.into(),
            port: to_port,
        },
        cable: None,
        duplex: false,
    }
}

/// A **duplex** connection — one physical connector carrying both directions. Same as [`conn`] but
/// with the `duplex` flag set, so `build_patch` also adds the reverse leg.
fn conn_duplex(from: &str, from_port: u32, to: &str, to_port: u32) -> Connection {
    Connection {
        duplex: true,
        ..conn(from, from_port, to, to_port)
    }
}

fn device(id: &str, type_id: &str) -> DeviceInstance {
    DeviceInstance {
        id: id.into(),
        type_id: type_id.into(),
        params: vec![],
        config: vec![],
    }
}

/// A `computer` sized to the 8i6's USB shape — 8 sends / 6 returns — written as the hidden
/// `usb_sends`/`usb_returns` structural config (what web enumeration writes on USB connect). The
/// default computer is the built-in 2×2 card, which wouldn't match the 8i6's 8-ch send / 6-ch
/// return; these round-trip tests need the interface's shape.
fn computer_8x6(id: &str) -> DeviceInstance {
    DeviceInstance {
        id: id.into(),
        type_id: "computer".into(),
        params: vec![],
        config: vec![
            crate::scene::ConfigSetting {
                key: "usb_sends".into(),
                value: 8.0,
            },
            crate::scene::ConfigSetting {
                key: "usb_returns".into(),
                value: 6.0,
            },
        ],
    }
}

/// The pinned canonical patch as a scene: `synth → AD → DA → speaker`, tapped at the speaker.
fn canonical_patch() -> Patch {
    Patch {
        devices: vec![
            device("synth", "synth_voice"),
            device("ad", "ad_converter"),
            device("da", "da_converter"),
            device("spk", "speaker"),
        ],
        connections: vec![
            conn("synth", 0, "ad", 0),
            conn("ad", 0, "da", 0),
            conn("da", 0, "spk", 0),
        ],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    }
}

/// Peak `|speaker volts|` over `blocks` blocks after striking a note on `device`.
fn peak_after_note(scene: &mut BuiltScene, device: &str, blocks: usize) -> f32 {
    let ev = scene
        .event_input(device)
        .expect("device has an event input");
    let mut events = EventQueue::with_capacity(4);
    events.push(
        0,
        ev,
        EventMessage::NoteOn {
            note: NOTE,
            velocity: 100,
        },
    );
    let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
    let mut peak = 0.0_f32;
    for _ in 0..blocks {
        scene
            .schedule_mut()
            .process_with_events(&mut out, &mut events);
        peak = out.as_slice().iter().fold(peak, |p, &v| p.max(v.abs()));
    }
    peak
}

/// The canonical patch, assembled from a scene, is **byte-for-byte identical** to the same chain
/// built by hand directly on the engine — output parity. Same device order ⇒ same node indices ⇒
/// same per-node seeding ⇒ identical dither, so equality (not just "close") holds. This is the
/// oracle that `build_patch` wires, taps, and orders the graph exactly like a hand-built engine.
#[test]
fn canonical_patch_matches_a_hand_built_graph() {
    // Assembled from the scene.
    let patch = canonical_patch();
    let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("canonical patch builds");
    let scene_ev = scene.event_input("synth").expect("synth event input");

    // The same chain, built directly — identical configs and order to the catalog's builders.
    let mut g = Graph::new();
    let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
    let ad = g.add(AdConverter::new(
        SampleRate::new(48_000.0),
        BitDepth::new(16),
        Volts::new(1.0),
        Ohms::new(1_000_000.0),
    ));
    let da = g.add(DaConverter::new(
        SampleRate::new(48_000.0),
        BitDepth::new(16),
        Volts::new(1.0),
        Ohms::new(150.0),
    ));
    let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
    g.connect_ideal(voice, 0, ad, 0);
    g.connect_ideal(ad, 0, da, 0);
    g.connect_ideal(da, 0, spk, 0);
    g.set_output(spk, 0);
    let mut hand = compile(g, BLOCK_LEN, rate(), 0).expect("hand graph compiles");
    let hand_ev = hand.event_input(voice, 0).expect("voice event input");

    // Strike the same note on both and compare every sample over several blocks.
    let mut scene_events = EventQueue::with_capacity(4);
    scene_events.push(
        0,
        scene_ev,
        EventMessage::NoteOn {
            note: NOTE,
            velocity: 100,
        },
    );
    let mut hand_events = EventQueue::with_capacity(4);
    hand_events.push(
        0,
        hand_ev,
        EventMessage::NoteOn {
            note: NOTE,
            velocity: 100,
        },
    );

    let mut a = VoltageBuffer::zeros(BLOCK_LEN, rate());
    let mut b = VoltageBuffer::zeros(BLOCK_LEN, rate());
    for _ in 0..16 {
        scene
            .schedule_mut()
            .process_with_events(&mut a, &mut scene_events);
        hand.process_with_events(&mut b, &mut hand_events);
        assert_eq!(
            a.as_slice(),
            b.as_slice(),
            "scene-built output must match the hand-built graph"
        );
    }
}

/// End-to-end: the scene-built canonical patch is silent until a note, then audible — wiring,
/// output tap, compile, and event-input resolution all work.
#[test]
fn scene_built_patch_is_silent_then_audible() {
    let mut scene = build_patch(&canonical_patch(), BLOCK_LEN, rate(), 0).expect("builds");

    let mut idle = VoltageBuffer::zeros(BLOCK_LEN, rate());
    let mut none = EventQueue::with_capacity(1);
    let mut idle_peak = 0.0_f32;
    for _ in 0..8 {
        scene
            .schedule_mut()
            .process_with_events(&mut idle, &mut none);
        idle_peak = idle
            .as_slice()
            .iter()
            .fold(idle_peak, |p, &v| p.max(v.abs()));
    }
    assert!(idle_peak < 0.01, "silent before any note, got {idle_peak}");

    let sounding = peak_after_note(&mut scene, "synth", 32);
    assert!(sounding > 0.05, "audible after note, got {sounding}");
}

/// The resolved `ParamHandle` controls the right node: driving the synth's LEVEL (device param 0)
/// to zero silences it even with a note held — so generic `(device, param id)` addressing lands on
/// the actual smoother.
#[test]
fn resolved_param_handle_controls_its_node() {
    let mut scene = build_patch(&canonical_patch(), BLOCK_LEN, rate(), 0).expect("builds");
    let level = *scene
        .param("synth", 0)
        .first()
        .expect("synth has param 0 (LEVEL)");

    let ev = scene.event_input("synth").expect("synth event input");
    let mut events = EventQueue::with_capacity(4);
    events.push(
        0,
        ev,
        EventMessage::NoteOn {
            note: NOTE,
            velocity: 100,
        },
    );
    let mut params = ParamQueue::with_capacity(1);
    params.set(level, 0.0); // glide LEVEL from its 1.0 default to 0 (over ~5 ms)

    // Measure the *tail* only: LEVEL glides to 0 over ~5 ms, so the first blocks still sound while
    // it ramps down. By block 16 (~16 ms here) the glide is long settled — steady state is silence.
    let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
    let mut tail_peak = 0.0_f32;
    for block in 0..64 {
        scene
            .schedule_mut()
            .process_io(&mut out, &mut params, &mut events);
        if block >= 16 {
            tail_peak = out
                .as_slice()
                .iter()
                .fold(tail_peak, |p, &v| p.max(v.abs()));
        }
    }
    assert!(
        tail_peak < 0.01,
        "LEVEL=0 should silence the voice, got {tail_peak}"
    );
}

/// Control resolution is total: unknown device, unknown param id, and a no-event device all return
/// an empty handle slice / `None` rather than panicking.
#[test]
fn control_resolution_is_total() {
    let scene = build_patch(&canonical_patch(), BLOCK_LEN, rate(), 0).expect("builds");
    assert!(!scene.param("synth", 0).is_empty());
    assert!(scene.param("synth", 99).is_empty(), "param id out of range");
    assert!(scene.param("nope", 0).is_empty(), "unknown device");
    assert!(scene.event_input("synth").is_some());
    assert!(
        scene.event_input("spk").is_none(),
        "speaker has no event input"
    );
}

/// A multi-node device (the two-stage `channel_strip`) assembles inside a full patch: connections
/// to its exposed ports remap onto the right internal nodes, and the chain still sounds.
#[test]
fn multi_node_device_in_a_patch() {
    let patch = Patch {
        devices: vec![
            device("synth", "synth_voice"),
            device("strip", "channel_strip"),
            device("ad", "ad_converter"),
            device("da", "da_converter"),
            device("spk", "speaker"),
        ],
        connections: vec![
            conn("synth", 0, "strip", 0),
            conn("strip", 0, "ad", 0),
            conn("ad", 0, "da", 0),
            conn("da", 0, "spk", 0),
        ],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    };
    let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("multi-node patch builds");
    // The strip exposes each stage's gain + power, concatenated in node order: device params
    // 0..=3 (in_gain, in_power, out_gain, out_power); 4 is past the face.
    assert!(!scene.param("strip", 0).is_empty());
    assert!(!scene.param("strip", 3).is_empty());
    assert!(scene.param("strip", 4).is_empty());
    let sounding = peak_after_note(&mut scene, "synth", 32);
    assert!(sounding > 0.05, "audible through the strip, got {sounding}");
}

/// A device-level param group resolves, through `build_patch`, to **all** its target handles: the
/// full 8i6's single Power control (exposed param id 205, the last param) drives every stage's
/// `powered` — 16 handles from one id (2 preamps + 6 ADs + 4 DAs + 4 amps) — while an ungrouped gain
/// drives exactly one. This is the plumbing the wasm `set_param` fans a value over.
#[test]
fn device_power_group_fans_out_to_every_stage() {
    let patch = Patch {
        devices: vec![device("if", "scarlett_8i6")],
        connections: vec![],
        output: PortRef {
            device: "if".into(),
            port: 2, // Line Out 1 (analog)
        },
    };
    let scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("8i6 patch builds");
    assert_eq!(
        scene.param("if", 205).len(),
        16,
        "the Power group drives all sixteen stages"
    );
    assert_eq!(
        scene.param("if", 0).len(),
        1,
        "an ungrouped gain drives exactly one stage"
    );
}

/// The classic playable loop **closes through the computer**: synth → 8i6 (record) → computer
/// (loopback) → 8i6 (USB return → monitor) → speaker. The computer's USB output is a **round-trip
/// latency** source, so `build_patch` wires the return edge delayed — placing the block of latency
/// on the physically-correct leg (the DAW's playback trailing its input). `compile` would in any
/// case auto-break the resulting digital cycle by delaying one of its edges; the declared-latent
/// output is the *hint* that fixes **which** leg carries it. This also exercises a **multichannel
/// digital delayed edge** end-to-end (8-lane send, 6-lane return).
/// The monitor is powered on and the 8i6 matrix is left at its identity default (Pre1→USB1 on the
/// record side, DAW1→Line1 on the playback side), so the note returns to Line Out 1 and is audible.
#[test]
fn playable_loop_closes_through_the_computer() {
    let patch = Patch {
        devices: vec![
            device("synth", "synth_voice"),
            device("if", "scarlett_8i6"),
            computer_8x6("computer"),
            device("spk", "speaker"),
        ],
        connections: vec![
            conn("synth", 0, "if", 0),    // synth → combo in 1
            conn("if", 0, "computer", 0), // 8i6 USB send (8ch) → computer USB in
            conn("computer", 0, "if", 7), // computer USB out (6ch, DELAYED) → 8i6 USB return
            conn("if", 2, "spk", 0),      // 8i6 Line Out 1 (monitor L) → speaker
        ],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    };
    // Builds despite the loop — the delayed return edge breaks the cycle.
    let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("the round-trip loop builds");

    // Power the whole 8i6 on (it boots powered-off) and open the monitor; the identity matrix
    // already routes Pre1→USB1 (record) and DAW1→Line1 (playback), so the loop carries the note.
    let power: Vec<_> = scene.param("if", 205).to_vec();
    let monitor: Vec<_> = scene.param("if", 204).to_vec();
    let ev = scene.event_input("synth").expect("synth event input");
    let mut events = EventQueue::with_capacity(4);
    events.push(
        0,
        ev,
        EventMessage::NoteOn {
            note: NOTE,
            velocity: 100,
        },
    );
    let mut params = ParamQueue::with_capacity(20);
    for h in power.iter().chain(monitor.iter()) {
        params.set(*h, 1.0);
    }
    let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
    let mut peak = 0.0_f32;
    for _ in 0..128 {
        scene
            .schedule_mut()
            .process_io(&mut out, &mut params, &mut events);
        peak = out.as_slice().iter().fold(peak, |p, &v| p.max(v.abs()));
    }
    assert!(
        peak > 0.01,
        "the note returns through the computer to the speaker, got {peak}"
    );
}

/// **The acoustic feedback loop builds.** The product howlround chain —
/// `condenser mic → 8i6 (combo in → preamp → AD → matrix → DA → Line Out 1) → powered monitor →
/// air → mic` — is a genuine graph cycle (the 8i6's matrix is one node, so its input path and
/// output path close a loop through it). `build_patch` + `compile` **auto-break it** at the air
/// edge (the prefer-acoustic policy; *which* edge carries the delay is pinned by the engine test
/// `acoustic_feedback`), so the loop builds and runs — feedback is not special-cased, and the
/// loop is not rejected as `Cycle`. We build it and run a few blocks to exercise the delayed acoustic edge on the real product graph.
#[test]
fn acoustic_feedback_loop_through_the_8i6_builds_and_runs() {
    let patch = Patch {
        devices: vec![
            device("mic", "condenser_mic"),
            DeviceInstance {
                id: "if".into(),
                type_id: "scarlett_8i6".into(),
                params: vec![],
                config: vec![crate::scene::ConfigSetting {
                    key: "phantom".into(),
                    value: 1.0,
                }],
            },
            device("mon", "powered_monitor"),
        ],
        connections: vec![
            conn("mon", 0, "mic", 0), // monitor Air → mic capsule (the acoustic air hop)
            conn("mic", 0, "if", 0),  // mic XLR → 8i6 combo in 1
            conn("if", 2, "mon", 0),  // 8i6 Line Out 1 → monitor In (closes the loop)
        ],
        output: PortRef {
            device: "if".into(),
            port: 3, // Line Out 2 — a valid analog tap outside the loop
        },
    };
    let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0)
        .expect("the acoustic feedback loop auto-breaks at the air edge and builds");
    // Run a few blocks: the delayed acoustic edge copy runs on the real product graph, panic-free.
    let mut params = ParamQueue::with_capacity(1);
    let mut events = EventQueue::with_capacity(1);
    let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
    for _ in 0..4 {
        scene
            .schedule_mut()
            .process_io(&mut out, &mut params, &mut events);
    }
}

/// The `device → node → daw()` resolution: the `computer` in a built scene resolves to a live
/// [`DawControl`] surface (only its recorder node's `daw()` answers `Some`), a non-DAW device
/// resolves to `None`, and an unknown id resolves to `None`. Driving the transport over the facade
/// mutates the compiled schedule's node, so a subsequent lookup sees the state — the seam the wasm
/// layer drives.
#[test]
fn built_scene_resolves_the_computer_daw() {
    let patch = Patch {
        devices: vec![
            device("synth", "synth_voice"),
            device("if", "scarlett_8i6"),
            computer_8x6("computer"),
            device("spk", "speaker"),
        ],
        connections: vec![
            conn("synth", 0, "if", 0),
            conn("if", 0, "computer", 0),
            conn("computer", 0, "if", 7),
            conn("if", 2, "spk", 0),
        ],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    };
    let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("the loop builds");

    // The synth is not a DAW; an unknown id resolves to nothing.
    assert!(scene.daw("synth").is_none(), "a synth exposes no DAW");
    assert!(
        scene.daw("nope").is_none(),
        "an unknown device resolves to None"
    );

    // The computer resolves to its recorder facade — an 8-send / (default) 1-track DAW.
    let daw = scene.daw("computer").expect("the computer is a DAW");
    assert_eq!(daw.track_count(), 1, "default track_count");
    assert!(!daw.transport().is_rolling(), "boots stopped");
    daw.transport_mut().play();

    // A second resolution sees the mutation persisted on the schedule's node.
    assert!(
        scene.daw("computer").unwrap().transport().is_rolling(),
        "transport state persists on the compiled node"
    );
}

/// The same playable loop, but the two USB connections collapse into **one duplex cable**: a single
/// duplex `Connection` (8i6 USB Out → computer USB In) that `build_patch` expands into both the 8-ch
/// send *and* the 6-ch return (computer USB Out → 8i6 USB In, resolved through each jack's
/// `duplex_partner`). The loop only sounds if **both** legs carry signal, so audibility proves the
/// reverse leg was added; and `connection_losses` stays 1:1 with the scene's connections.
#[test]
fn a_duplex_usb_cable_carries_both_send_and_return() {
    let patch = Patch {
        devices: vec![
            device("synth", "synth_voice"),
            device("if", "scarlett_8i6"),
            computer_8x6("computer"),
            device("spk", "speaker"),
        ],
        connections: vec![
            conn("synth", 0, "if", 0),
            conn_duplex("if", 0, "computer", 0), // ONE duplex USB-C cable = 8-ch send + 6-ch return
            conn("if", 2, "spk", 0),
        ],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    };
    let mut scene =
        build_patch(&patch, BLOCK_LEN, rate(), 0).expect("the duplex round-trip loop builds");
    assert_eq!(
        scene.connection_losses().len(),
        3,
        "one tracked loss per scene connection — the duplex reverse leg isn't double-counted"
    );

    // Power the 8i6 on and open the monitor; the identity matrix routes the note through the loop.
    let power: Vec<_> = scene.param("if", 205).to_vec();
    let monitor: Vec<_> = scene.param("if", 204).to_vec();
    let ev = scene.event_input("synth").expect("synth event input");
    let mut events = EventQueue::with_capacity(4);
    events.push(
        0,
        ev,
        EventMessage::NoteOn {
            note: NOTE,
            velocity: 100,
        },
    );
    let mut params = ParamQueue::with_capacity(20);
    for h in power.iter().chain(monitor.iter()) {
        params.set(*h, 1.0);
    }
    let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
    let mut peak = 0.0_f32;
    for _ in 0..128 {
        scene
            .schedule_mut()
            .process_io(&mut out, &mut params, &mut events);
        peak = out.as_slice().iter().fold(peak, |p, &v| p.max(v.abs()));
    }
    assert!(
        peak > 0.01,
        "the duplex cable's send and return both carry signal, so the note is audible, got {peak}"
    );
}

/// A `duplex` flag on a jack that isn't duplex (the synth's analog output has no `duplex_partner`)
/// is rejected — there's no paired port to carry the reverse leg.
#[test]
fn duplex_on_a_non_duplex_jack_errors() {
    let patch = Patch {
        devices: vec![device("synth", "synth_voice"), device("if", "scarlett_8i6")],
        connections: vec![conn_duplex("synth", 0, "if", 0)],
        output: PortRef {
            device: "if".into(),
            port: 2,
        },
    };
    let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(matches!(err, BuildError::NotDuplex { .. }), "got {err:?}");
}

/// The INST/hi-Z structural config toggles a preamp's input impedance, which the loading divider
/// bakes at compile — so the *same* patch, built with `inst1` off vs on, yields a **different**
/// connection loss on a high-output-impedance source feeding preamp 1. A synth (≈1 Ω Zout) is too
/// stiff to show it, so this drives the preamp through a lossy cable (its series R stands in for a
/// high source impedance): line-Z (10 kΩ) loads it hard; inst-Z (1.5 MΩ) barely at all.
#[test]
fn inst_config_changes_the_baked_loading_divider() {
    let patch = |inst: f32| Patch {
        devices: vec![
            device("synth", "synth_voice"),
            DeviceInstance {
                id: "if".into(),
                type_id: "scarlett_8i6".into(),
                params: vec![],
                config: vec![crate::scene::ConfigSetting {
                    key: "inst1".into(),
                    value: inst,
                }],
            },
        ],
        // synth out → preamp 1 in, through a 10 kΩ cable so the input impedance matters.
        connections: vec![Connection {
            from: PortRef {
                device: "synth".into(),
                port: 0,
            },
            to: PortRef {
                device: "if".into(),
                port: 0,
            },
            cable: Some(crate::scene::CableSpec {
                type_id: None,
                resistance_ohms: 10_000.0,
                capacitance_farads: 0.0,
            }),
            duplex: false,
        }],
        output: PortRef {
            device: "if".into(),
            port: 2,
        },
    };
    let loss = |inst: f32| {
        build_patch(&patch(inst), BLOCK_LEN, rate(), 0)
            .expect("builds")
            .connection_loading_loss(0)
            .expect("analog connection has a loss")
    };
    let line = loss(0.0); // 10 kΩ input: divider ≈ 10k/(1+10k+10k) ≈ 0.5 ⇒ ≈ −6 dB
    let inst = loss(1.0); // 1.5 MΩ input: divider ≈ 1.5M/(1+10k+1.5M) ≈ 0.993 ⇒ ≈ −0.06 dB
    assert!(line < -5.0, "line-Z loads the source hard, got {line} dB");
    assert!(inst > -0.2, "inst-Z barely loads it, got {inst} dB");
}

/// Regression oracle for the balanced preamp front-end: the **unbalanced** synth into the
/// preamp's balanced combo input rides the engine's grounding edge, whose hot leg uses the
/// *same* divider formula an unbalanced input would — the differential Zin plays exactly the
/// role an unbalanced Zin plays, so the loading gain is **numerically the plain unbalanced
/// divider**. Hand calc (synth Zout = 1 Ω, no cable, line-Z 10 kΩ):
/// gain = 10 000/(1 + 0 + 10 000) = 0.99990001 → 20·log10 = −0.000869 dB.
#[test]
fn balanced_preamp_keeps_the_unbalanced_loading_gain() {
    let patch = Patch {
        devices: vec![device("synth", "synth_voice"), device("if", "scarlett_8i6")],
        connections: vec![conn("synth", 0, "if", 0)], // synth → combo in 1
        output: PortRef {
            device: "if".into(),
            port: 2,
        },
    };
    let scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("builds");
    let loss_db = scene
        .connection_loading_loss(0)
        .expect("synth→preamp is analog");

    // Exactly the unbalanced 1→1 solve — what this same edge baked before the balanced face.
    let unbal_gain =
        engine::divider_gain(Ohms::new(1.0), Ohms::ZERO, InputZ::new(Ohms::new(10_000.0)));
    let unbal_db = 20.0 * unbal_gain.log10();
    assert!(
        (loss_db - unbal_db).abs() < 1e-7,
        "grounding-edge gain must equal the unbalanced divider: {loss_db} vs {unbal_db} dB"
    );
    // And the hand number: −0.000869 dB.
    assert!(
        (loss_db - (-0.000_869)).abs() < 1e-5,
        "hand calc −0.000869 dB, got {loss_db}"
    );
}

/// `mics` condenser mics into the 8i6's combo inputs (mic i → combo i), the shared 48V config
/// at `phantom`, tapped at Line Out 1. XLR-into-Combo is the legal jack fit.
fn mics_into_8i6(phantom: f32, mics: &[&str]) -> Patch {
    let mut devices = vec![DeviceInstance {
        id: "if".into(),
        type_id: "scarlett_8i6".into(),
        params: vec![],
        config: vec![crate::scene::ConfigSetting {
            key: "phantom".into(),
            value: phantom,
        }],
    }];
    let mut connections = Vec::new();
    for (i, id) in mics.iter().enumerate() {
        // A vox radiating into each mic's capsule (over the ideal-air acoustic edge). Its
        // construction default is exactly 1 Pa peak; against the condenser's 0.01 V/Pa sensitivity
        // that is a 10 mV differential — the same capsule-tone amplitude the mic used to
        // self-generate, so the hand calcs below are unchanged. (Frequency is a runtime param, set
        // per-test where period alignment matters — scene `ParamSetting`s aren't applied at build.)
        let vox = format!("{id}_vox");
        devices.push(device(&vox, "vox"));
        devices.push(device(id, "condenser_mic"));
        connections.push(conn(&vox, 0, id, 0)); // vox → mic capsule (acoustic)
        connections.push(conn(id, 0, "if", i as u32)); // mic → 8i6 combo input
    }
    Patch {
        devices,
        connections,
        output: PortRef {
            device: "if".into(),
            port: 2, // Line Out 1 (analog)
        },
    }
}

/// Phantom power at the scene level: a condenser mic into the 8i6's combo input is
/// **dead with 48V off and alive with it on** — the same patch, rebuilt with the structural
/// `phantom` config toggled, and nothing else changed. Routed Pre 1 → Line 1 through the matrix
/// so the capsule tone reaches the analog tap. Hand calc for the powered amplitude — 10 mV
/// differential capsule tone through the chain's baked dividers, at the preamp's **default gain**
/// (its +8 dB floor, `MicPreamp::MIN_GAIN` = ×2.511886; monitor amp still unity):
///   mic (150 Ω) → preamp (10 kΩ diff):   10 000/(150 + 10 000)  = 0.985222
///   preamp voltage gain (default +8 dB):                        = 2.511886
///   preamp (150 Ω) → input meter (1 MΩ): 10⁶/1 000 150          = 0.999850
///   meter (150 Ω) → AD (1 MΩ):           10⁶/1 000 150          = 0.999850
///   AD → matrix (Pre 1 → Line 1 = ×1) → DA: digital, ≈ unity at 1 kHz
///   DA (150 Ω) → monitor amp (10 kΩ):    10 000/10 150          = 0.985222
/// ⇒ 0.01 × 0.985222² × 0.999850² × 2.511886 = 24.3726 mV peak at Line Out 1 — and **zero DC**: the
/// 37.86 V pedestal is common-mode, cancelled at the preamp's difference *before* its clamp.
/// Off, both mic legs sit at 0 V, so only the AD's dither (~30 µV LSB) reaches the tap.
#[test]
fn phantom_config_powers_the_mic_through_the_8i6() {
    // Peak and mean at Line Out 1 over the tail blocks (params long settled, tone in steady
    // state). Each 384-sample block at 384 kHz is 1 ms = exactly one 1 kHz period, so the block
    // peak is the amplitude and the block mean is the DC.
    let run = |phantom: f32| {
        let mut scene = build_patch(&mics_into_8i6(phantom, &["mic"]), BLOCK_LEN, rate(), 0)
            .expect("mic → 8i6 builds with 48V either way (XLR seats in the combo jack)");
        // Route Pre 1 → Line Out 1: crosspoint (in 0, out 8) = exposed id 8 + 0·14 + 8 = 16.
        let xpoint = *scene.param("if", 16).first().expect("matrix crosspoint");
        // Drive the vox to 1 kHz so each 384-sample block is exactly one period (see below).
        let vox_freq = *scene.param("mic_vox", 1).first().expect("vox pitch param");
        let mut params = ParamQueue::with_capacity(2);
        params.set(xpoint, 1.0);
        params.set(vox_freq, 1_000.0);
        let mut events = EventQueue::with_capacity(1);
        let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
        let (mut peak, mut mean) = (0.0_f32, 0.0_f64);
        for block in 0..48 {
            scene
                .schedule_mut()
                .process_io(&mut out, &mut params, &mut events);
            if block >= 32 {
                for &v in out.as_slice() {
                    peak = peak.max(v.abs());
                    mean += f64::from(v);
                }
            }
        }
        (peak, (mean / (16.0 * BLOCK_LEN as f64)) as f32)
    };

    // 48V off: the mic is dead — only converter dither at the tap, no tone.
    let (dead_peak, _) = run(0.0);
    assert!(
        dead_peak < 1e-3,
        "unfed mic must be dead, got {dead_peak} V"
    );

    // 48V on: the capsule tone arrives at the hand-calc amplitude, with no DC.
    let (peak, dc) = run(1.0);
    assert!(
        (peak - 24.3726e-3).abs() < 5e-4,
        "hand calc 24.3726 mV at Line Out 1 (+8 dB default preamp gain), got {peak} V"
    );
    assert!(dc.abs() < 1e-4, "pedestal must not leak: DC = {dc} V");
}

/// The 8i6's 48V is **one switch over both preamps** (the real unit's single global button):
/// two mics into the two combo inputs and the *one* `phantom` key wakes both. Observed at the
/// post-preamp input meters' block-peak readouts (ids 1 and 3). Hand calc: 10 mV ×
/// 0.985222 (mic→preamp divider, 10 000/10 150) × 2.511886 (preamp default gain, +8 dB floor) ×
/// 0.999850 (preamp→meter divider) = 24.743 mV peak ⇒ 20·log10(0.024743/0.774597 V) = −29.91 dBu
/// on both meters (the +8 dB default lifts the old unity-gain −37.91 dBu by exactly 8 dB). Off,
/// both mics are dead and both meters sit at the −60 dB reading floor.
#[test]
fn one_phantom_key_engages_both_preamps() {
    let peaks = |phantom: f32| {
        let mut scene = build_patch(
            &mics_into_8i6(phantom, &["mic1", "mic2"]),
            BLOCK_LEN,
            rate(),
            0,
        )
        .expect("two mics → 8i6 builds");
        let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
        for _ in 0..4 {
            scene.schedule_mut().process(&mut out);
        }
        let in1 = scene.readout("if", 1).expect("In 1 Peak readout");
        let in2 = scene.readout("if", 3).expect("In 2 Peak readout");
        (
            scene.schedule().readout_value(in1).expect("In 1 value"),
            scene.schedule().readout_value(in2).expect("In 2 value"),
        )
    };

    let (in1, in2) = peaks(1.0);
    assert!((in1 - (-29.91)).abs() < 0.1, "In 1 Peak {in1} dBu");
    assert!((in2 - (-29.91)).abs() < 0.1, "In 2 Peak {in2} dBu");

    let (in1, in2) = peaks(0.0);
    assert!(in1 <= -60.0 + 1e-3, "dead mic 1 at the floor, got {in1}");
    assert!(in2 <= -60.0 + 1e-3, "dead mic 2 at the floor, got {in2}");
}

/// A `vox → mic → 8i6 input 1` patch (the acoustic edge into a mic capsule, the mic's XLR into the
/// combo preamp), phantom on so the condenser lives. Output is Line Out 1 (unused — the assertions
/// read the post-preamp In 1 Peak meter). `mic` selects `condenser_mic` / `dynamic_mic`.
fn vox_mic_8i6(mic: &str) -> Patch {
    Patch {
        devices: vec![
            device("vox", "vox"),
            device(mic, mic),
            DeviceInstance {
                id: "if".into(),
                type_id: "scarlett_8i6".into(),
                params: vec![],
                config: vec![crate::scene::ConfigSetting {
                    key: "phantom".into(),
                    value: 1.0,
                }],
            },
        ],
        connections: vec![
            conn("vox", 0, mic, 0), // vox Air → mic capsule (acoustic edge, ideal air)
            conn(mic, 0, "if", 0),  // mic XLR → 8i6 combo input 1
        ],
        output: PortRef {
            device: "if".into(),
            port: 2, // Line Out 1 (unused; readouts drive the assertions)
        },
    }
}

/// **The micing scenario, gain-staging by hand calc.** Drive a vox into a mic into the 8i6's
/// preamp and read the post-preamp **In 1 Peak** meter (dBu, exposed readout 1; the
/// meter taps the preamp output, so this is `differential × mic→preamp divider × preamp gain ×
/// preamp→meter divider`, floored at −60 dBu). The whole front-end gain-staging story falls out of
/// `v = pressure × sensitivity` + the existing dividers/rail — nothing special-cased.
///
/// Shared terms (0 dBu = √0.6 = 0.774597 V; rail = 10 V; default preamp gain = `MIN_GAIN` ≈ ×2.5119
/// = +8 dB; PAD = 10^(−10/20) = ×0.316228; preamp→meter divider = 10⁶/1 000 150 = 0.999850):
///   condenser: 0.01 V/Pa, 150 Ω → mic→preamp divider 10 000/10 150 = 0.985222
///   dynamic:   0.002 V/Pa, 200 Ω → mic→preamp divider 10 000/10 200 = 0.980392
#[test]
fn micing_scenario_gain_staging() {
    // Build `vox → mic → 8i6`, set the vox loudness (dB SPL → Pa), pitch (1 kHz so each 384-sample
    // 384 kHz block = exactly one period ⇒ block peak = amplitude), the preamp gain (linear ×) and
    // PAD, run to steady state, and read In 1 Peak (dBu).
    let read_in1_peak = |mic: &str, spl: f32, gain: f32, pad: bool| -> f32 {
        let mut scene = build_patch(&vox_mic_8i6(mic), BLOCK_LEN, rate(), 0)
            .expect("vox → mic → 8i6 builds (acoustic edge + XLR into the combo jack)");
        let loudness = *scene.param("vox", 0).first().expect("vox loudness");
        let pitch = *scene.param("vox", 1).first().expect("vox pitch");
        let preamp_gain = *scene.param("if", 0).first().expect("Gain 1");
        let preamp_pad = *scene.param("if", 1).first().expect("Pad 1");
        let mut params = ParamQueue::with_capacity(4);
        params.set(loudness, spl_to_pascals(spl).get());
        params.set(pitch, 1_000.0);
        params.set(preamp_gain, gain);
        params.set(preamp_pad, if pad { 1.0 } else { 0.0 });
        let mut events = EventQueue::with_capacity(1);
        let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
        for _ in 0..48 {
            scene
                .schedule_mut()
                .process_io(&mut out, &mut params, &mut events);
        }
        let h = scene.readout("if", 1).expect("In 1 Peak readout");
        scene.schedule().readout_value(h).expect("In 1 Peak value")
    };

    // --- A whisper sits below the noise/resolution floor ---------------------------------------
    // 30 dB SPL = 632.5 µPa. Condenser at default gain: 632.5e-6 × 0.01 × 0.985222 × 2.511886 ×
    // 0.999850 = 15.65 µV = −93.89 dBu — below the meter's −60 floor, below the 8i6's 16-bit AD
    // LSB (1 V/2¹⁵ = 30.5 µV = −88.1 dBu), and below the condenser's own f32 phantom-pedestal
    // cancellation floor (~4 µV). A whisper is simply lost: you need a sensitive mic and
    // a lot of gain. (We assert the buried floor, not a precise recovered magnitude — at high gain
    // the condenser's amplified pedestal residual dominates a signal this small, the documented
    // f32 pedestal-cancellation boundary; the physical gain-staging lesson is carried by the two
    // cases below.)
    let whisper = read_in1_peak("condenser_mic", 30.0, MicPreamp::MIN_GAIN, false);
    assert!(
        (whisper - (-60.0)).abs() < 1e-3,
        "whisper at default gain reads the −60 floor (buried), got {whisper} dBu"
    );

    // --- A dynamic reads ~14 dB below a condenser (same source, same gain) ----------------------
    // 94 dB SPL ≈ 1.0024 Pa, default gain. The ~14 dB gap is emergent from transducer efficiency
    // (5× sensitivity = 13.98 dB) plus the slightly deeper dynamic loading divider (+0.04 dB).
    //   condenser: 1.0024 × 0.01  × 0.985222 × 2.511886 × 0.999850 = 24.80 mV = −29.89 dBu
    //   dynamic:   1.0024 × 0.002 × 0.980392 × 2.511886 × 0.999850 =  4.94 mV = −43.91 dBu
    let condenser = read_in1_peak("condenser_mic", 94.0, MicPreamp::MIN_GAIN, false);
    let dynamic = read_in1_peak("dynamic_mic", 94.0, MicPreamp::MIN_GAIN, false);
    assert!(
        (condenser - (-29.89)).abs() < 0.1,
        "condenser at 94 dB SPL / +8 dB: −29.89 dBu, got {condenser}"
    );
    assert!(
        (dynamic - (-43.91)).abs() < 0.1,
        "dynamic at 94 dB SPL / +8 dB: −43.91 dBu, got {dynamic}"
    );
    assert!(
        (condenser - dynamic - 14.02).abs() < 0.1,
        "dynamic sits ~14 dB below the condenser, got {} dB",
        condenser - dynamic
    );

    // --- A cranked amp needs the PAD to avoid clipping ------------------------------------------
    // 120 dB SPL = 20 Pa, condenser, preamp at ×100 (+40 dB). Post-divider = 20 × 0.01 × 0.985222
    // = 0.19704 V.
    //   No PAD: × 100 = 19.70 V → clamped to the 10 V rail (meter 9.9985 V = +22.22 dBu). The
    //           unclipped ideal is +28.11 dBu, so ~5.9 dB is being clipped off — audible distortion.
    //   PAD on: × 0.316228 × 100 = 6.231 V → below the rail (meter 6.230 V = +18.11 dBu), clean.
    let cranked_no_pad = read_in1_peak("condenser_mic", 120.0, 100.0, false);
    let cranked_pad = read_in1_peak("condenser_mic", 120.0, 100.0, true);
    assert!(
        (cranked_no_pad - 22.22).abs() < 0.1,
        "cranked amp with no PAD pins the 10 V rail at +22.22 dBu (clipping), got {cranked_no_pad}"
    );
    assert!(
        (cranked_pad - 18.11).abs() < 0.1,
        "cranked amp with the PAD lands at +18.11 dBu, clear of the rail (no clip), got {cranked_pad}"
    );
    // The PAD dropped the peak below the rail: without it the preamp clips, with it it doesn't.
    assert!(
        cranked_pad < cranked_no_pad && cranked_pad < 20.0,
        "the PAD must bring the cranked source under the rail: no-pad {cranked_no_pad}, pad {cranked_pad}"
    );
}

/// A cabled connection assembles (the cable's R·C rides the edge). Smoke test that the cable path
/// builds and runs; the electrical effect itself is the engine's own tested concern.
#[test]
fn cabled_connection_builds() {
    let mut patch = canonical_patch();
    patch.connections[0].cable = Some(crate::scene::CableSpec {
        type_id: None,
        resistance_ohms: 150.0,
        capacitance_farads: 1e-9,
    });
    let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("cabled patch builds");
    assert!(peak_after_note(&mut scene, "synth", 32) > 0.05);
}

#[test]
fn balanced_trs_xlr_adapter_connects_luma_and_scarlett_in_both_directions() {
    // Scarlett Line Out 3 is a balanced ¼-inch line jack (output id 4); Luma Line In is balanced
    // XLR. Their two analog conductors are compatible electrically, but only the catalog's named
    // passive adapter lead makes the unlike physical endpoints legal.
    let patch = |cable| Patch {
        devices: vec![device("if", "scarlett_8i6"), device("luma", "luma_leveler")],
        connections: vec![Connection {
            cable,
            ..conn("if", 4, "luma", 0)
        }],
        output: PortRef {
            device: "luma".into(),
            port: 0,
        },
    };
    let adapter = crate::scene::CableSpec {
        type_id: Some("balanced_trs_xlr_3m".into()),
        resistance_ohms: 0.15,
        capacitance_farads: 1.5e-10,
    };
    build_patch(&patch(Some(adapter.clone())), BLOCK_LEN, rate(), 0)
        .expect("the catalog-backed balanced adapter builds");

    let err = build_patch(&patch(None), BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(
        matches!(err, BuildError::ConnectorMismatch { .. }),
        "got {err:?}"
    );

    // The reverse route uses the same passive lead. Scarlett inputs 3–6 are balanced TRS
    // differential inputs, so a balanced XLR Luma output can land on Line In 3 without a
    // lossy fake unbalancing stage.
    let reverse = Patch {
        devices: vec![device("luma", "luma_leveler"), device("if", "scarlett_8i6")],
        connections: vec![Connection {
            cable: Some(adapter),
            ..conn("luma", 0, "if", 2)
        }],
        output: PortRef {
            device: "if".into(),
            port: 2, // Line Out 1 is the first analogue-facing Scarlett output.
        },
    };
    build_patch(&reverse, BLOCK_LEN, rate(), 0)
        .expect("the same balanced adapter connects Luma output to Scarlett Line In 3");
}

/// The static loading-loss readout reads each connection's baked divider. In the canonical patch
/// `synth(1 Ω)→ad(1 MΩ)` [conn 0], `ad→da` digital [conn 1], `da(150 Ω)→spk(10 kΩ)` [conn 2]:
/// hand calc for conn 2 is 10000/(150+10000) = 0.985222 → 20·log10 = −0.1293 dB; conn 0 is
/// essentially unloaded; conn 1 is digital (ideal, no loss).
#[test]
fn connection_loading_loss_reads_the_baked_divider() {
    let scene = build_patch(&canonical_patch(), BLOCK_LEN, rate(), 0).expect("builds");

    let loss2 = scene.connection_loading_loss(2).expect("da→spk is analog");
    assert!((loss2 - (-0.1293)).abs() < 1e-3, "conn 2 loss {loss2}");

    let loss0 = scene
        .connection_loading_loss(0)
        .expect("synth→ad is analog");
    assert!(
        loss0.abs() < 1e-3,
        "1 Ω into 1 MΩ is ~unloaded, got {loss0}"
    );

    assert!(
        scene.connection_loading_loss(1).is_none(),
        "digital edge has no resistive loading loss"
    );
    assert!(
        scene.connection_loading_loss(9).is_none(),
        "out-of-range connection index"
    );
}

/// A cable's series resistance joins the divider and deepens the loading loss. A 1 kΩ cable on
/// `da(150 Ω)→spk(10 kΩ)` gives 10000/(150+1000+10000) = 0.89686 → −0.9458 dB, well past the
/// ideal wire's −0.1293 dB.
#[test]
fn a_cable_deepens_the_loading_loss() {
    let mut patch = canonical_patch();
    patch.connections[2].cable = Some(crate::scene::CableSpec {
        type_id: None,
        resistance_ohms: 1000.0,
        capacitance_farads: 1e-9,
    });
    let scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("builds");
    let loss = scene.connection_loading_loss(2).expect("analog");
    assert!((loss - (-0.9458)).abs() < 1e-3, "cabled conn 2 loss {loss}");
}

/// A metering device's readouts resolve by `(device, id)` and appear in the snapshot. A silent
/// scene reads the VU meter's floor; resolution is total (out-of-range id, non-meter device,
/// unknown device all → `None`).
#[test]
fn meter_readout_resolves_and_snapshots() {
    // synth → vu (inline analog meter) → ad → da → spk.
    let patch = Patch {
        devices: vec![
            device("synth", "synth_voice"),
            device("vu", "vu_meter"),
            device("ad", "ad_converter"),
            device("da", "da_converter"),
            device("spk", "speaker"),
        ],
        connections: vec![
            conn("synth", 0, "vu", 0),
            conn("vu", 0, "ad", 0),
            conn("ad", 0, "da", 0),
            conn("da", 0, "spk", 0),
        ],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    };
    let mut scene = build_patch(&patch, BLOCK_LEN, rate(), 0).expect("builds");

    assert!(scene.readout("vu", 0).is_some(), "VU readout");
    assert!(scene.readout("vu", 1).is_some(), "peak-dBu readout");
    assert!(scene.readout("vu", 2).is_none(), "readout id out of range");
    assert!(
        scene.readout("synth", 0).is_none(),
        "the synth exposes no readouts"
    );
    assert!(scene.readout("nope", 0).is_none(), "unknown device");

    // Render a few silent blocks; the VU meter settles at its reading floor.
    let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
    for _ in 0..8 {
        scene.schedule_mut().process(&mut out);
    }
    let snapshot = scene.readout_snapshot();
    let vu = snapshot
        .iter()
        .find(|(d, _)| d == "vu")
        .expect("vu in snapshot");
    assert_eq!(vu.1.len(), 2, "VU + peak");
    assert!(vu.1[0] < -50.0, "silent VU near floor, got {}", vu.1[0]);
}

#[test]
fn unknown_type_is_an_error() {
    let patch = Patch {
        devices: vec![device("x", "no_such_device")],
        connections: vec![],
        output: PortRef {
            device: "x".into(),
            port: 0,
        },
    };
    let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(matches!(err, BuildError::UnknownType { .. }), "got {err:?}");
}

#[test]
fn duplicate_device_id_is_an_error() {
    let patch = Patch {
        devices: vec![device("dup", "speaker"), device("dup", "speaker")],
        connections: vec![],
        output: PortRef {
            device: "dup".into(),
            port: 0,
        },
    };
    let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(
        matches!(err, BuildError::DuplicateDevice { .. }),
        "got {err:?}"
    );
}

#[test]
fn unknown_device_reference_is_an_error() {
    let mut patch = canonical_patch();
    patch.connections.push(conn("ghost", 0, "spk", 0));
    let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(
        matches!(err, BuildError::UnknownDevice { .. }),
        "got {err:?}"
    );
}

#[test]
fn port_out_of_range_is_an_error() {
    let mut patch = canonical_patch();
    patch.output.port = 5; // speaker has only output port 0
    let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(
        matches!(err, BuildError::OutputPortOutOfRange { .. }),
        "got {err:?}"
    );
}

/// The output tap must resolve to an **analog** port — it's rendered as a voltage. Tapping a digital
/// output (here the AD converter's) is rejected at build, so `load_patch` keeps the running scene
/// rather than `render_quantum` faulting on it (an `unreachable`, which is session-fatal). The bench
/// and UI steer to analog outputs; this guarantees the engine can never be handed a non-analog tap.
#[test]
fn non_analog_output_tap_is_rejected() {
    let patch = Patch {
        devices: vec![device("ad", "ad_converter")],
        connections: vec![],
        output: PortRef {
            device: "ad".into(),
            port: 0, // the AD converter's output is digital
        },
    };
    let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(
        matches!(err, BuildError::OutputTapNotAnalog { .. }),
        "got {err:?}"
    );
}

/// `ConnectorMismatch` renders a legible message (the worklet surfaces build errors as text).
/// The rejection path itself needs two **same-domain** ports with different connectors; the gate
/// is covered by `connectors_compatible` (catalog) + the TS `evaluateConnection` mirror, and here
/// we pin the error's Display.
#[test]
fn connector_mismatch_displays_legibly() {
    let e = BuildError::ConnectorMismatch {
        from: "amp".into(),
        to: "spk".into(),
        from_connector: Connector::Xlr,
        to_connector: Connector::QuarterInch,
    };
    let msg = format!("{e}");
    assert!(msg.contains("incompatible connectors"), "got {msg}");
    assert!(msg.contains("amp") && msg.contains("spk"), "names in {msg}");
}

/// A cross-domain wire (digital output → analog input) is left to the engine's domain check and
/// surfaces as `Compile(DomainMismatch)`, not a panic and not a `ConnectorMismatch` — the connector
/// gate is domain-scoped, so cross-domain falls through to compile (domain-then-connector precedence).
#[test]
fn domain_mismatch_surfaces_as_compile_error() {
    let patch = Patch {
        devices: vec![device("ad", "ad_converter"), device("spk", "speaker")],
        // AD output is digital; speaker input is analog → DomainMismatch at compile.
        connections: vec![conn("ad", 0, "spk", 0)],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    };
    let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(
        matches!(
            err,
            BuildError::Compile(CompileError::DomainMismatch { .. })
        ),
        "got {err:?}"
    );
}

/// The channel-count guard is **config-aware**: a default (2×2) computer — not the 8×6 one the loop
/// tests configure — can't legally receive the 8i6's 8-lane USB send. `build_patch` reads the
/// computer's *instance* face (2-wide), so the mismatch surfaces as a legible `ChannelCountMismatch`
/// pre-compile, the flip side of the 8×6 tests that prove the matching shape is accepted.
#[test]
fn a_default_computer_cannot_receive_the_interfaces_wide_send() {
    let patch = Patch {
        devices: vec![device("if", "scarlett_8i6"), device("computer", "computer")],
        // 8i6 USB send (8 ch) → default computer USB in (2 ch): a channel-count mismatch.
        connections: vec![conn("if", 0, "computer", 0)],
        output: PortRef {
            device: "computer".into(),
            port: 0,
        },
    };
    let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(
        matches!(
            err,
            BuildError::ChannelCountMismatch {
                from_channels: 8,
                to_channels: 2,
                ..
            }
        ),
        "got {err:?}"
    );
}

/// Two `computer`s on **different** `usb_sends`/`usb_returns` configs: each connection is checked
/// against *its own* instance's descriptor. The wide (8×6) computer accepts the 8i6's 8-ch send even
/// though a default (2×2) computer is listed after it — a type-keyed descriptor map (last instance
/// wins) would have checked it against the 2-ch face and wrongly rejected it.
#[test]
fn each_computer_is_validated_against_its_own_config() {
    let patch = Patch {
        devices: vec![
            device("if", "scarlett_8i6"),
            computer_8x6("wide"),
            device("narrow", "computer"),
            device("spk", "speaker"),
        ],
        connections: vec![
            conn("if", 0, "wide", 0), // 8i6 USB send (8 ch) → wide computer USB in (8 ch)
            conn("if", 2, "spk", 0),  // 8i6 Line Out 1 → speaker
        ],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    };
    let scene = build_patch(&patch, BLOCK_LEN, rate(), 0)
        .expect("the 8-ch send matches the wide computer's own 8-ch input");

    // The scene carries each instance's own descriptor: 8 send lanes vs the default 2.
    let usb_in_channels = |id: &str| {
        scene.device_descriptors()[id]
            .ports
            .iter()
            .find(|p| p.direction == PortDirection::Input && p.id == 0)
            .map(|p| p.channels)
    };
    assert_eq!(usb_in_channels("wide"), Some(8));
    assert_eq!(usb_in_channels("narrow"), Some(2));
    assert_eq!(scene.device_descriptors().len(), 4, "one per device");
}

/// The flip side: the 8i6's 8-ch send into the **narrow** computer is a legible
/// `ChannelCountMismatch` even when a wide computer is listed after it (with a type-keyed map, the
/// wide face would have passed the pre-compile check, leaving only the engine's lane error).
#[test]
fn a_narrow_computer_rejects_the_wide_send_regardless_of_order() {
    let patch = Patch {
        devices: vec![
            device("if", "scarlett_8i6"),
            device("narrow", "computer"),
            computer_8x6("wide"),
            device("spk", "speaker"),
        ],
        connections: vec![conn("if", 0, "narrow", 0), conn("if", 2, "spk", 0)],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    };
    let err = build_patch(&patch, BLOCK_LEN, rate(), 0).unwrap_err();
    assert!(
        matches!(
            err,
            BuildError::ChannelCountMismatch {
                from_channels: 8,
                to_channels: 2,
                ..
            }
        ),
        "got {err:?}"
    );
}
