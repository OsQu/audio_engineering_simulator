# AES-79 worklog

## 2026-11-08 17:30
- Did: traced the warning to the host Homebrew wasm-pack (0.15.0). Nothing in the repo pins wasm-pack,
  and CI's taiki-e/install-action already installs the latest. Asked the human to run `brew upgrade wasm-pack` (I-297).
- Result: `wasm-pack --version` → 0.15.1; `pnpm run wasm` builds cleanly with no "newer version" warning.
- Learned: this ticket needs no repo change. The only repo commit is this `.work/` record.
- Next: none. Done.
