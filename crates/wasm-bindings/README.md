# wasm-bindings — building the browser artifact

The engine compiled to `wasm32-unknown-unknown` with `wasm-bindgen` glue, consumed by the Epic-3
browser harness. Story 3.1 ships only the minimal compute-only surface for the feasibility
benchmark (see `src/lib.rs`).

## Prerequisites (one-time, user-run system installs)

The `wasm32-unknown-unknown` target is already pinned in `rust-toolchain.toml`. You also need
`wasm-pack`, which is a network installer / system-level tool — install it yourself:

```sh
cargo install wasm-pack          # or: brew install wasm-pack
```

`wasm-pack` reads `Cargo.lock` and **auto-fetches a `wasm-bindgen-cli` matching the `wasm-bindgen`
crate version** (currently `0.2.125`) — so the crate↔CLI versions can't drift. Do **not** hand-
install a separate `wasm-bindgen-cli`; let `wasm-pack` manage it.

## Build

Release artifact for the browser (ES modules), written to `crates/wasm-bindings/pkg/`:

```sh
# Scalar baseline.
wasm-pack build crates/wasm-bindings --target web --release

# SIMD build — this is the real deployment; the benchmark compares the two to show the SIMD win.
RUSTFLAGS="-C target-feature=+simd128" wasm-pack build crates/wasm-bindings --target web --release
```

`+simd128` is passed via `RUSTFLAGS` (not baked into `.cargo/config.toml`) precisely so both
variants are buildable from explicit commands. The release profile is `panic = "abort"`
(workspace-wide) — a panic in an AudioWorklet kills the stream, so we abort instead of unwind.

## Portability gate (no wasm-pack needed)

`cargo wasm` (the workspace alias) type-checks `engine + capture + wasm-bindings` for `wasm32`
without building an artifact — it runs in the normal pre-push gate and in CI. The full
`wasm-pack build` (which also runs `wasm-bindgen` + `wasm-opt`) is what catches *bindgen* breakage
a bare `cargo check` can't; it runs as a dedicated CI step.
