# Epic 8 — The Acoustic Domain & Micing — Design Notes & Delivery Record

Companion archive to `IMPLEMENTATION_PLAN.md`. Epic 8 added a **fourth signal carrier — acoustic sound
pressure (Pascals @ the analog rate)** — so an instrument is *a source you mic*, not a line-out. It stood up
the carrier headless (a `SoundSource`/vox + the two mics as acoustic→analog transducers, §8.1), surfaced it
through the product layer (the web mirror of the carrier + the micing gain-staging scenario, §8.2), and closed
the loop (a `PoweredMonitor` voltage→acoustic transducer + acoustic feedback / howlround emerging from a
delayed air edge, §8.3). Stories **8.1–8.3 are all done**; the plan keeps a tight summary of Epic 8, and
**this file is the full record** — the *settled design notes* (with the rejected alternatives and the
reasoning that justified each choice), the per-task breakdown, and the per-task *Delivered* / *Deviations*
notes.

Read this when a later epic's design decision turns on **why** Epic 8 was built the way it was, or when you
need the exact API/behavior of something Epic 8 shipped. For *what exists and what binds you going forward*,
the plan's Epic 8 summary is enough; come here for the depth behind it.

The plan's section ordering (Goal → Watch out → Design notes → Tasks → Validate → Delivered → Deviations) is
preserved per story.

---

**Progress:** Stories 8.1 ✅, 8.2 ✅, 8.3 ✅ — **Epic 8 complete.** 8.1 — the headless acoustic carrier, vox
source & mic transduction; 8.2 — the web/product mirror of the carrier + the micing-scenario exit gate; 8.3 —
speakers as acoustic sources (the `PoweredMonitor` converter) + acoustic feedback (the air edge is delay-eligible,
so howlround emerges from a delayed loop). **405 engine + 67 devices + 283 web tests green.** This Epic activates
the **"air link" seam** both microphones' docs have long anticipated ("a real acoustic path… with a
pressure↔volts transduction seam at each end; until then the sine is the seam"). It adds a fourth signal
carrier — **acoustic sound pressure** — so that an instrument is a *source you mic*, not a line-out: the
richest lesson of the medium (mic choice, phantom, gain staging from mic level, PAD, the dynamic-vs-condenser
gap) becomes real. Sound *generation* stays deliberately trivial here (a sine at a chosen loudness);
recognizable **SoundFont/multisample instruments are a separate future Epic**, not this one.

**Goal:** make the acoustic side a first-class, physically-honest carrier — sound pressure in **Pascals**,
oversampled against the same analog continuum as volts, measured in **dB SPL** exactly as volts are measured
in dBu — and turn the two microphones into genuine **acoustic→analog transducers** fed by a patchable sound
source. Loudness at the source (whisper vs. shout, an acoustic guitar vs. a cranked amp) drives the whole
front-end gain-staging story, and it all emerges from `v = pressure × sensitivity` + the existing phantom /
balanced / noise-floor physics. Anchored to `PROJECT_PLAN.md` §4 (the "instrument / source" concept, made
real) and §5.5 (see the scope note below).

**Exit criteria:** you can place a sound source in the studio, mic it with a condenser or a dynamic, and the
recorded level, the required preamp gain, and the clipping/PAD behavior all match hand calculations and
real-world intuition across the SPL range — with the acoustic carrier compile-validated like every other
domain and the hot path still zero-alloc / panic-free.

**Watch-outs:** keep the realism budget on volts-and-converters — **air is ideal** (no propagation, no room,
no polar response, no distance); pressure rides at the **`AnalogRate`**, so do **not** invent an acoustic
"sample rate" or any global acoustic↔analog rate relationship (§7). SPL is a *measurement* — a buffer holding
dB SPL is a category error, exactly as a buffer holding dBu would be.

### Decisions that bind this Epic (settled in design discussion)

- **Acoustic is a real carrier, not a fudge.** Sound pressure is `Pascals` (a unit newtype like `Volts`),
  stored linearly in a `PressureBuffer` oversampled at the one `AnalogRate` (pressure is continuous-time
  analog — **not** a digital rate). **SPL** (ref 20 µPa) is a measurement helper, never storage — the anchor
  is **1 Pa = 94 dB SPL** (≈93.98). This mirrors the volts/dBu split exactly.
- **The microphone is the acoustic→analog converter** — the input-side bookend to the speaker. Converters
  remain the *only* cross-domain bridge (§5.6): an acoustic edge may only join acoustic ports; acoustic↔analog
  is rejected at `compile` as `DomainMismatch` (free — the check already compares `domain()`). Unlike the
  AD/DA, the transduction is a **pointwise multiply by sensitivity (V/Pa)** — same rate, no anti-alias FIR,
  no resample.
- **Air is ideal.** The acoustic edge is a **unity copy** (`EdgeKind::AcousticRoute`) with **no impedance
  divider** — air is not a Thévenin/load network. Distance/inverse-square attenuation, propagation delay, and
  polar/off-axis response are **deferred** (distance is where "move the mic closer/further" becomes real).
