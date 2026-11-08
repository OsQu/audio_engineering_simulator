use super::descriptor::{
    ConfigKind, Connector, FormFactor, PortDirection, PortDomain, PortKind, connectors_compatible,
};
use super::expand::{
    build_asset_node, build_expansion, describe, describe_device, describe_expansion, descriptors,
    entry, instantiate, instantiate_with_assets,
};
use super::guitar::{ELECTRIC_GUITAR_Z_OUT_OHMS, TIM_GM_6MB, gm_preset};
use super::spec::{CatalogEntry, DeviceConfig, NodeFace, NodeSpec, PortUi, ReadoutSpec};
use super::*;
use crate::assets::{AssetBuildError, BuildContext, SoundFontBankLibrary};
use crate::scene::ConfigSetting;
use engine::{Domain, Graph, Node, ParamId, ReadoutId};
const FIXTURE_SOUNDFONT_ID: &str = "test-gm";

const ASSET_FIXTURE: CatalogEntry = CatalogEntry {
    type_id: "asset_soundfont_fixture",
    name: "Asset SoundFont Fixture",
    form_factor: FormFactor::Rackmount { rack_units: 1 },
    nodes: &[NodeSpec::Asset {
        build: soundfont_fixture,
        face: soundfont_face,
    }],
    internal: &[],
    params: &[],
    param_groups: &[],
    param_grid: None,
    inputs: &[PortUi {
        label: "MIDI",
        kind: PortKind::Midi,
        connector: Connector::Din5,
    }],
    outputs: &[PortUi {
        label: "Digital",
        kind: PortKind::Digital,
        connector: Connector::Digital,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[]),
    configs: &[],
};

const COLD_DESCRIPTOR_FIXTURE: CatalogEntry = CatalogEntry {
    type_id: "cold_asset_fixture",
    name: "Cold Asset Fixture",
    form_factor: FormFactor::Rackmount { rack_units: 1 },
    nodes: &[NodeSpec::Asset {
        build: must_not_build,
        face: soundfont_face,
    }],
    internal: &[],
    params: &[],
    param_groups: &[],
    param_grid: None,
    inputs: &[PortUi {
        label: "MIDI",
        kind: PortKind::Midi,
        connector: Connector::Din5,
    }],
    outputs: &[PortUi {
        label: "Digital",
        kind: PortKind::Digital,
        connector: Connector::Digital,
    }],
    delayed_outputs: &[],
    duplex_links: &[],
    readouts: ReadoutSpec::Static(&[]),
    configs: &[],
};

fn soundfont_face(_config: &DeviceConfig) -> NodeFace {
    NodeFace {
        inputs: engine::SoundFontSource::input_face().to_vec(),
        outputs: engine::SoundFontSource::output_face().to_vec(),
        params: Vec::new(),
        readouts: Vec::new(),
    }
}

fn soundfont_fixture(
    config: &DeviceConfig,
    context: &BuildContext,
) -> Result<Box<dyn Node>, AssetBuildError> {
    let bank = context.soundfont(FIXTURE_SOUNDFONT_ID).ok_or_else(|| {
        AssetBuildError::MissingSoundFont {
            asset_id: FIXTURE_SOUNDFONT_ID.into(),
        }
    })?;
    Ok(Box::new(engine::SoundFontSource::from_bank(
        bank,
        gm_preset(config, "preset", 0.0),
    )?))
}

fn must_not_build(
    _config: &DeviceConfig,
    _context: &BuildContext,
) -> Result<Box<dyn Node>, AssetBuildError> {
    panic!("describing an asset-backed catalog entry must not build its node")
}

#[test]
fn asset_builder_requires_a_stable_id_and_normalizes_preset_config() {
    let missing = SoundFontBankLibrary::default();
    let config = [ConfigSetting {
        key: "preset".into(),
        value: 200.0,
    }];
    let err = match build_asset_node(
        soundfont_fixture,
        &DeviceConfig::new(&config),
        &BuildContext::new(&missing),
    ) {
        Err(err) => err,
        Ok(_) => panic!("a missing stable asset id must fail construction"),
    };
    assert_eq!(
        err,
        AssetBuildError::MissingSoundFont {
            asset_id: FIXTURE_SOUNDFONT_ID.into()
        }
    );
    assert!(matches!(
        crate::BuildError::from(err),
        crate::BuildError::Asset(_)
    ));

    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/soundfonts/TimGM6mb.sf2"
    ))
    .expect("the vendored TimGM6mb.sf2 asset");
    let mut assets = SoundFontBankLibrary::default();
    assets.insert(
        FIXTURE_SOUNDFONT_ID,
        engine::SoundFontBank::parse(&bytes).expect("valid GM soundfont"),
    );
    let node = build_asset_node(
        soundfont_fixture,
        &DeviceConfig::new(&config),
        &BuildContext::new(&assets),
    )
    .expect("the supplied bank builds an independent source");
    assert_eq!(node.inputs().len(), 1, "fixture is a SoundFont source");
    assert_eq!(gm_preset(&DeviceConfig::new(&config), "preset", 0.0), 127);
}

