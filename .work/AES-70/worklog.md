# AES-70 worklog

## 2026-11-02 — planning
- Gap pass against 10.6 tip `0003465`: DOM-exact `[data-jack]` targets; four competing pointerdown starts
  (draggable, camera backdrop, window patch handler, cable onclick); drag release commits the last move's
  cached verdict; no pointer id/capture/cancel on patch; verdict reasons never rendered; focus windows
  already `data-interaction="disabled"`.
- Wrote companion design + thin Story block + this plan. Next: Oskari's design review before any code.

## 2026-11-02 — parked
- Asked Oskari to review the Story 10.7 design (I-222): decisions 1–7, especially #5 (illegal drag release
  → pending pick with visible reason). No answer within 15 min → parked. Waiting for design sign-off before
  implementing Task 10.7.1. No code written yet.

## 2026-11-02 — resumed
- Oskari approved the design as-is (I-222), including #5 (illegal drag release → pending + reason).
  Recorded in plan.md / companion / plan block. Picking up Task 10.7.1 (pure patch-targets.ts).

## 2026-11-02 — resumed after failed run
- Previous run died right after recording the I-222 approval (uncommitted). Committing that, then
  implementing Task 10.7.1. Branch now reported as based on `e10-s1/regression-harness`; the history
  still contains 10.2–10.6 (AES-64…69), so code baseline is unchanged.

## 2026-11-02 — Task 10.7.1 done
- `web/src/patch-targets.ts`: `resolveSocketTargets(CableResolutionInput)` (local, unsuppressed,
  presented, current, centre exposed; one target per physical socket) + `pickSocketTarget` (client px,
  8/12 px minimum, viewport/blocker clip, partial-exposure clip, exact > nearest > order > id).
- `web/test/patch-targets.test.ts`: 26 hand-calc cases. Web gates green (check, typecheck, 394 tests).
- Choice: an owner with no silhouette (bench DUT) gets order −1, i.e. anything above it occludes; the
  bench stack doesn't overlap so this is moot today but physically right.
- Next: 10.7.2 (classifyPress, pointer-session reducer, patching release re-resolution).

## 2026-11-02 — Task 10.7.2 done
- `gesture-ownership.ts` (classifyPress), `pointer-session.ts` (pure `step` + `beginPointerSession`
  driver), `patching.ts` release re-resolution, illegal → pending + `rejection`, `pointerCancel`,
  `aliasFor`. 419 web tests, check + typecheck green.
- Deviations from the draft (recorded in the companion file): no `pending` press fact (a pending pick
  only owns socket presses, so panning while a pick is pending still works); JackHit kept + `aliases`;
  session listens on window for move/up/cancel (the known regression dispatches pointerup on window).
- Next: 10.7.3 studio arbiter + adapters.

## 2026-11-03 — Task 10.7.3 done (resumed after a crashed run; the uncommitted work was intact)
- `stage-gestures.ts` capture-phase arbiter on the WorldView viewport; `patch.begin/hover/bindStage` on
  the shared pointer session; `beginMove` / `camera.beginPan`; cable select on release without move;
  rejection reason (overlay label + banner); `[data-pointer-blocker]` on toolbars, focus windows, banner,
  inspector; `WorldApi.pickFrame`. Both known regressions graduated (8/8 with --repeat-each=4).
- Learned: (1) the approved radius max(8, r) misses the max-zoom oracle press 4 px outside a ~15 px jack
  → added EDGE_HALO_PX 6 (I-232). (2) Fixture bug: TestSceneSession.proposeTopology structuredClone'd
  the $state proxy and threw — click-click could never draw a cable in fixtures; now snapshots.
  (3) Full browser run: 30/32; the 2 failures (worklet acceptance, rack-flip transitioning window) are
  parallel-load flakes — both pass in isolation, and the worklet spec passes on the base commit too.
- Next: 10.7.4 — workbench on the arbiter, delete the legacy window/draggable/startPan paths, browser matrix.

## 2026-11-03 — Task 10.7.4 done; Story 10.7 implemented
- BenchStage on the same `stageGestures` arbiter (bench targets incl. both DUT faces, beginMove, beginPan,
  cable select on release, rejection label). Workbench window keeps only capture-phase audio resume +
  `patch.hover`. Deleted the legacy paths: controller window handlers, `jackAnchors`/`project`,
  `draggable`/`DRAG_EXCLUDE`, `camera.startPan`, `jack-anchors.ts` (patchableJackKey).
- New `patch-matrix` fixture + `patch-gestures.spec.ts` (8 cases). Browser: 40/40 full suite (3 workers),
  60/60 on `--repeat-each=6` of the new + graduated specs. Web: check, typecheck, 420 unit tests green.
- Rust gate: `cargo docs` failed on a pre-existing broken intra-doc link in crates/devices/src/build.rs
  (`[instantiate]`, untouched since 37d94fe, same on main) — fixed the path; fmt/lint/test(534)/wasm/docs green.
- Bench test gotcha: the bench stack is taller than Playwright's 720 px window; the spec sets 1400×1400
  and picks a jack that is actually under the pointer.
- Not done here: the manual browser matrix (no interactive browser in the factory run) — flagged in the
  companion file and PR for Oskari.

## 2026-11-03 — wrap-up
- All four Tasks done; Rust + web + browser gates green (manual browser matrix left to Oskari). PR draft
  in `.work/AES-70/pr.md`. Pushing the branch and moving AES-70 to finished.

## 2026-11-06 — merge origin/e10-s1/regression-harness (ship request)
- Picking up to merge the upstream base (it now carries rebased/squashed 10.2–10.6) and resolve conflicts.
- Conflict: only `docs/IMPLEMENTATION_PLAN.md`, the Epic 10 **Progress** paragraph. Upstream reworded the
  10.3 sentence (✅ complete, graduated the rack-drag regression) and still said 10.7 "remains coarse";
  this branch said 10.7 delivers targets + the arbiter. Resolved: upstream's 10.3–10.6 wording + this
  branch's 10.7 status.
- Upstream also brought test-only changes (populated-rack-drag moved from stable-layers.spec.ts to the
  new placement-preview.spec.ts, extra placement/effective-placement unit tests, 10.2/10.3 doc notes) and
  removed `.work/AES-69/`; all taken as-is.
- Gates after merge: web check/typecheck green, 422 unit tests, browser 40/40 (3 workers); Rust fmt, lint,
  test (534), wasm, docs green.
