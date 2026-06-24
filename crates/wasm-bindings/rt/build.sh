#!/usr/bin/env sh
# Build the real-time first-sound worklet artifact (Story 3.2, Phase A — static page).
#
# Produces rt/pkg/ (the `--target no-modules` wasm-bindgen glue + the wasm binary) and concatenates,
# into the served rt/processor.js: a TextDecoder/TextEncoder polyfill, then the glue, then the
# processor implementation. Why concatenate: AudioWorkletGlobalScope is a single classic script with
# no `import` and no `importScripts`, and a top-level `let` (how the no-modules glue defines
# `wasm_bindgen`) is not reliably shared across separate addModule() scripts — so everything must
# live in the *same* file, in dependency order. The polyfill must come first because the glue
# constructs a TextDecoder eagerly at load time and the worklet scope lacks it (see worklet-polyfill.js).
set -eu
cd "$(dirname "$0")/.." # crate root: crates/wasm-bindings

echo "building no-modules wasm → rt/pkg/"
wasm-pack build --target no-modules --release --out-dir rt/pkg

echo "concatenating polyfill + glue + processor → rt/processor.js"
cat rt/worklet-polyfill.js rt/pkg/wasm_bindings.js rt/processor-impl.js >rt/processor.js

echo "done. serve the crate dir and open rt/:"
echo "  python3 -m http.server 8000   # then open http://localhost:8000/rt/"