#[test]
fn gm_preset_clamps_rounds_and_rejects_non_finite_values() {
    let settings = [
        ConfigSetting {
            key: "low".into(),
            value: -0.6,
        },
        ConfigSetting {
            key: "middle".into(),
            value: 16.6,
        },
        ConfigSetting {
            key: "high".into(),
            value: 127.6,
        },
        ConfigSetting {
            key: "bad".into(),
            value: f32::NAN,
        },
    ];
    let config = DeviceConfig::new(&settings);
    assert_eq!(gm_preset(&config, "low", 4.0), 0);
    assert_eq!(gm_preset(&config, "middle", 4.0), 17);
    assert_eq!(gm_preset(&config, "high", 4.0), 127);
    assert_eq!(gm_preset(&config, "bad", 4.0), 4);
}

#[test]
fn guitar_catalog_entries_pin_their_faces_presets_and_fresh_voice_state() {
    let electric = describe_device("electric_guitar", &DeviceConfig::EMPTY)
        .expect("electric guitar descriptor");
    let acoustic = describe_device("acoustic_guitar", &DeviceConfig::EMPTY)
        .expect("acoustic guitar descriptor");
    assert_eq!(electric.ports[0].domain, PortDomain::Events);
    assert_eq!(electric.ports[1].domain, PortDomain::Analog);
    assert_eq!(electric.ports[1].kind, PortKind::Instrument);
    assert_eq!(acoustic.ports[0].domain, PortDomain::Events);
    assert_eq!(acoustic.ports[1].domain, PortDomain::Acoustic);
    assert_eq!(acoustic.ports[1].kind, PortKind::Acoustic);
    let electric_preset = electric.configs.first().expect("electric preset selector");
    assert_eq!(electric_preset.kind, ConfigKind::Select);
    assert_eq!(electric_preset.default, 27.0);
    assert_eq!(electric_preset.options.len(), 6);
    assert_eq!(electric_preset.options[0].value, 26.0);
    assert_eq!(electric_preset.options[5].value, 31.0);

    let acoustic_preset = acoustic.configs.first().expect("acoustic preset selector");
    assert_eq!(acoustic_preset.kind, ConfigKind::Select);
    assert_eq!(acoustic_preset.default, 25.0);
    assert_eq!(acoustic_preset.options.len(), 2);
    assert_eq!(acoustic_preset.options[0].value, 24.0);
    assert_eq!(acoustic_preset.options[1].value, 25.0);

    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/soundfonts/TimGM6mb.sf2"
    ))
    .expect("the vendored TimGM6mb asset");
    let mut assets = SoundFontBankLibrary::default();
    assets.insert(
        TIM_GM_6MB,
        engine::SoundFontBank::parse(&bytes).expect("valid GM soundfont"),
    );
    let context = BuildContext::new(&assets);
    let electric_entry = entry("electric_guitar").expect("electric guitar entry");
    let first = build_expansion(electric_entry, &DeviceConfig::EMPTY, &context)
        .expect("shared bank builds an electric guitar");
    let second = build_expansion(electric_entry, &DeviceConfig::EMPTY, &context)
        .expect("the same bank builds an independent electric guitar");
    assert_eq!(first.nodes.len(), 2);
    assert_eq!(
        first.nodes[1].outputs()[0]
            .analog()
            .expect("electric DA is analog")
            .z_out()
            .get(),
        ELECTRIC_GUITAR_Z_OUT_OHMS
    );
    assert!(
        !std::ptr::addr_eq(&*first.nodes[0], &*second.nodes[0]),
        "each guitar owns a fresh SoundFont synth over the shared immutable bank"
    );
    let acoustic_entry = entry("acoustic_guitar").expect("acoustic guitar entry");
    let acoustic_build = build_expansion(acoustic_entry, &DeviceConfig::EMPTY, &context)
        .expect("shared bank builds an acoustic guitar");
    assert_eq!(acoustic_build.nodes.len(), 3);
    assert_eq!(
        acoustic_build.nodes[2].outputs()[0].domain(),
        Domain::Acoustic
    );
}

/// Each entry's hand-authored UI metadata lines up, position-for-position, with its *exposed*
/// face — the open ports + concatenated params its nodes actually present. The guard against a node
/// (or an internal edge) changing what's exposed without the catalog following; the `zip` in
/// `describe` would otherwise silently truncate.
#[test]
fn catalog_aligns_with_exposed_face() {
    for entry in CATALOG {
        let face = describe_expansion(entry, &DeviceConfig::EMPTY);
        // Generated labels size to the built face, so materialize the grid with the same counts
        // `describe` uses: columns = exposed return lanes, rows = the matrix's own crosspoints over
        // the columns (so a crossbar whose inputs exceed the input ports still aligns). `Named`
        // axes ignore the counts and self-size, so this still checks the 8i6's hand-named 14×14.
        let m_out: usize = face.outputs.iter().map(|p| usize::from(p.channels)).sum();
        let grid_crosspoints = face
            .params
            .len()
            .saturating_sub(entry.params.len())
            .saturating_sub(entry.param_groups.len());
        let grid_rows = grid_crosspoints.checked_div(m_out).unwrap_or(0);
        // The exposed param face is the ungrouped UIs ++ the generated matrix crosspoints ++ one
        // entry per group.
        let grid = entry
            .param_grid
            .as_ref()
            .map_or(0, |g| g.labels(grid_rows, m_out).len());
        assert_eq!(
            entry.params.len() + grid + entry.param_groups.len(),
            face.params.len(),
            "{} params",
            entry.type_id
        );
        assert_eq!(
            entry.inputs.len(),
            face.inputs.len(),
            "{} inputs",
            entry.type_id
        );
        assert_eq!(
            entry.outputs.len(),
            face.outputs.len(),
            "{} outputs",
            entry.type_id
        );
        // Per-metering-node readout counts, in node order (mirrors `describe`).
        let mut node_readout_counts: Vec<usize> = Vec::new();
        let mut last_node: Option<usize> = None;
        for r in &face.readouts {
            if last_node != Some(r.node) {
                node_readout_counts.push(0);
                last_node = Some(r.node);
            }
            if let Some(c) = node_readout_counts.last_mut() {
                *c += 1;
            }
        }
        assert_eq!(
            entry.readouts.label(&node_readout_counts).len(),
            face.readouts.len(),
            "{} readouts",
            entry.type_id
        );
    }
}

