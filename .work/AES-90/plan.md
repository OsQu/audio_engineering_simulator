# AES-90 — Codebase hygiene (behavior-neutral)

## Intent
Apply the audit's behavior-neutral hygiene findings: remove project-plan ids from source, guard against
their return, delete dead code / narrow visibility, split the giant files, fix the README. No behavior
change: every gate stays green, `cargo test` runs the same tests (534 before; list in
`tests-before.txt`), web Vitest 423 before.

## Approach
One commit per ticket section, in order. Section 1 is a comment pass (two parallel helpers: crates/ and
web/), reviewed by me before commit. Sections 3–4 by hand; file splits are pure moves.

## Slices
| # | Slice | Status |
| --- | --- | --- |
| 1 | Remove plan refs from source / test names; SCHEMA_VERSION one-liner | done |
| 2 | Guard: `scripts/check-plan-refs.sh`, CI step, CLAUDE.md rule + pre-push gate | done |
| 3 | Dead code, visibility narrowing, stale comments, duplicated web helpers, clock rename | done |
| 4 | File splits: catalog.rs, build.rs tests, schedule/tests.rs phenomena, `test_util::rate()` | done |
| 5 | README status + doc links | done |

## Decisions
- The audit file `ticket-refs-in-source.txt` wasn't available in the worktree; the list was regenerated
  with the guard's own grep (215 lines incl. `§` refs) plus a bare-dotted-id sweep.
- The guard greps every file under the listed dirs (incl. crate READMEs / Cargo.toml), per the ticket.

- Engine visibility: only `drain_ring` and `PeakEnvelope` narrowed; the rest have external users
  (`tests/no_alloc.rs`) or are intended API (I-284).
- `PORTAL_LEN` and `pcmToFloat32` stay exported: they have real importers/tests (I-285).
- Phenomena move keeps three modules in-crate (they read private `Schedule` fields); moved modules
  get `tests/phenomena/support.rs` with copied helpers (I-286).
- Section 4 lands as three commits (catalog split, build tests, phenomena + `rate()`), one move each.
- `instantiate` / `build_asset_node` are test-only → `#[cfg(test)]`; their seam docs moved to
  `instantiate_with_assets`.

## Open questions
- `benchScene` builds the DUT alone, yet `defaultBenchTap` can fall back to `spk`, which isn't in that
  scene. Behavior, so untouched here; flagged in the PR.
