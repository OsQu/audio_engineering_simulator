# wasm-bindings — building the browser artifact

The engine compiled to `wasm32-unknown-unknown` with `wasm-bindgen` glue, consumed by the Epic-3
browser harness. Two surfaces (see `src/lib.rs`): `BenchEngine` — the frozen Story 3.1 compute-only
feasibility-benchmark fixture; and `RtEngine` — the Story 3.2 real-time surface the AudioWorklet
drains one quantum at a time, exposing its captured host block zero-copy (`out_ptr`/`out_len`) for a
`Float32Array` view over wasm memory.

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

## Feasibility benchmark (the Story 3.1 gate)

`bench/` is a **throwaway** static page that loads the artifact and times `BenchEngine::render_blocks`
in a `performance.now()` loop — the gate that decides whether the oversampled chain can run inside an
AudioWorklet (and so picks the 3.2 execution model). It is *not* the Epic-4 UI and uses no bundler.

```sh
sh crates/wasm-bindings/bench/build.sh   # builds pkg-scalar/ + pkg-simd/ (release)
cd crates/wasm-bindings && python3 -m http.server 8000
# open http://localhost:8000/bench/  →  click "Run benchmark"
```

It reports the realtime ratio (throughput headline), per-quantum mean/max against the ~2.667 ms
quantum budget, and a verdict, for both the scalar and `+simd128` builds side by side. No COOP/COEP
needed (no `SharedArrayBuffer` until 3.4). The `pkg*` dirs are gitignored; the page + `build.sh` are
tracked.

## Real-time first sound (Story 3.2, Phase A)

`rt/` is a **throwaway** static page (no bundler — the Vite/TS harness is Phase B / Story 3.2.5) that
plays the canonical patch live in an `AudioWorkletProcessor`, drained one quantum at a time from
`RtEngine::render_quantum`.

```sh
sh crates/wasm-bindings/rt/build.sh      # builds rt/pkg/ + concatenates rt/processor.js (release)
cd crates/wasm-bindings && python3 -m http.server 8000
# open http://localhost:8000/rt/  →  click "start" (a user gesture is required to begin audio)
```

Two seams worth knowing:
- **`--target no-modules`, not `--target web`.** `AudioWorkletGlobalScope` has no `import` /
  `importScripts` and no reliable `fetch`. `build.sh` **concatenates** `worklet-polyfill.js` +
  no-modules glue + `processor-impl.js` into the served `processor.js`, in that order (a top-level
  `let` is not reliably shared across separate `addModule()` scripts, so it must all be one file).
  The polyfill comes first because the worklet scope also lacks `TextDecoder`/`TextEncoder` (a Chrome
  gap) and the glue constructs a `TextDecoder` eagerly at load time — only ever used for panic text.
- **The wasm crosses as raw bytes via `processorOptions`.** The main thread (`main.js`) fetches the
  `.wasm` bytes (the worklet has no reliable `fetch`) and hands the `ArrayBuffer` to the processor
  through `new AudioWorkletNode(…, { processorOptions: { bytes } })`; the processor compiles +
  instantiates them synchronously in its **constructor** (`initSync`) — no init message, no
  ready/error handshake. We pass *bytes*, not a compiled `WebAssembly.Module`: a Module is only
  structured-cloneable within one agent cluster and an AudioWorklet is a separate realm (cloning it in
  can silently fail), and recompiling from bytes in the worklet is the recommended approach anyway. The
  `AudioContext` is pinned to `sampleRate: 48000` (the engine's host rate) so quanta line up 1:1 (1024
  analog ÷ M=8 = 128 = one render quantum); the mono block is duplicated to all channels.

No COOP/COEP needed (no `SharedArrayBuffer` until 3.4). `rt/pkg/` and the generated `rt/processor.js`
are gitignored; `index.html` / `main.js` / `processor-impl.js` / `build.sh` are tracked.

## Portability gate (no wasm-pack needed)

`cargo wasm` (the workspace alias) type-checks `engine + capture + wasm-bindings` for `wasm32`
without building an artifact — it runs in the normal pre-push gate and in CI. The full
`wasm-pack build` (which also runs `wasm-bindgen` + `wasm-opt`) is what catches *bindgen* breakage
a bare `cargo check` can't; it runs as a dedicated CI step.
