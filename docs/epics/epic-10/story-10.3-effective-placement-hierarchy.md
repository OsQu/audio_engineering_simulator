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

## Task 10.3.3 — Studio parent-owned previews  · planned

_(program design drafted at pickup)_

## Task 10.3.4 — Workbench parent-owned previews & regression proof  · planned

_(program design drafted at pickup)_
