# Story 10.3 — Effective placement hierarchy & live previews — program design

> Companion to the Story 10.3 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Task 10.3.1 — Pure effective-placement resolution  · done

File-tree diff:

```text
web/src/
  effective-placement.ts          stored → effective placement and diagnostics
  projection.ts                   consumes effective placement through compatibility wrappers
web/test/
  effective-placement.test.ts     free, mounted, bench, and unresolved fixture matrix
```

Key types and signatures:

```ts
type EffectivePlacement =
  | { kind: "studio-free"; room: Room; space: string; wall: Wall; facing: DeviceFacing; position: Vec3 }
  | { kind: "studio-mounted"; room: Room; rack: Rack; space: string; wall: Wall; facing: DeviceFacing; uSlot: number }
  | { kind: "workbench"; x: number; y: number; facing: DeviceFacing }
  | { kind: "unresolved"; diagnostic: PlacementDiagnostic };

function resolveEffectivePlacement(
  scene: Scene,
  catalog: readonly DeviceDescriptor[],
  deviceId: string,
): EffectivePlacement;
```

`studio-mounted` validates its rack parent, descriptor, U-slot span, and room before exposing a physical
location. Projection owns the conversion of that effective location to top/elevation rectangles, retaining
the existing `FRAME_MARGIN` frame origin.

## Task 10.3.2 — Pure preview resolution  · done

`PlacementPreview` is an immutable overlay, not a mutated copy of `Scene`. A studio preview carries the
complete candidate free location, rack location, or mount target needed to resolve without reading transient
Svelte state. A preview rack is visible to every mounted child; a candidate mount inherits the target rack's
effective room, wall, and facing. An invalid target becomes `unresolved`, matching committed resolution.

Key types and signatures:

```ts
type PlacementPreview =
  | { kind: "studio-free"; deviceId: string; space: string; wall: Wall; position: Vec3 }
  | { kind: "studio-rack"; rackId: string; space: string; wall: Wall; position: Vec3 }
  | { kind: "studio-mount"; deviceId: string; rackId: string; uSlot: number }
  | { kind: "workbench"; deviceId: string; x: number; y: number };

function resolveEffectivePlacement(
  scene: Scene,
  catalog: DeviceDescriptor[],
  deviceId: string,
  preview?: PlacementPreview,
): EffectivePlacement;
```

The resolver applies a preview only when it targets the queried device or its parent rack. It does not
calculate drag legality or mutate/commit placement; `placement.ts` remains the command layer. Tests use the
same candidate values both as a preview and as a committed scene edit to assert equivalent effective output.

## Task 10.3.3 — Studio parent-owned previews  · done

`App` owns a `PlacementPreview | null` and derives stage items with that overlay. `WorldView` retains only
pointer mechanics: it reports a candidate and renders the parent-supplied resolved item rects. A normal
release asks `App` to commit the same candidate; pointer cancellation clears the preview without calling
the command path. This replaces the current local `{ id, x, y, legal }` drag state.

Call-stack (diff):

```text
draggable pointer move
- WorldView.localDrag = { id, x, y, legal }
+ WorldView.onPreviewCandidate(id, x, y, legal)
+   App.preview = placementPreviewAt(layout(), id, x, y)
+     placedItemsFor(layout(), preview)

draggable release / cancel
+ App.onPreviewCommit(...) → placement.moveTo(...)
+ App.onPreviewCancel() → preview = null
```

Key signatures:

```ts
function placementPreviewAt(
  ctx: LayoutCtx,
  id: string,
  x: number,
  y: number,
): PlacementPreview | null;

interface WorldViewProps {
  drag?: { id: string; legal: boolean } | null;
  onPreviewCandidate?: (id: string, x: number, y: number, legal: boolean) => void;
  onPreviewCommit?: (id: string, x: number, y: number) => void;
  onPreviewCancel?: () => void;
}
```

`placementPreviewAt` reuses the current elevation/top conversion and rack-slot selection rules but does
not write the scene. Its output is the exact input committed on normal release. Cables continue to use
committed geometry in this task; previewed chassis and rack descendants are the only renderer change.

## Task 10.3.4 — Workbench parent-owned previews & regression proof  · done

`Workbench` owns `PlacementPreview | null` for its flat bench, mirroring `App`'s studio ownership. It
derives each bench offset through `resolveEffectivePlacement(scene, catalog, id, preview)` and passes only
the resolved offset plus drag identity into `BenchStage`. `BenchStage` retains the shared pointer action,
but has no local `{ id, x, y }` state and never writes `scene.ui.bench`.

```text
draggable pointer move
  BenchStage.onPreviewCandidate(id, x, y)
    Workbench.preview = { kind: "workbench", deviceId: id, x, y }
      BenchStage.offsetFor(id) → resolveEffectivePlacement(..., preview)

pointer release / cancellation
  BenchStage.onPreviewCommit(id, x, y) → Workbench writes ui.bench[id], then clears preview
  BenchStage.onPreviewCancel()         → Workbench clears preview only
```

Proposed `BenchStage` prop seam:

```ts
offsetFor: (id: string) => { x: number; y: number };
drag?: { id: string } | null;
onPreviewCandidate?: (id: string, x: number, y: number) => void;
onPreviewCommit?: (id: string, x: number, y: number) => void;
onPreviewCancel?: () => void;
```

The final pure regression extends `effective-placement.test.ts`: a workbench preview resolves to the
same effective placement as committing its `x/y` into a cloned scene, while an absent preview leaves the
durable result unchanged. The existing shared-action cancellation path is used unchanged, so cancellation
has no command/write callback at either stage.

## Story Validate — close-out

- Mounted devices inherit room/wall/facing from their rack on every wall, ignoring stale copied fields
  (`effective-placement.test.ts`); top-view and per-wall projection stay covered by `projection.test.ts`.
- Rack previews move every occupant: pure (`effective-placement.test.ts`, `placement.test.ts`) and in the
  browser, where the Story 10.1 known regression `rack-preview-leaves-occupant-behind` started passing and
  was graduated to `web/test-browser/placement-preview.spec.ts`.
- Cancel is mutation-free and commit matches the final preview for free, rack, mount, eject, and workbench
  candidates (`placementPreviewAt` vs `moveTo` equivalence in `placement.test.ts`).
- Invalid references (missing rack/room/placement, bad U-slot span) resolve to `unresolved` diagnostics.
