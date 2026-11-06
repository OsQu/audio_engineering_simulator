---
name: verify-live
description: Verify a UI- or audio-affecting change in the running simulator, not just in tests. Starts the Vite dev server, drives the real studio / device workbench / test fixture in a headless browser through the Playwright MCP with the real AudioWorklet, reads engine health (state, overruns, drops), output peak and device meters from stable data-testid hooks, takes screenshots of the touched views and looks at them, collects console errors, stops the server, and writes a short evidence block for the worklog or PR. Use after any change to web/, the worklet, or engine code that reaches the browser, or when asked to "check it live", "verify in the browser", "does it actually work/sound/look right", or to screenshot the app.
---

# Verify live

Tests prove the pieces; this proves the running app. Drive the real simulator headless with the
**Playwright MCP** (`mcp__playwright__*`), read numbers from **stable hooks** (never from prose), look at
screenshots, and report an evidence block. It's **required for any UI- or audio-affecting change**.

What it **can't** prove, and must say so: how it *sounds* (the headless worklet renders, but nobody
listens: tone, clicks, zipper noise and latency feel stay with the human) and whether it *looks right* by
taste (you can catch broken layout, overlap and missing elements, but not "is this pleasant").

## The hooks

All values are `data-*` attributes on `data-testid` elements, covered by
`web/test-browser/live-hooks.spec.ts` (rename one and that spec fails):

| `data-testid` | attributes | where |
| --- | --- | --- |
| `debug-panel` | `open` (studio `<details>`) | studio toolbar Debug menu; workbench Debug drawer |
| `engine-status` | `data-state` = `idle` / `booting` / `suspended` / `playing` / `error`; `data-sample-rate` | inside the Debug surface |
| `engine-overruns` | `data-value` (compute-budget overruns), `data-quanta` | inside the Debug surface |
| `engine-drops` | `data-event`, `data-param` (queue drops) | inside the Debug surface |
| `output-peak` | `data-dbfs` (now), `data-hold-dbfs` (max since start); `-inf` = silence | studio Debug VU; workbench "Output peak" |
| `meter` | `data-device`, `data-label`, `data-unit`, `data-value` (`-inf` below the floor) | every device meter on stage |

`?debug=1` on any route opens the Debug surface on load. The studio's Debug menu only exists **after
▶ start**. `suspended` means the worklet is built but the AudioContext waits for a user gesture.

## Procedure

Work from the repo root. `PORT` below is `$VITE_PORT` when it's set (factory worktrees lease it),
otherwise `5173`. Keep every MCP output under `.playwright-mcp/` (gitignored).

1. **Build what the browser loads.** If you changed anything under `crates/` or `web/worklet/`, run
   `cd web && pnpm run wasm` first; otherwise the page runs the stale artifact. On a fresh checkout,
   `pnpm install` too.

2. **Start the server in the background** (Bash `run_in_background: true`):

   ```sh
   cd web && exec pnpm exec vite --host 127.0.0.1 --port "${VITE_PORT:-5173}" --strictPort
   ```

   `--strictPort` makes a taken port fail loudly instead of silently moving. Then block until it
   answers (foreground `sleep` is blocked; this loop is fine):

   ```sh
   until curl -sf -o /dev/null "http://127.0.0.1:${VITE_PORT:-5173}/"; do perl -e 'select(undef,undef,undef,0.3)'; done
   ```

3. **Navigate** with `browser_navigate` to the view the change touches, always with `?debug=1`:
   - studio (default scene): `http://127.0.0.1:PORT/?debug=1`
   - device workbench: `http://127.0.0.1:PORT/devices/<typeId>?debug=1` (typeIds come from the catalog:
     `synth_voice`, `scarlett_8i6`, …; a bare `/devices` lists them)
   - fixture: `http://127.0.0.1:PORT/__test__/studio/<fixtureId>` (ids in
     `web/src/testing/browser-fixtures.ts`; these mount the studio tree without audio)

   MCP tools write page snapshots to `.playwright-mcp/page-*.yml` rather than inline. You rarely need
   refs from them: `browser_click`/`browser_hover` accept selectors like `role=button[name="▶ start"]`,
   and step 4 does the rest in code.

