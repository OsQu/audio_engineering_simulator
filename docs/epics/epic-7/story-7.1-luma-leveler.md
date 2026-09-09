# Story 7.1 — More devices: Luma Leveler — program design

> Companion to the Story 7.1 block in
> [`IMPLEMENTATION_PLAN.md`](../../IMPLEMENTATION_PLAN.md). Holds the per-Task program design — the
> shape of the code — drafted just-in-time as each Task is picked up.

## Task 7.1.1 — Luma analog leveler model and catalog face · done

The chassis expands to exactly one analog node. The node has a fixed balanced line face rather than a
receiver/compressor/driver internal graph: its difference-first detector and equal/opposite output pair
are one physical operation, while its six exposed values map directly to the hardware controls/readings.

File-tree delta:

```text
crates/engine/src/node.rs
+ crates/engine/src/node/luma_leveler.rs
crates/engine/src/lib.rs
crates/devices/src/catalog.rs
```

Key shape:

```rust
pub struct LumaLeveler {
    // two-conductor analog input and output; rate-baked detector state;
    // six ParamDecl / ReadoutDecl entries; no allocation in process.
}

impl LumaLeveler {
    pub const PEAK_REDUCTION: ParamId = ParamId(0); // 0.0..1.0, threshold/drive control
    pub const GAIN: ParamId = ParamId(1);           // linear voltage gain, 0..+40 dB face mapping
    pub const LIMIT: ParamId = ParamId(2);          // 0 Compress (~3:1), 1 Limit (high ratio)
    pub const POWERED: ParamId = ParamId(3);        // 5 ms output de-click gate
    pub const GAIN_REDUCTION_DB: ReadoutId = ReadoutId(0);
    pub const OUTPUT_DBU: ReadoutId = ReadoutId(1);

    pub fn new() -> Self; // fixed 600 Ω balanced input/output and honest default state
}
```

Hot-path flow:

```text
LumaLeveler::process(params, [V+, V−], [out+, out−])
  differential = V+ − V−                    // precedes detector and gain computer
  envelope.step(abs(differential))           // fixed ~10 ms attack, simplified release
  soft_knee_gain(envelope, peak_reduction, limit)
  post_gain = compression_gain × makeup_gain
  out+ = +0.5 × differential × post_gain × powered
  out− = −0.5 × differential × post_gain × powered
  readouts = reduction dB, output dBu        // sampled after process, before UI selector mapping
```

`prepare(AnalogRate)` bakes one-pole detector coefficients. `process` only reads already-sized lanes and
smoothed params, updates scalar state, and writes both legs; it never logs, allocates, or indexes outside
the contract. The physical `±0.5` legs preserve the requested differential level: `out+ − out−` equals
the post-leveler differential signal.

Catalog shape: one `Rackmount { rack_units: 3 }` entry with balanced `Connector::Xlr` line input and
output, no structural configs, four exposed controls, and two readings. The faceplate-only three-position
meter selector is deliberately absent from the catalog/engine because it does not alter the device state
or audio; it selects which live reading a single physical needle depicts.

Test oracles:

- a settled differential sine at a stated level verifies common-mode cancellation and equal/opposite
  output legs, with the `V+ − V−` and `±0.5` hand calculation in the test comment;
- static signals above the threshold verify the documented soft-knee / Compress-versus-Limit ordering and
  a bounded 0–40 dB gain-reduction range;
- a detector step at the analog rate verifies the ~63% one-time-constant response at 10 ms, with the
  `1 − e^-1` calculation stated;
- power changes glide to silence via the declared smoother, and output-dBu / gain-reduction readings are
  derived from the processed voltage rather than copied from a control value;
- catalog tests prove the single node's balanced XLR face, IDs, controls, and readings agree.

## Task 7.1.2 — Luma physical faceplate · done

