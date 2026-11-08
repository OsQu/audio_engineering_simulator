# AES-95 — Quality tracking with rust-code-analysis

## Intent
Let fx record per-function complexity for this repo at each merge and warn when work makes code
more complex. The repo pins and runs the analyzer; fx only parses its output.

## Approach
- `scripts/rust-code-analysis.sh`: runs a pinned `rust-code-analysis-cli` (0.0.25) from a
  version-specific cache root shared by all worktrees (`~/.cache/rust-code-analysis-cli/<ver>`);
  `--install` does `cargo install --locked --version <ver> --root <that dir>`. It doesn't touch the
  global `~/.cargo/bin`, so a different version elsewhere on PATH can't leak in.
- `fx.toml` `[quality]`: one `rust-code-analysis` analyzer over `crates` and `web/src`, `-X` globs for
  tests, starting limits cognitive 20 / cyclomatic 15, recalibrated from the first scan.
- Docs: a short README note on the script and that `.svelte` files aren't covered.

## Slices
| # | Slice | Status |
|---|-------|--------|
| 1 | Pinning script + README note | done |
| 2 | Install the pinned tool (human approval: package install) | done |
| 3 | fx.toml [quality] + verify with `fx toml check` / `fx quality scan` | done |
| 4 | Recalibrate limits from the baseline | done |

## Decisions (approved, I-303: agent may run the pinned install)
- Cache-root install instead of `~/.cargo/bin`: same version in every worktree, no global change.
- Excludes are explicit `-X` globs even though fx's adapter skips tests too: smaller output, and the
  scan reads the same without fx.
- target/ and web/public/ are outside the scanned paths (`crates`, `web/src`), so need no glob.

## Open questions
- (resolved) Install approved: "you can run it" (I-303).
