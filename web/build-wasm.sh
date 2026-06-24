#!/usr/bin/env sh
# Build the worklet's wasm asset for the Vite harness (Story 3.2, Phase B). Run via `npm run wasm`.
#
# Produces, into web/public/ (Vite static assets, served from the web root and NOT processed by the
# bundler):
#   - processor.js               TextDecoder/TextEncoder polyfill + no-modules wasm-bindgen glue +
#                                processor implementation, concatenated in that order. The worklet is
#                                one classic script (AudioWorkletGlobalScope has no import/importScripts
#                                and constructs a TextDecoder eagerly), so it must all live in one file.
#   - wasm_bindings_bg.wasm      fetched at runtime by main.ts; compiled inside the worklet.
set -eu
cd "$(dirname "$0")" # web/

TMP="$PWD/.wasm-build"
mkdir -p public

echo "building no-modules wasm → $TMP"
# Absolute --out-dir: wasm-pack resolves a relative --out-dir against the crate dir, not the cwd.
wasm-pack build ../crates/wasm-bindings --target no-modules --release --out-dir "$TMP"

echo "concatenating polyfill + glue + processor → public/processor.js"
cat worklet/worklet-polyfill.js "$TMP/wasm_bindings.js" worklet/processor-impl.js >public/processor.js
cp "$TMP/wasm_bindings_bg.wasm" public/wasm_bindings_bg.wasm
rm -rf "$TMP"

echo "done. install deps and run the dev server:"
echo "  cd web && npm install && npm run dev   # then open http://localhost:5173/"