The Luma's physical controls remain the four catalog-bound controls from Task 7.1.1. The three-position
meter selector is local presentation state: it switches the one needle between the always-live
Gain-Reduction and Output-dBu readings, without writing a nonexistent audio param or adding persisted scene
semantics.

File-tree delta:

```text
web/src/device-ui.ts
web/src/skin.ts
+ web/src/widgets/LumaLeveler.svelte
web/src/widgets/VuFace.svelte                 // backward-compatible presentation extension
web/test/faceplate.test.ts
```

Surface shape:

```text
LumaLeveler.svelte
  Chassis(handle, flipped, finish="grey", name)
    front — 482.6 × 133.35 mm warm-silver face
      Control(2)                         // Compress / Limit toggle, with original paired legends
      Control(1, dark, ~32 mm)           // Output Gain
      VuFace(large, scaleLabel)          // selected gain reduction / +4 dBu / +10 dBu needle
      Control(0, dark, ~32 mm)           // Peak Reduction
      local meter selector (GR | +4 | +10)
      Control(3)                         // Power toggle
    back — dark chassis
      Socket(input, 0, XLR) · tube/transformer silhouettes · Socket(output, 0, XLR)
```

`VuFace` stays a pure display component. Its props gain only a scale label and optional physical width;
existing callers retain the same VU scale and dimensions. The Luma maps `Gain Reduction` to a reversed
0–40 dB sweep (zero reduction at the right), and maps Output dBu against +4 or +10 reference marks. The
selector is an accessible three-button radio group owned by `LumaLeveler.svelte`; it deliberately resets
with the rendered surface because it is not a device state.

The faceplate uses `Chassis`, `Place`, `Control`, `Legend`, and `Socket` at 1 px/mm. Its
unique brushed warm-silver sheet, cream meter card, dark hardware, restrained red meter detail, and
original Luma typography are local decorative CSS; no general material token is justified by one device.
Rear tube/transformer forms are noninteractive decoration. `skin.ts` adds the grey/dark Luma skin and the
registry maps `luma_leveler` to this one physical faceplate. The faceplate guardrail adds literal coverage
for params `0..3`, input `0`, and output `0`.

Browser review state after implementation: `/devices/luma_leveler`, 100% browser zoom, a recorded workbench
camera zoom and matching front/rear views. Patch a balanced source or DA output into Line In, then Line Out
to the monitor path; record whether meter motion and AudioWorklet audibility are observed. Stop for the
separate visual-approval gate before Task 7.1.3.

Visual iteration 1 (2026-09-09): inspected in the in-app browser at
`/devices/luma_leveler`. The initial 1.4 px/mm full-frame pass exposed the gain-reduction readout colliding
with the Peak Reduction group and redundant Mode/Limit text. The correction makes the VU card self-contained,
puts its numerical reading below it, and retains only the catalog-bound Limit label. The selector changed
state in-browser and console logs contained no warnings or errors. A post-reload pass used the bench's default
3.0 px/mm state; it confirms the corrected front grouping, while a fixed full-frame follow-up is still needed
for approval. Audio and meter motion remain unverified: the bench had no input source patch. The browser
reported denied Web MIDI permission, which is unrelated to this analog device.

Visual iteration 2 (2026-09-09): increased primary front/rear legends, knob scales, and the meter's
numeric caption after the user reported that the physical-scale labels were too small. The browser reload
completed with no console warnings or errors; web check, typecheck, and 287 Vitest assertions pass. Resume
at the fixed full-frame review state for a final front/rear readability check.

Accepted for the visual gate by the user on 2026-09-09. Delivered: the original Luma 3U front/rear,
catalog-bound controls and XLR anchors, local accessible GR / +4 / +10 meter selector, and readable
physical-scale silkscreen. Browser evidence: selector interaction works and no console warnings/errors;
meter motion and audio remain unverified because this review bench has no input source patch. No commit was
created; the user retains the repository verification/commit gate.

## Task 7.1.3 — Post-approval sonic refinement · done

