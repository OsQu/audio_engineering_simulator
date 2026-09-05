# Story 9.3 — Shared SoundFont assets across the browser boundary — program design

> Companion to the Story 9.3 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Task 9.3.1 — Split the parsed bank from per-device voice state  · done

The first task stays inside `engine`: make the ownership boundary explicit without changing the
event/sample/DA topology or the hot-path algorithm proven in 9.2.

File-tree diff:

```text
 crates/engine/src/lib.rs                    re-export SoundFontBank
 crates/engine/src/node.rs                   re-export SoundFontBank
 crates/engine/src/node/sound_font.rs        split parse ownership from synth construction
 crates/engine/tests/soundfont.rs            shared-bank/state-isolation coverage; migrate chassis
 crates/engine/tests/no_alloc.rs             construct measured chassis from a pre-parsed bank
```

Key types and signatures:

```rust
/// Opaque immutable SoundFont data, parsed once on the cold path and cheaply cloneable.
#[derive(Clone)]
pub struct SoundFontBank {
    inner: Arc<rustysynth::SoundFont>,
}

impl SoundFontBank {
    pub fn parse(sf2_bytes: &[u8]) -> Result<Self, SoundFontError>;
}

impl SoundFontSource {
    /// Allocate independent voice/envelope/playback state over a shared immutable bank.
    pub fn from_bank(bank: &SoundFontBank, preset: u8) -> Result<Self, SoundFontError>;

    /// Keep the direct-byte convenience for headless callers, implemented as parse + from_bank.
    pub fn new(sf2_bytes: &[u8], preset: u8) -> Result<Self, SoundFontError>;
}
```

Construction flow:

```text
SoundFontBank::parse(bytes)                    cold; parses ~6 MB once
  rustysynth::SoundFont::new(Cursor<&[u8]>)
  Arc::new(sound_font)

SoundFontSource::from_bank(&bank, preset)      cold; once per instrument/rebuild
  Synthesizer::new(&bank.inner, settings)      fresh mutable voice pool
  Program Change(channel 0, normalized preset)

SoundFontSource::process(...)                  hot; algorithm unchanged from 9.2
  event sub-slicing → synth.render → mono fold → clamp
```

Tests must distinguish the two kinds of sharing, not merely show that both sources make sound:

- an internal unit assertion uses `Arc::ptr_eq` to prove cloned banks share the parsed allocation;
- two sources built from the same bank receive different notes and prove their voices/envelopes do not
  cross-talk;
- malformed bytes still fail in `SoundFontBank::parse`, while synth construction retains its distinct
  `Build` failure surface;
- the existing pitch/polyphony/velocity/timing and 128-block allocation gates run through
  `from_bank`, ensuring the ownership refactor did not weaken 9.2.

## Task 9.3.2 — Asset-aware, fallible device construction and preset config  · done

The devices crate owns the cold-path construction vocabulary, without teaching `engine` about asset
ids or serialized scenes.

```text
SoundFontBankLibrary (stable String id → parsed SoundFontBank)
        ↓ explicit borrow
BuildContext
        ↓
build_patch_with_assets(...) → instantiate_with_assets(...)
        ↓
AssetNodeBuilder(config, context) → Result<Box<dyn Node>, AssetBuildError>
```

- `build_patch` remains the asset-free convenience wrapper; it constructs an empty library and routes
  through `build_patch_with_assets`, so ordinary catalog entries keep their existing simple builder
  signature and behavior.
- `AssetBuildError` names a missing stable SoundFont id or a `SoundFontSource` construction failure;
  `BuildError::Asset` carries it to a scene-building caller as a legible cold-path failure.
- `gm_preset(config, "preset", default)` rounds finite scalar config, clamps it to GM `0..=127`, and
  falls back to the normalized default for non-finite values. It is structural configuration only;
  neither a smoothed param nor an event-lane program change.
- A private catalog test fixture builds a `SoundFontSource` from an id-resolved bank. It proves the
  missing-bank error surface and a supplied bank's construction path without publishing the 9.4
  instrument device.

## Task 9.3.3 — Browser/worklet/WASM asset delivery and provenance  · done

`build-wasm.sh` stages the one tracked font into generated `web/public/soundfonts/`; the main thread
fetches it once alongside the WASM bytes and includes it as `{ id: "tim_gm_6mb", bytes }` in the initial
worklet processor options. The worklet passes that bundle to `SceneEngine` before it builds the initial
patch. `SceneEngine` parses each bundle entry cold into its persistent `SoundFontBankLibrary`, then uses
that library for every scene build. The GPL-2-only attribution, upstream Debian/MuseScore provenance,
and SHA-256 are recorded beside the source asset and staged with it.

## Task 9.3.4 — Cache-lifecycle integration and failure hardening  · done

Native binding tests construct `SceneEngine` from the real initial bundle, confirm the stable-id bank
survives a schedule hot-swap, and confirm malformed bytes return `BuildError::Asset` before any scene
exists. Rebuilds call `build_patch_with_assets` over the engine-owned library, so an existing parsed bank
is retained rather than reparsed; future asset-backed devices borrow it when their fresh schedules build.
