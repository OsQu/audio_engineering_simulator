# AES-74 — Warn when a config widget's key is missing from the descriptor

## Intent
`ConfigSwitch`/`ConfigButton` (and `ConfigSelect`) render nothing when `configDesc(key)` is undefined, so a
stale WASM artifact or a typo'd key becomes an invisible element. Make the miss name itself in dev.

## Approach
- Pure resolver `resolveConfigWidget(handle, key, kind?)` in `device-handle.ts`: returns `{ desc }` or
  `{ problem }` (a message naming device + key, and the expected kind when it mismatches).
- Shared `MissingConfig.svelte`: visible `⚠ <key>` chip (title = message) + `console.warn` in an `$effect`.
- Each config widget: `{#if desc} … {:else}<MissingConfig …/>{/if}`.
- Vitest (node-only, can't mount Svelte) covers the resolver's found / missing / wrong-kind branches.

## Slices
| Slice | What | Status |
| --- | --- | --- |
| 1 | resolver + MissingConfig + wire 3 widgets + tests | done |

## Decisions
- Both warn and placeholder (ticket allowed either): the warn reaches the console, the chip shows *where*.
- Not dev-gated: the web tsconfig has `types: []` (no `vite/client`, so `import.meta.env` is untyped) and a missing key is a bug in any build — a visible ⚠ beats an invisible element. Avoids a tsconfig change.
- Included `ConfigSelect` (same silent branch, plus a kind mismatch) — same 3-line change.
- Logic lives in a pure TS function because Vitest runs in node without a Svelte/DOM environment.