**Outcome:** replace the fixed 500 ms detector recovery with a deliberately small optical-memory
approximation. A sustained reduction recovers more slowly than an equally deep, short reduction. This
is the one deferred behavior that materially changes the learner's operating feel without inventing a
maintenance control, changing the analog signal domain, or starting a component-level tube simulation.

**Owned files:**

```text
crates/engine/src/node/luma_leveler.rs
docs/epics/epic-7/story-7.1-luma-leveler.md
```

No catalog, parameter, readout, scene, UI, or shared engine-contract changes are needed. The existing
Peak Reduction, Gain, Compress/Limit, and meter behavior stay exactly as exposed by the approved face.
Tube/transformer coloration and frequency-dependent sidechain emphasis remain deferred.

**Processing shape:** `LumaLeveler` gains a scalar `optical_memory` and rate-baked coefficients for a
125 ms charge and a 6 s fade. After the existing differential detector computes its instantaneous
gain reduction, memory moves toward the normalized reduction (`reduction_db / 40`). It charges with the
125 ms coefficient when reduction increases and fades with the 6 s coefficient when it falls. On each
detector-decay sample, the release time is interpolated from 500 ms at empty memory to 5 s at full
memory; attack remains the existing fixed 10 ms.

```text
differential = V+ − V−
detector = attack_or_release(detector, abs(differential), release_ms(optical_memory))
reduction = soft_knee_gain(detector, Peak Reduction, Compress/Limit)
optical_memory = one_pole(optical_memory, reduction / 40, charge_or_fade_coeff)
output = balanced_split(differential × reduction_gain × Gain × Power)
```

Memory is internal, bounded to `0..=1`, reset only with node construction, and has no new control or
readout: it is the device's optical history, not a hidden user setting. `prepare(AnalogRate)` continues
to bake every coefficient; the hot path remains allocation-free and only updates scalars.

**Numeric oracle:** use a fixed 384 kHz test signal driven far enough above the full-drive threshold to
reach the 40 dB cap. A 25 ms hold charges memory to approximately
`1 − exp(−25 / 125) = 0.181`, so its initial release time is about
`0.5 + 4.5 × 0.181 = 1.31 s`. A 1 s hold charges it to approximately
`1 − exp(−1000 / 125) = 0.9997`, so it begins its release at about `5.0 s`. After the same 250 ms of
low-level recovery signal, the long-held detector must retain more gain reduction than the short-held detector. The test
will assert that ordering from the public gain-reduction readout, and separately assert both output legs
remain equal and opposite during the recovery. Existing 10 ms attack and fixed-face tests continue to
pass unchanged.

**Acceptance checks:**

- `cargo test -p engine luma_leveler`
- `cargo fmt --check`
- targeted catalog test if the existing Luma catalog coverage shares this node test path
- final Story gate: `cargo test --workspace`, `cd web && pnpm run check && pnpm run typecheck && pnpm run test`, and `cd web && pnpm run wasm`

Delivered on 2026-09-09: a 256-step rate-baked optical release table, 125 ms optical-memory charge,
6 s fade, and a 500 ms–5 s history-dependent detector release. There are no new controls, catalog
semantics, readouts, UI behavior, or digital-domain conversion. The two new node tests prove stated
memory charge values and gain-reduction ordering after a 250 ms recovery interval while checking both
balanced legs remain equal and opposite.

Validation passed: `cargo fmt --check`; `cargo test -p engine luma_leveler` (8 tests); `cargo test -p devices luma_leveler`;
`cargo test --workspace` (including allocation-free process-path coverage); `cd web && pnpm run check`,
`pnpm run typecheck` (0 errors; 2 pre-existing `FloatingWindow` warnings), `pnpm run test` (22 files,
287 tests), and `pnpm run wasm`. No commit was created because autonomous per-task commit authority was
not granted. Remaining intentional fidelity limits are tube/transformer coloration and a frequency-dependent
sidechain emphasis control.
