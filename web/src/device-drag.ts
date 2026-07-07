// The shared "drag a device box to reposition it" concern — one place, used by both the spatial scene
// (WorldView) and the flat workbench (BenchStage). It's the pointer mechanics only: press the body,
// track the pointer with capture, convert the screen delta into position units via the view's live
// zoom, snap, gate on legality, and report the candidate position back. Everything view-specific — what
// the position *means* (world mm vs a flat bench offset), its y orientation, the grid, the legality
// rule, and where the committed value is stored — is injected by the caller, mirroring how `cable-view`
// takes a `CableLayout` and both views share one `PatchController`.
//
// A pointerdown on a control (knob/fader/switch = slider/button), a patch jack, or any other
// self-gesturing element must NOT start a move, so those opt out via `DRAG_EXCLUDE` (a press that hits
// one of these selectors is left alone). Pointer capture keeps the drag tracking outside the element.

import { snapToGrid } from "./spatial";

/** Elements whose pointerdown must not start a device move: controls (knob/fader = slider, switch =
 *  button), text inputs/links, and patch jacks. A press landing on any of these is left to its own
 *  gesture. Shared so the scene view and the bench exclude exactly the same things. */
export const DRAG_EXCLUDE =
  'button, input, select, textarea, a, [role="slider"], [role="switch"], [data-jack]';

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
  /** Override the excluded-selectors set (defaults to {@link DRAG_EXCLUDE}). */
  exclude?: string;
}

/** Svelte action: make `node` a drag handle that repositions the device it renders. Attach to the whole
 *  device body; controls/jacks inside opt out via {@link DRAG_EXCLUDE}. */
export function draggable(node: HTMLElement, params: MoveDraggable) {
  let p = params;
  // Grab bookkeeping: the pointer's start client px + the item's origin position at grab.
  let startX = 0;
  let startY = 0;
  let ox = 0;
  let oy = 0;

  // The candidate position for a pointer event: origin + (screen delta ÷ zoom), y-oriented, snapped,
  // floor-clamped, and run through the legality gate.
  function candidate(ev: PointerEvent): { x: number; y: number; legal: boolean } {
    const scale = p.scale() || 1;
    const dx = (ev.clientX - startX) / scale;
    const dy = (ev.clientY - startY) / scale;
    const x = snapToGrid(ox + dx, p.gridStep ?? 0);
    let y = snapToGrid(oy + (p.invertY ? -dy : dy), p.gridStep ?? 0);
    if (p.clampFloor) y = Math.max(0, y);
    return { x, y, legal: p.canPlace?.(x, y) ?? true };
  }

  function onMove(ev: PointerEvent): void {
    const c = candidate(ev);
    p.onMove(c.x, c.y, c.legal);
  }

  function onUp(ev: PointerEvent): void {
    node.removeEventListener("pointermove", onMove);
    node.removeEventListener("pointerup", onUp);
    node.removeEventListener("pointercancel", onUp);
    const c = candidate(ev);
    p.onEnd(c.x, c.y, c.legal);
  }

  function onDown(ev: PointerEvent): void {
    if (ev.button !== 0) return; // primary button only
    if ((ev.target as HTMLElement | null)?.closest(p.exclude ?? DRAG_EXCLUDE)) return;
    ev.preventDefault();
    startX = ev.clientX;
    startY = ev.clientY;
    const o = p.origin();
    ox = o.x;
    oy = o.y;
    node.setPointerCapture(ev.pointerId);
    p.onStart?.(ox, oy);
    node.addEventListener("pointermove", onMove);
    node.addEventListener("pointerup", onUp);
    node.addEventListener("pointercancel", onUp);
  }

  node.addEventListener("pointerdown", onDown);
  return {
    update(next: MoveDraggable): void {
      p = next;
    },
    destroy(): void {
      node.removeEventListener("pointerdown", onDown);
      node.removeEventListener("pointermove", onMove);
      node.removeEventListener("pointerup", onUp);
      node.removeEventListener("pointercancel", onUp);
    },
  };
}
