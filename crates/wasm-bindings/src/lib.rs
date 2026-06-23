//! Browser/WASM bindings for the engine.
//!
//! Epic 3 hosts the engine in the browser. This crate is the JS-facing surface, built to
//! `wasm32-unknown-unknown` with `wasm-bindgen` (via `wasm-pack`). See the crate `README.md` for
//! the build + install commands.
//!
//! **Scope (Story 3.1):** only the *minimal compute-only* surface needed for the
//! faster-than-real-time **feasibility benchmark** — the gate that decides whether the oversampled
//! voltage chain can run inside an AudioWorklet. The benchmark loops the engine entirely inside
//! WASM and is timed from JS, so there is **no per-quantum marshalling and no `unsafe`** here yet.
//! The zero-copy raw-memory `process` hot path (a `Float32Array` view over linear memory) is a
//! Story 3.2 concern, built when the worklet actually drains output every quantum.
//!
//! Task 3.1.2 stands up the build pipeline + this smoke export; Task 3.1.3 adds the real
//! `render_blocks` compute surface.

use wasm_bindgen::prelude::*;

/// Smoke export: proves the `wasm-bindgen` pipeline produces working glue **and** that the
/// `engine` crate links into the WASM artifact (it touches an engine type). Returns the canonical
/// analog rate in Hz. Joined by the real `render_blocks` compute surface in Task 3.1.3.
#[wasm_bindgen]
#[must_use]
pub fn canonical_analog_rate_hz() -> f64 {
    engine::AnalogRate::new(384_000.0).as_hz()
}
