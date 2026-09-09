# Epic 9 — Instruments: SoundFont-Based MIDI Voices — Design Notes & Delivery Record

Companion archive to `IMPLEMENTATION_PLAN.md`. Epic 9 turned the simulation's MIDI event lane into
recognizable, browser-playable GM instruments without weakening the engine's real-time contract. It began
with a feasibility gate for `rustysynth`, made the renderer an engine node behind the existing DA boundary,
delivered one shared SoundFont asset through the browser/worklet/WASM seam, and closed with electric and
acoustic guitar devices that expose physically honest output carriers.

Stories **9.1–9.4 are complete**. The implementation plan keeps the short version of what binds future
work; this file records the full Epic-level design and delivery. The per-story companion files remain the
task-level records and should be consulted for exact files, APIs, test cases, and deviations:

- [Story 9.1 — feasibility spike](./epics/epic-9/story-9.1-soundfont-feasibility.md)
- [Story 9.2 — SoundFontSource + DA chassis](./epics/epic-9/story-9.2-soundfont-source.md)
- [Story 9.3 — shared SoundFont assets](./epics/epic-9/story-9.3-soundfont-assets.md)
- [Story 9.4 — instrument devices + web](./epics/epic-9/story-9.4-instrument-devices.md)

---

## Progress, outcome, and scope

Epic 9 delivers a single, vendored GM SoundFont—TimGM6mb—through a browser-playable signal path:

```text
QWERTY / Web-MIDI / focus keybed
              │ note-on / note-off + velocity
              ▼
       SoundFontSource (48 kHz Sample lane)
              │
              ▼
          DaConverter (8× to analog rate)
          ┌───────────────┴────────────────┐
          ▼                                ▼
electric_guitar                       acoustic_guitar
instrument-level volts                PoweredMonitor → pressure (Pa)
          │                                │
   analog chain / GainStage                mic → preamp/interface → Speaker tap
```

The goal was recognizable, polyphonic instrument sound while retaining the simulation's established
boundaries: events are not audio, digital audio crosses the existing DA converter, volts remain the
electric output carrier, and acoustic pressure must cross a microphone to become voltage. It intentionally
does **not** aim for detailed guitar, amp, room, or performance realism.

## Decisions that bind later work

- **`rustysynth` is admitted to the hot path.** The 9.1 spike proved v1.3.6 is allocation-free during
  steady rendering, robust on the note path, deterministic, and wasm32-compatible. The production node keeps
  the third-party implementation isolated in `engine::node::sound_font`; no `rustysynth` type appears in the
  public engine API.
- **TimGM6mb is the v1 asset.** It is the one tracked, GPL-2-only SoundFont asset and is fetched as browser
  data rather than embedded into the WASM binary. The small VintageDreamsWaves demo font was rejected by
  `rustysynth`'s structural sanity check, so any future font is a compatibility decision that must be parsed
  before adoption.
- **A bank is shared; a player is never shared.** `SoundFontBank` owns immutable parsed source data behind an
  `Arc`; every `SoundFontSource::from_bank` builds a fresh synthesizer with independent voices, envelopes, and
  playback state. The cache is keyed by stable asset id (`tim_gm_6mb`), not by scene device id.
- **Asset delivery is cold-path construction context.** Browser fetch, transfer, parsing, cache mutation,
  and source construction occur before initial compile or during a structural rebuild. `SceneEngine` owns the
  cache above schedules, so a failed rebuild cannot replace the running scene and a successful rebuild does
  not reparse the font.
- **Preset selection is structural.** A selected GM program is normalized at construction and causes the
  ordinary config-to-recompile flow. It is neither a smoothed parameter nor an event-lane MIDI program change.
- **The source owns a digital boundary; instruments own their physical faces.** `SoundFontSource` produces
  mono samples at 48 kHz. A DA converter is mandatory before a device exposes volts; the acoustic guitar then
  uses the existing `PoweredMonitor` voltage-to-pressure transducer. No direct sample-to-volts or
  sample-to-pressure shortcut was added.
- **V1 MIDI remains deliberately narrow.** Event messages provide note-on, note-off, and velocity. Pitch bend,
  sustain pedal, CC, and broader program-change behavior remain optional future depth.

## Story 9.1 — Feasibility spike: GO

Before production wiring, a throwaway harness tested `rustysynth` v1.3.6 plus TimGM6mb against the engine's
non-negotiable runtime contract. It rendered a recognizable middle-C GM note from bytes; its first and steady
renders made zero allocations across 128 blocks with a held chord and an 80-note voice-stealing burst; and
adversarial note events plus exhausted polyphony did not panic. A byte-backed `Cursor` path compiled for
`wasm32-unknown-unknown`, proving a browser deployment does not need a filesystem API.

The result was **GO**: no fork and no OxiSynth fallback. The spike itself was removed as planned, leaving the
durable findings in the [Story 9.1 record](./epics/epic-9/story-9.1-soundfont-feasibility.md).

## Story 9.2 — Engine source and honest DA chassis

`SoundFontSource` became the production node. It consumes the event lane, renders the source at 48 kHz,
folds stereo to mono as `0.5 × (L + R)`, clamps the internal full-scale sample bus to `±1`, and feeds an
ordinary `DaConverter` into analog volts. Fixed stack chunks match `rustysynth`'s render block without heap
scratch, and event sub-slicing preserves event ordering; the remaining timing granularity is the synth's
64-frame internal block (about 1.3 ms), documented as the honest control rate of this source.

