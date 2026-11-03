AES-70: Story 10.7 — Unified patch targets & gesture lifecycle

Stacked on AES-69 (Story 10.6). Design approved by Oskari (I-222); the program design is in
`docs/epics/epic-10/story-10.7-unified-patch-targets-gesture.md`.

## What changed and why

Patching used DOM-exact hit tests on a tiny `[data-jack]` span, and four `pointerdown` starts competed:
`draggable`, the camera backdrop, the window patch handler and the cable `onclick`. This caused the last
two known regressions:

- a press near a socket started a device move;
- a drag release committed the *last pointer-move's* verdict instead of the release target.

This Story replaces both mechanisms with one model shared by the studio and the workbench.

- **Socket targets from the resolved view** (`patch-targets.ts`, pure).
  - `resolveSocketTargets` derives targets from the same `CableResolutionInput` that draws cables.
  - A socket counts only when its owner is local and not flipping, its face is presented, its geometry
    is current, and its centre isn't covered by a device painted above it.
  - `pickSocketTarget` works in client px after the camera. The order is exact hit, then nearest centre,
    then paint order, then socket ID.
  - It applies the 8/12 px mouse/touch minimum and clips by the viewport, by UI blockers, and by
    partial exposure.
  - It ignores legality, so an illegal nearest socket is never skipped.
- **One press owner per gesture** (`gesture-ownership.ts`, pure; `stage-gestures.ts`, DOM).
  - A capture-phase arbiter on each stage viewport classifies each press as control, socket, cable,
    device or pan, then calls exactly that owner.
  - The competing starts are deleted.
- **One pointer lifecycle** (`pointer-session.ts`).
  - It guards pointer ID and primary button, and uses a 4 px (mouse/pen) or 8 px (touch) move threshold.
  - Release is separate from cancel (`pointercancel`, lost capture, blur, unmount), and a cancel never
    commits.
  - Ownership is cleared before capture is released.
  - Patching, device moves (`beginMove`), pan (`camera.beginPan`) and cable select all run on it.
- **Release re-resolves** (`patching.ts`).
  - Both modes pick the target at the release point, so drag+drop and click+click return identical
    verdicts.
  - New, approved behaviour: an illegal release keeps the pick as pending and shows the reason (an
    overlay label and the banner). It no longer silently drops the cable.
- **Legacy removed:** `jack-anchors.ts` (`patchableJackKey`), `draggable` / `DRAG_EXCLUDE`,
  `camera.startPan`, the controller's window handlers, and the now-unused `jackAnchors` / `project`.

## Look at closely

- **The edge halo (I-232, deviation from the approved formula).** The hit radius is
  `max(8|12 px, rendered r + 6 px)`. With the approved `max(8, r)`, the max-zoom oracle failed: a press
  4 px outside a ~15 px jack started a device move. Tune `EDGE_HALO_PX` in `patch-targets.ts` if it feels
  too generous.
- **A pending pick owns only socket presses (I-230).** The approved design had every press during a
  pending pick go to the patch controller. That would have stopped panning while holding a pick.
- **The capture phase swallows owned presses** (`stopPropagation` in `stage-gestures.ts`). Any widget
  with its own `pointerdown` must match `CONTROL_SELECTOR`. Native controls and ARIA sliders/switches
  already do, and `[data-control]` exists as an escape hatch. Toolbars, focus windows, the banner and the
  cable inspector carry `data-pointer-blocker`.
- **Fixture bug fixed:** `TestSceneSession.proposeTopology` called `structuredClone` on the `$state`
  proxy and threw. No fixture patch commit could ever land a cable before this.
- **Out-of-ticket:** a pre-existing broken intra-doc link (`[instantiate]` in `crates/devices/src/build.rs`,
  also broken on `main`) made `cargo docs` fail. I fixed the path.

## Validation

- Web:
  - `pnpm run check` and `typecheck` pass, and all 420 unit tests pass;
  - new pure tests: `patch-targets`, `gesture-ownership`, `pointer-session`, and an extended
    `patching.test.ts`.
- Browser:
  - full suite 40/40 with 3 workers;
  - 60/60 for `patch-gestures.spec.ts` plus the graduated `known-regressions.spec.ts` at
    `--repeat-each=6`;
  - the known-regression list is empty.
  - Under heavier parallelism, `worklet-patch-acceptance` and the rack-flip lifecycle test can flake.
    Both pass in isolation, and the worklet one also passes on the base commit.
- Rust: `cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs` all pass.
- **Not done: the manual browser matrix** from the source design: stock scene, min/default/max zoom,
  touch on a real device. Please run it before signing off the Validate gate.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
