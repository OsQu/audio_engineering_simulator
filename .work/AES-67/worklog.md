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

## 2026-10-30 — Task 10.4.3 done (a779db8); 10.4.4 in progress — PARKED (human takeover)
- 10.4.3: `web/src/socket-geometry.ts` + tests (pure index, revision/generation guard, explicit
  stale/unmounted/empty/duplicate outcomes, duplex aliases, `projectSocket`). 10.4.4 design is in the
  Story companion file.
- 10.4.4 WIP (this commit, NOT green — App/BenchStage/Workbench not yet migrated, so typecheck fails):
  - socket-geometry.ts: + optional `FaceGeometry.home` (bench un-offset origin), + `projectAnchors` (tested).
  - NEW face-geometry.ts: `measureFace` (rect-ratio, camera/translation invariant, hidden side
    un-mirrored) and the `faceGeometry` Svelte action (ResizeObserver + font-loadingdone revisions,
    measures only when settled, sets `data-geometry=pending|current` on the slot).
  - patch-controller: implements GeometryRegistrar; `jackAnchors` is now `$derived` from
    `geometry` × `project(slots)`; `measure()` was removed.
- Remaining for 10.4.4:
  1. App.svelte: make `.face-slot` a real box (width/height 100%, but keep focus slots `display: contents`),
     put `use:faceGeometry` on the studio slot (sides both, presented = effectiveFacing, settled =
     face.state === "presented", content = [typeId, ports, config values, skin finish]), call
     `patch.project(() => studio slots with frame = cableView.deviceSurfaceRect(sceneLayout(), id, worldApi))`,
     and delete the measure effect + `measureJacks` + the settle-callback rAF.
  2. BenchStage.svelte: `use:faceGeometry` on each `.device` slot (DUT: one side; support: both),
     homeOf = clientToSurface(box.left, box.top) − offsetFor(id), with the device order in `content`;
     project frame = home + offsetFor(id); delete the measure effect + settle rAF.
  3. Workbench.svelte: delete the rAF + 120 ms measure effect.
  4. jack-anchors.ts: remove `measureJacks` (keep `patchableJackKey`).
  5. Gates: pnpm typecheck/check/test/format and Playwright (PLAYWRIGHT_PORT=40131). The
     `test-browser/support/fixture-page.ts` 550 ms wait can become a wait for `[data-geometry="current"]`.
- Then 10.4.5: browser matrix (zoom, both flip directions, reversal/cancel, DUT duplicates, focus
  isolation with a focusable fixture, duplex, differently configured instances) + legacy cleanup.
- Known pre-existing: known-regressions "populated rack drag" passes unexpectedly (Story 10.3 fixed it) —
  recorded as project learning I-132.

## 2026-10-30 21:13 fx (broker, lane: factory)

- **Did:** The factory run was interrupted for a takeover by oskari before it wrote a worklog entry; its uncommitted changes are in the WIP commit that follows.
- **Next:** oskari continues interactively.

## 2026-11-01 — Task 10.4.4 done (resumed after takeover; no source changes from the takeover)
- Migrated App (studio frame = pure `deviceSurfaceRect`), BenchStage (frame = registered home + offsetFor),
  Workbench (timer effect removed); `measureJacks` sweep deleted; all rAF/480/120 ms measure timers gone.
- Fixture wait is now `geometrySettled` (no `[data-geometry="pending"]`) instead of a 550 ms sleep.
- New browser spec `socket-geometry.spec.ts`: studio lead stays on its sockets through wheel zoom and a
  body drag with an unchanged `data-geometry-generation`; workbench tap marker = DUT output socket through
  zoom, generations unchanged.
- Learned: fixtures never start the engine, so a patch can't commit there (topology needs acceptance) — use
  `cable-hover`'s pre-wired connections for lead assertions. The bench scene is DUT-only by design.
- Gates: vitest 355 ✓, typecheck/biome ✓, Playwright 14 ✓ + the pre-existing rack-drag unexpected pass.
- Next: 10.4.5 matrix + legacy cleanup.

## 2026-11-01 — Task 10.4.5 done; Story 10.4 complete pending review
- Browser matrix added (rack inheritance, mid-flip view switch, DUT slots, focus isolation); the focus test
  was mutation-checked (it fails with the `patchableJackKey` gate removed). First attempt was vacuous: the
  synth's jacks are on its back face, which a focus copy never shows, so the press is now dispatched onto the
  copy's jack element.
- Legacy cleanup: `measureRoot` → `stageRoot`, stale "measured jack anchor" comments rewritten.
- Final gates: vitest 355 ✓, typecheck ✓ (2 pre-existing FloatingWindow a11y warnings), biome ✓, Playwright
  18 ✓ + 1 pre-existing unexpected pass (known-regressions "populated rack drag" — Story 10.3 fixed it;
  I-132). No Rust changes on this branch, so the cargo gates are unaffected.
- Story 10.4 Validate gate: met, except duplex + differently-configured instances are pure-test-only
  (no fixture device for them in the browser). Left the Story marked 🚧 for Oskari to sign off.

## 2026-11-01 — fix: every structural edit threw DataCloneError (inherited from e10-s2)
- Symptom (Oskari): `+ Add` in the studio → `DataCloneError: … (candidate) => sceneOps.addDevice(…) could
  not be cloned` from `engine.ts send`.
- Cause: `topology-transaction.submit` spreads the proposal into the request, so `request.delta.payload`
  is the edit closure; `SceneSession.proposeTopology` (and the retry path) posted that request as
  `transaction`. It type-checked because `TopologyRequest` is a structural superset of
  `TopologyRequestMetadata`. Introduced by e06a557 on e10-s2 — add, remove, connect, disconnect, cable type,
  and workbench edits all failed in the real worklet.
- Why nothing caught it: the studio fixtures override `proposeTopology`, and the worklet smoke test posts a
  hand-built metadata-only request — nothing exercised session → port.
- Fix: `wireTransaction` (topology-protocol.ts) picks exactly the wire fields, applied once at the transport
  boundary in `engine.ts` `send`, so no caller can leak a closure.
- Tests: unit regression in topology-protocol.test.ts; new `studio-topology.spec.ts` adds gear through the
  real worklet (mutation-checked: fails with the fix reverted).
- Gates: vitest 356 ✓, typecheck/biome ✓, Playwright 19 ✓ + the pre-existing rack-drag unexpected pass.
