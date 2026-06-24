#!/usr/bin/env sh
# Build both WASM variants the feasibility benchmark page compares: a scalar baseline and the real
# +simd128 deployment build. Each goes to its own out-dir (both gitignored) so index.html can load
# them side by side. Release profile (panic=abort, wasm-opt) — the numbers must reflect deployment.
#
# Then serve this crate dir and open the page, e.g.:
#   cd crates/wasm-bindings && python3 -m http.server 8000
#   open http://localhost:8000/bench/
set -eu

# Run from the wasm-bindings crate dir regardless of where this is invoked.
cd "$(dirname "$0")/.."

echo "building scalar → pkg-scalar/"
wasm-pack build --target web --release --out-dir pkg-scalar

echo "building +simd128 → pkg-simd/"
RUSTFLAGS="-C target-feature=+simd128" wasm-pack build --target web --release --out-dir pkg-simd

echo "done. serve this dir and open bench/:"
echo "  python3 -m http.server 8000   →   http://localhost:8000/bench/"
