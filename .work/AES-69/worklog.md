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

## 2026-11-02 — Task 10.6.2 done (studio cables + portals from resolved connections)
- App composes `studioCableInput` (view-resolver.ts) from preview-aware resolved objects; patch.project
  uses the same frames + settled face side. Main passes in the cable band, foreground passes + portal chips
  in the overlay; clip chains via SilhouetteClips/CablePass (nested <g clip-path>, evenodd complements).
  Deleted sceneCableLayout, tipPatch, cableAnchor, bothInView/oneInView, portalOffset (+ their tests);
  contract matrix (socketCases/connectionCases) now runs through the resolver; +preview-following test.
- New browser spec cable-presentation.spec.ts (4) + "cable-portal" fixture.
- Learned: the generic Out jack sits near mid-height, so top/bottom exit is a near tie that measurement
  rounding at different camera fits can flip — browser test asserts "on an edge", pure tests own tie order.
- Assumption I-221: portal chips stay in the overlay SVG (not the HTML interaction div); foreground passes
  have no hit path.
- Gates: vitest ✓, typecheck ✓, biome ✓, Playwright 29 ✓ (+2 expected 10.7).
- Next: 10.6.3 workbench U2.

## 2026-11-02 — Task 10.6.3 done (workbench U2)
- benchCableInput (support-device silhouettes from slot frames; DUT omitted), BenchStage on the shared
  pipeline (main + new foreground svg), listening ring/badge from resolveEndpoint(output). Removed the
  whole legacy CableLayout/cableEndpoints/estimate path from cable-view.ts + tests.
- Browser: workbench badge test (add Synth Voice, listen to it, rotate → ring⇄badge). The added device
  lands below the fold, so the test clicks the rotate control via DOM rather than hovering.
- Gates: vitest 369 ✓, typecheck ✓, biome ✓, Playwright 30 ✓ (+2 expected 10.7).
- Next: 10.6.4 held leads.

## 2026-11-02 — Task 10.6.4 done (held leads); Story 10.6 complete pending review
- PatchState.srcPoint and pointerMove's srcAnchor removed; App + BenchStage derive `heldCable` via
  resolveHeldCable (lead / floating / hidden), drawn "in hand" in the z-5 band with the held pass mask.
  leadsShown/connShown deleted (resolver owns U1 suppression).
- Browser: held lead across a view switch and across a source flip (patch-gestures fixture).
- Found (pre-existing, for 10.7): a connected rear socket's centre is covered by its own cable's hit path
  (same as the old oneCable) — exact-socket-wins arbitration is 10.7.
- Gates: vitest 368 ✓, typecheck ✓, biome ✓, Playwright 32 ✓ (+2 expected 10.7), stress ×6 of
  cable/face/layers specs 126/126 ✓; cargo fmt/lint/test(534)/wasm ✓. `cargo docs` still fails on the
  pre-existing `[instantiate]` link in crates/devices (no crate changes on this branch; AES-68 proposed it).
- Next: Oskari reviews the Story Validate gate; PR draft in pr.md.
