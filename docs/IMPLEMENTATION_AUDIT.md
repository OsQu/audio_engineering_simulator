# Implementation Audit — 2026-07-02

Scope: the whole workspace (`crates/engine`, `crates/devices`, `crates/capture`,
`crates/wasm-bindings`, `crates/harness`, `web/`) audited against `PROJECT_PLAN.md` and
`IMPLEMENTATION_PLAN.md` at the end of Story 4.5. The question asked: what is good and supports
future development, and what gaps or underlying problems could prevent the project from developing
further.

**Verification status at audit time:** `cargo fmt --check`, `cargo lint`, `cargo test`
(all workspaces), `cargo wasm`, and `cargo docs` all green locally. The `web` gates could not run
in this worktree (`node_modules` not installed) but are covered by the CI `web` job.

---

## 1. Verdict

The project is in unusually good architectural health. The core claims the plans make about the
engine — zero-alloc/panic-free hot path, strict carrier separation, determinism, local-solve-only —
are **actually true in the code and mechanically enforced**, not aspirational. The seams the plans
say exist for Epic 5 (multichannel, clock domains, reactive edges, multi-node devices) are mostly
real. The risks are concentrated in three places: **one real correctness bug** in the
device-descriptor layer (pinned by a test that defends the wrong behavior), the **unguarded
Rust↔TS contract**, and **`App.svelte` accreting into a god component** just before the two
stories (4.6, 4.7) that will land on top of it.

---

## 2. What is genuinely good (and pays off later)

### Engine (`crates/engine`)

- **The invariants hold and are enforced mechanically.** `tests/no_alloc.rs` installs a counting
  `GlobalAlloc` and asserts literally zero allocations across 128 blocks of `process` /
  `process_io`, deliberately covering the paths most likely to allocate (per-sample Gaussian, hum,
  the balanced lift, event delivery, the param de-zipper). Every `unwrap`/`expect`/`unreachable!`
  outside tests is on a compile-time path or a compile-guarded dead arm. Denormals are flushed
  everywhere state feeds back. Greps for `thread_rng`/`Instant`/`SystemTime` are clean.
- **Determinism discipline is better than most engines.** Node RNG streams split in node-index
  order; edge coupling streams split from a *separately salted* root, and every edge consumes its
  split whether or not it couples — so adding a cable never perturbs a neighbor's noise
  realization (`schedule.rs:48-50, 778-795`).
- **The two-pool schedule design** (`input_pool` / `output_pool` as disjoint `Vec`s) gives the
  borrow checker structural non-aliasing with zero `unsafe` in the run loop.
- **The per-conductor lift** (`Lifted` + conductor inference by fixpoint) makes "balanced" pure
  composition — CMRR emerges from leg symmetry, never a flag. This is the plan's philosophy
  actually realized in the type system.
- **`compile` is the single fallible gate and is exhaustive** — rich `CompileError` naming the
  offending node/port; this is what makes `process` legitimately total.
- **Extension seams are real, not claimed:**
  - *Multichannel digital*: `Port::lane_count()` already returns `AudioFormat::channels()`; the
    pool allocator and `DigitalRoute` already loop over lane counts. Only node bodies assume mono.
  - *Clock domains*: `ClockDomainId::SINGLE` is an explicit placeholder; cross-rate edges are
    cleanly rejected (`ClockCrossingUnsupported`), so an SRC is an additive `EdgeKind` variant.
  - *Reactive impedance*: `EdgeTransform` is documented as deliberately a struct behind one
    constructor + one `process` so a 2nd-order transfer function generalizes it without touching
    callers — the best-prepared extension point in the crate.
  - *Multi-core*: flat `Vec<Step>` over index-addressed pools, nodes own their state; partitioning
    is unobstructed.
- **No TODO/FIXME/dead code/`#[allow(dead_code)]` anywhere in the engine.** Documentation quality
  is exceptional — every guarded `unreachable!` and every placeholder carries its rationale.

### Product layer (`crates/devices`, `wasm-bindings`, `capture`, `harness`)

- **Descriptor-derivation-from-node is real** for the numeric/domain half: `expand()` reads
  ranges/defaults/domains off freshly built nodes, and `descriptors_carry_engine_truth` checks
  them **bit-exactly** (`to_bits()`) — those fields cannot drift from engine truth.