- **Scope relationship to `PROJECT_PLAN.md` §5.5.** That section keeps acoustics out of scope on the
  **output** side (the speaker stays in volts; no V→SPL, no air→ear). This Epic adds an **input-side** acoustic
  carrier — a source's sound pressure into a mic capsule — with air idealized to a direct handoff; and, in
  **Story 8.3**, a matching **output-side** radiation stage: a *distinct* voltage→acoustic converter (a powered
  monitor) that radiates `pressure = drive_volts × sensitivity`, so a device can be **mic'd**. This is a
  deliberate, scoped extension of §5.5 (settled at 8.3 planning): the **voltage-capture `Speaker` terminus stays
  untouched** (the §5.5 output tap in volts — the whole `da→spk` capture path is unchanged); what's new is
  **speaker→air→mic**, a closed device-to-device loop, **still not air→ear** (no listener, no room, no
  propagation/distance). It is *not* instrument realism (§2): the source is a sine, the bar is "recognizable +
  real dynamics," and the machine-gun effect is acceptable. _(A one-line note in PROJECT_PLAN §4/§5.5 recording
  this input+output acoustic carrier — device-to-device only — is a reasonable follow-up.)_
- **Sensitivity calibration is backward-consistent.** Mic sensitivities are chosen so the pre-Epic-8 mic
  defaults reproduce at 94 dB SPL: condenser **~10 mV/Pa** (10 mV @ 1 Pa == the old 0.01 V default), dynamic
  **~2 mV/Pa** (== 0.002 V). The dynamic's **~14 dB deficit** (⇒ ~14 dB more preamp gain) becomes *emergent*
  from transducer efficiency, no longer an asserted LEVEL.
- **Air is patched as an "acoustic cable"** in the web layer — a distinct/dashed acoustic↔acoustic link
  reusing the existing patch machinery (the engine only ever sees an acoustic edge). Proximity/placement-based
  pickup is deferred.
- **SoundFont / multisample instruments are a separate future Epic** — no rustysynth, no sample assets, no
  polyphony machinery here.

### Story 8.1 — The acoustic carrier, vox source & mic transduction (headless engine) — ✅ **Done**

*Goal:* stand up the acoustic carrier end to end in the headless engine and prove the transduction physics by
hand calc — a `SoundSource` emitting sound pressure at a chosen SPL, carried over an ideal acoustic edge into
a microphone that transduces it to a balanced mic-level voltage, with the existing phantom / passive /
noise-floor behavior riding on top. No devices/UI yet (that is Story 8.2); this is the engine-before-product
slice, validated with tests as the oracle for the physical domain (§9).

*Watch out:*
- **Hot-path contract holds** (`CLAUDE.md` §6): `SoundSource` and the mic transduction are zero-alloc,
  panic-free, denormal-safe; the `no_alloc` counting-allocator test must stay green.
- **Determinism:** `SoundSource`'s oscillator is an `f64` phase accumulator with no ambient entropy (like
  `SynthVoice`/the mics); same seed ⇒ identical output.
- **One rate:** `PressureBuffer` is at `AnalogRate` — do not add a pressure rate or an acoustic↔analog rate
  relationship (§7). The acoustic lane is the same continuous proxy as the voltage lane.
- **Don't drop coverage when removing the self-tone:** the two mics' existing capsule-tone tests must be
  *rewritten* to drive the mic from a pressure input and assert transduction, not deleted.

*Design notes (settled at planning):*
- **The mics lose their self-tone.** The deterministic capsule sine and its `FREQ` / `LEVEL`-as-volts params
  are **removed** and replaced by a single `SENSITIVITY` (V/Pa) param; the mic now transduces whatever pressure
  arrives on its acoustic input, and an **unpatched mic is silent** (only its self-noise applies).
  *Rejected:* keeping the tone behind a "built-in test oscillator" toggle — the seam comments always framed the
  tone as a stand-in "until the air link," and a mic emitting a tone into a silent room is dishonest.
