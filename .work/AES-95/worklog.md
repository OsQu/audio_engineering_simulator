# AES-95 worklog

## 2026-11-08 — pick-up and finish
- Did: the first run ended before committing anything; started fresh. Wrote plan.md and
  scripts/rust-code-analysis.sh, which pins 0.0.25 in a shared cache root (`--install`). Asked to run
  the install (I-303, approved) and ran it (21 s thanks to sccache). Added fx.toml [quality] (crates +
  web/src, tests excluded with -X globs) and a README section. Commit eb523cb.
- Verified: `fx toml check "$PWD"` ok (quality rust-code-analysis); `fx quality scan` → 1088 records,
  720 rust + 368 typescript. Excludes take the scanned files from 180 to 157; no tests/target/public in
  the output. .svelte files are not covered (documented in fx.toml and README).
- Baseline: cognitive p50 0 / p90 4 / p99 20 / max 129, cyclomatic p50 1 / p90 5 / p99 16 / max 92.
  Kept limits 20 / 15 (I-304): 10 and 17 functions over, led by engine `schedule.rs:compile` 129/92.
- Learned: `fx toml check` without an arg checks the main repo, not the worktree (recorded, I-305).
- Next: none; ready for review.
