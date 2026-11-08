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

## 2026-11-08 — merge origin/main (ship conflict)
- **Picked up:** branch was finished; the human asked to merge origin/main (12 commits: per-instance
  descriptors, ScheduleSlot removal, allocation-free record drain, bench speaker fix, worklet teardown).
- **Conflicts & resolution:**
  - `devices/build.rs` + `catalog.rs`: main edited code this branch had moved. Took main's build.rs, re-cut
    its tests into `build/tests.rs` (3-way merged with the branch's moved tests → main's two new
    computer tests landed cleanly), re-applied the one doc-link change. Ported main's
    `instantiate_described` / `describe_face` into `catalog/expand.rs`. `instantiate_with_assets` lost
    its only production caller, so it is now a `#[cfg(test)]` wrapper; seam docs + intra-doc links
    moved to `instantiate_described`.
  - `engine/daw.rs`, `lib.rs`: took main's `drain_ring_into`, made `pub(crate)` (no external users,
    same as the `drain_ring` it replaces); `PeakEnvelope` stays crate-private.
  - `schedule/swap.rs`: main deleted it; deletion kept (the branch only had the `rate()` swap there).
  - `wasm-bindings/lib.rs`: main's `descriptors_to_js` replaces `describe_device`.
  - web: `connectionConnectors` exported again (main's new test imports it); `benchScene` doc from
    main (it now adds the speaker); `wireMidi` imports dropped (main moved it onto the session);
    `fmtReading`/`dawUi` kept; orphan session comment dropped.
- **Verified:** all Rust gates + plan-refs + web format/check/typecheck/test green. 536 Rust tests =
  534 − 2 (main removed swap.rs) + 4 (main's new); Vitest 429.
- **Learned:** local `main` was fast-forwarded mid-merge by another process — diff against
  `git merge-base HEAD MERGE_HEAD`, not `main`.
