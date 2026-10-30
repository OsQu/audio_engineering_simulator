# AES-4: Check if FX is working

## Intent
Confirm the fx factory can provision and run this repo end-to-end by running the project's test gates
in a fresh worktree.

## Approach
1. Confirm the setup hook artifacts exist (web/node_modules, web/public wasm) — the earlier
   ERR_PNPM_IGNORED_BUILDS failure was fixed in 4489ccc (esbuild approved).
2. Run the Rust gate: `cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`.
3. Run the web gate: `pnpm run check`, `pnpm run typecheck`, `pnpm run test`.
4. Record results; no code changes expected.

## Slices
- [x] Verify setup artifacts
- [x] Rust gate
- [x] Web gate
- [x] Worklog + PR draft, push, `fx mv AES-4 finished`

## Decisions
- No source changes: the ticket is a smoke test of the factory, not a feature.

## Open questions
- none

## Result
fx is working: the setup hook provisioned the worktree and every gate runs. All pass except
`cargo docs`, which fails on main (a broken intra-doc link, unrelated to fx). That's proposed as AES-5.
