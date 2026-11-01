# Story 10.5 — Resolved presentation, stable layers & rack assemblies — program design

> Companion to the Story 10.5 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Landed baseline (gap pass, 2026-11-01)

What the code does today, which this Story changes:

- **Stacking is per-item CSS z, overridden by interaction state.** `placedItemsFor` (projection.ts) emits
  `z: 0` for racks, `1`/`3` for rear/front devices around the `z 2` cable layer. `WorldView` then overrides
  it inline: `drag ? 10 : openChromeId ? 6 : it.z`. `BenchStage` repeats the same override on
  `.device-group` with its own `zBase`. These overrides are the root of every hover/drag occlusion failure.
- **The toolbar is a child of the chassis.** `DeviceChrome` sits inside `.device` (or `.device-group`). The
  chassis forms the stacking context, so the toolbar can only rise if the whole chassis rises with it.
- **Rack frames and occupants are siblings, not parent and child.** Since Story 10.3 the preview already
  moves every occupant (known regression `rack-preview-leaves-occupant-behind` now passes, see I-132), and no
  DOM stacking context traps the occupants. Rack parentage exists only implicitly through
  `effective.kind === "studio-mounted"`.
- **Order within one z band is the `items` array order:** racks, then devices in scene order. That is already
  stable, but nothing states it or tests it.
- **Eject and remove-rack are not U3.**
  - `sceneOps.unmount` clears `place.rack` and keeps the stale stored `position`, from before the device was
    mounted. It also keeps the device's own stale `facing`, which may differ from the rack's.
  - `removeRack` does the same for every occupant.
  - Neither one computes a legal free position. The gear can land overlapping other gear, or somewhere
    unrelated to where it visibly was.

## Task 10.5.1 — Pure resolved objects, paint layers & silhouettes  · done

This task gives each object a deterministic physical identity. The total paint order, the semantic layer,
the rack parentage and the opaque silhouette all come from one pure function. Hover, focus, toolbar grace
and drag are **not inputs**, so they can't change the order, and the types enforce that.

File-tree diff:

```text
web/src/
  view-resolution.ts      + ResolvedObject, PaintLayer, resolveStudioObjects, resolveBenchObjects,
                            compareObjects (total order), silhouetteOf
  projection.ts           unchanged here; PlacedItem.z is deleted in 10.5.2 once WorldView stops reading it
web/test/
  object-resolution.test.ts  layer/order matrix per view, preview-does-not-reorder, flip-uses-settled-side,
                             rack-below-occupants invariant, top view, bench DUT/support, silhouettes
```

Key types and signatures:

```ts
/** Semantic physical layers, bottom → top. The renderer maps each to a CSS z band. */
type PaintLayer =
  | "furniture"        // rack frames (and top-view footprints of racks)
  | "rear-object"      // back-shown chassis: below the main cable pass
  | "cable"            // main cable pass (Story 10.6 owns its content)
  | "front-object"     // front-shown chassis: above the main cable pass
  | "cable-foreground" // front-socket lead redraw (today's tipPatch; 10.6 owns masks)
  | "interaction";     // toolbars, drag/legality outlines: never physical

interface ResolvedObject {
  readonly id: string;
  readonly kind: "rack" | "device";
  /** The rack this device is mounted in (the effective one, including a candidate mount preview). */
  readonly parentId: string | null;
  readonly layer: Exclude<PaintLayer, "cable" | "cable-foreground" | "interaction">;
  /** Index into the total order (layer rank, scene order, id). Later covers earlier. */
  readonly order: number;
  /** World mm (y-up), as WorldView consumes it today. null where layout is not pure (bench flow). */
  readonly rect: Rect2 | null;
}

interface Silhouette {           // the opaque outer clip; 10.6 consumes these for cable masks
  readonly objectId: string;
  readonly rect: Rect2;          // same space as ResolvedObject.rect
  readonly radius: number;       // matches `.content` border-radius (CHASSIS_RADIUS_MM = 6)
}

function resolveStudioObjects(input: {
  ctx: LayoutCtx;
  faces: ReadonlyMap<string, ResolvedFace>;   // layer follows the *settled* face side
  preview?: PlacementPreview;
}): ResolvedObject[];                          // already sorted by `order`

function resolveBenchObjects(input: {
  scene: Scene; dutId: string; faces: ReadonlyMap<string, ResolvedFace>;
}): ResolvedObject[];                          // rect: null; DUT = front-object

function silhouetteOf(object: ResolvedObject): Silhouette | null;
```