- **The "open port ⇒ exposed" convention** derives a device's face from its node graph with no
  hand-listing; `catalog_aligns_with_exposed_face` guards the zip-truncation footgun.
- **`build_patch`'s error surface is total and specific** — every failure mode is a typed
  `BuildError` naming the offending element; a malformed patch can never panic the audio thread.
- **Hot-swap correctness is carefully handled and regression-tested** — stale queues cleared, the
  `blocks` note-clock reset on install (the deep-swap bug's regression test), old scene dropped
  off-block; zero-copy output geometry pinned by test.
- **`capture` is the clean boundary the plan demands** — outside the sim, no clock domain, reuses
  the same windowed-sinc decimator as the modeled AD so it adds no artifacts of its own.
- **The harness is a driver, not a second engine** — `render_to_samples` loops the one real
  `process_with_events` path; `tests/render.rs` is the numeric analog-domain oracle.

### Web (`web/`)

- **Pure, DOM-free, Vitest-tested logic modules** (`spatial.ts`, `connections.ts`,
  `scene-store.ts` parse/serialize) — the "tests are the oracle" temperament applied to the UI.
- **Single 3-D coordinate truth + projection** is genuinely 4.6-ready: `project()` already handles
  `"top"`/`"side"`; per-view 2-D positions were correctly avoided.
- **`WorldView` is the thin, ignorant seam it was designed to be** (positioned boxes + pointer
  mechanics only) — the WebGL escape hatch is intact.
- **Descriptor-driven `Panel`** — most new devices need zero UI code.
- **CI covers both stacks** — the Rust gate plus a `web` job (typecheck, Biome, Vitest, build) and
  a real `wasm-pack build` step catching bindgen breakage. (The plan's 4.3 note "no web CI job
  exists yet" is stale — it exists now.)

---

## 3. Findings — bugs

### F1 (bug, user-reachable): multi-node device params are misaddressed — `ParamDescriptor.id`
**Where:** `crates/devices/src/catalog.rs` (`describe`), pinned by `descriptors_carry_engine_truth`.

`describe()` sets a param descriptor's `id` to the **node-local** `ParamId` (`id: p.id.0`), while
ports and readouts use the **enumerated position** (`id: i as u32`). Resolution is strictly
positional (`BuiltScene::param` does `handles.get(param_id as usize)`), and the UI passes the
descriptor id straight to `set_param` (`Panel.svelte` → `engine.ts`). For any single-node device
node-local id == position, so nothing is visibly wrong today. For the multi-node `channel_strip`
(two `GainStage`s) the exposed params are positions `[0,1,2,3]` but the descriptor emits
`[0,1,0,1]`:

- The UI addresses the *input* stage's gain/power when the user turns the *output* stage's knobs;
  positions 2/3 are unreachable.
- The duplicate ids also collide as Svelte keyed-each keys (`{#each params as p (p.id)}`).

Compounding it, `descriptors_carry_engine_truth` asserts `pd.id == ep.id.0` — **a test actively
defends the defect** — and the doc comments already disagree with each other (`ParamDescriptor.id`
documented as "index in the exposed param list"; the code matches neither doc consistently).

This nullifies the value of the chassis/multi-node seam for exactly the Epic-5 direction the
codebase is built toward. **Fix:** enumerate like ports/readouts (`id: i as u32`) and flip the
test to assert position.

### F2 (bug, minor): initial/post-swap param application bypasses drop accounting
**Where:** `crates/wasm-bindings/src/lib.rs` (`from_patch`, `render_quantum` swap-install).

Scene-load and post-swap initial params are applied with `let _ = self.params.set(...)`, bypassing
`push_param` — overflow past `PARAM_QUEUE_CAP` (256) is dropped **without** incrementing
`param_drops`. A large scene's initial load is precisely the case the cap comment worries about,
yet the health counter that is supposed to evidence it never moves. As device count grows toward
Epic 5, a big scene could silently load with stale defaults.

### F3 (latent coupling): loading-loss ↔ edge index correlation is positional across crates
**Where:** `crates/devices/src/build.rs` (records `graph.connection_count()` pre-wire, reads
`schedule.edge_gain(ei)` post-compile).

Works today and is tested, but it silently assumes `compile` preserves edge insertion order as
stable indices across internal-device edges and scene edges. Nothing on the engine side pins that
invariant; if `compile` ever reorders/dedups edges, every per-cable loss readout misaligns with no
error. Cheap fix: a documented contract + an engine-side test, or key edges explicitly.

---

## 4. Findings — structural risks, by horizon

### Near term (Stories 4.6 / 4.7)

- **R1 — `App.svelte` is a god component (1308 lines)** holding scene structural mutation, rack
  placement math, the cable-drag state machine, DOM jack measurement, the cable inspector, the
  param mirror/engine sync, transport bring-up, and persistence — plus inline render snippets.
  Specific pressure points:
  - Rack/placement legality (`rackOccupants`, `rackSlotAt`, `canPlace`) is real geometry logic
    living **untested in the view** instead of beside the tested `spatial.ts`.
  - Two independent global `<svelte:window>` pointer state machines (App's cable drag, WorldView's
    pan/drag) coordinate implicitly; 4.6 adds reach + grid-snap interactions on top.
  - The cable subsystem (~350 interleaved lines) is a self-contained feature begging to be a
    component/module.
  - A device-specific leak in the generic renderer: `device.typeId === "synth_voice"` special-cases
    the ADSR screen — Epic 5's device count will multiply these branches unless "screen" becomes a
    descriptor-driven concept.
  Decomposing *before* 4.6/4.7 land is markedly cheaper than after.
- **R2 — the front view is hardcoded at call sites.** `project()` supports top/side, but
  `App.svelte` passes `"front"` everywhere and `WorldView` bakes the elevation convention into
  layout (`bottom: {y}px`, `ROOM_HEIGHT - worldY`); rack rendering is front-only. 4.6 is not
  "a projection argument" — it needs a view-selection thread through App + WorldView. Known, but
  worth budgeting for at 4.6 planning.
- **R3 — the readout transport shape won't carry 4.7 waveforms.** Readings arrive as a whole-object
  `Record<string, number[]>` replaced ~47×/s (every meter re-renders whether changed or not), with
  no per-probe channel and no transferable buffers. Fine for scalars — and the plan already says
  4.7 is a distinct mechanism — this audit confirms none of the existing transport is reusable for
  it.

### Epic-5 horizon (breadth: many devices, bigger scenes)

- **R4 — the Rust↔TS contract is enforced by nothing structural.** `catalog.ts` / `scene.ts` are
  hand-written mirrors; catalog values deserialize straight into typed `$state` with no runtime
  validation. Roughly **25 fields across six types** (`ParamDescriptor`, `PortDescriptor`,
  `ReadoutDescriptor`, `FormFactor`, `CableType`, patch shapes) would break **silently** on a Rust
  rename — `undefined` at runtime, no type error, no test. The only wire-shape guard is a Rust test
  asserting camelCase on essentially one field. Also: both TS mirror header comments point at
  nonexistent files (`crates/wasm-bindings/src/{catalog,scene}.rs` — they live in
  `crates/devices/src/`). Options in rough order of cost: fix the stale pointers; add a Rust test
  serializing one full descriptor/catalog JSON fixture that the TS Vitest suite parses against its
  types (one shared fixture = a real cross-language contract test); or codegen (`tsify`) if the
  surface keeps growing.
- **R5 — catalog scale guards are missing.** At 7 devices these don't bite; at 30 they will:
  - No `type_id` uniqueness test — a copy-paste duplicate silently shadows the second entry.
  - Label alignment is length-checked only — a swapped/mislabeled knob passes CI.
  - Not every entry is instantiated + compiled in any test (`gain_stage`, `three_band_eq` never
    appear as devices in a patch test). A cheap "every catalog entry instantiates and compiles
    standalone" loop test closes this permanently.
- **R6 — per-entry catalog boilerplate** is ~60–90 lines of nested struct literals with positional
  hand-authored labels. Tolerable now; consider a small builder/macro once entries multiply, since
  positional authoring is exactly where R5's silent misalignment comes from.
- **R7 — structural-edit cost scales wrong (shape, not yet magnitude).** Every `hotSwap()` re-posts
  the entire patch and re-pushes **all params of all devices** (O(devices × params) per single
  cable change) into a fixed 256-slot queue (see F2). `measureJacks` does a document-wide
  `querySelectorAll` + `getBoundingClientRect` per jack, and its trigger `$effect` runs
  `JSON.stringify` over whole scene subtrees on every reactive pass. Lookups are O(n)
  `.find`/`.filter` throughout. All fine at 7 devices; quadratic-ish at venue scale. Nothing here
  needs fixing today — but it should be on the 5.2 profiling checklist.
- **R8 — `powered` exists in three flavors** (`GainStage` and `SynthVoice` near-verbatim
  param-gate copies; `CondenserMic` a structural `bool`). The plan already admits the
  framework-level gate is the right end state; every new Epic-5 device otherwise copies the pattern
  a fourth, fifth, sixth time. Do the extraction before seeding the device wave, not after.
- **R9 — build-time-parameterized devices are genuinely inexpressible today** (static
  `&'static [fn() -> Box<dyn Node>]`, no structural config on `DeviceInstance`). This is a
  *disclosed* gap with the extension named in module docs — fine — noted here only because the
  N-channel mixer is likely Story 5.1's first ask.

### Longer term

- **R10 — persistence has no migration story.** `parseScene` discards on any version mismatch.
  Correct while localStorage is disposable; it becomes user-hostile the moment a scene is worth
  keeping — any Epic-5 field addition wipes the saved studio. Adopt a migration step (or decide
  scenes are export-files) before scenes carry value.
- **R11 — worklet ↔ main-thread protocol is half-typed.** `ControlMessage` is a proper union on
  the TS side, but the worklet is plain JS re-implementing the switch stringly, and inbound
  messages are unchecked `as` casts. As the message set grows (4.7 probes, losses, health), a
  shared message-type module (or making the processor's logic a typed TS file that the build
  concatenates) would remove a silent-drift surface.
- **Deliberate deferrals verified as safe to stay deferred:** the SAB event ring (postMessage clean
  at human rates; seam is `EventQueue::push`), Story 2.3 golden files (numeric oracles hold the
  line), native↔WASM parity tests, clock domains (scaffolded), ground-topology hum (injection
  point clean). None of these is a hidden blocker.

---

## 5. Documentation health

- `IMPLEMENTATION_PLAN.md` is 1503 lines and growing: Epic 4's story detail lives inline, unlike
  Epics 1–3 which were compressed into `EPIC_N_NOTES.md` on completion. Doing the same extraction
  at Epic 4's exit will keep the plan navigable.
- Stale notes found: "no web CI job exists yet" (4.3 delivery notes — one exists now); the TS
  mirror header paths (R4); the `ParamDescriptor.id` doc comment contradicting both the code and
  `ParamSetting.id`'s doc (F1).
- Otherwise the plans and code agree remarkably well — nearly every "known simplification" claimed
  in the plan was verified present and honestly labeled in the code.

---

## 6. Recommended actions, ranked

1. **Fix F1** (param id = exposed position) + flip the pinning assertion. Small, real, and it
   unblocks the multi-node seam Epic 5 depends on.
2. **Add the cheap catalog guards (R5):** type_id uniqueness, every-entry-instantiates-compiles.
   Two small tests that make the whole Epic-5 device wave safer.
3. **Close the Rust↔TS seam (R4):** fix the stale pointers now; add a shared serialized-fixture
   contract test next.
4. **Fix F2** (count initial-load param drops) and consider sizing the queue from the patch.
5. **Decompose `App.svelte` (R1) before Story 4.6** — extract the cable subsystem and move rack
   placement math into the tested pure module.
6. **Extract framework-level `powered` (R8) before Story 5.1 seeds devices.**
7. **Pin the edge-index contract (F3)** with an engine-side test or explicit keying.
8. At Epic 4 exit: compress Epic 4 into `EPIC_4_NOTES.md`; put R7's scaling shapes on the 5.2
   profiling checklist; decide the persistence/migration story (R10) before scenes carry value.