/// Every target of a param group must carry an **identical** decl (range/default/smooth) — the
/// descriptor derives the group's exposed range/default from the first target, so a mismatch would
/// silently misreport the others. An authoring guard (e.g. the 8i6's Power group over `GainStage`,
/// `AdConverter`, and `DaConverter` `powered` params, which are deliberately declared identical).
#[test]
fn catalog_group_targets_carry_identical_decls() {
    for entry in CATALOG {
        if entry.param_groups.is_empty() {
            continue;
        }
        let nodes: Vec<Box<dyn Node>> = entry
            .nodes
            .iter()
            .map(|spec| match spec {
                NodeSpec::Plain(build) => build(&DeviceConfig::EMPTY),
                NodeSpec::Asset { .. } => panic!("catalog test fixture is not in CATALOG"),
            })
            .collect();
        for group in entry.param_groups {
            let decl_of = |&(ni, id): &(usize, ParamId)| {
                *nodes[ni]
                    .params()
                    .iter()
                    .find(|d| d.id == id)
                    .unwrap_or_else(|| panic!("{} group targets a missing param", entry.type_id))
            };
            let first = decl_of(&group.targets[0]);
            for target in group.targets {
                let d = decl_of(target);
                assert_eq!(
                    d.min.to_bits(),
                    first.min.to_bits(),
                    "{} min",
                    entry.type_id
                );
                assert_eq!(
                    d.max.to_bits(),
                    first.max.to_bits(),
                    "{} max",
                    entry.type_id
                );
                assert_eq!(
                    d.default.to_bits(),
                    first.default.to_bits(),
                    "{} default",
                    entry.type_id
                );
                assert_eq!(
                    d.smooth_ms.to_bits(),
                    first.smooth_ms.to_bits(),
                    "{} smooth_ms",
                    entry.type_id
                );
            }
        }
    }
}

/// Each descriptor carries the exposed param id (its **position** in the exposed face — what the
/// host addresses, matching ports/readouts, *not* the node-local `ParamId`) plus the node's real
/// ranges/defaults (bit-exact, derived not retyped) and real port domains — so the UI can never
/// misaddress a param, show a stale range, or wire a wrong-domain port.
#[test]
fn descriptors_carry_engine_truth() {
    for entry in CATALOG {
        let face = describe_expansion(entry, &DeviceConfig::EMPTY);
        let desc = describe(entry, &DeviceConfig::EMPTY);

        for (i, (pd, ep)) in desc.params.iter().zip(&face.params).enumerate() {
            assert_eq!(pd.id, i as u32, "{} param id", entry.type_id);
            // Bit-exact: derived from the decl, not hand-retyped, so identity holds.
            assert_eq!(pd.min.to_bits(), ep.min.to_bits(), "{} min", entry.type_id);
            assert_eq!(pd.max.to_bits(), ep.max.to_bits(), "{} max", entry.type_id);
            assert_eq!(
                pd.default.to_bits(),
                ep.default.to_bits(),
                "{} default",
                entry.type_id
            );
        }

        let inputs: Vec<_> = desc
            .ports
            .iter()
            .filter(|p| p.direction == PortDirection::Input)
            .collect();
        assert_eq!(inputs.len(), face.inputs.len(), "{} inputs", entry.type_id);
        for (pd, ep) in inputs.iter().zip(&face.inputs) {
            assert_eq!(
                pd.domain,
                ep.domain.into(),
                "{} input domain",
                entry.type_id
            );
            // Channels are the port's lane count — engine truth, derived not retyped.
            assert_eq!(pd.channels, ep.channels, "{} input channels", entry.type_id);
        }
    }
}

/// A multi-node device's descriptor param ids are the **exposed positions** (`0..n`), not the
/// node-local `ParamId`s — which would collide. The two-stage `channel_strip` exposes both stages'
/// gain (`ParamId(0)`) + power (`ParamId(1)`); node-local ids would be `[0,1,0,1]`, misaddressing
/// the second stage (`BuiltScene::param` indexes positionally) and colliding as UI keys. The
/// descriptor must instead expose `[0,1,2,3]`.
#[test]
fn multi_node_descriptor_param_ids_are_exposed_positions() {
    let strip = descriptors()
        .into_iter()
        .find(|d| d.type_id == "channel_strip")
        .expect("channel_strip is in the catalog");

    let ids: Vec<u32> = strip.params.iter().map(|p| p.id).collect();
    assert_eq!(
        ids,
        vec![0, 1, 2, 3],
        "exposed positions, not node-local ids"
    );
}

