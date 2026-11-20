# AES-74 worklog

## 2026-11-20
- **Did:** Added `resolveConfigWidget(handle, key, kind?)` in `web/src/device-handle.ts` (returns the descriptor or a
  `problem` message naming the device and key). Added a shared `MissingConfig.svelte` (a visible `⚠ <key>` chip with a
  `title` and `role="alert"`, plus `console.warn` in an `$effect`). Wired it into the `{:else}` branch of
  `ConfigSwitch`, `ConfigButton` and `ConfigSelect`. Vitest `web/test/config-widget.test.ts` covers the hit, missing-key,
  no-configs and kind-mismatch cases. Marked the item DONE in `docs/IMPROVEMENTS.md`.
- **Gates:** `pnpm run format`/`check`/`typecheck`/`test` green (433 tests; svelte-check's 2 warnings are in
  FloatingWindow, already there). `scripts/check-plan-refs.sh` ok. `fx quality check` ok (Δcog +3, Δcyc +4).
- **Learned / corrected:** The plan was to dev-gate the placeholder with `import.meta.env.DEV`, but the web tsconfig has
  `types: []` (no `vite/client`), so svelte-check fails on it. I dropped the gate instead of changing the tsconfig: a
  missing key is a bug in any build. Vitest is node-only and can't mount Svelte, so the branch logic lives in a pure
  function.
- **Live check not done:** I tried, with a typo'd `phantom` key on the 8i6 faceplate. In this headless Playwright session,
  `/devices/<typeId>` drew the Synth Voice face for every type (scarlett_8i6, electric_guitar), with or without my
  change. The studio view drew no devices even after I seeded `aes.scene`. This was the state before my change, and
  I didn't investigate it further. All temporary typos are reverted.
- **Next:** none. PR draft is in pr.md.
