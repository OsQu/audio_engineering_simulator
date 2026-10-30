AES-65: Story 10.2 — Acknowledged topology transactions

## What changed and why
Structural (topology) edits used to `hotSwap` the scene optimistically and fire an unacknowledged
`loadPatch`, so a browser test couldn't tell a gesture bug from an engine transport failure, and a late
reply could clobber concurrent edits. Now every structural change is a serialized, acknowledged transaction.

- **Pure state machine** — `web/src/topology-transaction.ts`: one in-flight request, correlation by engine
  generation + id, base topology revision, 5 s deadline → `unknown` (never `rejected`), `recover` bumps the
  generation so stale replies are inert.
- **Versioned protocol** — `web/src/topology-protocol.ts`, `engine.ts`, `worklet/processor-impl.js`:
  `topologyAccepted` (compiler queued) vs `topologyInstalled` (live after the render-quantum swap; descriptors
  now refresh here) vs `topologyRejected`; protocol mismatch fails closed at readiness.
- **Session migration** — `SceneSession.proposeTopology(label, edit)`: compile a detached candidate, apply
  `edit` to the *current* scene only on acceptance. All producers (cables, config, track count, add/remove
  device, workbench, load/reload) migrated; Retry / Recover buttons in the studio status bar.
- **Proof** — pure matrix tests + real-worklet Playwright specs for accept→install, reject, mismatch.
- **Docs** — plan + companion file reconciled to ✅ with as-built designs (the e10-s4 header already said so).

## Look closely at
- `session.svelte.ts` `#receiveTopology` / `recoverTopology`: the delta is a closure replayed on the live
  scene; `replaceTopology` uses `Object.assign`, so load paths intentionally replace the whole scene.
- `processor-impl.js` `pendingPatchInstall`: install notification rides the existing `lossesDirty` swap hook.
- Residuals (companion file): no `busy`/`unavailable` from the synchronous worklet; processor `error` not mapped
  to `processor-failure`; deadline literal `5_000`; gesture-level rejection retention proven only in pure tests.

## Gates
Vitest 323 ✓ · svelte-check 0 errors · Biome clean · Playwright worklet specs 3/3 ✓ (known-regression
specs are the expected-fail 10.1 fixtures). No Rust changes.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