#[test]
fn luma_leveler_expands_as_one_balanced_analog_line_processor() {
    let descriptor = descriptors()
        .into_iter()
        .find(|d| d.type_id == "luma_leveler")
        .expect("luma_leveler is in the catalog");
    assert_eq!(
        descriptor.form_factor,
        FormFactor::Rackmount { rack_units: 3 }
    );
    assert_eq!(
        descriptor
            .params
            .iter()
            .map(|p| p.label.as_str())
            .collect::<Vec<_>>(),
        ["Peak Reduction", "Gain", "Limit", "Power"]
    );
    assert_eq!(
        descriptor
            .readouts
            .iter()
            .map(|r| (r.label.as_str(), r.unit.as_str()))
            .collect::<Vec<_>>(),
        [("Gain Reduction", "dB"), ("Output", "dBu")]
    );
    assert!(descriptor.configs.is_empty());
    assert_eq!(descriptor.ports.len(), 2);
    for port in &descriptor.ports {
        assert_eq!(port.domain, PortDomain::Analog);
        assert_eq!(port.channels, 2, "balanced line has two conductors");
        assert_eq!(port.kind, PortKind::Line);
        assert_eq!(port.connector, Connector::Xlr);
    }

    let mut graph = Graph::new();
    let built = instantiate("luma_leveler", &DeviceConfig::EMPTY, &mut graph)
        .expect("luma_leveler instantiates");
    assert_eq!(built.nodes.len(), 1);
    assert_eq!(graph.connection_count(), 0);
    assert_eq!(built.inputs.len(), 1);
    assert_eq!(built.outputs.len(), 1);
    assert_eq!(built.params.len(), 4);
    assert_eq!(built.readouts.len(), 2);
}

/// The chassis seam: a multi-node device expands into several nodes wired by its internal edge,
/// and its exposed face maps to the right `(NodeId, …)`. The two-stage channel strip exposes
/// stage 0's input and stage 1's output (input and output on *different* nodes), and each stage's
/// gain + power params — concatenated in node order, so device params **2/3** resolve to the
/// **second** node (a non-trivial remap, the case a naive "everything is node 0" impl would get
/// wrong).
#[test]
fn multi_node_device_expands_and_maps() {
    let mut g = Graph::new();
    let strip = instantiate("channel_strip", &DeviceConfig::EMPTY, &mut g)
        .expect("channel_strip is in the catalog");

    assert_eq!(strip.nodes.len(), 2, "two internal nodes");
    assert_eq!(g.connection_count(), 1, "one internal edge wired");

    // Exposed input is stage 0's input; exposed output is stage 1's output.
    assert_eq!(strip.inputs, vec![(strip.nodes[0], 0)]);
    assert_eq!(strip.outputs, vec![(strip.nodes[1], 0)]);

    // Each stage's gain (ParamId 0) + power (ParamId 1) exposed, in node order — device params
    // 2/3 map to the *second* node. No groups, so each exposed param is a single target.
    assert_eq!(
        strip.params,
        vec![
            vec![(strip.nodes[0], ParamId(0))],
            vec![(strip.nodes[0], ParamId(1))],
            vec![(strip.nodes[1], ParamId(0))],
            vec![(strip.nodes[1], ParamId(1))],
        ]
    );
}

/// A single-node device is the trivial case: one node, no internal edges, and the node's own face
/// exposed as-is.
#[test]
fn single_node_device_is_identity() {
    let mut g = Graph::new();
    let spk =
        instantiate("speaker", &DeviceConfig::EMPTY, &mut g).expect("speaker is in the catalog");

    assert_eq!(spk.nodes.len(), 1);
    assert_eq!(g.connection_count(), 0, "no internal edges");
    assert_eq!(spk.inputs, vec![(spk.nodes[0], 0)]);
    assert_eq!(spk.outputs, vec![(spk.nodes[0], 0)]);
}

/// The vox source's descriptor face: no inputs, one **acoustic** output (mono), and two params
/// (loudness in Pa, pitch in Hz — the engine-native units; dB SPL is web display only).
#[test]
fn vox_descriptor_face() {
    let vox = descriptors()
        .into_iter()
        .find(|d| d.type_id == "vox")
        .expect("vox is in the catalog");

    assert_eq!(vox.ports.len(), 1, "one port: the acoustic Air out");
    let out = &vox.ports[0];
    assert_eq!(out.direction, PortDirection::Output);
    assert_eq!(out.domain, PortDomain::Acoustic);
    assert_eq!(out.channels, 1, "acoustic is mono");
    assert_eq!(out.kind, PortKind::Acoustic);
    assert_eq!(out.connector, Connector::Acoustic);

    // Engine truth: LEVEL default = 1 Pa, FREQ default = 220 Hz.
    assert_eq!(vox.params.len(), 2);
    assert_eq!(vox.params[0].label, "Loudness");
    assert_eq!(vox.params[0].default, 1.0);
    assert_eq!(vox.params[1].label, "Pitch");
    assert_eq!(vox.params[1].default, 220.0);
    assert!(vox.configs.is_empty());
}

