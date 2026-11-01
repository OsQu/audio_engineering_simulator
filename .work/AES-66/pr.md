AES-66: Story 10.3 — Effective placement hierarchy & live previews

## What changed
- **`web/src/effective-placement.ts`** — one pure resolver: `studio-free` / `studio-mounted` / `workbench` /
  `unresolved`. Mounted devices take room, wall and facing only from their rack; copied placement fields stay
  readable for save compatibility but are no longer truth. Corrupt references yield diagnostics, never a
  mixed fallback.
- **`PlacementPreview`** — an immutable overlay (free device, rack, candidate mount, workbench offset) fed
  into the same resolver. A rack preview moves every occupant.
- **`placementPreviewAt`** (`placement.ts`) — produces the exact candidate that the normal release commits.
- **Ownership** — `App` (studio) and `Workbench` (bench) own preview state; `WorldView` and `BenchStage` only
  report candidates and render the resolved result. Cancel clears the preview without touching the scene.
- **Validate close-out** — eject-candidate and all-walls inheritance oracles; the Story 10.1 known regression
  "populated rack drag moves the occupant before release" now passes and moved to
  `web/test-browser/placement-preview.spec.ts`.

## Look closely at
- `resolveEffectivePlacement` precedence: a `studio-free` preview wins over a stored rack mount (eject), and a
  `studio-rack` preview applies only to the queried device's own parent rack.
- `placementPreviewAt` vs `moveTo`/`moveToTop`: they must keep computing the same location. The
  equivalence tests in `placement.test.ts` are the guard.
- The graduated browser test was owned by Story 10.5 in the 10.1 plan; 10.3 fixed it as a side effect.
- Cable endpoints and hit targets still use committed geometry while dragging. That is deferred to 10.6 and 10.7.

## Verification
Vitest 335 passed · Biome clean · svelte-check 0 errors · Playwright 10 passed (5 tracked expected
failures owned by 10.5/10.7). Web-only change; no Rust touched. Based on `e10-s1/regression-harness`, which now contains
Story 10.2 (AES-65).
