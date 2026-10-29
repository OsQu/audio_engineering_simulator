# AES-2: Try fx — plan

## Intent
Smoke-test the fx factory setup: provision the worktree via fx.toml's setup hook, then run every test suite.

## Approach / slices
1. [x] Verify setup artifacts (web/node_modules, web/public wasm) exist — the earlier ERR_PNPM_IGNORED_BUILDS failure was fixed in 4489ccc.
2. [x] Rust gate: `cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`.
3. [x] Web gate: `pnpm run check`, `pnpm run typecheck`, `pnpm run test`.
4. [x] Record results in worklog + pr.md.

## Decisions
- No code changes unless a test fails because of the fx environment — the ticket is a smoke test.

## Open questions
- none
- Fixed the pre-existing broken doc link in `crates/devices/src/build.rs` (separate commit) — one line, needed for the `cargo docs` gate to pass; recorded as assumption I-94.
