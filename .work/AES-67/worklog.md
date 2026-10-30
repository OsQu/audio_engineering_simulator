# AES-67 worklog

## 2026-10-30 — session 1
- Read 10.4.1 WIP (view-resolution.ts, presentation-state.ts) and the studio/bench/chassis/patch code.
- Learned: flips are CSS transitions on Chassis `.flipper` (0.45 s); durable facing is written immediately;
  jacks are re-measured by rAF + 480 ms (studio/bench) / 120 ms (workbench) timers; focus windows render the
  same `data-jack` keys and are excluded only from measurement, not from pointer hits.
- Drafted Task 10.4.2 program design in the Story companion file; wrote plan.md.
- Next: get Oskari's sign-off on the 10.4.2 design (blocking), then implement.