/// The powered monitor's descriptor face: a line-level ¼" **input** + an acoustic Air
/// **output** (mono, `Connector::Acoustic`, mating only with a mic capsule), and one Volume knob.
/// The output-side mirror of the vox's face.
#[test]
fn powered_monitor_descriptor_face() {
    let mon = descriptors()
        .into_iter()
        .find(|d| d.type_id == "powered_monitor")
        .expect("powered_monitor is in the catalog");

    assert_eq!(mon.ports.len(), 2, "a line input and an acoustic output");
    let input = mon
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Input)
        .expect("a line input");
    assert_eq!(input.domain, PortDomain::Analog);
    assert_eq!(input.kind, PortKind::Line);
    assert_eq!(input.connector, Connector::QuarterInch);
    assert_eq!(input.channels, 1, "a single-conductor line input");

    let air = mon
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Output)
        .expect("an acoustic Air output");
    assert_eq!(air.domain, PortDomain::Acoustic);
    assert_eq!(air.channels, 1, "acoustic is mono");
    assert_eq!(air.kind, PortKind::Acoustic);
    assert_eq!(air.connector, Connector::Acoustic);

    // One knob: Volume, defaulting to unity (1.0 linear, shown as 0 dB by the web).
    assert_eq!(mon.params.len(), 1);
    assert_eq!(mon.params[0].label, "Volume");
    assert_eq!(mon.params[0].default, 1.0);
    assert!(mon.configs.is_empty());
}

/// The condenser mic's port remap (single node): one node, no internal edges, its acoustic capsule
/// exposed as device input 0 and its balanced output as device output 0, and **no params**
/// (sensitivity is a fixed transducer property, not a knob).
#[test]
fn condenser_mic_expands_and_maps() {
    let mut g = Graph::new();
    let mic = instantiate("condenser_mic", &DeviceConfig::EMPTY, &mut g)
        .expect("condenser_mic is in the catalog");

    assert_eq!(mic.nodes.len(), 1, "one internal node");
    assert_eq!(g.connection_count(), 0, "no internal edges");
    assert_eq!(mic.inputs, vec![(mic.nodes[0], 0)], "the acoustic capsule");
    assert_eq!(mic.outputs, vec![(mic.nodes[0], 0)]);
    assert!(mic.params.is_empty(), "no knobs — sensitivity is fixed");
    assert!(mic.readouts.is_empty(), "the mic measures nothing");
}

/// The condenser mic's descriptor face: an acoustic capsule **input** (mono) + a balanced
/// (2-conductor) analog XLR **output** styled as a mic jack, no params, no configs.
#[test]
fn condenser_mic_descriptor_face() {
    let mic = descriptors()
        .into_iter()
        .find(|d| d.type_id == "condenser_mic")
        .expect("condenser_mic is in the catalog");

    assert_eq!(mic.ports.len(), 2, "the acoustic capsule in + the XLR out");
    let cap = mic
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Input)
        .unwrap();
    assert_eq!(cap.domain, PortDomain::Acoustic);
    assert_eq!(cap.channels, 1, "acoustic is mono");
    assert_eq!(cap.kind, PortKind::Acoustic);
    assert_eq!(cap.connector, Connector::Acoustic);
    let out = mic
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Output)
        .unwrap();
    assert_eq!(out.domain, PortDomain::Analog);
    assert_eq!(out.channels, 2, "balanced = two conductors");
    assert_eq!(out.kind, PortKind::Mic);
    assert_eq!(out.connector, Connector::Xlr);

    assert!(mic.params.is_empty(), "no knobs — sensitivity is fixed");
    assert!(mic.configs.is_empty(), "no structural options on the mic");
    assert!(mic.readouts.is_empty());
}

/// The dynamic mic expands like the condenser — one node, its capsule as device input 0, its
/// output as device output 0, and no params.
#[test]
fn dynamic_mic_expands_and_maps() {
    let mut g = Graph::new();
    let mic = instantiate("dynamic_mic", &DeviceConfig::EMPTY, &mut g)
        .expect("dynamic_mic is in the catalog");

    assert_eq!(mic.nodes.len(), 1, "one internal node");
    assert_eq!(g.connection_count(), 0, "no internal edges");
    assert_eq!(mic.inputs, vec![(mic.nodes[0], 0)], "the acoustic capsule");
    assert_eq!(mic.outputs, vec![(mic.nodes[0], 0)]);
    assert!(mic.params.is_empty(), "no knobs — sensitivity is fixed");
    assert!(mic.readouts.is_empty(), "the mic measures nothing");
}

/// The dynamic mic's descriptor face: an acoustic capsule input + a balanced XLR output, no
/// params, no configs (passive — no phantom toggle).
#[test]
fn dynamic_mic_descriptor_face() {
    let mic = descriptors()
        .into_iter()
        .find(|d| d.type_id == "dynamic_mic")
        .expect("dynamic_mic is in the catalog");

    assert_eq!(mic.ports.len(), 2, "the acoustic capsule in + the XLR out");
    let cap = mic
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Input)
        .unwrap();
    assert_eq!(cap.domain, PortDomain::Acoustic);
    assert_eq!(cap.channels, 1, "acoustic is mono");
    assert_eq!(cap.kind, PortKind::Acoustic);
    assert_eq!(cap.connector, Connector::Acoustic);
    let out = mic
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Output)
        .unwrap();
    assert_eq!(out.domain, PortDomain::Analog);
    assert_eq!(out.channels, 2, "balanced = two conductors");
    assert_eq!(out.kind, PortKind::Mic);
    assert_eq!(out.connector, Connector::Xlr);

    assert!(mic.params.is_empty(), "no knobs — sensitivity is fixed");
    assert!(
        mic.configs.is_empty(),
        "no structural options on the mic (passive — no phantom toggle)"
    );
    assert!(mic.readouts.is_empty());
}

