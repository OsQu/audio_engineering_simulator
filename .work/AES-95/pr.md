Track per-function complexity with a pinned rust-code-analysis-cli

## What

- `scripts/rust-code-analysis.sh` runs rust-code-analysis-cli **0.0.25** from a version-specific cache
  directory (`~/.cache/rust-code-analysis-cli/0.0.25`, override with `RCA_ROOT`). `--install` puts it
  there with `cargo install --locked --version 0.0.25 --root …`. Every checkout uses the same binary,
  and nothing global (`~/.cargo/bin`, PATH) changes. Without the binary the script exits 1 and prints
  the install command.
- `fx.toml` gets `[quality]`: one analyzer that scans `crates/` and `web/src/` and excludes tests
  (`tests/`, `tests.rs`, `test_util.rs`, `web/src/testing/`, `*.test.ts`), plus limits of cognitive 20
  and cyclomatic 15. The local factory uses this to record complexity at each merge and to warn when a
  change makes a function more complex than the limits.
- README: a short "Complexity metrics" section.

## Baseline (current main)

1088 functions: 720 Rust, 368 TypeScript. Cognitive: p50 0, p90 4, p99 20, max 129. Cyclomatic: p50 1,
p90 5, p99 16, max 92. The limits sit at about p99. 10 functions are over the cognitive limit and 17
over the cyclomatic one. The biggest is `crates/engine/src/schedule.rs` `compile` (129/92). Next come
`web/src` placement and pointer helpers (`evaluateConnection`, `planEjection`, `startEngine`).

## Look closely at

- The `-X` globs in `fx.toml`. They take the scan from 180 files to 157. `target/` and `web/public/`
  are outside the scanned paths, so they need no globs.
- `.svelte` components aren't analyzed (the tool skips them), so component script logic isn't covered.
- No code changed, so the cargo and web gates are unaffected.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
