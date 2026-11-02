# Story 10.6 — Unified cable, portal & held-lead presentation — program design

> Companion to the Story 10.6 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Landed baseline (gap pass, 2026-11-01, against the AES-68 / 10.5 tip `ed42be0`)

What the code does today, which this Story changes:

- **Three cable paths, three endpoint rules.** `cable-view.ts` resolves a full cable's ends through
  `cableEndpoints` (with the back-shown clamp). A portal stub uses `cableAnchor` alone, so it skips the
  clamp. The held lead uses a cached absolute `PatchState.srcPoint`, which is captured at press time and
  never re-resolved.
- **Hidden-face ends are interior estimates.** `resolveAnchor` falls back to 38 % / 62 % of the chassis
  width at 45 % height. `clampEnd` then cuts the *straight segment* at the rect, but the sagging *cubic* is
  drawn afterwards, so the curve can re-enter the chassis. When both ends share one rect, the estimate is
  left unchanged. A front-shown device never clamps; its panel paints over the estimate.
- **Masks are CSS-workaround callbacks.** `CableLayout.clampsEstimate` / `frontPatchOver` and
  `tipPatchEnd` choose the "tip patch". That is a full-path redraw at z 5, clipped to the owner's square
  rect. It is **not** reduced by front devices ordered above the owner, so a lead can paint across a
  different foreground device.
- **Portal stubs paint above everything.** The whole stub and its chip sit in the z 5 overlay. A
  hidden-face portal therefore shows its interior estimate on the wrong face. The persisted offset is
  added to that estimate.
- **Cables don't follow previews.** `sceneLayout().rect` and the `patch.project` frames use the
  *committed* `deviceRect` with no preview. `studioObjects` already has preview-aware rects (10.3/10.5),
  but nothing in the cable path reads them.
- **Hit paths ignore visibility.** `.cable-hit` strokes the whole path, so segments hidden under a
  hidden-owner chassis or outside a tip patch can still be clicked.
- **Workbench uses mirrored see-through attachments** (`benchCableLayout`: everything anchorable, no
  rect, no clamp, no patch). It does not use exits. Its listening marker is a bare circle at the monitored
  jack, even when that jack is on a hidden face. Bench objects have `rect: null`, because the stack is DOM
  flow. Its surface frames exist only as slot homes plus offsets inside `BenchStage`.
- **Already in place and reused:** the face-local socket geometry index with both sides registered per
  studio face (10.4), `projectSocket(…, presented=false)` (see-through mirroring), `physicalLeadsSuppressed`
  (U1, 10.4), resolved objects with layer, total order and silhouettes (10.5), `LAYER_Z` bands
  (`cable` 2 < `front-object` 3 < `cable-foreground` 5 < `interaction` 8).

## Coordinate space

All cable resolution happens in **surface coordinates**. These are the stage's own y-down space, which the
camera only scales and translates: room mm (`ROOM_HEIGHT − y`) in the studio, natural px in the workbench.
They are camera-independent and match where the geometry index's anchors already live. The studio
converts `ResolvedObject.rect` (world mm, y-up) once through `worldToSurface`. The workbench supplies its
slot frames directly. Nothing downstream uses world coordinates.

## Task 10.6.1 — Pure endpoint & connection resolution  · done

This task adds one pure module that turns *(objects, surface frames, faces, geometry, connections,
portal offsets)* into resolved endpoints, paths and mask passes. It has no Svelte, no DOM and no clock.
Rendering is unchanged in this task, and nothing consumes the module yet.

File-tree diff:

```text
web/src/
+ cable-resolution.ts      endpoint states, nearest-edge exits, paths, mask passes, portal chips,
                           pass-visibility predicate, resolveConnections, resolveHeldCable
  connections.ts           + cablePathFrom(start, end, leads) — the cubic preceded by optional exit leads
web/test/
+ cable-resolution.test.ts hand-calc exit oracles, mask-set matrix, invariants
```

Key types & signatures:

