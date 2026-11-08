# AES-79 — Upgrade wasm-pack

## Intent
`pnpm run wasm` (web/build-wasm.sh → `wasm-pack build`) prints a "newer version available"
warning: local wasm-pack is 0.15.0, latest is 0.15.1.

## Findings
- wasm-pack is not pinned anywhere in the repo. Locally it is a Homebrew install
  (`/opt/homebrew/bin/wasm-pack`, `brew info`: 0.15.0 → stable 0.15.1).
- CI installs it via `taiki-e/install-action` with `tool: wasm-pack` (unpinned → latest, already 0.15.1).
- So the fix is a host tool upgrade, not a repo change.

## Approach / slices
| # | Slice | Status |
|---|-------|--------|
| 1 | Ask human to run `brew upgrade wasm-pack` (system-modifying → inbox) | done |
| 2 | Verify `wasm-pack --version` = 0.15.1 and `pnpm run wasm` has no warning | done |

## Decisions
- No repo pin added: CI already floats to latest; pinning would just create a new bump chore.
- Decisions (approved, I-297): the human ran `brew upgrade wasm-pack` on the host; no version pin in CI/README.
