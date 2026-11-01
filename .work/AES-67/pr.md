AES-67: Story 10.4 — Explicit faces & face-local socket geometry

Stacked on `e10-s1/regression-harness` (which now carries Stories 10.2 and 10.3). Every studio, workbench, and focus face now has an
explicit, stable identity. Cable anchors come from per-face socket geometry with revisions, replacing the
global transformed `[data-jack]` sweep and its timers.

## What changed

- **10.4.1, pure face model** (`view-resolution.ts`, `presentation-state.ts`). Face IDs name a rendered
  chassis slot:
  - each studio or supporting device has one slot;
  - the workbench DUT has independent `dut-front` and `dut-back` slots;
  - each focus window has its own non-patchable slot.

  Flips carry a per-object generation that only goes up. A finish or cancel from an older generation is a
  no-op.
- **10.4.2, lifecycle integration** (`face-lifecycle.ts`, App, BenchStage, Workbench, Chassis).
  - A flip settles through the `CSSTransition.finished` promise of the transition its generation started
    (Web Animations API), not through bubbled `transitionend` or a timer. A rapid reversal therefore can't
    settle the new generation.
  - When there's no transition to wait for (reduced motion, nothing visible, an empty rack), the flip
    settles synchronously.
  - While a device or its rack flips, every lead touching it is hidden: cable, tip patch, portal, and the
    held rubber-band. The pending pick and its banner stay.
  - Face slots carry `data-face-instance`, `data-interaction` and `data-flip-owner`.
  - Jacks under a disabled slot can't be picked (`patchableJackKey`). This fixes a latent bug where a
    focus-window jack could start a cable using the stage anchor of the same key.
- **10.4.3, socket-geometry index** (`socket-geometry.ts`).
  - Pure index of face-local mm geometry, keyed by `(face instance, side)`.
  - Revision and generation guard, with explicit outcomes: `current`, `stale`, `unmounted`, `empty`,
    `duplicate`.
  - Duplex sockets register once with both endpoint aliases.
  - `projectSocket` / `projectAnchors` produce the existing `JackAnchor` map, so `cable-view` is unchanged
    in shape.
- **10.4.4, DOM registration** (`face-geometry.ts`, `PatchController`).
  - Each face slot measures its sockets as ratios relative to its own frame, which cancels out zoom and all
    translation. The hidden side is un-mirrored.
  - It re-measures only when the slot's revision changes: descriptor ports or config, skin, layout size via
    ResizeObserver, or fonts finishing loading.
  - `jackAnchors` is now derived from that geometry plus each view's frames. Studio frames come from the
    projection; bench frames are the registered home origin plus the drag offset.
  - The rAF + 480 ms and 120 ms measure timers are gone.
- **10.4.5, browser matrix and cleanup.** The fixtures now wait on `[data-geometry]` instead of sleeping
  550 ms. `WorldApi.measureRoot` is renamed to `stageRoot`.

## Also fixes an inherited bug (from e10-s2)

Every structural edit (add/remove gear, connect, disconnect, cable type, workbench edits) threw
`DataCloneError` in the real worklet. `SceneSession` posted the full `TopologyRequest`, whose
`delta.payload` is the edit closure. `engine.ts` `send` now strips `loadPatch` transactions to their wire
metadata (`wireTransaction`). A new `studio-topology.spec.ts` drives the real session → worklet path,
which no test covered before. It's mutation-checked: it fails with the fix reverted.

## Review closely

- `web/src/face-lifecycle.ts`: ties each generation to its own transitions; a rack waits until all of its
  occupants' transitions finish.
- `web/src/face-geometry.ts` `measureFace`: the ratio math, the hidden-side un-mirroring, and the claim
  that the frame is exactly the chassis box. `.face-slot.frame` is now a real 100% × 100% box in the
  studio; focus slots stay `display: contents`.
- `BenchStage.svelte` home-origin projection. The bench stack's layout isn't pure, so the stack order is
  one of the revision inputs.
- Projection while a flip is in progress uses the durable facing (what the DOM shows), while the resolved
  face stays on its settled side. Leads are suppressed in that window anyway.

## Tests

- Vitest: 358 pass, including the new `socket-geometry.test.ts` and the extended `view-resolution.test.ts`.
  typecheck and Biome pass.
- Playwright: `face-lifecycle.spec.ts` (7 tests) and `socket-geometry.spec.ts` (2 tests) pass. The
  focus-isolation test was mutation-checked: it fails with the gate removed.
- Full browser suite is green: 20 pass, and the remaining known regressions stay marked as expected failures, owned by 10.5/10.7.
- Duplex sockets and differently configured instances of the same type are covered by pure tests only. No
  fixture device exercises them in the browser.
- `studio-topology.spec.ts` (1 test) passes against the real worklet.
- No Rust changes.

Program design: `docs/epics/epic-10/story-10.4-explicit-faces-socket-geometry.md`.
