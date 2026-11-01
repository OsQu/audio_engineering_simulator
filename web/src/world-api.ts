// The coordinate seam every patchable stage exposes so cables can be placed and jack anchors measured in
// the stage's own surface-local pixel space — without any consumer touching the pan/zoom transform. Both
// stages implement it: the scene view's `WorldView` (a spatial room) and the workbench `BenchStage` (one
// bolted-down device). It lives here, not inside a widget, so the bench doesn't import a type from the
// scene-view widget it deliberately isn't (Story 6.3).

/** A point in a stage's surface-local pixel space (y increases downward — SVG/DOM convention). */
export interface SurfacePoint {
  x: number;
  y: number;
}

/** Coordinate converters a patchable stage exposes (to its `overlay` snippet, and to a parent via
 *  `bind:api`) so cables can be placed/measured in the surface's own space. Surface-local coords are
 *  pan/zoom-invariant — `clientToSurface` divides the zoom back out.
 *  - `worldToSurface`: the stage's world units → surface-local. WorldView maps world mm (x right, y **up**
 *    from the floor) → surface-local (y **down** from top); the bench's world ≡ surface mm (identity).
 *  - `clientToSurface`: a viewport client point (e.g. a measured DOM rect) → surface-local.
 *  - `stageRoot`: the transformed surface element — the root that scopes stage-only DOM queries (the flip
 *    lifecycle's transitions), so a duplicate faceplate *outside* the surface (a focus window) is never
 *    mistaken for a stage face. `null` until the surface mounts. */
export interface WorldApi {
  worldToSurface: (worldX: number, worldY: number) => SurfacePoint;
  clientToSurface: (clientX: number, clientY: number) => SurfacePoint;
  stageRoot: () => HTMLElement | null;
}