4. **Start audio, play, read.** Do it in **one** `browser_run_code_unsafe` call, so the reads before,
   during and after the note come from the same moment as the actions. The default-studio version:

   ```js
   async (page) => {
     const read = () => page.evaluate(() => {
       const ds = (id) => ({ ...document.querySelector(`[data-testid="${id}"]`)?.dataset });
       const meters = [...document.querySelectorAll('[data-testid="meter"]')].map((m) => ({ ...m.dataset }));
       return {
         status: ds("engine-status"), overruns: ds("engine-overruns"), drops: ds("engine-drops"),
         peak: ds("output-peak"),
         liveMeters: meters.filter((m) => m.value !== "-inf"), // all are "-inf" when silent
       };
     });
     await page.getByRole("button", { name: "▶ start" }).click();
     await page.waitForSelector('[data-testid="engine-status"][data-state="playing"]', { state: "attached", timeout: 30000 });
     await page.waitForSelector('[data-testid="engine-overruns"][data-value]:not([data-value=""])', { state: "attached" });
     const before = await read();
     // Play: the default scene's keys are the MIDI controller; QWERTY plays its open window (a = C).
     await page.getByRole("button", { name: "ctrl — drag to move" }).hover();
     await page.getByRole("button", { name: "open MIDI Controller" }).click();
     await page.keyboard.down("a");
     await page.waitForTimeout(500);
     const during = await read();
     await page.keyboard.up("a");
     await page.waitForTimeout(300);
     const after = await read();
     return { before, during, after };
   }
   ```

   Adapt it to the route: on a **workbench** there's no ▶ start (the bench boots `suspended` and
   resumes on the first click, so replace the start click with
   `page.getByTestId("debug-panel").click()`), and QWERTY plays the "Send to" target without opening
   a window. On the **studio**, the Debug menu and all its hooks only exist **after ▶ start**. Skip
   the note if the change isn't audible.

   The read returns `dataset` objects, so the keys are camelCase (`sampleRate`, `holdDbfs`) and each
   carries a `testid` key. Healthy:
   - `status.state` = `playing`, `sampleRate` = `48000`;
   - `overruns.value` = `0` (a few right at start-up can happen on a loaded machine; a count that keeps
     growing between reads is a finding); `drops` `event`/`param` = `0`;
   - after a note, `peak.holdDbfs` is not `-inf`. `holdDbfs` is the max since start, so a short press
     still counts. A hold at `0.0` means the output reached full scale. In the default scene that's
     the current gain staging (one note reads 0.0 / RMS ≈ −0.8 dBFS), so report it as an observation
     for the human's ears, not as a pass or fail. A change *you* made that newly hits 0.0 is a finding.
   - The headless browser can see the host's MIDI ports (status may read `MIDI: IAC Driver Bus 1`);
     that's the host leaking in, not a problem.

5. **Screenshot and look.** `browser_take_screenshot` with
   `filename: ".playwright-mcp/verify-<view>.png"` (relative to the repo root) for every view the change
   touches (the studio wall, the workbench, an open device window). **Read each PNG** with the Read tool
   and say what you see. The render loop never idles, so prefer Playwright screenshots over other
   browser tools.

6. **Collect console errors.** `browser_console_messages` with `level: "error"` and `all: true` (the
   default only covers the time since the last navigation). Any error is a finding unless you can show
   it predates your change (reproduce it on `main`).

7. **Stop.** `browser_close`, then stop the background Vite task (TaskStop on its id) and confirm the
   port is free (`lsof -i :${VITE_PORT:-5173}` prints nothing). Check `git status --short` shows nothing
   from the run. `.playwright-mcp/` is ignored; leave the screenshots there for the human, or
   `rm -rf .playwright-mcp` once nobody needs them.

## Evidence block

Paste this into the worklog or the PR, filled in:

```md
**Live check** — <route(s)>, <HEAD sha, + "uncommitted" if the tree is dirty>, headless Chromium via Playwright MCP
- engine: playing @ 48000 Hz · overruns 0 / <quanta> quanta · drops 0 event, 0 param
- output peak: hold <x.x> dBFS after <what was played> (silent before: -inf)
- meters: <device · label = value unit> (only the relevant ones)
- console errors: none | <messages>
- screenshots: .playwright-mcp/verify-<view>.png — <one line on what each shows>
- not checked live: how it sounds, and subjective look. Human to listen/look at: <specific things>
```

Report failures as failures: a hook missing, `error` state, growing overruns, a silent peak after a
note, or console errors go in the block as findings, not footnotes.
