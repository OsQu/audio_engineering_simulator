// The shared "drag a device box to reposition it" concern — one place, used by both the spatial scene
// (WorldView) and the flat workbench (BenchStage). It's the pointer mechanics only: press the body,
// track the pointer with capture, convert the screen delta into position units via the view's live
// zoom, snap, gate on legality, and report the candidate position back. Everything view-specific — what
// the position *means* (world mm vs a flat bench offset), its y orientation, the grid, the legality
// rule, and where the committed value is stored — is injected by the caller, mirroring how both stages
// feed one cable-resolution pipeline and share one `PatchController`.
//
// Which press is a device move is decided once by the stage arbiter (stage-gestures.ts, Story 10.7):
// controls and near-socket presses never reach here. `beginMove` runs on the shared pointer session, so
// the pointer is captured and a cancel never commits.

import { beginPointerSession } from "./pointer-session";
import { snapToGrid } from "./spatial";

/** The caller's seam for one draggable device. Position units are the caller's (world mm for the scene,
 *  surface-mm offset for the bench); the action only does px→units via `scale` and reports candidates. */
export interface MoveDraggable {
  /** The item's position at grab time (read lazily, so it always reflects the committed value). */
  origin: () => { x: number; y: number };
  /** Live px-per-unit zoom: the client-px delta is divided by this to get a position delta. */
  scale: () => number;
  /** True when the position's y grows *up* while the screen's grows down (world elevation), so a
   *  downward drag lowers y. False for a y-down surface (the bench), where the two agree. */
  invertY?: boolean;
  /** Snap step in position units (0 ⇒ free). */
  gridStep?: number;
  /** Clamp y to ≥ 0 (a floor, for the world elevation). */
  clampFloor?: boolean;
  /** Legality for live feedback + the commit gate; absent ⇒ always legal (free placement). */
  canPlace?: (x: number, y: number) => boolean;
  /** Grab began (e.g. stop the world camera's auto-fit). */
  onStart?: (x: number, y: number) => void;
  /** Live candidate each move: the snapped position + whether it's a legal spot. */
  onMove: (x: number, y: number, legal: boolean) => void;
  /** Release: the final candidate. The caller commits it (only if `legal`) and clears any preview. */
  onEnd: (x: number, y: number, legal: boolean) => void;
  /** The pointer gesture was cancelled, so no placement may be committed. */
  onCancel?: () => void;
}

/** The candidate position for a pointer at client `(cx, cy)`, given the grab point and origin. */
function candidateAt(
  p: MoveDraggable,
  grab: { x: number; y: number },
  origin: { x: number; y: number },
  cx: number,
  cy: number,
): { x: number; y: number; legal: boolean } {
  const scale = p.scale() || 1;
  const dx = (cx - grab.x) / scale;
  const dy = (cy - grab.y) / scale;
  const x = snapToGrid(origin.x + dx, p.gridStep ?? 0);
  let y = snapToGrid(origin.y + (p.invertY ? -dy : dy), p.gridStep ?? 0);
  if (p.clampFloor) y = Math.max(0, y);
  return { x, y, legal: p.canPlace?.(x, y) ?? true };
}

/** Start a device move the stage arbiter (stage-gestures.ts) has already decided this press owns. It
 *  runs on the shared pointer session: a release reports `onEnd`, while pointercancel / lost capture /
 *  blur / unmount report `onCancel` only — a cancelled move never reaches the commit path. */
export function beginMove(root: HTMLElement, e: PointerEvent, p: MoveDraggable): void {
  const grab = { x: e.clientX, y: e.clientY };
  const origin = p.origin();
  const session = beginPointerSession(root, e, {
    move: (ev) => {
      const c = candidateAt(p, grab, origin, ev.clientX, ev.clientY);
      p.onMove(c.x, c.y, c.legal);
    },
    release: (ev) => {
      const c = candidateAt(p, grab, origin, ev.clientX, ev.clientY);
      p.onEnd(c.x, c.y, c.legal);
    },
    cancel: () => p.onCancel?.(),
  });
  if (session) p.onStart?.(origin.x, origin.y);
}