Rules (each one gets a table row in the tests):

- A rack is `furniture` in both facings, so it always paints behind its occupants. A mounted device takes its
  layer from its **rack's** settled side, a free device from its own.
- A device's layer follows the resolved face's **settled** side (`face.face`), not the durable facing that a
  flip writes at once. It switches when the flip settles, which is the same moment its leads reappear (U1).
  This choice avoids a front↔rear reorder in the middle of an animation.
- Order is `(layer rank, scene index, id)`. The scene index is the position in `scene.ui.racks` for racks and
  in `scene.patch.devices` for devices. A preview changes a rect and a parent, never an index.
- Top view: rack footprints are `furniture` and free devices are `front-object`. No mounted device is
  emitted (as today).
- Bench: the DUT and front-shown support devices are `front-object`, back-shown support devices are
  `rear-object` (today's `zBase`). The rect stays null because the bench is a DOM flow layout. Its
  silhouettes come from registered frames in 10.6.

*As built:* the dependency runs `view-resolution → projection`. `resolveStudioObjects` consumes
`placedItemsFor` for its rects and view membership, so wrapping the other way would have created an import
cycle. `resolveBenchObjects` takes the resolved face list, not a map.

Rejected alternative: keep the numeric `z` on `PlacedItem` and only delete the overrides. That would fix the
symptom, but the order would stay implicit and the layers would have no names for 10.6's mask sets to refer
to.

## Task 10.5.2 — Studio renders resolved order + interaction overlay  · planned (design drafted)

Call-stack (diff):

```text
App.svelte
-  placedItems = placedItemsFor(layout(), preview)                 // z by facing
+  studioObjects = resolveStudioObjects({ ctx: layout(), faces: studioFaces, preview })
   committedItems = placedItemsFor(layout())                       // collision set, unchanged
   <WorldView items={studioObjects} …>
WorldView.svelte
   {#each items as it (it.id)}                                     // resolved order = DOM order
-    <div class="device" style="z-index: {drag ? 10 : chromeOpen ? 6 : it.z}">
-      <div class="content">…</div><DeviceChrome>…</DeviceChrome>
+    <div class="device" data-paint-layer={it.layer} style="z-index: {LAYER_Z[it.layer]}"
+         onpointerenter/leave, onfocusin/out → showChrome/hideChromeSoon>
+      <div class="content">…</div>
   {/each}
+  <div class="interaction" style="z-index: {LAYER_Z.interaction}">  // pointer-events: none
+    {#if drag} <div class="drag-outline" class:illegal …rect of drag.id/>  // rack ⇒ assembly bounds
+    {#each items as it (it.id)}
+      <div class="object-chrome" data-chrome-for={it.id} class:open={openChromeId === it.id}
+           style="left/bottom/width from it.rect" onpointerenter/leave, onfocusin/out>
+        <DeviceChrome>{@render controls(it.id)}</DeviceChrome>
```

Key shapes:

```ts
// WorldView: one fixed band per semantic layer; the cable SVGs keep their bands.
const LAYER_Z: Record<PaintLayer, number> = {
  furniture: 0, "rear-object": 1, cable: 2, "front-object": 3, "cable-foreground": 5, interaction: 8,
};
interface WorldItem { id: string; rect: Rect2; layer: ObjectLayer }  // `background` ⇒ layer === "furniture"
```

- **No interaction state reaches a physical z.** `.device.dragging` keeps only its cursor. The
  shadow/outline treatment moves to `.drag-outline`. For a rack, that outline is the frame rect, which already
  bounds its occupants, so the feedback follows the assembly without covering it.
- **Toolbar reveal is state, not CSS `:hover`,** because the toolbar is no longer a descendant. Enter or
  focus on either the chassis or its toolbar opens it. Leaving either one starts the same 300 ms grace.
  `DeviceChrome` loses its host-relative `z-index: 4`. The bar itself stays `pointer-events: none`, and only
  its buttons and selects take hits, so the hit box is the controls only.
- `PlacedItem.z` is deleted, and the projection tests stop asserting it. Layer coverage now lives in
  `object-resolution.test.ts`.
- Regression graduation (`known-regressions.spec.ts`):
  - **Removed:** `rack-hover-occludes-occupant`, both `*-device-hover-reorders-cables`, and the stale
    `rack-preview-leaves-occupant-behind`.
  - **Moved and rewritten** into `test-browser/stable-layers.spec.ts`. The cable cases assert that
    `data-paint-layer` and the computed z are unchanged across hover, and the layer is `rear-object`/
    `front-object`. The rack case keeps `topmostDeviceAt`. The populated-rack-drag case moves there as a
    required pass.
- New browser cases in `stable-layers.spec.ts`:
  - hover rack → wait past the grace → focus the occupant → drag the rack: at each step,
    `topmostDeviceAt(occupant centre)` is the occupant;
  - the occupant's toolbar buttons are `elementFromPoint`-topmost at their own centres;
  - while the rack is hovered, the occupant's control centre still resolves inside the occupant.

## Task 10.5.3 — Workbench renders resolved order + interaction overlay  · planned

_(program design drafted at pickup)_ The same contract applied to `BenchStage`. Its toolbars are positioned
from the registered home origin + offset frame, the same frame 10.4.4 already projects anchors from.

## Task 10.5.4 — U3 atomic ejection  · planned

A pure planner computes **every** ejected free placement before anything is written. The command then
either applies all of them or none.

File-tree diff:

```text
web/src/
  ejection.ts             NEW  planEjection (pure), applyEjection (writes through the scene proxy)
  scene-ops.ts            unmount / removeRack → plan, then apply-or-fail; return EjectionResult
  App.svelte              eject/remove-rack report failure; chips disabled while owner is transitioning
web/test/
  ejection.test.ts        hand-calc nearest-grid oracles, tie-breaks, multi-occupant, failure atomicity
```

Key types and signatures:

```ts
type EjectionRequest =
  | { kind: "device"; deviceId: string }   // retained rack IS an obstacle
  | { kind: "rack"; rackId: string };      // removed frame is NOT an obstacle; occupants in slot order

type EjectionPlan =
  | { ok: true; placements: Readonly<Record<string, Placement>> }   // free: no `rack`
  | { ok: false; reason: "not-mounted" | "unresolved" | "no-free-space"; deviceId?: string };

function planEjection(scene: Scene, catalog: DeviceDescriptor[], req: EjectionRequest): EjectionPlan;
function applyEjection(scene: Scene, req: EjectionRequest, plan: EjectionPlan & { ok: true }): void;
```

Search (view-independent, so removing a rack from the top view uses the rack's wall):

- Work in the rack's own wall elevation. The origin is the device's effective slot rect `(x₀, y₀)`, computed
  the same way `deviceRect` does it.
- Candidates are the `GRID_MM` lattice with `0 ≤ x ≤ wallLength − w` and `0 ≤ y ≤ room.height − h`. The
  wall length is `room.width` for front/back and `room.depth` for left/right.
- Candidates are sorted by `(x−x₀)² + (y−y₀)²`, ties broken by smaller y, then smaller x.
- Obstacles are the elevation rects of every other free device, rack and mounted device on that space and
  wall. Devices already allocated by this plan count too. The ejectee itself never does. The removed rack is
  excluded, but a retained rack is included.
- The result sets `space`, `wall` and `facing` from the rack (effective truth). The position is
  `elevationToWorld(rack.position, size, wall, room, x, y)`, so the depth coordinate sits flush with the
  rack. The device is no longer mounted.
- If any occupant has no candidate, `ok: false` and the scene is untouched.