/// The 8i6 exposes the shared 48V toggle: **one** `phantom` key (both preamps read it — the real
/// unit's single global button), structural like the per-channel INST keys, default off.
#[test]
fn scarlett_8i6_exposes_the_shared_phantom_config() {
    let dev = descriptors()
        .into_iter()
        .find(|d| d.type_id == "scarlett_8i6")
        .expect("scarlett_8i6 is in the catalog");

    let phantom = dev
        .configs
        .iter()
        .find(|c| c.key == "phantom")
        .expect("the 8i6 offers the 48V config");
    assert_eq!(phantom.label, "48V");
    assert_eq!(phantom.kind, ConfigKind::Toggle);
    assert_eq!(phantom.default, 0.0, "48V boots off");
    assert_eq!(
        dev.configs.iter().filter(|c| c.key == "phantom").count(),
        1,
        "one shared key, not one per preamp"
    );
}

/// The full Scarlett 8i6 expands into its 28 internal nodes wired by 40 internal edges, and its
/// exposed face maps to the right `(NodeId, …)`. This pins the big remap: 9 inputs (2 combo, 4 line,
/// S/PDIF, USB return, MIDI-in), 9 outputs (USB send, S/PDIF, 4 line, 2 phones, MIDI-out), the
/// param face — the preamp + phones controls, the 196 matrix crosspoints, the Monitor group, and the
/// Power group over all 16 powered stages — and the 4 input-meter readouts. The param map is
/// spot-checked at its boundaries (the full enumeration would be 206 entries).
#[test]
fn scarlett_8i6_expands_with_mixed_face_io() {
    let mut g = Graph::new();
    let dev = instantiate("scarlett_8i6", &DeviceConfig::EMPTY, &mut g)
        .expect("scarlett_8i6 is in the catalog");
    let n = &dev.nodes;

    // 22 original signal/routing nodes + 4 line receivers + 2 input meters.
    assert_eq!(n.len(), 28, "28 internal nodes");
    // 34 original edges + 2 meter hops + 4 receiver→AD hops.
    assert_eq!(g.connection_count(), 40, "40 internal edges");

    // Inputs: 2 preamps, 4 line receivers, S/PDIF-in demux, USB-return demux, MIDI-in.
    assert_eq!(
        dev.inputs,
        vec![
            (n[0], 0),
            (n[1], 0),
            (n[4], 0),
            (n[5], 0),
            (n[6], 0),
            (n[7], 0),
            (n[12], 0),
            (n[13], 0),
            (n[24], 0),
        ]
    );
    // Outputs: USB-send mux, S/PDIF-out mux, monitor amps (Line 1/2), line-out DAs (3/4), phones
    // amps, MIDI-out.
    assert_eq!(
        dev.outputs,
        vec![
            (n[14], 0),
            (n[15], 0),
            (n[18], 0),
            (n[19], 0),
            (n[20], 0),
            (n[21], 0),
            (n[22], 0),
            (n[23], 0),
            (n[24], 0),
        ]
    );
    // Param face boundaries: 8 hand-authored ungrouped + 196 crosspoints + Monitor + Power = 206.
    assert_eq!(dev.params.len(), 206);
    assert_eq!(
        dev.params[0],
        vec![(n[0], ParamId(0))],
        "id 0 = preamp 1 Gain"
    );
    assert_eq!(
        dev.params[6],
        vec![(n[22], ParamId(0))],
        "id 6 = phones 1 amp Gain"
    );
    assert_eq!(
        dev.params[7],
        vec![(n[23], ParamId(0))],
        "id 7 = phones 2 amp Gain"
    );
    // Crosspoints 8..=203 are all on the matrix (node 25), ParamId 0..=195, in order.
    assert_eq!(dev.params[8], vec![(n[25], ParamId(0))], "first crosspoint");
    assert_eq!(
        dev.params[203],
        vec![(n[25], ParamId(195))],
        "last crosspoint"
    );
    // Monitor group (id 204): the two monitor amps' GAIN. Power group (id 205): all 16 `powered`.
    assert_eq!(
        dev.params[204],
        vec![(n[18], ParamId(0)), (n[19], ParamId(0))],
        "Monitor group over the monitor pair"
    );
    assert_eq!(dev.params[205].len(), 16, "Power group over all 16 stages");

    // The two input meters (nodes 26,27) each expose VU + peak-dBu, in node order → 4 readouts,
    // which the faceplate binds as the level ring around gain knobs 1 and 2.
    assert_eq!(
        dev.readouts,
        vec![
            (n[26], ReadoutId(0)),
            (n[26], ReadoutId(1)),
            (n[27], ReadoutId(0)),
            (n[27], ReadoutId(1)),
        ],
        "In 1/In 2 VU + peak readouts"
    );
}

