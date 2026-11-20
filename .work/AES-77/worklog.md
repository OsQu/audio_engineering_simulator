# AES-77 worklog

## 2026-11-20
- Did: read luma_loop bug doc, IMPROVEMENTS.md, EPIC_5_NOTES (ii), web/src/connections.ts + tests.
- Learned: the Luma fix fully supersedes the ticket; only dead `wouldCreateCycle` + stale comments
  + an unmarked open decision in EPIC_5_NOTES remain.
- Next: ask Oskari whether to close as-is or do the tidy.

## 2026-11-20 (later)
- Did: Oskari chose the tidy (I-440). Deleted `wouldCreateCycle` + its 3 unit tests, kept the
  "submits a loop-closing drag for engine-level validation" test under a renamed describe, fixed
  the stale duplex comments (connections.ts, connections.test.ts), marked EPIC_5_NOTES (ii) resolved.
- Gates: pnpm format/check/typecheck/test green (426 tests; 2 pre-existing svelte-check warnings in
  FloatingWindow.svelte), check-plan-refs green. No Rust touched, so cargo gates unaffected.
  `fx quality check`: ok (Δcog -13, Δcyc -8).
- Result: ticket closed as superseded by the Luma-loop fix, plus dead-code removal.
