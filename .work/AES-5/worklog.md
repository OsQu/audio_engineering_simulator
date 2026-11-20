# AES-5 worklog

## 2026-11-20
- **Did:** Reproduced on branch base 038712f. `cargo docs` (forced rebuild of `devices`) is clean — no errors,
  no warnings. `build.rs` now links `crate::catalog::instantiate_described` (changed in 9639347). Ran the full
  gate: check-plan-refs, fmt --check, lint, test, wasm, docs — all green.
- **Learned:** The bug was fixed incidentally by later work on main after the ticket was filed against 8656e9c.
  The `NodeBuilder` private-link warning is also gone after the catalog split.
- **Result:** No code change needed. Branch carries only this `.work/` record; it can be closed without merging.
