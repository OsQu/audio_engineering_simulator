# AES-69 worklog

## 2026-11-01 — Story 10.6 planned; design awaiting review
- Did: story-planning gap pass against the AES-68 tip (ed42be0). Wrote companion design
  `docs/epics/epic-10/story-10.6-unified-cable-portal-held-lead.md` (baseline, 10.6.1 program design, task
  sketches), thin Story block + Epic progress line in IMPLEMENTATION_PLAN.md, plan.md.
- Learned: three endpoint rules exist today (cableEndpoints w/ clamp, cableAnchor for portals, cached
  srcPoint for held). Cables use committed rects (no preview) even though studioObjects is preview-aware.
  Tip patch clips only to the owner rect (no subtraction of devices above). Hit paths ignore visibility.
  SVG `mask` doesn't clip hit testing → use nested clip-paths.
- Next: Oskari reviews design + held-lead occlusion question; then Task 10.6.1.

## 2026-11-01 — Parked on design review (I-163)
- Waiting for Oskari to approve the 10.6 design and answer the held-lead occlusion question
  (A "in hand", recommended / B fully physical). No implementation started.
- Next: on approval, start Task 10.6.1 (pure cable-resolution.ts).

## 2026-11-02 — Design approved (I-163: "A"); picking up Task 10.6.1
- Oskari approved the design with held-lead option A ("in hand"). Recorded in plan.md, companion, plan block.
- No tree changes since the parked WIP commit.
- Next: 10.6.1 pure cable-resolution.ts + tests.

## 2026-11-02 — Task 10.6.1 done (pure cable-resolution.ts)
- `cable-resolution.ts`: resolveEndpoint (socket/exit/off-view/suppressed/pending/unresolved), nearestEdge +
  edgeExit (tie bottom→right→left→top, EXIT_LEAD 24), settledPasses (main/foreground), passVisibleAt +
  silhouetteContains (rounded), resolveConnection(s) (full/portal/none), resolveHeldCable (held pass, A).
  `connections.ts`: cableRoute (exit lead + cubic). Input takes ProjectedSlot[] per object — the same shape
  patch.project already builds, so studio + bench feed it identically.
- 21 tests (hand-calc exits, mask matrix, portal invariance, held lead follows device). Nothing consumes it yet.
- Gates: vitest 394 ✓, typecheck ✓, biome ✓.
- Next: 10.6.2 studio rendering.