```ts
/** A chassis outline in surface coords — the clip both passes and hit paths consume. */
interface SurfaceSilhouette { objectId: string; rect: SurfaceRect; radius: number; order: number;
                              layer: ObjectLayer }

interface CableResolutionInput {
  silhouettes: ReadonlyMap<string, SurfaceSilhouette>;   // preview-aware; only objects in this view
  /** The resolved face per object: its id (geometry slot), settled presented side, state. */
  faces: ReadonlyMap<string, ResolvedFace>;
  /** Sides the stage presents for an object — studio: [settled side]; bench DUT: both. */
  presentedSides(objectId: string): readonly DeviceFacing[];
  geometry: SocketGeometryIndex;
  /** Is this object local to the view? (studio: same space+wall; bench: always) */
  local(objectId: string): boolean;
  /** U1: the object or its rack is mid-flip. */
  suppressed(objectId: string): boolean;
}

type Edge = "bottom" | "right" | "left" | "top";      // also the tie order

type EndpointPresentation =
  | { kind: "socket";  objectId; point: SurfacePoint; side: DeviceFacing } // on a presented face
  | { kind: "exit";    objectId; edge: Edge; point: SurfacePoint;           // hidden face: on the edge
                       lead: SurfacePoint }                                  //  + EXIT_LEAD outward
  | { kind: "off-view";  objectId }   // portal address — no physical point
  | { kind: "suppressed"; objectId }  // U1 transition
  | { kind: "pending";   objectId }   // geometry not current yet
  | { kind: "unresolved"; objectId }; // no face / no socket alias / no silhouette → diagnostic

resolveEndpoint(input, ref: PortRef, direction: "input" | "output"): EndpointPresentation
nearestEdge(rect: SurfaceRect, p: SurfacePoint): Edge   // ties: bottom, right, left, top

type CablePass =
  | { kind: "main"; exclude: readonly string[] }                     // hidden-face owners
  | { kind: "foreground"; owner: string; include: string;            // front socket owner
      exclude: readonly string[] };                                  //  minus silhouettes above it

interface CablePath { d: string; start: SurfacePoint; end: SurfacePoint }

type ResolvedConnection =
  | { kind: "full";   key; from: EndpointPresentation; to: EndpointPresentation;
      path: CablePath; passes: readonly CablePass[] }
  | { kind: "portal"; key; portalKey; local: EndpointPresentation; remote: string /*device*/;
      chip: SurfacePoint; path: CablePath; passes: readonly CablePass[] }
  | { kind: "none";   key; reason: "not-local" | "suppressed" | "pending" | "unresolved" };

resolveConnections(input, connections: readonly Connection[],
                   portalOffsets: Readonly<Record<string, PortalOffset>>): ResolvedConnection[]

/** The same visibility predicate the SVG clips implement; tests and (10.7) hit testing use it. */
passVisibleAt(pass: CablePass, silhouettes, p: SurfacePoint): boolean
```

The decisions this shape encodes:

