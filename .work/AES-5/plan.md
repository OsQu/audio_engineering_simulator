# AES-5 — Fix broken intra-doc link that fails `cargo docs`

## Intent
Make `cargo docs` (and the full gate) pass on main.

## Approach
Point the unresolved `instantiate` link in `crates/devices/src/build.rs` at its real path; optionally fix the
`AssetNodeBuilder` → private `NodeBuilder` link warning in the catalog.

## Slices
| Slice | Status |
| --- | --- |
| Reproduce on the branch base (038712f) | done — does not reproduce |
| Fix link | not needed — already fixed upstream |
| Full gate | green |

## Decisions
- No code change: commit 9639347 already rewrote the link to `crate::catalog::instantiate_described`, and the
  catalog split (catalog/expand.rs, catalog/spec.rs) removed the private-item link warning. Ticket is obsolete.
