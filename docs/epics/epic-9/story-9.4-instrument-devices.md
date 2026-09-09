# Story 9.4 — Instrument devices + web — program design

> Companion to the Story 9.4 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Delivered

Two playable, asset-backed guitar devices now share the parsed `tim_gm_6mb` bank while retaining
independent synth state. `electric_guitar` exposes a loaded instrument-level voltage output;
`acoustic_guitar` instead radiates acoustic pressure and must be mic'd before it can reach an analog
monitor. Both expose the curated GM guitar programs 24–31 as structural preset selection, are playable
through the shared focus keybed/QWERTY/Web-MIDI route, and use the same browser-fetched SoundFont bundle
as the worklet.

## Task 9.4.1 — Production asset-aware catalog expansion  · done

The first task closes the deliberate Story-9.3 fixture gap without publishing either guitar yet. Asset-backed
nodes become a real catalog capability, while catalog descriptors remain available before any asset bytes have
loaded. The ordinary builder path stays infallible and unchanged in spirit.

File-tree diff:

```text
 crates/engine/src/node/sound_font.rs     expose one source of truth for the node's fixed port faces
 crates/devices/src/catalog.rs           node-spec enum; descriptor/runtime expansion split; real fixture tests
 crates/devices/src/build.rs             propagate asset construction failure separately from unknown type
```

Key types and signatures (names are provisional, responsibilities are settled):

```rust
type NodeBuilder = fn(&DeviceConfig) -> Box<dyn Node>;
type AssetNodeBuilder =
    fn(&DeviceConfig, &BuildContext) -> Result<Box<dyn Node>, AssetBuildError>;
type NodeFaceBuilder = fn(&DeviceConfig) -> NodeFace;

enum NodeSpec {
    Plain(NodeBuilder),
    Asset {
        build: AssetNodeBuilder,
        face: NodeFaceBuilder,
    },
}

/// Descriptor-only fixed declarations; never scheduled or processed.
struct NodeFace {
    inputs: Vec<InputPort>,
    outputs: Vec<OutputPort>,
    params: Vec<ParamDecl>,
    readouts: Vec<ReadoutDecl>,
}

fn describe_expansion(entry: &CatalogEntry, config: &DeviceConfig) -> FaceExpansion;

fn build_expansion(
    entry: &CatalogEntry,
    config: &DeviceConfig,
    context: &BuildContext,
) -> Result<BuiltExpansion, AssetBuildError>;

pub fn instantiate_with_assets(
    type_id: &str,
    config: &DeviceConfig,
    graph: &mut Graph,
    context: &BuildContext,
) -> Result<Option<BuiltDevice>, AssetBuildError>;
```

`SoundFontSource` exposes constructor-independent fixed-face helpers, and its constructor uses those same
helpers. The catalog's asset face builder calls them; it does not duplicate the 48 kHz/24-bit/event-capacity
constants. `NodeFace` is metadata, not a dummy `Node`, so there is no unloaded synth state that could leak into
a runnable graph.

Call-stack delta:

```text
descriptors()
  describe(entry, EMPTY)
-   expand(entry, EMPTY)                     builds every node
+   describe_expansion(entry, EMPTY)         plain nodes may self-describe; asset nodes use NodeFace

build_patch_with_assets(...)
  for device in patch.devices
-   instantiate_with_assets(...) -> Option<BuiltDevice>
+   instantiate_with_assets(...) -> Result<Option<BuiltDevice>, AssetBuildError>
+     build_expansion(entry, config, context)
+       NodeSpec::Plain(builder)              builder(config)
+       NodeSpec::Asset { build, .. }         build(config, context)?
      None                                   BuildError::UnknownType
+     Err                                    BuildError::Asset
```

Tests pin the distinction: unknown type adds no graph nodes and remains `UnknownType`; missing asset returns
the stable-id `AssetBuildError`; a supplied TimGM bank builds through an actual asset `NodeSpec`; descriptor
generation succeeds with an empty asset library and a test counter proves the asset builder was never called.

## Task 9.4.2 — Headless electric/acoustic guitars and preset config  · done

`electric_guitar` expands to `SoundFontSource → DaConverter`, exposing MIDI input and a quarter-inch
instrument output. Its 1 V DA reference and 100 kΩ output impedance make the existing input-loading and
cable lessons observable. `acoustic_guitar` expands to `SoundFontSource → DaConverter → PoweredMonitor`;
the same 1 V reference and a 1 Pa/V monitor expose only acoustic pressure (about 94 dB SPL at full scale).

The shared `preset` select has the eight GM guitar options (24–31), with Clean Guitar (27) defaulting on
electric and Steel Guitar (25) on acoustic. Construction borrows the stable-id bank from `BuildContext`;
each expansion creates fresh mutable voice state. Native catalog coverage pins the faces, calibration,
options/defaults, cache reuse, state isolation, and invalid/missing asset behavior.

## Task 9.4.3 — Guitar faceplates, preset selector, and focus keybed  · done

The TypeScript catalog mirror gained enumerated `select` configuration metadata, and `ConfigSelect.svelte`
binds an authored numeric option to structural `setConfig`. Electric and acoustic faceplates share the
guitar chassis, while `GuitarFocus.svelte` composes the preset selector with the existing `Keybed`.
Both type ids are registered as dedicated focusable instruments, so QWERTY and Web-MIDI dispatch follows
the existing topmost-focus rule. Faceplate and focus tests cover the registry, ports, controls, config
widget binding, and focus eligibility.

## Task 9.4.4 — Audible electric and acoustic product paths  · done

The bindings crate now has end-to-end `SceneEngine` fixtures over the real bundle. The electric fixture
routes GM Distortion Guitar through a 1,000× `GainStage` into `Speaker`; it proves hard clipping at the
speaker-loaded predicted rail of `10 V × 10 kΩ / (10 kΩ + 150 Ω) = 9.852216 V`, then proves the host
capture reaches full scale. The acoustic fixture routes the guitar's air output through a dynamic mic,
the powered/routed Scarlett 8i6, and the speaker tap, proving non-silent voltage and host output.

The worklet bundle deserializer explicitly requests Serde's byte-buffer path, allowing the browser's
`ArrayBuffer`/`Uint8Array` SoundFont bytes to cross the WASM boundary without being mistaken for a JS
sequence. This fixed the runtime `invalid type: byte array, expected a sequence` failure.

Validation passed: `cargo test --workspace`, `cargo check -p wasm-bindings --target wasm32-unknown-unknown`,
`pnpm --dir web test` (285 tests), and `pnpm --dir web typecheck` (0 errors; two pre-existing accessibility
warnings). Manual browser validation covered QWERTY, Web-MIDI, preset swaps, electric distortion, and
acoustic micing.
