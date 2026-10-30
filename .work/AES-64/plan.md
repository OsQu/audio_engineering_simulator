# AES-64 — Story 10.1 Regression harness & resolver contract

## Intent
Land the Story 10.1 harness so later Epic 10 stories (10.2–10.7) migrate onto a pure resolver with
executable regression proof. Program design: `docs/epics/epic-10/story-10.1-regression-harness.md`.

## Slices
- 10.1.1 Playwright/Vite browser seam + deterministic `TestSceneSession` fixture route — ✅ (24e019f)
- 10.1.2 Known-regression suite (`test.fail` with owner Story) — ✅ (3ee5377)
- 10.1.3 Real-worklet patch-acceptance smoke — ✅ (564c6e4)
- 10.1.4 Pure resolver-contract fixture matrix + initial `viewResolver` — ✅ (3cc3fa4)
- Factory session: verify gates, port-isolate Playwright, docs, push — ✅

## Decisions
- Playwright port env-overridable via `PLAYWRIGHT_PORT` (default 4173, `--strictPort`) so parallel
  worktrees/agents don't collide; `reuseExistingServer: false` stays.
- Known regressions use `test.fail` (fail-closed on unexpected pass), not skip/retry.

## Open questions
- None blocking. Stacked branches e10-s2..s4 build on this; merge order s1 → s4.
