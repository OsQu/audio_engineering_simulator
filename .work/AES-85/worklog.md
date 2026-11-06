# AES-85 worklog

## 2026-11-06 21:00 — base
- Branch was created from origin/main (8656e9c), which has no Playwright at all; local `main` (09624ad)
  is 88 commits ahead with the Epic 10 regression harness (Playwright, test-browser/). Fast-forwarded
  onto it (`reset --hard` was denied by the sandbox; `merge --ff-only` is the safe equivalent).
  Assumption I-268. Pushing this branch carries those commits.
- Rebuilt wasm (`pnpm run wasm`), `pnpm install --frozen-lockfile`.

## 21:03 — recon on the live app
- Studio default scene = ctrl (MIDI controller) → synth → Scarlett 8i6 → … Notes need an open instrument
  window: hover `ctrl — drag to move`, click `open MIDI Controller` (hover chip; not clickable before
  hover), then QWERTY `a`.
- Every page load logged a console error: favicon.ico 404. Fixed with an empty `data:,` icon.
- The level message is momentary, so a check racing a key release reads −∞ → added a peak hold.

## 21:05 — hooks (0bc668f)
- `live-status.ts` (resolveEngineState, peakDbfs, dbfsAttr, debugRequested) + unit tests.
- engine.ts: optional `onAudioState` / `onFault` handlers; session keeps audioState, sampleRate, fault,
  lastHealth, peakHold, derived `engineState`.
- `EngineHealth.svelte` (engine-status / engine-overruns / engine-drops) in studio Debug menu and
  workbench Debug drawer header (the bench previously showed no health at all).
- `output-peak` on Vu + DebugPanel, `meter` on Meter (device via Reading's handle), `debug-panel`
  + `?debug=1` on the studio `<details>`.
- `live-hooks.spec.ts`: studio (start, play note, hold ≠ −inf, meter attrs, no console errors) +
  workbench (suspended → click → playing). Biome/svelte-check/vitest green.

## 21:08 — skill (99543cd) + dogfood
- Wrote `.claude/skills/verify-live/SKILL.md`; linked from CLAUDE.md task loop (no AGENTS.md on this base).
- Fresh general-purpose subagent ran the skill cold on the default scene. Result: playing @ 48000 Hz,
  0 overruns / 9312 quanta, drops 0/0, hold 0.0 dBFS after holding `a` (−inf before), 6 live computer
  meters mid-note (Track/Send/Return 1 Peak 0.0, RMS −0.8), no console errors, screenshot read
  (controller window + Debug popover over the wall), clean `git status`. Acceptance met.
- Its friction list → skill fixes: one canonical `browser_run_code_unsafe` snippet (start/wait/play/read
  in one call, role selectors so no snapshot refs), server-ready curl poll, camelCase dataset keys,
  `all: true` for console messages, confirm port freed, dirty-tree commit note, host MIDI leak note,
  and what a 0.0 dBFS hold means. Re-ran the snippet myself: works.
- Full `test:browser` on VITE_PORT: 42/42 pass. vitest 428 pass.

## Result
Done. Open for the human: whether one default note hitting 0.0 dBFS (RMS −0.8) is intended gain
staging or clipping worth a ticket; AGENTS.md line once the sibling ticket lands it.