The node/chassis tests establish silence until a note, pitch, note timing, release, polyphony, velocity
scaling, deterministic rendering, no allocations, panic-free adversarial input, and wasm32 compilation.
The detailed delivery—including source API constraints and intentional simplifications—is in the
[Story 9.2 companion](./epics/epic-9/story-9.2-soundfont-source.md).

## Story 9.3 — Shared asset delivery and lifecycle

The parsed data/player split became explicit:

```text
TimGM6mb.sf2 bytes
  → SoundFontBank::parse (once, cold)
  → SoundFontBankLibrary["tim_gm_6mb"]
  → BuildContext
  → SoundFontSource::from_bank (fresh source for each device/rebuild)
```

`AssetNodeBuilder` and `BuildContext` made asset-aware catalog expansion fallible without making ordinary
catalog devices complex. A missing stable id or a failed source construction becomes `BuildError::Asset`,
while an unknown device type remains distinct. Descriptor generation uses explicit asset-free node-face
metadata, so opening the device catalog neither parses the six-megabyte font nor constructs a silent source.

The web build stages the tracked asset into generated public output. The main thread fetches it once and sends
`{ id, bytes }` with initial worklet options; `SceneEngine` parses the bundle before compiling its first patch
and retains the bank over hot swaps. The implementation also records the font's licensing, provenance, and
checksum beside the source asset. See the [Story 9.3 companion](./epics/epic-9/story-9.3-soundfont-assets.md)
for lifecycle tests and the exact failure contract.

## Story 9.4 — Guitar devices, surfaces, and audible proof

### Catalog and carrier faces

The catalog now publishes two asset-backed devices over the shared bank:

- `electric_guitar`: `SoundFontSource → DaConverter`, MIDI input plus a quarter-inch, high-impedance
  instrument voltage output. Its 1 V DA reference and 100 kΩ source impedance deliberately make loading and
  cable lessons visible in the existing analog chain.
- `acoustic_guitar`: `SoundFontSource → DaConverter → PoweredMonitor`, MIDI input plus an acoustic output.
  The 1 V DA reference and 1 Pa/V transducer make full scale approximately 1 Pa peak (about 94 dB SPL). It
  cannot be heard at the analog speaker tap unless a microphone transduces its pressure output.

Both expose a structural `preset` select covering GM guitar programs 24–31. Electric defaults to Clean Guitar
(27); acoustic defaults to Steel Guitar (25). Each construction borrows the same immutable bank but owns fresh
voice state.

### Web and interaction surface

The Rust `ConfigKind::Select` and authored options are mirrored in TypeScript. `ConfigSelect.svelte` binds a
numeric structural setting; `Guitar.svelte` supplies shared electric/acoustic faceplates; and
`GuitarFocus.svelte` combines the selector with the existing keybed. The device registry, skin mapping, and
focus policy treat both guitars as dedicated playable instruments, preserving the established topmost-focus
QWERTY/Web-MIDI routing rule.

### Runtime correction and audible exit paths

Browser SoundFont bytes arrive as `ArrayBuffer`/`Uint8Array`, not a JavaScript sequence. The WASM binding's
SoundFont asset deserializer explicitly uses Serde's byte-buffer path, fixing the worklet error:

```text
invalid SoundFont bundle: invalid type: byte array, expected a sequence
```

End-to-end `SceneEngine` tests use the real TimGM bundle. The electric test plays GM Distortion Guitar through
a 1,000× existing `GainStage` and proves its hard-clipped 10 V rail reaches the speaker tap after the modeled
150 Ω/10 kΩ divider: `10 × 10000 / 10150 = 9.852216 V`; host capture reaches full scale. The acoustic test
routes pressure through a dynamic mic and a powered/routed Scarlett 8i6 into a `Speaker`, proving non-silent
analog and host output.

The [Story 9.4 companion](./epics/epic-9/story-9.4-instrument-devices.md) contains the detailed task record,
component names, and validation evidence.

## Validation

The final exit gates passed:

- `cargo test --workspace` (including SoundFont, allocation, asset lifecycle, product-path, engine, device,
  capture, and bindings tests)
- `cargo check -p wasm-bindings --target wasm32-unknown-unknown`
- `pnpm --dir web test` — 285 passing tests
- `pnpm --dir web typecheck` — zero errors; two pre-existing `FloatingWindow.svelte` accessibility warnings
- Manual browser pass: QWERTY, Web-MIDI, preset changes, electric distortion, and acoustic micing

## Deferred and intentionally absent

- No detailed guitar modeling, pickups, pedals, amp/cabinet simulation, or dedicated distortion device. The
  existing `GainStage` is sufficient to demonstrate clipping; named guitar effects remain future content.
- No pitch bend, sustain, controller messages, velocity curves beyond the source's current behavior, or broad
  instrument library. These are the former optional Story 9.5 direction.
- No user-imported SoundFonts, dynamic asset manifests, cache eviction, or multiple shipped banks.
- No room, listener, or air-to-ear model. Acoustic guitar output remains device-to-device pressure and must be
  mic'd, consistent with Epic 8's acoustic scope.

These are boundaries, not incomplete Epic 9 work. They preserve a small, deterministic v1 while leaving clear
additive seams for later instrument depth.