/// The 8i6's routing matrix defaults to the **identity** — input i → output i (hardware inputs to
/// USB sends, USB returns to the analog / S-PDIF outs) — reproducing the standard fixed routing, so
/// behavior is unchanged until the user re-routes. The 196 crosspoints are exposed params 8..=203
/// (14×14, after the 8 hand-authored params); the diagonal cells default to 1.0, the rest to 0.0.
#[test]
fn scarlett_8i6_matrix_defaults_to_fixed_routing() {
    let dev = descriptors()
        .into_iter()
        .find(|d| d.type_id == "scarlett_8i6")
        .expect("scarlett_8i6 is in the catalog");
    let n = 14;
    for i in 0..n {
        for j in 0..n {
            let id = 8 + i * n + j; // crosspoint (i, j) exposed id
            let expected = if i == j { 1.0 } else { 0.0 };
            assert_eq!(
                dev.params[id].default, expected,
                "crosspoint ({i} → {j}) at id {id}"
            );
        }
    }
}

/// An unknown type id has no entry — `instantiate` returns `None` (no nodes added), the lookup
/// `build_patch` relies on to reject a bad `typeId` cleanly.
#[test]
fn unknown_type_does_not_instantiate() {
    let mut g = Graph::new();
    assert!(instantiate("does_not_exist", &DeviceConfig::EMPTY, &mut g).is_none());
    assert_eq!(g.node_count(), 0, "nothing added for an unknown type");
}

