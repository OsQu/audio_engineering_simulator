AES-6: Check if FX is working

Verification-only ticket: no code changes, just the `.work/AES-6/` notes (removed at merge).

Confirmed that a ticket worktree created by fx from `fx.toml` is usable:
- the setup hook ran (`pnpm install` + wasm build put `web/node_modules` and the wasm/worklet artifacts in `web/public/`);
- `RUSTC_WRAPPER=sccache` and the `VITE_PORT` lease reach the agent's environment;
- `cargo test`: all 534 Rust tests pass;
- `pnpm run test` (web): 23 files, 291 tests pass.

Not run: `cargo lint` / `cargo wasm` / `cargo docs` / `cargo fmt --check` — the ticket asked for tests only.
