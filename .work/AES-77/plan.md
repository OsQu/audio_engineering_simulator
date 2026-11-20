# AES-77 — Make the web wouldCreateCycle check domain-aware

## Intent
Story 5.9 open decision (ii) / docs/IMPROVEMENTS.md: the web cycle check was device-level and
domain-blind, while the engine breaks digital/acoustic cycles with a delayed edge. Check whether the
Luma-loop fix (docs/bugs/2026-09-09-luma_loop.md) already covers it; close as done if so.

## Finding
Covered. `evaluateConnection` (web/src/connections.ts) no longer calls `wouldCreateCycle`; patch
edits are engine-validated transactions (patchAccepted/patchRejected), so the Rust compiler is the
only cycle oracle. One-way digital loops reach the engine, which breaks them at a digital/acoustic
edge and rejects all-analogue ones. A test (`submits a loop-closing drag ... for engine-level
validation`) pins this. IMPROVEMENTS.md already marks the item DONE (superseded).

Leftovers (cosmetic, no behaviour):
- `wouldCreateCycle` is exported but only used by its own tests ("retained for diagnostics").
- Stale comments: connections.ts duplex block says "the feedback-loop check below is deliberately
  skipped" (there is none); connections.test.ts:267 says a one-way loop "would be rejected by
  wouldCreateCycle".
- docs/EPIC_5_NOTES.md open decision (ii) not marked resolved.

## Slices
| # | Slice | Status |
|---|-------|--------|
| 1 | Verify coverage | done |
| 2 | Ask Oskari: close as-is vs. tidy leftovers | done (I-440) |
| 3 | Tidy: delete `wouldCreateCycle` + its tests, fix stale comments, mark (ii) resolved | done |

## Decisions (approved, I-440)
- Close as superseded by the Luma fix, with a tidy: delete the dead `wouldCreateCycle` and its 3
  unit tests, fix the two stale comments, mark EPIC_5_NOTES open decision (ii) resolved. Rationale:
  dead code that looks like a legality check invites someone to wire it back in.
- EPIC_8_NOTES' mention of `wouldCreateCycle` is left alone: it's a historical task record.