- **Exit edge is local-only.** `nearestEdge` runs on the hidden socket's *see-through* projection
  (`projectSocket(…, presented=false)`, the position the socket physically has from the viewer's side)
  against the owner's silhouette rect. The other endpoint is not an input, so moving a portal or remote
  device can never swap the side. Oracle: a frame at (100, 200), 482.6 × 88.9, with a socket at
  (130, 260) gives distances left 30, right 452.6, top 60, bottom 28.9, so the edge is **bottom**, the exit
  is (130, 288.9) and the lead is (130, 288.9 + `EXIT_LEAD`). A 100 × 100 frame with the socket at its
  centre is a four-way tie, which resolves to **bottom**.
- **`EXIT_LEAD = 24` surface units**, a straight outward-normal segment before the sag. The path is
  `M exit L lead C …` with the cubic computed from the lead points (`cableControlPoints` unchanged). A
  hidden end therefore never draws through its own chassis, and a curve that swings back over it is still
  masked (main pass `exclude`).
- **Mask passes (design decision 6, points 3–5).**
  - Every drawable cable gets one `main` pass. Its `exclude` lists each endpoint owner whose end is an
    `exit`.
  - Each `socket` endpoint on a `front-object` owner adds a `foreground` pass. That pass is `include` =
    the owner, with `exclude` = every *device* silhouette with a greater `order` (furniture never covers
    a front device).
  - Rear sockets need no foreground pass, because rear objects already sit under the `cable` band.
  - Portal stubs get exactly the same passes as full cables.
- **Portal chip = local physical end + persisted offset.** For a `socket` end the base is the socket
  point; for an `exit` end it is the `lead` point. The default offset is unchanged (`±PORTAL_LEN`, 36).
  The stub path runs from that base to the chip. An offset saved against the old interior estimate
  therefore shifts slightly on load. This is accepted; the scene schema is unchanged.
- **Non-drawable states are explicit.** A connection with any `suppressed` / `pending` / `unresolved` end,
  or with no local end, is `kind: "none"` with a reason, not a fallback point. `unresolved` also records a
  diagnostic.
- **`resolveHeldCable` lives here, not in `patching.ts`.** It keeps the scene-side resolver free of the
  patch controller (source design §2). Its shape is drafted at 10.6.4 pickup. 10.6.1 lands the endpoint
  function it will reuse.

Invariant tests, alongside the oracles:

- a local socket's endpoint and exit are identical whether its remote end is local, on another wall, or
  in another room;
- no `main` or `foreground` pass is visible inside an excluded silhouette (`passVisibleAt` sampled along
  the path);
- the matrix covers a front socket crossing another front device, crossing a rear device, overlapping
  endpoint chassis, and the same three cases as portals;
- identical input gives identical output.

## Task 10.6.2 — Studio cables & portals render from resolved connections  · done

File-tree diff:

```text
web/src/
  view-resolver.ts          - sceneCableLayout   + worldRectToSurface, studioCableInput
  cable-resolution.ts       + roundedRectPath, insideClipId/outsideClipId, passClipIds
  cable-view.ts             - bothInView, oneInView, portalOffset, cableAnchor, tipPatchEnd
  App.svelte                cableInput / studioSlots / resolvedCables; cablePass + portalChip snippets
  widgets/SilhouetteClips.svelte  + per-svg inside/outside clip defs (outside = evenodd complement)
  widgets/CablePass.svelte        + nests a pass's clip chain as <g clip-path> (recursive snippet)
  testing/browser-fixtures.ts     + "cable-portal" fixture
web/test-browser/
+ cable-presentation.spec.ts  socket meet, edge exit + clip chain, portals, drag-preview following
```

Call-stack (diff):

```text
  App (derived)
-   sceneLayout() → cableEndpoints / cableAnchor / tipPatchEnd (committed rects, estimates)
+   studioCableInput({ctx, faces, objects (preview-aware), presentation, geometry, preview})
+   resolveConnections(input, connections, portals) → drawnCables
  WorldView cables snippet (LAYER_Z.cable)
+   SilhouetteClips prefix="main"; for each drawn cable: CablePass(main) { Cable | portal stub; hit path }
  WorldView overlay snippet (z 5)
-   tipPatch (owner rect clip) + onePortal (whole stub + chip)
+   SilhouetteClips prefix="fg"; CablePass(foreground) { Cable }; then portal chips
  patch.project
-   frames from committed deviceSurfaceRect, presented = durable facing
+   studioSlots: frames from preview-aware resolved objects, presented = settled face side
```

Notes:

- Foreground passes carry no hit path. The main pass owns selection, as before, so a lead redrawn over a
  front panel can't steal presses meant for that panel's sockets.
- Portal chips paint last in the overlay SVG. They stay in surface coords for the existing drag math, and
  sit above all physical content.
- The bench still uses `cableEndpoints` and `benchCableLayout` until 10.6.3.

## Task 10.6.3 — Workbench hidden-face exits & listening badge (U2)  · planned
_(program design drafted at pickup)_

Expected shape:

- Bench silhouettes come from the slot frames `BenchStage` already projects (home + offset, geometry
  size). The DUT presents both sides, so it has no exits.
- Support devices' hidden sockets exit and mask exactly as in the studio.
- The bare `tap-marker` becomes a labeled "listening" badge on the device whenever the monitored socket
  is hidden. The badge references the socket through its endpoint alias, so it is never a patch target.
- `benchCableLayout` is deleted.

## Task 10.6.4 — Held leads on the shared pipeline  · planned
_(program design drafted at pickup)_

Expected shape:

- `PatchState.srcPoint` is removed. The held source is a logical `Endpoint`, re-resolved every frame
  through `resolveEndpoint`, so it follows device/rack previews, view switches and settled flips.
- `resolveHeldCable` returns `lead` (the source endpoint plus the pointer, with the source's passes),
  `floating` (source off-view: pointer marker plus banner), or `suppressed` (source flipping: banner only).
- Cancel removes only transient state.
- *Decided (I-163, option A):* a held lead is "in hand". It paints above objects (the
  `cable-foreground` band) and is masked only by a hidden-face source owner.
- This task also finishes legacy removal in `cable-view.ts`, which keeps just labels and portal keys, and
  runs the Story's browser Validate matrix.
