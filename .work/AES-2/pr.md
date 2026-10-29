AES-2: Try fx

Smoke test of the fx factory on this repo: provision a worktree with the `fx.toml` setup hook, then run every test suite.

**Result:** fx works. The setup hook (pnpm install + wasm build) succeeds now that esbuild's build script is approved (4489ccc). Everything is green:
- Rust: `cargo fmt --check`, `cargo lint`, `cargo test` (534 tests), `cargo wasm`, `cargo docs`
- Web: Biome, svelte-check (0 errors; 2 existing a11y warnings in `FloatingWindow.svelte`), Vitest (291 tests)

**The one code change:** `cargo docs` was already failing on `main`. The module doc in `crates/devices/src/build.rs` linked `` [`instantiate`] ``, but that file now imports only `instantiate_with_assets`, so the link didn't resolve. It now links `crate::instantiate`. This is a one-line doc fix in its own commit, so it's easy to drop if you'd rather fix it differently (for example by pointing the doc at `instantiate_with_assets`).
