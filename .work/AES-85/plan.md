# AES-85 — Agent live-verification skill

## Intent
Give agents a repeatable procedure (a project skill) to run the real simulator in a headless browser via
the Playwright MCP and come back with evidence (engine state, overruns/drops, output peak, meters,
screenshots, console errors) — plus stable DOM hooks so the procedure doesn't scrape prose.

## Approach
1. **Base**: branch fast-forwarded from origin/main to local `main` (09624ad, merged Epic 10 regression
   harness) — origin/main has no Playwright at all. (assumption I-268)
2. **Hooks** (web/):
   - Session keeps structured state next to the prose strings: last `HealthMessage`, sample rate, and an
     engine state (`idle | booting | suspended | playing | error`).
   - One `EngineHealth.svelte` with `data-testid="engine-status"` (`data-state`, `data-sample-rate`),
     `engine-overruns` (`data-value`), `engine-drops` (`data-event`, `data-param`). Used by the studio
     Debug menu and the workbench Debug drawer header.
   - `output-peak` (`data-dbfs`) on the studio VU and the workbench "Output peak" stat; one shared
     `peakDbfs()` helper.
   - `Meter.svelte`: `data-testid="meter"` + `data-label`/`data-unit`/`data-value`.
   - `?debug=1` opens the studio Debug menu (workbench drawer is already open by default; flag also
     forces it open there).
3. **Spec**: `web/test-browser/live-hooks.spec.ts` — boots the studio with `?debug=1`, starts audio, and
   asserts every hook exists with parseable values (so renames break a test).
4. **Skill**: `.claude/skills/verify-live/SKILL.md` (+ `.codex` mirror if that's the convention).
5. `.playwright-mcp/` in `.gitignore`; link the skill from the agent guide (repo has no AGENTS.md on main →
   CLAUDE.md; see open question).
6. Dogfood: run the skill end-to-end on the default scene and record the evidence block.

## Slices
- [x] S1 hooks + unit test (`live-status.ts`) — 0bc668f
- [x] S2 Playwright spec `test-browser/live-hooks.spec.ts` — 0bc668f
- [x] S3 skill + gitignore + CLAUDE.md link — 99543cd
- [x] S4 dogfood: fresh subagent ran the skill cold; feedback folded into the skill

## Decisions
- Health readout moves into the workbench Debug drawer too: the skill needs overruns on `/devices/*`
  routes, which previously showed no health at all.
- Hooks carry machine values in `data-*` attributes; visible text stays human prose.

- Worklet `error` messages count as an engine fault only before `ready`; after it they're rejected
  hot-swaps while the old patch keeps playing (still a status line + console.error).
- The spec doesn't assert overruns == 0: real worklet starves under parallel Chromium workers (AES-70).
- Added `data:,` favicon: the /favicon.ico 404 was a console error on every load.
- 0.0 dBFS peak in the default scene is current gain staging, reported as an observation, not a failure.

## Open questions
- AGENTS.md doesn't exist on main (a sibling ticket creates it). Link from CLAUDE.md now; whoever lands
  second adds the AGENTS.md line.
