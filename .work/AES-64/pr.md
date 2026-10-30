AES-64: Story 10.1 — Regression harness & resolver contract

## What changed and why
Epic 10 replaces scattered visibility/placement logic with one pure presentation model. Before migrating,
this Story lands the proof harness:

- **Browser seam (10.1.1)** — Playwright + Vite config; an explicit test-only route
  (`BrowserFixtureRoot`) mounts the *real* `App` tree with a deterministic `TestSceneSession` (no
  AudioContext/worklet). `App` accepts an optional session; production default unchanged.
- **Known-regression suite (10.1.2)** — `test-browser/known-regressions.spec.ts`: six cases (hover stacking,
  cable layer, click-click vs drag connection, near-socket drag at zoom extremes, rack drag occupant) marked
  `test.fail` with their owning Story (10.5 / 10.7). An unexpected pass fails the run, forcing the fixing
  Story to move the case into the required suite.
- **Real-worklet smoke (10.1.3)** — `WorkletSmokeRoot` + `worklet-patch-acceptance.spec.ts` run a minimal
  compatible patch through the generated production worklet.
- **Resolver contract (10.1.4)** — `web/src/view-resolver.ts` (initial pure shape) and a table-driven
  fixture matrix `web/test/fixtures/resolver-contract.ts` + `view-resolver.test.ts`.
- Playwright port is overridable via `PLAYWRIGHT_PORT` (default 4173, strict) for parallel worktrees.

## Look closely at
- `web/src/Root.svelte` / `router.svelte.ts`: the fixture route must only be reachable by its reserved URL
  prefix, never as a fallback.
- `App.svelte` / `BenchStage.svelte` session-injection refactor — production behavior should be identical.
- `known-regressions.spec.ts`: assertions are semantic (`elementFromPoint`, client rects, connection state),
  not screenshots.

## Validation
`pnpm --dir web test:browser` (8 passed incl. 6 expected failures), `check`, `typecheck` (0 errors),
`test` (312 passed). Requires `pnpm run wasm` first for the worklet smoke.