- **The source node is generic** (`SoundSource`): a sine at `FREQ` (Hz) emitted at a loudness knob in **dB SPL**
  (log — the engineer's unit; stored/emitted as Pa). Kept generic so a future drum/amp/guitar reuses it; the
  catalog (Story 8.2) labels one instance "vox".
- **The acoustic edge is a unity copy** (`EdgeKind::AcousticRoute`, mirroring `DigitalRoute`) — no divider, no
  cable R·C: air is ideal.
- **The carrier extends the open seams additively** — `Domain::Acoustic`, `Lane::Acoustic(PressureBuffer)` +
  `pressure()`/`pressure_mut()`, `AcousticFace` + `InputPort::Acoustic`/`OutputPort::Acoustic`
  (`lane_count() == 1`). No existing node's ports change except the two mics gaining one acoustic input.

*Tasks:*
- **Task 8.1.1 — `Pascals` unit + SPL↔Pa conversions.** A `repr(transparent)` newtype mirroring `Volts`, and
  `spl_to_pascals` / `pascals_to_spl` (ref 20 µPa) alongside the other level helpers. *Validate (hand calc):*
  1 Pa = 93.98 dB SPL; 94 dB SPL ⇒ ≈1 Pa; 0 dB SPL ⇒ 20 µPa; 30 dB SPL ⇒ 632 µPa; 120 dB SPL ⇒ 20 Pa.
- **Task 8.1.2 — `PressureBuffer` + the acoustic carrier seam.** `PressureBuffer` (linear Pa @ `AnalogRate`,
  mirroring `VoltageBuffer`); `Domain::Acoustic`; `Lane::Acoustic` + typed accessors; `AcousticFace` and the
  `InputPort`/`OutputPort` acoustic arms (`domain()`, `lane_count() == 1`, `acoustic()`, `From` impls). No node
  yet. *Validate:* the workspace compiles; unit tests for `domain()`/`lane_count()` and the lane accessors.
- **Task 8.1.3 — compile: the `AcousticRoute` edge + lane pool.** Add `EdgeKind::AcousticRoute` (unity
  pressure copy, mirroring `DigitalRoute`) and the acoustic arm in the lane-pool allocation. *Validate:* using
  scaffold test nodes (like `digital_seam`/`event_seam`), an acoustic edge copies pressure src→dst unchanged,
  and an acoustic↔analog edge is rejected as `DomainMismatch`.
- **Task 8.1.4 — the `SoundSource` node (vox engine node).** A sine at `FREQ`, emitted at a `LEVEL` knob in
  dB SPL → Pa → `Lane::Acoustic`; one acoustic output, no inputs; deterministic phase accumulator; zero-alloc,
  panic-free. *Validate (hand calc):* the emitted peak pressure is ≈1 Pa at 94 dB SPL and 20 Pa at 120 dB SPL
  (read back via `pascals_to_spl` on the block peak); silent at construction until prepared.
- **Task 8.1.5 — the mics become transducers.** `CondenserMic` and `DynamicMic` each gain one acoustic input;
  `process` computes the differential `s = pressure × sensitivity`, keeping the phantom pedestal / passive-0
  common-mode / power-gate logic on top; the self-tone + `FREQ`/`LEVEL`-volts params are replaced by
  `SENSITIVITY` (V/Pa). Existing capsule-tone tests rewritten to drive from a pressure input. *Validate (hand
  calc):* condenser 10 mV/Pa ⇒ 10 mV differential at 94 dB SPL (== old default), dynamic 2 mV/Pa ⇒ 2 mV
  (~14 dB less); whisper 30 dB SPL ⇒ ~6.3 µV; cranked amp 120 dB SPL ⇒ 200 mV; a dead/unpowered condenser is
  still silent regardless of input pressure.

*Validate (Story 8.1):* ✅ **met** — a headless `SoundSource → acoustic edge → mic → balanced receiver` chain
recovers `differential = pressure × sensitivity` to hand-calc tolerance across the oracle SPLs, the
condenser-vs-dynamic sensitivity gap is confirmed numerically, and the full gate is green
(`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`, `no_alloc` included).

*Delivered:* the acoustic carrier is a first-class fourth domain, end to end, headless.
- **Task 8.1.1** — `Pascals` newtype (`signal/pascals.rs`, mirroring `Volts`) + `spl_to_pascals` /
  `pascals_to_spl` (`level.rs`, ref 20 µPa). Anchor **1 Pa = 93.98 dB SPL** and the 30/94/120 dB oracles
  asserted. SPL is a measurement helper, never storage.
- **Task 8.1.2 + 8.1.3** — landed **together** (one indivisible compile unit: adding `Domain::Acoustic` /
  `Lane::Pressure` forces every exhaustive match — edge-kind assignment, the copy loop, the pool allocator —
  to gain its acoustic arm, so the workspace can't compile without the edge in place). `PressureBuffer`
  (`signal/pressure.rs`, linear Pa @ `AnalogRate`, mono), `Domain::Acoustic`, `Lane::Pressure` +
  `pressure()`/`pressure_mut()`, `AcousticFace` + `InputPort`/`OutputPort::Acoustic` (`lane_count() == 1`).
  `EdgeKind::AcousticRoute` = a **unity pressure copy** (ideal air — no divider, no cable R·C, the acoustic
  peer of `DigitalRoute`); `alloc_lane` grows an acoustic arm; the existing domain-match check rejects
  acoustic↔analog **for free**. `acoustic_seam` white-box tests prove the mono lane, the unity copy, and the
  `DomainMismatch`.
- **Task 8.1.4** — `SoundSource` node (`node/sound_source.rs`), the first node that *emits* pressure: a sine
  at `FREQ` radiating at a `LEVEL` = **peak pressure in Pascals** (linear, like `GainStage`'s gain — the
  dB-SPL knob is the UI's concern). Deterministic free-running phase; zero-alloc/panic-free. Oracles: 94 dB
  SPL ⇒ ≈1 Pa peak, 120 dB ⇒ 20 Pa.
- **Task 8.1.5** — both mics became **acoustic→analog transducers**: one acoustic input, `differential =
  pressure × sensitivity`, the self-tone oscillator + `LEVEL`/`FREQ` params **removed** and replaced by a
  fixed `SENSITIVITY` (V/Pa) — mics now have **no control params**; an unpatched mic is silent. Condenser
  keeps its phantom pedestal / power-gate on top; dynamic stays passive (0 CM, always alive). Sensitivities
  (condenser 10 mV/Pa, dynamic 2 mV/Pa) reproduce the old defaults at 94 dB SPL, and the **~14 dB gap is now
  emergent** from transducer efficiency. The engine phantom-integration tests were adapted (pedestal tests use
  an unconnected capsule; the audio-recovery test is fed by a `SoundSource`).

*Deviations / notes (not bugs):*
- **A minimal `vox` catalog device was pulled forward from Story 8.2.** Two rich `devices`-crate integration
  tests (phantom through the 8i6) depended on the mic's now-removed self-tone; `build_patch` handles acoustic
  connections for free, so feeding a `vox` at exactly 1 Pa into the capsule keeps their 10 mV hand-calcs
  verbatim. **Only the device's *existence*** (with engine-native Pa/Hz params) moved earlier — its
  skeuomorphic **faceplate, the acoustic-cable rendering, and the dB-SPL loudness knob stay in 8.2.** Likewise
  `Connector::Acoustic` / `PortKind::Acoustic` are **declarations only** (styling is 8.2).
- **Precision boundary:** a whisper's ~6 µV differential is below `f32`'s resolution when it rides the
  condenser's ~37.86 V phantom pedestal (ULP ≈ 4 µV), so the small-signal SPL-range oracle is asserted on the
  **pedestal-free dynamic mic** (exact), and the condenser's transduction magnitude is asserted at the 94 dB
  anchor with a loose tolerance. This is the same f32-cancellation reality the pre-8.1 pedestal tests already
  noted, not a new limitation.
- **Web untouched:** the Rust gate is green, but with acoustic ports + the `vox` device now in the catalog the
  browser harness won't render them correctly until Story 8.2 (the `pnpm` checks aren't part of the cargo gate).

### Story 8.2 — Micing in the studio (web + scenarios) — ✅ **Done**

*Goal:* surface the acoustic carrier through the product layer so you can **place a source, mic it, and hear the
front-end come alive** — drag a vox and a mic into the studio, wire the capsule with an acoustic cable, and watch
loudness (whisper → shout → cranked amp) drive the gain-staging story (`PROJECT_PLAN.md` §4, §7). Almost all of
the *engine/devices* surface this story once owned was **pulled forward by Story 8.1** (the `vox` device, the mics'
acoustic capsule ports + descriptors, `Connector::Acoustic` / `PortKind::Acoustic`, `build_patch`'s acoustic-edge
expansion, and the free acoustic↔analog `DomainMismatch`), so 8.2 is now essentially a **`web/` story** — the
client-side mirror of the new carrier — plus the numeric validation and a small connector test.

*Watch out:*
- **The web mirrors legality, it doesn't own it** (Epic 4 decision): `build_patch` is authoritative. Keep the
  `catalog.ts` unions and `connectorsCompatible` a faithful mirror of the Rust — a drift shows as the UI allowing a
  patch the engine then rejects (or vice-versa).
- **`format.ts` couples display to taper today** (`taper === "log"` ⇒ `20·log10(value)` dB) — correct only because
  every current log param is a voltage-gain. The vox breaks that: Loudness is dB **SPL** over *pressure*, Pitch is a
  log-travel **Hz** knob. Decouple **display (by unit) from travel (by taper)** or both vox knobs read wrong.
- **dB SPL is a display, never storage** — the engine param stays linear `Pascals`; the web converts `Pa ↔ dB SPL`
  (ref 20 µPa, `1 Pa = 94 dB SPL`) for the knob only. Don't push dB SPL back into the scene/param value.
- **Air stays ideal** — the acoustic cable is a *rendering* distinction with no pickable properties (no R·C, no
  length); distance/attenuation/polar remain deferred. Don't add it to the R·C cable catalog.
- Run `pnpm run format` + `pnpm run check` + `pnpm run typecheck` after web edits (the cargo gate doesn't cover `web/`).

*Design notes (settled at planning):*
- **Acoustic cable = rendering-only, no options.** A distinct **dashed** acoustic↔acoustic link, reusing the
  existing patch machinery; the engine only ever sees the unity `AcousticRoute` edge. *Rejected:* a
  pickable cable-catalog entry (like the R·C presets) — it would imply parameters (length/attenuation) the Epic
  explicitly defers, over-building for ideal air.
- **Loudness knob shows dB SPL, ~0–130.** The web maps the vox's linear `Pascals` param to a dB-SPL readout
  (`20·log10(Pa / 20e-6)`, silence = −∞) and back; the existing **log taper's travel is reused as-is** (it already
  maps "dB below max, snapping to silence at the bottom", which is the right feel — the engine ceiling is 100 Pa ≈
  134 dB SPL). *Rejected:* a raw-Pascals knob (not how loudness is discussed; poor feel).
- **No bespoke faceplates.** The generic descriptor-driven `Panel` already renders the vox (Loudness + Pitch knobs,
  acoustic output jack) and the mics (acoustic capsule input jack + XLR output) — so the story needs **jack/cable
  styling + the dB-SPL knob fix**, not new Svelte faceplate components. *(A skeuomorphic bespoke `Vox`/mic faceplate
  is optional future polish, deferred — this is a deviation from the coarse sketch's "vox faceplate" wording,
  justified: the fallback Panel + shared widgets already cover it, per the Epic-5 component-over-coordinate rule.)*
- **Validation = a durable Rust scene test + manual by-eye** (no new web test infra, per the project's deferred-e2e
  stance): the numbers are asserted Rust-side (tests are the oracle for the analog domain); the browser check is a
  human confirmation the UI patches and renders.

*Tasks:*
- **Task 8.2.1 — Web carrier types + legality mirror.** Add `"acoustic"` to `PortDomain`, `PortKind`, and
  `Connector` in `web/src/catalog.ts`; confirm `connectorsCompatible` (equality already covers it) and the
  `connections.ts`/`patching.ts` legality (`domain` match + `wouldCreateCycle`, both carrier-agnostic) accept a
  vox→mic-capsule patch and reject acoustic↔analog. Add the missing acoustic case to the Rust
  `connectors_compatible_matrix` test. *Validate:* client-side, a vox→capsule patch reads legal and an
  acoustic↔line patch reads illegal (mirroring `build_patch`); `pnpm run typecheck` green; the Rust connector test
  passes.
- **Task 8.2.2 — Acoustic jack & cable styling.** A signal-palette token (`--ae-signal-acoustic`) + `Jack.svelte`
  styling for `data-kind="acoustic"` / `data-domain="acoustic"`; a **dashed** acoustic cable in `cable-view.ts`; a
  `CableInspector.svelte` acoustic branch mirroring the digital "ideal (no cable)" line. *Validate (by-eye):* the
  vox's Air out and the mics' Capsule in render as distinct acoustic jacks, and the acoustic cable draws dashed with
  an "ideal — no cable" inspector.
- **Task 8.2.3 — dB-SPL loudness knob.** Decouple `format.ts` display from taper — format **by unit** (`"dB"` ⇒
  gain dB as today; new `"dB SPL"` ⇒ `20·log10(value/20e-6)`, `−∞` at 0; everything else ⇒ number + unit, so a
  log-taper Hz knob reads Hz); set the vox Loudness param's unit to `"dB SPL"` in the catalog. *Validate:* the vox
  Loudness knob reads **94 dB SPL at 1 Pa** and Pitch reads Hz; a `format.ts` Vitest unit test covers the dB/dB-SPL/
  unit branches (cheap, pure logic — no browser).
- **Task 8.2.4 — The micing scenario (the exit gate).** A Rust harness/devices scene-level integration test:
  `vox → mic → 8i6 preamp → AD`, asserting the gain-staging by hand calc — a whisper (30 dB SPL) sits near the
  preamp noise floor, a cranked amp (120 dB SPL) needs the −10 dB PAD to avoid clipping, and a dynamic mic reads
  ~14 dB below a condenser on the same source at the same gain. Plus a manual in-browser pass building the chain.
  *Validate:* the exit criteria below, green.

*Validate (Story 8.2):* ✅ **met** — in the browser you can drag a vox and a condenser (or dynamic) into the studio, wire the
capsule with an acoustic cable, and drive the loudness knob (in dB SPL) to watch the front-end gain-staging emerge
— whisper near the noise floor, cranked amp needing the PAD, dynamic needing more gain than a condenser; the
supporting numbers are pinned by the Rust scene test; the full cargo gate **and** `pnpm run check`/`typecheck`/`format`
are green. Deferred (record, don't build): distance/inverse-square attenuation, propagation delay, polar/off-axis
response, proximity/placement-based pickup, and bespoke skeuomorphic vox/mic faceplates.

*Delivered:* the acoustic carrier is now surfaced through the product layer — you can place a vox, mic it, and hear
the front-end gain-staging come alive. Since Story 8.1 pulled the engine/devices surface forward, this was
essentially a `web/` story plus the numeric exit gate.
- **Task 8.2.1** — web carrier types + legality mirror (`web/src/catalog.ts`): `"acoustic"` added to `PortDomain`,
  `PortKind`, and `Connector`. `connectorsCompatible` (equality) mates acoustic only with itself; the
  `connections.ts`/`patching.ts` legality is carrier-agnostic, so it accepts a vox→capsule patch and rejects
  acoustic↔analog for free. Mirrored Rust-side by an acoustic arm in the `connectors_compatible_matrix` test, and
  covered web-side by two `connections.test.ts` cases (legal acoustic edge, illegal acoustic→analog) plus a
  `cableAllowed("acoustic") === false` assertion (air carries no R·C cable).
- **Task 8.2.2** — acoustic jack & cable styling: a `--ae-signal-acoustic` palette token (a light sky-cyan, kept
  clear of mic-blue and digital-teal) in `tokens.css`; `Jack.svelte` rings the acoustic connector with it; a
  **dashed** acoustic cable in `Cable.svelte` (all three strokes dashed together so the shadow doesn't fill the
  gaps — it reads as intangible ideal air); a `CableInspector.svelte` acoustic branch showing "acoustic link —
  ideal air (no cable)", mirroring the digital ideal-link line.
- **Task 8.2.3** — dB-SPL loudness knob: `format.ts` display is now keyed on **unit**, not taper — `"dB"` ⇒ gain
  `20·log₁₀` dB (as before), the new `"dB SPL"` ⇒ `20·log₁₀(p / 20µPa)` (−∞ at 0), everything else ⇒ number + unit
  (so a log-taper Hz knob reads Hz). The vox Loudness param's unit became `"dB SPL"` in the catalog (linear Pa
  stays the stored value; the log taper's dB-below-max travel is reused as-is). A `format.test.ts` Vitest covers
  the switch/dB/dB-SPL/unit branches. (Same commit also shrank the vox from a 1.7 m person-sized tower to a
  compact ~200 mm desktop box, on the scale of a mic — the old size dwarfed the gear and was awkward to patch.)
- **Task 8.2.4** — the micing scenario, the exit gate: `micing_scenario_gain_staging` in `build.rs`, a
  `vox → mic → 8i6 preamp` scene test reading the post-preamp **In 1 Peak** meter (dBu) and asserting the
  gain-staging by hand calc. A whisper (30 dB SPL) is buried at the −60 floor — below the AD LSB and the
  condenser's f32 phantom-pedestal cancellation floor (you need a sensitive mic + lots of gain). At 94 dB SPL /
  default gain, the condenser reads −29.89 dBu and the dynamic −43.91 dBu — the **~14 dB gap emergent** from
  transducer efficiency (5× sensitivity) plus the deeper dynamic loading divider. A cranked amp (120 dB SPL) at
  ×100 gain pins the 10 V rail (+22.22 dBu, ~5.9 dB clipped) without the PAD and lands clean at +18.11 dBu with
  it. Nothing special-cased — it all falls out of `v = pressure × sensitivity` + the existing dividers/rail.

*Deviations / notes (not bugs):*
- **No new web faceplates or e2e test infra**, as planned — the generic descriptor-driven `Panel` + shared widgets
  already render the vox and mics, so the story was jack/cable styling + the dB-SPL fix, not new Svelte components.
  The gain-staging numbers are pinned by the durable Rust scene test (tests are the oracle for the analog domain);
  the browser is a manual by-eye confirmation.
- **The whisper case asserts the buried −60 floor, not a precise recovered magnitude** — at high gain the
  condenser's amplified phantom-pedestal residual dominates a signal that small (the documented Story-8.1 f32
  boundary); the physical gain-staging lesson is carried by the 94 dB and 120 dB cases.

### Story 8.3 — Speakers as acoustic sources & acoustic feedback — ✅ **Done**

*Goal:* close the loop — let a speaker **radiate sound pressure you can mic**, the output-side mirror of the
microphone, so **acoustic feedback (howlround / Larsen) emerges from the physics** the moment you patch a
`mic → preamp → monitor → air → mic` loop and push the gain up. A new voltage→acoustic **converter** node (a
powered monitor) transduces `pressure = drive_volts × sensitivity (Pa/V)`, **distinct** from the voltage-capture
`Speaker` terminus (which stays in volts, untouched — §5.5's output tap is unchanged; see the Epic-8 scope bullet
above). The enabler is that the acoustic domain, like digital, can **carry a one-block delay to break a graph
cycle** — and because a small delay in the air hop is *physically true*, that is exactly where the loop's latency
belongs. The "ring out the monitors" / gain-before-feedback lesson is then not special-cased: it falls out of
`v = pressure × sensitivity` + the delayed air edge. Anchored to `PROJECT_PLAN.md` §4 (a device as a *source you
mic*) and the §5.5 scope extension settled at planning (device-to-device radiation, still no air→ear).

*Watch out:*
- **Feedback must *emerge*, never be scaffolded.** No feedback-detector, no gain-limiter, no special demo device —
  howlround is the geometric consequence of loop gain ≥ 1 through the delayed loop. The realistic self-limit is
  the **rail clip** (the loop stops growing when the preamp/monitor pins its rail) — that too is emergent, not
  coded.
- **The latency lands on the *air* edge, always.** The existing auto-break (§5.9, `schedule.rs` step 5c) cuts the
  *lowest-index* breakable edge, gated to `Domain::DigitalAudio`. A realistic loop through the 8i6 crosses the
  interface's **AD→matrix→DA** (digital edges) *and* the air edge, so lowest-index could land the delay on a
  digital edge. Change the policy to **prefer an acoustic edge** among the breakable edges on a cycle (air carries
  the physical delay), falling back to digital — so the delay is physically honest regardless of what else is in
  the loop. This is a touch more than the coarse sketch's "one-condition change," and deliberately so.
- **Hot-path contract holds** (`CLAUDE.md` §6): the converter's `process` is zero-alloc, panic-free,
  denormal-safe; the delayed-edge machinery is already alloc-free (the `delayed_steps` copy path is
  carrier-agnostic — it copies a pressure lane exactly as it copies a sample lane). `no_alloc` stays green.
- **Air stays ideal, still.** The air edge is the unity `AcousticRoute` from 8.1 — the one-block delay is a
  **cycle-break mechanism** (block-size-dependent), **not** a calibrated propagation time. Distance→delay-line and
  inverse-square attenuation stay deferred (they pair with 8.2's deferred distance work).
- **Don't touch the `Speaker` terminus.** The whole `da→spk` capture path depends on it staying a pure
  voltage→voltage tap. The radiator is a **new, separate** device/node.

*Design notes (settled at planning):*
- **A new "powered monitor" device, not an extension of `Speaker`.** One new catalog device: an analog **line
  input** (¼", line-level) → an **acoustic output** (`Connector::Acoustic`, mates only with a mic capsule) with a
  **Volume** knob (a log-taper attenuator from 0 dB, exactly like the 8i6's monitor amp). *Rejected:* adding an
  acoustic output to the existing `speaker` device (muddies the §5.5 voltage terminus and risks the capture path);
  *rejected:* a bespoke guitar-combo-amp or a separate power-amp + passive-cab pair (more surface than the lesson
  needs — the 8i6 preamp already supplies the loop gain, per the planning decision to keep new surface minimal).
- **The converter node mirrors the dynamic mic.** `inputs: [InputZ…]` (voltage in), `outputs: [AcousticFace…]`
  (mono pressure out), `process`: `pressure = drive_volts × sensitivity × volume`. Passive, no phantom — the exact
  reverse of `dynamic.rs`. It reads `inputs[0].voltage()`, writes `outputs[0].pressure_mut()`.
- **Sensitivity calibrated so the *feedback-resistance* lesson emerges.** Target **≈ 1 Pa/V** (pinned exactly by
  hand-calc in the tests against the 8i6 preamp's real gain range + dividers): loop gain
  `L = mic_sensitivity(V/Pa) × preamp_gain × dividers × monitor_sensitivity(Pa/V) × volume`. At ≈1 Pa/V a
  **condenser** (10 mV/Pa) crosses unity around **+40 dB** of preamp gain, while a **dynamic** (2 mV/Pa) needs
  ≈ +54 dB — the same ~14 dB sensitivity gap now showing up as *"the dynamic is harder to ring"* (why loud stages
  favor dynamics). Not special-cased — it's the 8.1 sensitivity gap seen through the loop.
- **Prefer-acoustic cycle-break** (see Watch out): the delay always lands on the air hop. A pure-**analog** cycle
  (no digital *and* no acoustic edge) still correctly rejects as `CompileError::Cycle` — analog feedback has
  nowhere to carry latency. The `Cycle` doc comments (`schedule.rs` ~138, ~209) get updated from "no digital edge"
  to "no digital-or-acoustic edge."
- **Validation = engine hand-calc tests + a devices scenario + manual by-eye** (house style, deferred-e2e): the
  *mechanism* is proved at the engine level with raw nodes (exact gains, the air edge unambiguously the delayed
  leg); the *product loop* is proved through real catalog devices via `build_patch`; the browser is a human
  confirmation the loop patches and rings. No new web test infra.

*Tasks:*
- **Task 8.3.1 — The powered-monitor converter node (engine).** A new `node/…` (e.g. `PoweredMonitor`): analog
  voltage input + acoustic output, `pressure = drive_volts × sensitivity × volume`, a `Volume` param (log taper,
  attenuator from 0 dB). Zero-alloc / panic-free / denormal-safe; `no_alloc` green. *Validate (hand calc):* 1 V
  drive × sensitivity × unity volume = the calibrated peak pressure (asserted in Pa **and** its dB SPL); volume
  −6 dB halves the pressure; a `declares_faces` test confirms `inputs()[0].domain() == Analog` and
  `outputs()[0].domain() == Acoustic` (`lane_count() == 1`, mono).
- **Task 8.3.2 — Acoustic edges delay-eligible; the air edge carries the latency (engine cycle-break).** Extend
  `schedule.rs` step 5c's breakable filter to `DigitalAudio || Acoustic`, and **prefer an acoustic edge** when a
  cycle has both; update the `CompileError::Cycle` doc comments. *Validate:* a raw-node
  `DynamicMic → GainStage → PoweredMonitor → [acoustic edge] → DynamicMic` loop compiles (no `Cycle`) with
  **exactly one delayed edge = the acoustic one**; with **sub-unity** loop gain a step reaches the geometric
  steady state `1/(1−L)` and a transient re-enters **one block later** scaled by `L` (hand-calc); with
  **super-unity** gain successive block peaks **grow** (ratio `L`) toward the rail, then self-limit at the clip;
  a **mixed** digital+acoustic cycle delays the acoustic edge; a **pure-analog** cycle still rejects as `Cycle`.
- **Task 8.3.3 — The `powered_monitor` catalog device + web (devices).** A `CatalogEntry` (line input ¼" +
  acoustic output `Connector::Acoustic`, a Volume param) building the 8.3.1 node. Confirm `build_patch` assembles
  a `mic → 8i6 → powered_monitor → [acoustic] → mic` loop, **compiles auto-broken with the delay on the air
  edge** (proving prefer-acoustic end-to-end, even across the 8i6's digital edges). Web renders via the generic
  descriptor-driven `Panel` + 8.2's acoustic jack/cable styling (no new Svelte). *Validate (by-eye):* the monitor
  renders with a line jack + a dashed acoustic out; the loop patches; routing the 8i6 for direct monitoring and
  raising the gain makes the input meter climb toward feedback. `pnpm run check`/`typecheck`/`format` green.
- **Task 8.3.4 — The feedback scenario (the exit gate).** A harness/devices scene-level integration test pinning
  the product-chain numbers by hand calc: the loop gain `L` for a condenser vs. a dynamic through the 8i6 preamp
  + the monitor; assert **below unity** the loop is bounded (settles), **above unity** the block peaks grow toward
  the rail and self-limit (howlround, emergent), and the **condenser rings at a lower gain than the dynamic**
  (the ~14 dB feedback-resistance gap). Plus a manual in-browser pass building and ringing the loop. *Validate:*
  the exit criteria below, green.

*Validate (Story 8.3):* ✅ **met** — a `mic → preamp → monitor → air → mic` patch **compiles** (auto-broken, the
one block of latency sitting on the **air** edge); a below-unity loop settles to a bounded, one-block-trailing
steady state; pushing the gain past unity makes the level **build toward feedback** and self-limit at the rail —
with the condenser ringing at a lower gain than the dynamic — all pinned by hand-calc Rust tests; an all-analog
cycle with **no** delay-eligible edge still rejects as `Cycle`; the full cargo gate **and** `pnpm run
check`/`typecheck` are green. *(Manual by-eye — build the loop in the browser and hear/see it ring — remains a
human confirmation, per the deferred-e2e stance.)* *Deferred (record, don't build):* a **distance→delay-line** +
inverse-square attenuation (the one-block delay is a cycle-break mechanism, not calibrated propagation — pairs with
8.2's deferred distance work); speaker frequency-response / cabinet coloration (flat for now); and **air→ear / room
acoustics** (out of scope — the loop is device-to-device).

*Delivered:* the loop is closed — a device can be **mic'd**, and acoustic feedback emerges from the physics.
- **Task 8.3.1** — the `PoweredMonitor` node (`engine/src/node/powered_monitor.rs`), the output-side mirror of the
  microphone: analog voltage in, mono acoustic (pressure) out, `pressure = drive_volts × sensitivity × volume`
  (pointwise, ideal air — no state, no rate). A **Volume** param (linear attenuator from unity). Passive, the exact
  reverse of `dynamic.rs`. Distinct from the `Speaker` terminus, which is untouched (still the §5.5 volts tap).
  Anchor pinned by hand calc: 1 V × 1 Pa/V × unity = 1 Pa = 93.98 dB SPL; volume −6 dB halves the pressure.
- **Task 8.3.2** — acoustic edges became **delay-eligible** at `schedule.rs`'s step-5c cycle-break, and the policy
  now **prefers an acoustic edge** over a digital one on the same cycle (`breakable_in(Acoustic).or_else(||
  breakable_in(DigitalAudio))`), so a loop's one block of latency always lands on the physical air hop. The
  delayed-edge copy path was already carrier-agnostic (it copies a pressure lane like any other), so no runtime
  change was needed there; the `CompileError::Cycle` doc/message/step comments were updated to "digital **or
  acoustic**". Engine tests (`acoustic_feedback` module) prove: a real `mic → receiver → gain → monitor → air → mic`
  loop compiles with **exactly one delayed edge = the acoustic one**; a mixed digital+acoustic cycle delays the
  acoustic edge; a delayed acoustic self-loop carries feedback (below-unity converges, super-unity grows then pins
  at the rail); a pure-analog cycle still rejects as `Cycle`.
- **Task 8.3.3** — the `powered_monitor` catalog device (`devices/src/catalog.rs`): a line-level ¼" input + an
  acoustic **Air** output (`Connector::Acoustic`, mates only with a mic capsule) + a Volume knob (unit `"dB"`, log
  taper — reuses 8.2's gain-dB display). `build_patch` assembles the real product loop `condenser mic → 8i6 →
  Line Out 1 → monitor → air → mic` — a genuine graph cycle (the 8i6's matrix is one node) — and it **compiles
  auto-broken and runs**. **No new web code**: the device is fully descriptor-driven, and `PortKind::Line` /
  `Connector::Acoustic` / the `"dB"` unit were all already handled since 8.2 (wasm rebuilt; the generic `Panel` +
  8.2 jack/cable styling render it).
- **Task 8.3.4 — the exit gate** — a real-node feedback loop (`acoustic_feedback` module) seeded by a constant
  drive so the loop gain is exactly `L = gain × sensitivity` (bridging impedances ⇒ edge dividers ≈ 1), pinned by
  hand calc: below unity (L = 0.5) settles to the fixed point 1.0 Pa; above unity (L = 2.0) rings up 2 → 6 → … and
  self-limits at the 10 Pa rail ceiling; and at the same gain the **dynamic** (L = 0.4, settles at 3.33 Pa)
  **resists the feedback that makes a condenser howl** (L = 2.0, pins the rail) — the same Story-8.1 ~14 dB
  sensitivity gap, now as feedback headroom.

*Deviations / notes (not bugs):*
- **Prefer-acoustic is a touch more than the coarse sketch's "one-condition change"** — the sketch delayed the
  lowest-index breakable edge; a realistic loop through the 8i6 crosses digital *and* the air edge, so preferring
  the acoustic edge is what keeps the latency on the physically-honest air hop (agreed at planning; the user's
  steer: "the delay to the air hop is true to the real world").
- **The exit-gate hand-calcs landed as engine-level real-node tests, not a devices/harness scene test.** A
  devices-level *ringing* loop through the 8i6 crosses its AD/DA and needs the matrix rerouted for direct
  monitoring *and* a seed injected into a closed loop — fiddly and not exactly hand-calculable. The engine real-node
  loop gives exact, deterministic numbers; the **product loop building and running** through real catalog devices
  is covered by 8.3.3's test. Consistent with the project's "tests are the oracle for the analog domain; verify the
  product seam builds" stance.
- **The feedback-resistance test uses two `DynamicMic`s at 10 mV/Pa vs 2 mV/Pa**, not the real `CondenserMic`: the
  condenser needs phantom (⇒ a `MicPreamp` in the loop, with its pad/air/floor), and its phantom pedestal is a DC
  offset the balanced difference cancels — irrelevant to the AC loop gain. Sensitivity is what drives feedback
  resistance, so this isolates the physics cleanly.