/// The catalog serializes (via JSON natively; the wasm bridge uses serde-wasm-bindgen) and exposes
/// the expected type ids in camelCase — the contract the TS `DeviceDescriptor` mirror consumes.
#[test]
fn catalog_serializes_with_expected_types() {
    let json = serde_json::to_string(&descriptors()).expect("descriptors serialize");
    for type_id in [
        "synth_voice",
        "gain_stage",
        "three_band_eq",
        "ad_converter",
        "da_converter",
        "speaker",
        "powered_monitor",
        "channel_strip",
        "vu_meter",
        "digital_meter",
        "scarlett_8i6",
        "computer",
        "condenser_mic",
        "dynamic_mic",
    ] {
        assert!(json.contains(type_id), "catalog missing {type_id}");
    }
    // camelCase field names are the wire contract (matches the TS mirror).
    assert!(json.contains("typeId"));
    assert!(json.contains("readouts"));
    // Ports carry their physical connector + channel count, serialized camelCase.
    assert!(json.contains("connector"));
    assert!(json.contains("quarterInch"));
    assert!(json.contains("channels"));
    // The 8i6's front inputs are combo jacks; its USB ports use the USB connector.
    assert!(json.contains("combo"));
    assert!(json.contains("usb"));
    // Structural config toggles serialize (the 8i6's INST + shared 48V keys) — the web renders
    // them.
    assert!(json.contains("configs"));
    assert!(json.contains("inst1"));
    assert!(json.contains("phantom"));
    // The mic's XLR connector serializes for the jack-fit check.
    assert!(json.contains(r#""connector":"xlr""#));
    assert!(json.contains("toggle"), "ConfigKind serializes camelCase");
}

/// Connector compatibility is same-connector, plus the **combo** jack mating with an XLR or a ¼"
/// plug (either direction). A signal-class difference isn't what's checked here — only physical fit.
#[test]
fn connectors_compatible_matrix() {
    // Same connector always fits.
    assert!(connectors_compatible(
        Connector::QuarterInch,
        Connector::QuarterInch
    ));
    assert!(connectors_compatible(Connector::Xlr, Connector::Xlr));
    assert!(connectors_compatible(Connector::Combo, Connector::Combo));
    // Combo accepts XLR and ¼", symmetrically.
    assert!(connectors_compatible(Connector::Combo, Connector::Xlr));
    assert!(connectors_compatible(Connector::Xlr, Connector::Combo));
    assert!(connectors_compatible(
        Connector::Combo,
        Connector::QuarterInch
    ));
    assert!(connectors_compatible(
        Connector::QuarterInch,
        Connector::Combo
    ));
    // Non-combo mismatches still reject; the digital split is equality (USB ≠ S/PDIF ≠ generic).
    assert!(!connectors_compatible(
        Connector::QuarterInch,
        Connector::Xlr
    ));
    assert!(!connectors_compatible(
        Connector::Speakon,
        Connector::QuarterInch
    ));
    assert!(!connectors_compatible(Connector::Usb, Connector::Spdif));
    assert!(!connectors_compatible(Connector::Usb, Connector::Digital));
    assert!(!connectors_compatible(Connector::Combo, Connector::Usb));
    // Acoustic (ideal air) mates only with itself — no physical plug, so it joins no other shape.
    assert!(connectors_compatible(
        Connector::Acoustic,
        Connector::Acoustic
    ));
    assert!(!connectors_compatible(Connector::Acoustic, Connector::Xlr));
    assert!(!connectors_compatible(
        Connector::Acoustic,
        Connector::Combo
    ));
    assert!(!connectors_compatible(
        Connector::Combo,
        Connector::Acoustic
    ));
}

/// Ports carry an authored physical connector, distinct from their signal-class `kind`: the synth's
/// instrument out and the gain stage's line in are both ¼" (so they interconnect despite differing
/// kinds), the AD's digital out is a digital connector, the synth's MIDI in is a 5-pin DIN, and the
/// speaker terminus takes a ¼" line feed today (keeping the default `da→spk` connection legal).
#[test]
fn ports_carry_authored_connectors() {
    let all = descriptors();
    let connector = |type_id: &str, dir: PortDirection, id: u32| {
        all.iter()
            .find(|d| d.type_id == type_id)
            .unwrap_or_else(|| panic!("no device {type_id}"))
            .ports
            .iter()
            .find(|p| p.direction == dir && p.id == id)
            .unwrap_or_else(|| panic!("no {type_id} port {id}"))
            .connector
    };
    assert_eq!(
        connector("synth_voice", PortDirection::Output, 0),
        Connector::QuarterInch
    );
    assert_eq!(
        connector("synth_voice", PortDirection::Input, 0),
        Connector::Din5
    );
    assert_eq!(
        connector("gain_stage", PortDirection::Input, 0),
        Connector::QuarterInch
    );
    assert_eq!(
        connector("ad_converter", PortDirection::Output, 0),
        Connector::Digital
    );
    assert_eq!(
        connector("speaker", PortDirection::Input, 0),
        Connector::QuarterInch
    );
}

/// The meter devices expose their node readouts as descriptors: the VU meter's VU + peak, the
/// digital meter's peak + RMS, ids in exposed (position) order — the surface the UI meters read.
#[test]
fn meter_devices_expose_their_readouts() {
    let all = descriptors();
    let vu = all
        .iter()
        .find(|d| d.type_id == "vu_meter")
        .expect("vu_meter");
    assert_eq!(vu.readouts.len(), 2);
    assert_eq!(vu.readouts[0].id, 0);
    assert_eq!(vu.readouts[0].label, "VU");
    assert_eq!(vu.readouts[1].id, 1);
    assert_eq!(vu.readouts[1].unit, "dBu");
    // The VU meter measures, so it exposes no control params.
    assert!(vu.params.is_empty());

    let dig = all
        .iter()
        .find(|d| d.type_id == "digital_meter")
        .expect("digital_meter");
    assert_eq!(dig.readouts.len(), 2);
    assert_eq!(dig.readouts[1].label, "RMS");
    assert_eq!(dig.readouts[1].unit, "dBFS");

    // A non-meter device exposes no readouts.
    let spk = all
        .iter()
        .find(|d| d.type_id == "speaker")
        .expect("speaker");
    assert!(spk.readouts.is_empty());
}

/// The chassis seam maps readouts to concrete `(NodeId, ReadoutId)`, in exposed order — the map
/// `build_patch` resolves to `ReadoutHandle`s. A single-node meter's two readouts map to its one
/// node with node-local ids 0 and 1.
#[test]
fn meter_readouts_map_to_nodes() {
    let mut g = Graph::new();
    let vu =
        instantiate("vu_meter", &DeviceConfig::EMPTY, &mut g).expect("vu_meter is in the catalog");
    assert_eq!(vu.nodes.len(), 1);
    assert_eq!(
        vu.readouts,
        vec![(vu.nodes[0], ReadoutId(0)), (vu.nodes[0], ReadoutId(1))]
    );
}

#[test]
fn asset_catalog_face_is_described_without_building_and_instantiates_fallibly() {
    let descriptor = describe(&COLD_DESCRIPTOR_FIXTURE, &DeviceConfig::EMPTY);
    assert_eq!(descriptor.ports.len(), 2);

    let missing = SoundFontBankLibrary::default();
    let error = match build_expansion(
        &ASSET_FIXTURE,
        &DeviceConfig::EMPTY,
        &BuildContext::new(&missing),
    ) {
        Err(error) => error,
        Ok(_) => panic!("the fixture requires its stable-id bank"),
    };
    assert_eq!(
        error,
        AssetBuildError::MissingSoundFont {
            asset_id: FIXTURE_SOUNDFONT_ID.into(),
        }
    );
    let mut graph = Graph::new();
    let unknown = instantiate_with_assets(
        "not-a-catalog-device",
        &DeviceConfig::EMPTY,
        &mut graph,
        &BuildContext::new(&missing),
    )
    .expect("unknown types are not asset failures");
    assert!(unknown.is_none());
}

/// Every device carries a sane physical form factor (content for the spatial world): a rackmount
/// unit spans at least 1U; a desktop unit has a positive footprint box. Guards against a 0-U or
/// zero-size device that the placement model couldn't lay out.
#[test]
fn catalog_carries_sane_form_factors() {
    for entry in CATALOG {
        match entry.form_factor {
            FormFactor::Rackmount { rack_units } => {
                assert!(rack_units >= 1, "{} rack_units", entry.type_id);
            }
            FormFactor::Desktop {
                width_mm,
                height_mm,
                depth_mm,
            } => {
                assert!(
                    width_mm > 0.0 && height_mm > 0.0 && depth_mm > 0.0,
                    "{} desktop footprint",
                    entry.type_id
                );
            }
        }
    }
}

/// The form factor serializes in the camelCase, internally-tagged shape the TS `FormFactor` mirror
/// consumes: a `kind` discriminant plus camelCase variant fields. Pins the wire contract.
#[test]
fn form_factor_serializes_as_tagged_camel_case() {
    let json = serde_json::to_string(&descriptors()).expect("descriptors serialize");
    assert!(json.contains("formFactor"));
    assert!(json.contains(r#""kind":"rackmount""#));
    assert!(json.contains("rackUnits"));
    assert!(json.contains(r#""kind":"desktop""#));
    assert!(json.contains("widthMm"));
}
