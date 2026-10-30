# AES-67 worklog

## 2026-10-30 — session 1
- Read 10.4.1 WIP (view-resolution.ts, presentation-state.ts) and the studio/bench/chassis/patch code.
- Learned: flips are CSS transitions on Chassis `.flipper` (0.45 s); durable facing is written immediately;
  jacks are re-measured by rAF + 480 ms (studio/bench) / 120 ms (workbench) timers; focus windows render the
  same `data-jack` keys and are excluded only from measurement, not from pointer hits.
- Drafted Task 10.4.2 program design in the Story companion file; wrote plan.md.
- Next: get Oskari's sign-off on the 10.4.2 design (blocking), then implement.

## 2026-10-30 — Task 10.4.2 done (f6d115a)
- Oskari approved the design and authorised 10.4.3–10.4.5 without per-task review (I-131).
- Built: face-lifecycle.ts (WAAPI CSSTransition.finished per generation), transitionOwner, patchableJackKey,
  face-slot wrappers in App/BenchStage/focus windows, lead suppression, measure-on-settle, reduced-motion CSS.
- Learned: BenchStage, not Workbench, owns the bench presentation state (only it flips) — deviation from draft.
  `data-face` is already read by jack-anchors' `closest("[data-face]")`, so the slot uses `data-face-side`.
- Pre-existing: known-regressions "populated rack drag moves the occupant before release" now *passes*
  (verified on the base commit too) — Story 10.3's preview fixed it, but its `test.fail` marker was never
  graduated, so `pnpm test:browser` reports 1 failure. Not touched here; flagged to the project.
- Next: Task 10.4.3 pure socket-geometry index.
