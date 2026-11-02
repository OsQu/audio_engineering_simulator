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
