# AES-68 worklog

## 2026-11-01 — Story 10.5 planned; design awaiting review
- Did: story-planning gap pass against e10-s4 code. Wrote companion design file, thin Story block in
  IMPLEMENTATION_PLAN.md, plan.md.
- Learned: hover/drag occlusion bugs are entirely the inline `drag ? 10 : chrome ? 6` overrides in
  WorldView + BenchStage, with DeviceChrome trapped inside the chassis stacking context. Rack occupants
  already follow previews (10.3); racks/occupants are siblings, so no stacking trap exists to undo.
  `unmount`/`removeRack` keep stale stored position + the device's own facing (violates U3).
- Next: Oskari reviews design + answers open questions; then Task 10.5.1.

## 2026-11-01 — Design approved (I-151: "approved 1"); Task 10.5.1 done
- Oskari approved; overlay tab order OK, U3 failure → status line.
- 10.5.1: `resolveStudioObjects` / `resolveBenchObjects` / `silhouetteOf` in view-resolution.ts;
  test/object-resolution.test.ts (10 tests). Deviation: view-resolution consumes placedItemsFor (not the
  reverse) to avoid a projection↔view-resolution import cycle.
- Gates: vitest 366 ✓, typecheck ✓ (2 pre-existing warnings), biome ✓. Not committed (CLAUDE.md §2).
- Next: 10.5.2 studio overlay — draft its program design first.

## 2026-11-01 — Task 10.5.2 done (studio stable layers + interaction overlay)
- Oskari (mid-turn): work the whole story, commit each task. Committed 10.5.1 as 2c7da61.
- WorldView: items in resolved order, z = fixed band per layer (`data-paint-layer`); drag/chrome z
  overrides deleted. New `.interaction` layer: per-object toolbars (`data-chrome-for`) + drag outline
  (`data-drag-outline`). Toolbar reveal is state (enter/focus on chassis or toolbar, shared 300 ms grace).
- PlacedItem.z removed; projection tests trimmed (layer coverage lives in object-resolution.test.ts).
- Graduated rack-hover, rear/front hover, populated-rack-drag into required `stable-layers.spec.ts`
  (6 tests), mutation-checked. face-lifecycle spec now finds flip buttons via `objectChrome`.
- Gates: vitest 365 ✓, typecheck ✓, biome ✓, Playwright 22 ✓ (+2 expected 10.7 failures).
- Next: 10.5.3 workbench.

## 2026-11-01 — Task 10.5.3 done (workbench stable layers + interaction overlay)
- BenchStage: z from resolved layer, toolbars/drag outline in `.interaction` at group home (offset* via
  `benchHome` action) + offset. Viewport pan now ignores `.interaction` presses (else the open click was
  swallowed by pointer capture — caught by face-lifecycle focus test).
- Corrected 10.5.1: the DUT is rear-object (both faces ⇒ sockets visible ⇒ below cable pass).
- face-lifecycle `flip` helper waits for the toolbar to be open (one 30 s flake before; 6/6 green after).
- Gates: vitest 365 ✓, typecheck ✓, biome ✓, Playwright 23 ✓ (+2 expected 10.7 failures).
- Next: 10.5.4 U3 ejection.

## 2026-11-01 — Task 10.5.4 done (U3) + browser-suite hardening; Story 10.5 complete pending review
- `ejection.ts`: pure planEjection (rack-wall elevation, GRID_MM lattice, d² then y then x, obstacles incl.
  allocated, slot order) + applyEjection; `eject` all-or-nothing. scene-ops unmount/removeRack take the
  catalog and return the plan; App reports failures to the status line and disables eject/remove-rack
  mid-flip. 8 hand-calc oracles; scene-ops tests that asserted the stale position rewritten;
  `ejection.spec.ts` asserts resolved world rects (camera refit made pixel comparisons meaningless).
- Hardening found real bugs (stress-running ×8 surfaced 30 s hover hangs):
  1. revealed toolbar bar must be a hit target (gaps fell through to the rack → rack took the hover);
  2. DeviceChrome reveal slides down, not up (mid-reveal overlap swallowed drag presses);
  3. stage viewports `overflow: clip` — `hidden` is programmatically scrollable; the failure screenshot
     showed a scrolled viewport (scrollIntoView/focus moved the surface under the pointer).
  Plus `cameraSettled` in openFixture. 3 × 128 stress runs green after.
- Gates: vitest 373 ✓, typecheck ✓, biome ✓, Playwright 25 ✓ (+2 expected 10.7), cargo fmt/lint/test(534)/wasm ✓.
  `cargo docs` FAILS on a pre-existing unresolved `[instantiate]` link in crates/devices — no crate changes on
  this branch since 1854ff8; proposed as a separate ticket.
- Next: Oskari reviews the Story Validate gate.

## 2026-11-01 — resumed: Oskari asked to solve merge conflicts and continue
- The e10 stack was squashed onto `e10-s1/regression-harness` (ef0a0ad). My branch still sat on the old
  e10-s4 commit 1854ff8. Plan: `git rebase --onto origin/e10-s1/regression-harness 1854ff8` (replay only
  the 4 AES-68 commits), resolve conflicts, re-run the gates.

## 2026-11-01 — rebased onto e10-s1/regression-harness; conflicts resolved
- `git rebase --onto origin/e10-s1/regression-harness 1854ff8`. The only textual conflict was the Epic 10
  Progress line in IMPLEMENTATION_PLAN.md: kept the base's "10.3 ✅ complete" wording and added the 10.5 status.
- Semantic overlap: the base already graduated "populated rack drag" into placement-preview.spec.ts, so I
  removed my duplicate from stable-layers.spec.ts. The base's new "eject candidate" oracle is about dragging a
  unit off its rack, not U3 — no conflict.
- Gates: vitest 375 ✓, typecheck ✓, biome ✓, Playwright 25 ✓ (+2 expected 10.7), stress ×6 96/96 ✓,
  cargo fmt/lint/test(534)/wasm ✓; cargo docs still fails on the pre-existing `[instantiate]` link (AES-82).
- Push: the rebase rewrote already-pushed history, so it needs a force push
  (`git push --force-with-lease origin aes-68-story-10-5-resolved-presentation-stable`). The auto-mode
  classifier denied that; left for Oskari to run.

## 2026-11-01 — parked: waiting on I-164 (force-push)
- Waiting for Oskari to force-push the rebased branch (or allow me to):
  `git push --force-with-lease -u origin aes-68-story-10-5-resolved-presentation-stable`. After that:
  `fx mv AES-68 finished`. All work and gates are done; nothing else is pending.

## 2026-11-02 — done
- Oskari force-pushed the rebased branch (795a1e0). Story 10.5 is complete on top of e10-s1; all gates are
  green except the pre-existing `cargo docs` link (AES-82). Waiting on Oskari's Validate-gate sign-off.
