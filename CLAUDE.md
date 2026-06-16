# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## 1. Current state

This repo is **pre-code** — two planning documents and one commit, no source tree yet. The next thing
to build is **Story 1.1** (Cargo workspace scaffold) in `IMPLEMENTATION_PLAN.md`. Until it lands there
is no build, test runner, or crate. The Rust conventions below are written *ahead* of the code on
purpose, so the workspace grows in the right direction from the first commit.

When real infrastructure exists, this file should gain a concrete **Commands** section (exact `cargo`
invocations, how to run one test) — re-run `/init` after Story 1.1.

## 2. How we work (the task loop)

This governs every body of work:

1. **Create tasks** for the work before starting.
2. After completing a task, make sure it **compiles, lints, and passes tests** before reporting it done.
3. **Do not commit.** Stop and let the user verify what was done.
4. **Discuss and follow up** on any changes together — the user commits the code himself.
5. When the user says he has verified and committed, **review his commit message** to confirm it
   accurately reflects the work.

Never run `git commit` unless explicitly asked. Committing is the user's verification gate, not a
mechanical step.

**System-modifying commands are the user's to run** — package installs, `brew`, global toolchain/config
changes, network installers. Surface the exact command and let the user run it (via `! <command>`).
Editing repo files and running project-local tooling (`cargo build`/`test`, etc.) is normal work.

> Toolchain note: this tool's non-interactive shell doesn't source `~/.zshenv`, so `cargo` isn't on
> PATH by default. Prefix cargo invocations with `source "$HOME/.cargo/env" &&`. Rust is managed by
> rustup (stable 1.96+); the `wasm32-unknown-unknown` target is installed.

## 3. Source of truth: read the plans first

Two documents govern everything. Keep them authoritative; update them when a decision changes.

| Doc | Role |
| --- | --- |
| `PROJECT_PLAN.md` | The *what and why* — vision, domain model, engine design, staged roadmap (§9), risks. |
| `IMPLEMENTATION_PLAN.md` | The *order and granularity* — Epic → Story → Task. Epic 1 is detailed to Task level; later epics stay coarse on purpose. |

Before working a task: find it in `IMPLEMENTATION_PLAN.md` and honor its **Goal / Watch out / Validate**
notes — they encode decisions and traps not recoverable from code. Each Story ends with a **Validate**
gate; don't start the next Story until it's green.

## 4. What this project is

A headless-first, voltage-native audio-engineering simulator. The central idea: in the analog domain the
signal **is a real oversampled voltage waveform in volts**, not a buffer with metadata. Levels, impedance
loss, clipping, noise, DC, phantom power, and hum must **emerge from the voltage physics** — never be
flagged or special-cased. Digital samples exist only *after* a modeled AD converter. **Derive everything
from the physical (volts) model, never the reverse.**

## 5. Architecture decisions — non-negotiable

Settled in design; these constrain every epic. Violating one is a bug, not a style choice.

- **Engine in Rust** (native for dev/test, `wasm32` + SIMD for the browser). **UI in TypeScript**, a pure
  consumer of the published engine API — never reaching into engine internals.
