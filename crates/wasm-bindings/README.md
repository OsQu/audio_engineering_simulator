# wasm-bindings — building the browser artifact

The engine compiled to `wasm32-unknown-unknown` with `wasm-bindgen` glue, consumed by the browser
harness in [`web/`](../../web). The JS-facing surface (see `src/lib.rs`) is **`SceneEngine`** — the
real-time engine the AudioWorklet drains one quantum at a time: built from a serialized `Patch` (via
the `devices` crate), controlled generically by device id, hot-swapped to a new scene at a block
boundary, and exposing its captured host block **zero-copy** (`out_ptr`/`out_len`) for a
`Float32Array` view over wasm memory. Plus the thin catalog bridge (`catalog`, `parse_patch`).

(Through Epic 3 the real-time surface was `RtEngine`, hardcoded to the canonical patch; Story 4.1
generalized it to `SceneEngine`. The Story 3.1 compute-only feasibility-benchmark fixture
`BenchEngine` and its throwaway `bench/` page were removed once Epic 3's gate was passed — recover
them from git history if the scaling probe is wanted again at Epic-5 scale.)

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

The browser artifact is built by the `web/` harness, which wraps `wasm-pack` so the output lands
where Vite expects it:

```sh
cd web && npm install && npm run wasm   # → web/build-wasm.sh
npm run dev                             # then open http://localhost:5173/
```

`build-wasm.sh` runs `wasm-pack build crates/wasm-bindings --target no-modules --release` and
concatenates a `TextDecoder`/`TextEncoder` polyfill + the no-modules glue + the worklet processor
into `web/public/processor.js`. Two seams worth knowing:

- **`--target no-modules`, not `--target web`.** `AudioWorkletGlobalScope` has no `import` /
  `importScripts` and no reliable `fetch`, so the worklet must be one classic script (a top-level
  `let` is not reliably shared across separate `addModule()` scripts). The polyfill comes first
  because the worklet scope also lacks `TextDecoder`/`TextEncoder` (a Chrome gap) and the glue
  constructs a `TextDecoder` eagerly at load time (only ever used for panic text).
- **The wasm crosses as raw bytes via `processorOptions`.** The main thread fetches the `.wasm`
  bytes (the worklet has no reliable `fetch`) and hands the `ArrayBuffer` + the runnable patch to the
  processor through `processorOptions`; the processor compiles + instantiates synchronously in its
  constructor (`initSync`) and builds `SceneEngine(patch)` — no init handshake. We pass *bytes*, not
  a compiled `WebAssembly.Module`: a Module is only structured-cloneable within one agent cluster and
  an AudioWorklet is a separate realm (cloning it in can silently fail). The `AudioContext` is pinned
  to `sampleRate: 48000` (the engine's host rate) so quanta line up 1:1 (1024 analog ÷ M=8 = 128 =
  one render quantum); the mono block is duplicated to all channels.

The release profile is `panic = "abort"` (workspace-wide) — a panic in an AudioWorklet kills the
stream, so we abort instead of unwind. `+simd128` is passed via `RUSTFLAGS` for the deployment build.

## Portability gate (no wasm-pack needed)

`cargo wasm` (the workspace alias) type-checks `engine + capture + wasm-bindings` for `wasm32`
without building an artifact — it runs in the normal pre-push gate and in CI. The full
`wasm-pack build` (which also runs `wasm-bindgen` + `wasm-opt`) is what catches *bindgen* breakage
a bare `cargo check` can't; it runs as a dedicated CI step.
