# Scarlett 8i6 gain-halo equipment Story

- Status: done
- Brief: [`../equipment-briefs/scarlett-8i6.md`](../equipment-briefs/scarlett-8i6.md)
- Approved scope/date: gain-halo increment approved 2026-09-11
- Autonomous execution and per-task commits approved: implementation yes; commits no
- Execution model: primary harness supervising one Terra-tier implementation agent
- Branch/base: current `main`; no branch or commit requested
- Current task and exact resume point: complete; user approved the visuals and requested no further work

## Goal and boundaries

- Story outcome: Gain 1 and Gain 2 show live off/green/amber/red input-level halos using the existing
  peak readouts and the reference device's documented dBFS thresholds.
- Explicitly deferred: DSP/readout changes, shared widget extraction, progress segments, peak hold, and
  unrelated generation-comment cleanup.
- Architecture seams that must remain unchanged: Rust catalog/model, `DeviceHandle`, session/worklet
  protocols, faceplate registry, and scene schema.
- Stop conditions: any need for a shared-contract change, materially different meter semantics, or
  unrelated faceplate redesign.

## Task plan

### Task 1 — Render and verify the two live gain halos · done

- Depends on: existing Scarlett readouts `1` and `3` and approved brief.
- Owned files: `web/src/widgets/Scarlett8i6.svelte`, one device-local TypeScript helper, one focused web
  test, this Story, the equipment brief, and the exact `docs/IMPROVEMENTS.md` gain-ring item.
- Shared seams/aggregation edits: none.
- Terra implementer assignment/status: completed by `/root/implement_gain_halos`, including the
  user-requested helper move into `web/src/widgets/Scarlett8i6/`; no commit.
- Program design: a pure `scarlettGainHaloState(peakDbu)` maps the ADC-calibrated boundaries to
  `"off" | "green" | "amber" | "red"`. The faceplate reads ids 1/3, places each existing `Control`
  inside a decorative `.gain-halo[data-level=...]` wrapper, and paints the ring without intercepting
  pointer input or shifting the channel layout.
- Acceptance checks: exact threshold-boundary tests; existing exposed-face test remains green; Svelte
  typecheck and formatting pass; fixed-view front/rear inspection; live independent halo response where
  browser/audio capability permits.
- Validation commands: `cd web && pnpm run format`, `pnpm run typecheck`, focused Vitest for the new
  helper and faceplate guardrail, then `pnpm run test`.
- Commit subject: none; user retains the commit gate.
- Implementer report: changed only the declared faceplate/helper/test/improvement files. Reported focused
  tests (14), full web tests (291), typecheck, formatter, and `git diff --check` passing; no gap found.
- Primary verification: reviewed the complete diff and surrounding handle/readout/catalog seams; reran
  focused tests (14), full web tests (291), typecheck (0 errors, two pre-existing `FloatingWindow.svelte`
  warnings), lint, production build, WASM build, and `git diff --check`. Browser-tested a real continuous
  path and inspected the front and rear at 3.0 px/mm.
- Delivered/decisions/deviations: `scarlettGainHaloState` owns the ADC-calibrated thresholds; readouts 1
  and 3 drive independent decorative halos. No shared widget, Rust, protocol, registry, or schema edit.
- Validation result: all declared automated checks pass. Live Input 1 evidence: -17.9 dBu → green,
  -3.4 dBu → amber, +2.9 dBu → red; Input 2 remained off. Gain 1 stayed keyboard-operable through the
  halo. No console errors were observed.
- Resume point: complete. The user approved the face and the organization correction was revalidated.

## Visual approval gate

- Workbench URL and fixed view: built-in browser at `/devices/scarlett_8i6`; 950 × 989 CSS-pixel viewport,
  device-pixel ratio 2, browser zoom 100%, workbench 3.0 px/mm; both faces shown, debug/keyboard collapsed.
- Evidence/screenshots: off-state front/rear plus live green, amber, and red Input 1 captures. Test path:
  Vox → condenser mic → Input 1 → USB send → computer → USB return → Line Out 1. 48V and device power on;
  peak readout pinned during state changes. The retained browser tab shows the final red/off comparison.
- Manager grade history: iteration 1 — 4/5. Silhouette/scale 4: unchanged 210 × 47.5 mm face at the fixed
  scale. Layout/grouping 4: halos sit concentrically around Gain 1/2 without moving adjacent controls.
  Signature components/I/O 4: the documented tri-colour gain rings now react independently while all
  front/rear sockets remain intact. Materials/colour/type 4: restrained glow is legible against the dark
  face and green/amber/red are distinct; existing faceplate treatment is preserved. Interaction/readability
  4: the ring does not intercept input and the gain remained operable; readout changes and colour changes
  agreed throughout the live test. No identity-critical category fell below 4.
- Current manager score: 4/5.
- Identity-critical categories all at least 4: yes.
- User decision/date: approved 2026-09-11.
- Requested changes: move the device-specific helper into `web/src/widgets/Scarlett8i6/`; delivered and
  independently revalidated with the 14 focused/guardrail tests and typecheck.
- Sonic tasks released by approval: none; this increment adds no DSP.

## End-to-end DSP approval gate

- Not applicable: existing engine readouts and audio behavior are unchanged.

## Architecture gap

- None. The existing `DeviceHandle.reading(id)` faceplate seam supplies all required live data.

## Story completion

- Final validation: formatter, focused tests (14), full web tests (291), typecheck (0 errors; two
  pre-existing warnings), lint, production build, WASM build, `git diff --check`, and live workbench
  off/green/amber/red verification all passed. The post-approval helper move passed focused tests,
  typecheck, and `git diff --check` again.
- Task commits: none requested.
- Delivered: independent live tri-colour halos around Gain 1/2, peak calibrated to the modeled ADC and
  official thresholds, with the pure mapping helper colocated under the Scarlett device folder.
- Intentionally shallow: no clip latch, peak hold, or segmented level progress.
- Deferred follow-ups: see the approved brief.