- **Two distinct signal types, never conflated:** `VoltageBuffer` (oversampled, linear volts, analog) vs
  `SampleBuffer` (digital, dBFS). AD/DA converters are the *only* bridge. (`SampleBuffer` and the digital
  domain don't exist until Story 1.6.)
- **Block-based, pull-based core:** `compile(graph) -> Schedule`, then `schedule.process(...)` — one code
  path for offline *and* real-time. Offline render is a test harness; **real-time interactive is the north star.**
- **Local solve only:** Thévenin source + voltage divider + cable R·C. No global nodal/SPICE solve — pro
  devices buffer their I/O, so connections solve locally. The schedule is a partitionable DAG.
- **Two input lanes:** smoothed continuous **control params** (knobs, de-zippered) and sample-accurate
  timestamped **events** (note-on/off, gate). Keep them genuinely separate.
- **Deterministic given a seed** (noise, hum phase) so tests and golden-file renders are stable.

## 6. Rust engineering conventions

These flow from the decisions above and should hold from day one.

### Crate layout (Task 1.1.1)
A Cargo workspace: `engine` (core lib) now, plus placeholder `wasm-bindings` and a render/CLI
test-harness crate so the shape doesn't churn later. Platform-specific code is gated and kept out of
`engine` core; the core stays portable to `wasm32`.

### Type design
- **Units are newtypes.** `Volts` (and peers) are distinct types, not bare `f32`. No implicit numeric
  conversion between domains — conversions are explicit, named helpers (e.g. `dbu_to_volts`), tested
  against hand calcs.
- **The signal-type split is the spine of correctness.** Never add a path that turns a `VoltageBuffer`
  into a `SampleBuffer` (or shares their rate) outside an AD/DA converter.
- **Buffers store linear values.** dB / dBFS are *measurement units* produced by conversion helpers,
  never a storage format. A buffer holding dB is a category error.

### Hot-path discipline (the `process` path)
Non-negotiable because a panic or glitch in a WASM AudioWorklet kills the audio stream. Win this from
day one while still headless — retrofitting it is painful.
- **No allocation.** Pre-allocate all buffers/scratch in a pool/arena at `compile` time; `process` only
  reads and writes them. No `Vec` growth, `Box`, `format!`, or collection building in the loop.
- **No panics.** No `unwrap`/`expect`/`panic!`/indexing that can panic, no `Result` returns. All fallible
  validation happens at construct/compile time; `process` is total.
- **Lock-free.** Cross-thread lanes (params, events, schedule swap) use lock-free structures; no `Mutex`
  on the audio path.
- **Flush denormals**, and prefer branch-light arithmetic in inner loops.
- Validation, allocation, and error reporting live in **graph construction and `compile`** — the two
  places that *are* allowed to be fallible and allocate.

### Determinism
All randomness flows through the seeded RNG abstraction (uniform + Gaussian, splittable per-device).
**No `thread_rng`, no ambient `Instant::now`/`SystemTime`** in the engine — they break reproducibility
and WASM portability. Same seed ⇒ identical output.

### Testing
- `approx` for float assertions; never `==` on floats.
- Analog-domain tests assert a number **computed by hand, with the hand calc in a comment** — tests are
  the oracle there (you can't hear cable loss or impedance ratios).
- Determinism makes golden-file render tests viable later (Epic 2).

## 7. Numeric & rate model — settled

- **One fundamental clock: the analog rate** — the proxy for "continuous." A parameter, never a constant.
  There is **no global oversample factor and no global digital base rate.** Digital sample rates are
  **per-converter, emergent**, stamped onto data by the AD that produced it. Crossing any clock boundary
  (analog→digital, or digital→digital between mismatched converters) is a *resample*. Don't build a type
  that can express a global analog↔digital rate relationship — there isn't one.
- **Scalar policy:** `f32` storage; reach for `f64` only where precision demands it (accumulators —
  summing nodes, filter state, the AD anti-alias filter).

## 8. Workflow conventions

- **Branches:** one per **Story**, `e<epic>-s<story>/<short-story-slug>` (e.g. `e1-s2/electrical-primitives`).
  A Story's Tasks are commits on that branch; PR (or fast-forward) to `main` and delete on merge once
  the Story's *Validate* gate is green.
- **Cargo aliases** (in `.cargo/config.toml`) shortcut the commands with fiddly flags:
  - `cargo lint` → clippy, all targets, warnings-as-errors
  - `cargo wasm` → wasm32 portability check (engine + bindings)
  - `cargo test` and `cargo fmt --check` are used as-is (already short; `fmt` isn't aliasable)
  - **Full pre-push gate** (mirrors CI): `cargo fmt --check && cargo lint && cargo test && cargo wasm`
- **CI** (`.github/workflows/ci.yml`) runs those same four gates plus `Swatinem/rust-cache`. WASM
  release profile is `panic = "abort"`; crate-level lints deny `clippy::all` and `unsafe_code`.

## 9. Validation philosophy

Audio is the ground-truth oracle for DSP (you can hear wrong dynamics/filters). **Tests are the oracle for
the analog domain** — cable loss, impedance ratios, noise floors, and calibration can't be heard reliably,
so they're asserted numerically against hand calculations.
