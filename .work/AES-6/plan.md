# AES-6: Check if FX is working

## Intent
Confirm the fx factory setup (fx.toml: worktree, sccache wrapper, setup hook, port lease) works by running the project's tests in an fx worktree.

## Approach
Check the setup hook's artifacts, then run the Rust and web test suites in this worktree; record results.

## Slices
1. Setup hook artifacts + env (sccache, VITE_PORT) — done
2. Rust: `cargo test` — done (534 passed)
3. Web: `pnpm run test` — done (291 passed)

## Decisions
- No code changes: verification-only ticket.
- Ran only the tests the ticket asks for, as separate commands: the human rejected the earlier chained full-gate (fmt+lint+test+wasm+docs) command.

## Open questions
- none
