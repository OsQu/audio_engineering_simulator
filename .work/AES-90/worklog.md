# AES-90 worklog

## 2026-11-08 — sections 1–3
- **Did:** baseline 534 Rust tests (`tests-before.txt`), 423 Vitest. Section 1 (plan refs) done by two
  parallel helpers (crates/, web/), reviewed and committed. Section 2: `scripts/check-plan-refs.sh`,
  CI step, CLAUDE.md rule + pre-push gate. Section 3: visibility narrowing, dead web exports, tokens.ts
  deleted, fmtReading/parseScene/dawUi dedup, per-device clock consts, stale headers.
- **Learned:** the audit's "no external users" list was partly wrong: `ByteRing` (via
  `MultitrackRecorder::playback_ring_mut`) and `PassiveSum`/`BalancedDriver`/`DcBlocker` are used by
  `engine/tests/no_alloc.rs`; `PORTAL_LEN` is imported by cable-resolution.ts; `pcmToFloat32` is unit
  tested directly. Kept those public. `instantiate`/`build_asset_node` are test-only → `#[cfg(test)]`.
  `benchScene` builds the DUT alone, yet its fallback tap names `spk`, which isn't in that scene —
  left as is (behavior), flagged in the PR.
- **Next:** section 4 file splits, then README.

## 2026-11-08 — sections 4–5, done
- **Did:** catalog.rs split (98 lines now; 18 per-device files + spec/descriptor/expand/guitar/tests),
  build.rs tests → build/tests.rs, 11 public-API phenomena modules → `engine/tests/phenomena/`,
  `test_util::rate()` replaces 30 copies, README status/links fixed.
- **Verified:** all gates green (fmt, lint, test, wasm, docs, check-plan-refs; web format/check/
  typecheck/test). `cargo test` runs 534 tests with identical names before/after
  (`tests-before.txt` vs `tests-after.txt`, compared by leaf name since module paths moved).
  Vitest 423 before/after.
- **Learned:** the catalog split was clean as a scripted line-range cut plus generated imports; the
  only manual work was `pub(super)` on spec fields and re-pointing intra-doc links.
- **Next:** none — PR draft written.
