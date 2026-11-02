AES-68: Story 10.5 — Resolved presentation, stable layers & rack assemblies

Stacked on `e10-s4/explicit-faces-socket-geometry`. Physical stacking is now resolver-owned. Hover, keyboard
focus, toolbar grace and drag can no longer change which chassis covers which. Toolbars live in a separate
interaction layer keyed by object ID. Eject and rack removal follow the approved policy U3 and are
all-or-nothing.

## What changed

- **10.5.1, pure resolved objects** (`view-resolution.ts`). `resolveStudioObjects` and `resolveBenchObjects`
  give every rack and device:
  - a semantic paint layer (`furniture` / `rear-object` / `front-object`);
  - a total order `(layer, scene index, id)`;
  - its rack parent;
  - an opaque silhouette, which Story 10.6 will use for cable masks.

  Interaction state is not an input. A device's layer follows its *settled* face, so a flip never reorders
  mid-animation. A rack is always furniture, so it always paints behind its occupants.
- **10.5.2, studio** (`WorldView`, `App`).
  - Items paint in resolved order, with one fixed z band per layer (`LAYER_Z`). The `drag ? 10 : chrome ? 6`
    overrides are deleted.
  - Toolbars and the drag/legality outline moved into an `.interaction` layer (`data-chrome-for`,
    `data-drag-outline`). A rack's outline is its frame, so it covers the whole assembly without hiding the
    occupants.
  - `PlacedItem.z` is removed.
- **10.5.3, workbench** (`BenchStage`).
  - Same contract. The both-faced DUT sits below the cable pass, as it did before.
  - Toolbars sit at the group's layout home (`offset*`) plus its drag offset.
  - The viewport pan ignores presses on the interaction layer.
- **10.5.4, U3 ejection** (`ejection.ts`).
  - A pure planner searches the `GRID_MM` lattice in the rack's own wall, nearest first by squared distance,
    ties broken by y then x, within the room bounds. Occupants are placed in slot order, and each placed
    one becomes an obstacle for the next.
  - Room, wall and facing come from the rack.
  - Failure leaves the scene untouched and is reported on the status line.
  - Eject and remove-rack are disabled while the rack is flipping.
  - This also fixes a bug: `unmount`/`removeRack` used to restore a stale stored position and the device's
    own facing.

## Also fixes (found while stress-running the browser suite)

- The revealed toolbar bar is a solid hit target. Before, a gap between two buttons fell through to the rack
  beneath, which then took over the open toolbar.
- The toolbar reveal slides down into place instead of up. While sliding up, the bar overlapped the top of
  the chassis and swallowed drag presses there.
- Stage viewports use `overflow: clip` instead of `hidden`. A hidden box can still be scrolled by
  focus()/scrollIntoView, which shifted the camera surface out from under the pointer.

## Review closely

- `view-resolution.ts` `resolveStudioObjects`: the layer rules and the total order.
- `ejection.ts`: obstacle selection (the removed frame is excluded; a retained rack and gear already placed
  by the plan are included) and atomicity.
- `WorldView`/`BenchStage` toolbar reveal. It is now state (enter or focus on the chassis or its toolbar,
  with a shared 300 ms grace), not CSS `:hover`. Tab order changed: toolbars now come after the stage
  objects (approved).
- `benchHome` in BenchStage. It is a small DOM adapter, because the bench's flow layout isn't pure.

## Tests

- Vitest: 373 pass. New: `object-resolution.test.ts` (10) and `ejection.test.ts` (8 hand-calc oracles).
  The `scene-ops`/`projection` tests were updated.
- Playwright: 25 pass, plus the 2 expected failures owned by Story 10.7. New: `stable-layers.spec.ts` (7)
  and `ejection.spec.ts` (2). The rack-hover, rear/front cable-hover and populated-rack-drag regressions
  graduated from known-regressions into required tests. The layer specs are mutation-checked: putting the
  z overrides back fails them.
- Stress: three runs of stable-layers, face-lifecycle and ejection with `--repeat-each=8`, 384/384 green.
- Rust: no crate changes. fmt, lint, test (534), and wasm are green. **`cargo docs` fails on the base
  already** (unresolved `[instantiate]` link in crates/devices); proposed as AES-82.

Program design: `docs/epics/epic-10/story-10.5-resolved-presentation-stable-layers.md`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
