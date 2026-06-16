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

> **Status:** early. The headless Rust engine is being built and validated first (Epic 1); the UI comes
> later as a pure consumer of the engine API. See the plans below.

## Documentation

- **[`PROJECT_PLAN.md`](PROJECT_PLAN.md)** — the *what and why*: vision, domain model, engine design, roadmap.
- **[`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md)** — the *order and granularity*: Epic → Story → Task.
- **[`CLAUDE.md`](CLAUDE.md)** — engineering conventions and the non-negotiable architecture decisions.

## Project structure

A Cargo workspace:

| Crate | Role |
| --- | --- |
| `crates/engine` | The core voltage engine (portable to `wasm32`). |
| `crates/wasm-bindings` | Browser/WASM bindings (placeholder until Epic 3). |
| `crates/harness` | Render/CLI test harness (placeholder until Epic 2). |

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
cargo wasm           # wasm32 portability check (engine + bindings)
cargo test           # run all tests
cargo fmt --check    # formatting check (drop --check to apply)
```

Run the full gate before pushing (this is exactly what CI runs):

```sh
cargo fmt --check && cargo lint && cargo test && cargo wasm
```
