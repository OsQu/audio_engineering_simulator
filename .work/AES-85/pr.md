Add verify-live skill and stable live-verification hooks

## What and why

Agents could already drive the running simulator headless through the Playwright MCP (the real
AudioWorklet runs, and screenshots work). But there was no procedure, and the only readable state was
prose ("▶ playing @ 48000 Hz · … health: 0 overruns …"). So "does it actually work and look right"
was left to the human. This PR adds both pieces:

**Hooks** (`web/`). The facts are exposed as `data-*` attributes on `data-testid`s, and the visible
text stays prose:

| testid | values |
| --- | --- |
| `engine-status` | `data-state` idle / booting / suspended / playing / error, `data-sample-rate` |
| `engine-overruns`, `engine-drops` | counts from the last health snapshot |
| `output-peak` | `data-dbfs` (now), `data-hold-dbfs` (max since start, so a short note can be checked after release) |
| `meter` | `data-device`, `data-label`, `data-unit`, `data-value` |
| `debug-panel` | the studio Debug menu (`?debug=1` opens it on load) and the workbench drawer |

`test-browser/live-hooks.spec.ts` pins all of them against the real worklet, on the studio (start, play
a note, peak hold, meters, no console errors) and on a workbench route. Renaming a hook fails it.

**Skill** (`.claude/skills/verify-live/SKILL.md`). Start Vite on the leased port → navigate (studio /
`/devices/<typeId>` / fixture) → one Playwright snippet that starts audio, plays a note and reads the
hooks before, during and after → screenshot and look → console errors → stop → an evidence block for
the worklog or PR. It says plainly that sound and taste stay with the human. CLAUDE.md's task loop now
requires it for UI- and audio-affecting changes.

A fresh agent given only the skill reported for the default scene: playing @ 48000 Hz, 0 overruns,
0/0 drops, output hold 0.0 dBFS after a note (−∞ before), live computer meters, no console errors,
a screenshot it described, and a clean `git status`. I folded its friction notes back into the skill.

## Look closely at

- `web/src/engine.ts`: two new optional handlers (`onAudioState`, `onFault`). A worklet `error`
  message counts as a fault **only before `ready`**. After that it's a rejected hot-swap, and the
  previous patch keeps playing.
- `web/src/session.svelte.ts`: structured twins of `status`/`health` plus a derived `engineState`.
  `fault` resets on `start()`.
- `DebugPanel.svelte`: the header now shows engine state, overruns and drops. This reverses the old
  "health deliberately not here" note, because the workbench had no health readout at all and the
  skill needs one on `/devices/*`.
- `Meter.svelte` / `Vu.svelte` hooks wrap the existing markup in `display: contents` spans, so layout
  shouldn't change.
- `web/index.html`: an empty `data:,` favicon. The `/favicon.ico` 404 was a console error on every load,
  which the "no console errors" check would always trip on.
- The spec doesn't assert `overruns == 0`. Under parallel Chromium workers the real worklet can
  starve, so the spec checks the hook's shape and the skill reports the number.

## Notes

- One default-velocity note in the default scene reaches 0.0 dBFS peak (RMS ≈ −0.8 dBFS). The skill
  treats that as current gain staging and asks the human to listen. Worth a look if it's unintended.
- The branch is based on local `main` with the Epic 10 regression harness (origin/main had no Playwright).
- `.playwright-mcp/` is now gitignored.
- Once AGENTS.md exists, it should point at the skill the way CLAUDE.md now does.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
