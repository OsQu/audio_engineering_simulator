Confirm `cargo docs` passes on main (no code change)

`cargo docs` failed on 8656e9c with an unresolved link to `instantiate` in `crates/devices/src/build.rs`.
That was already fixed on main: 9639347 rewrote the module docs to link
`crate::catalog::instantiate_described`, and the catalog split removed the private `NodeBuilder` link warning.

On the current main (038712f) the full gate — plan-ref check, `cargo fmt --check`, `cargo lint`, `cargo test`,
`cargo wasm`, `cargo docs` — is green with no warnings. This branch only carries the investigation notes; it
can be closed rather than merged.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
