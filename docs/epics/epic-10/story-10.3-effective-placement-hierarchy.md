# Story 10.3 — Effective placement hierarchy & live previews — program design

> Companion to the Story 10.3 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Task 10.3.1 — Pure effective-placement resolution  · planned

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

## Task 10.3.2 — Pure preview resolution  · planned

_(program design drafted at pickup)_

## Task 10.3.3 — Studio parent-owned previews  · planned

_(program design drafted at pickup)_

## Task 10.3.4 — Workbench parent-owned previews & regression proof  · planned

_(program design drafted at pickup)_
