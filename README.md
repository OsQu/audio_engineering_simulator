# Audio Engineer Simulator

A gamified "digital twin" of the audio-engineering medium. You build, wire, and operate realistic
signal chains — instruments → analog gear → across the AD/DA boundary → into the digital domain — and
the engine faithfully models the *signal between devices*.

The core idea: in the analog domain the signal **is a real voltage in a wire** — a time-varying `v(t)`
in actual volts, oversampled as a proxy for "continuous" — not a buffer with metadata. Levels,
impedance loss, clipping, noise, DC offset, phantom power, and hum aren't flags; they **emerge from the
voltage physics**. Digital samples exist only *after* a modeled AD converter samples that voltage.

The purpose is **learning** — building hands-on intuition about routing, the analog medium, AD/DA, and
DSP that's impractical to explore with real gear.

> **Status:** see [`docs/IMPLEMENTATION_PLAN.md`](docs/IMPLEMENTATION_PLAN.md) for what's done and
> what's next.

## Documentation

- **[`docs/PROJECT_PLAN.md`](docs/PROJECT_PLAN.md)** — the *what and why*: vision, domain model, engine design, roadmap.
- **[`docs/IMPLEMENTATION_PLAN.md`](docs/IMPLEMENTATION_PLAN.md)** — the *order and granularity*: Epic → Story → Task.
- **[`CLAUDE.md`](CLAUDE.md)** — engineering conventions and the non-negotiable architecture decisions.

## Project structure

A Cargo workspace (plus a TypeScript web harness):

| Crate / dir | Role |
| --- | --- |
| `crates/engine` | The core voltage engine (portable to `wasm32`; serde-free, UI-free). |
| `crates/devices` | Product/content layer: device catalog + scene IR + `build_patch` (engine + serde). |
| `crates/capture` | The implicit off-sim-clock capture (speaker volts → host samples); shared by harness + wasm. |
| `crates/wasm-bindings` | Browser/WASM bindings — `SceneEngine` + the catalog/patch JS bridge. |
| `crates/harness` | Native render/CLI test harness (offline WAV render + the waveform-plot demo). |
| `web/` | Vite + TypeScript browser harness that hosts the engine in an AudioWorklet. |

## Setup

**Prerequisite: Rust via [rustup](https://rustup.rs).** If you don't have it:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

The pinned toolchain, components (clippy, rustfmt), and the `wasm32-unknown-unknown` target are declared
in [`rust-toolchain.toml`](rust-toolchain.toml) and installed automatically by rustup on first build.

```sh
git clone <repo-url> && cd audio_engineer_simulator
cargo build          # builds the workspace (installs the toolchain on first run)
cargo test           # runs the test suite
```

## Common commands

Convenience aliases live in [`.cargo/config.toml`](.cargo/config.toml):

```sh
cargo lint           # clippy across all targets, warnings-as-errors
cargo wasm           # wasm32 portability check (engine + capture + bindings)
cargo test           # run all tests
cargo docs           # doc build with broken-intra-doc-link / bare-URL lints denied
cargo fmt --check    # formatting check (drop --check to apply)
```

Run the full gate before pushing (this is exactly what CI runs):

```sh
scripts/check-plan-refs.sh && cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs
```

`scripts/check-plan-refs.sh` keeps project-plan ids (Story/Task/Epic numbers, plan-doc section refs) out
of source, comments and test names — they belong in `docs/` and commit messages.

To run the browser harness (the live engine in an AudioWorklet):

```sh
cd web && pnpm install && pnpm run wasm && pnpm run dev   # then open http://localhost:5173/
```

### Complexity metrics

`scripts/rust-code-analysis.sh` runs a pinned
[rust-code-analysis-cli](https://github.com/mozilla/rust-code-analysis) (per-function cognitive and
cyclomatic complexity, Rust and TypeScript). The binary is installed into a version-specific cache
directory shared by all checkouts, so everyone measures with the same version:

```sh
scripts/rust-code-analysis.sh --install                     # once (~/.cache/rust-code-analysis-cli/<version>)
scripts/rust-code-analysis.sh -m -O json -p crates -p web/src   # JSON metrics, one line per file
```

The exact scan (paths and test excludes) and the complexity limits live in `fx.toml`'s `[quality]`.
Svelte components (`.svelte`) aren't analyzed.
