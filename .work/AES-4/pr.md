AES-4: Check if FX is working

Smoke test of the fx factory on this repo. No source changes, only the `.work/AES-4/` notes.

**Result:** fx provisions the worktree correctly. The setup hook's pnpm install and wasm build succeed
now that esbuild's build script is approved (4489ccc). All gates ran in the fresh worktree:

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | ✅ |
| `cargo lint` | ✅ |
| `cargo test` | ✅ 534 passed |
| `cargo wasm` | ✅ |
| `cargo docs` | ❌ already broken on main |
| `pnpm run check` | ✅ |
| `pnpm run typecheck` | ✅ (2 a11y warnings) |
| `pnpm run test` | ✅ 291 passed |

**Look at:** `cargo docs` fails on main because of an unresolved intra-doc link to `instantiate` in
`crates/devices/src/build.rs:3`. This isn't related to fx. The fix is proposed as AES-5.
