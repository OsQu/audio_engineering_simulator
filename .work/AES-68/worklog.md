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
