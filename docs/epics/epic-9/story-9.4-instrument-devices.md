# Story 9.4 — Instrument devices + web — program design

> Companion to the Story 9.4 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Task 9.4.1 — Production asset-aware catalog expansion  · planned

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

## Task 9.4.2 — Headless electric/acoustic guitars and preset config  · planned

_(program design drafted at pickup)_

## Task 9.4.3 — Guitar faceplates, preset selector, and focus keybed  · planned

_(program design drafted at pickup)_

## Task 9.4.4 — Audible electric and acoustic product paths  · planned

_(program design drafted at pickup)_
