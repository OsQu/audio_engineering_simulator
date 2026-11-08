Codebase hygiene: drop plan ids from source, prune dead code, split the giant files

Behavior-neutral cleanup from the codebase audit. All gates are green, and `cargo test` ran the same 534 tests, with the same names, before and after the hygiene commits (536 after merging main, which added 4 tests and removed 2). Vitest runs 423 before and after. Nothing a user sees changes. The WASM catalog JSON is unchanged, so no `pnpm run wasm` is needed.

## Commits (one per section; review them in order)
1. **Remove project-plan references.** Story/Task/Epic ids, bare ids (`pre-4.6`), `PROJECT_PLAN §x` refs and `osku_*` pointers are gone from `crates/` and `web/`. Comments now say what the code does, in the present tense. A plan section ref became the name of its concept, e.g. "the clock-domain rule" or "the DC operating point". The `SCHEMA_VERSION` changelog is one line now. A few web test titles lost their ids. Approved-decision ids (U1/U2/U3) were treated as plan ids too.
2. **Guard.** `scripts/check-plan-refs.sh` runs as a CI step and in the documented pre-push gate. CLAUDE.md §6 gets a "No project-plan ids in code" rule.
3. **Dead code and stale comments.**
   - **Engine:** `drain_ring` and `PeakEnvelope` become crate-private.
   - **Devices:** `instantiate`, `build_asset_node`, `AssetNodeBuilder` and `gm_preset` are no longer re-exported. `instantiate` and `build_asset_node` are test-only now.
   - **Catalog clocks:** `HOST_RATE_HZ` is replaced by per-device converter clocks (`SCARLETT_CLOCK_HZ`, `COMPUTER_CLOCK_HZ`, …). The values are unchanged.
   - **Web:** `tokens.ts` is deleted, and some unused exports and the dead workbench cast are removed. Duplicated helpers now have one copy each: `fmtReading`, `parseScene` reuse, and `session.dawUi()`.
4. **File splits, as pure moves (three commits):**
   - `catalog.rs` goes from 3.8k lines to 98. Each device gets its own `catalog/<type_id>.rs`, alongside `spec.rs`, `descriptor.rs`, `expand.rs`, `guitar.rs` and `tests.rs`.
   - `build.rs` tests move to `build/tests.rs`.
   - Public-API physics tests move to `crates/engine/tests/phenomena/`.
   - A shared `test_util::rate()` replaces 30 copies.
5. **README:** the status block now points at `docs/IMPLEMENTATION_PLAN.md`, and the doc links point into `docs/`.

## Where the audit list was wrong (kept on purpose)
- **`ByteRing`, `PassiveSum`, `BalancedDriver`, `DcBlocker`:** `crates/engine/tests/no_alloc.rs` uses these through the public API, so they stay public.
- **`dbv_to_volts`/`volts_to_dbv`/`headroom_db`, `Thevenin`, `AcousticFace`/`AudioFormat`:** kept public as intended API. The face types are returned by public `InputPort`/`OutputPort` methods.
- **`PORTAL_LEN`:** `cable-resolution.ts` imports it.
- **`pcmToFloat32`:** `waveform.test.ts` tests it directly.

## Look closely at
- **The phenomena move.** `converter_phenomena`, `acoustic_feedback` and `acoustic_seam` stay in `schedule/tests.rs` because they read private `Schedule` fields. The moved tests get `tests/phenomena/support.rs`, which has about 110 lines of helpers copied from `test_util`. The in-crate `test_util` is `#[cfg(test)]`, so integration tests can't reach it.
- **Catalog visibility.** Spec types and their fields are now `pub(super)` so that sibling files can build entries. Nothing new leaves the crate.
- **`session.dawUi(device)`.** It captures `this` as `session` because the getters run with the returned object as `this`. It behaves the same as the old per-view `dawFor`.

## Merged with main
main's per-instance descriptors were ported onto the split catalog: `instantiate_described` lives in `catalog/expand.rs`, and `instantiate_with_assets` is now a test-only wrapper. The new `drain_ring_into` is crate-private, like the `drain_ring` it replaced. The bench speaker issue noted in an earlier draft was fixed on main.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
