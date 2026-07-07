// The shared pan/zoom **camera** over a transformed surface — one place, used by both the spatial scene
// (WorldView) and the flat workbench (BenchStage). It owns the `translate(pan) · scale(zoom)` transform,
// cursor-anchored wheel zoom, and drag-to-pan; each view keeps only its own *content layout* and its
// world↔surface coordinate mapping. This is the twin of `device-drag`'s `draggable` (which moves one
// item): the camera moves the whole view. Mirrors how `PatchController` centralises the patch gesture.
//
// Held as a `$state` class (like PatchController): the reactive `zoom`/`panX`/`panY` fields drive the
// surface `transform`, and the methods adapt DOM pointer/wheel events to camera moves. Drag-to-pan is
// self-contained (pointer capture + element listeners on the grabbed backdrop), so a view needs no
// window handlers — it just calls `startPan` on a backdrop pointerdown and `wheelZoom` on wheel.

export interface CameraOpts {
  /** Initial zoom (px per surface unit). */
  zoom?: number;
  /** Zoom clamp. */
  minZoom?: number;
  maxZoom?: number;
  /** Zoom change per px of wheel scroll (gentle; trackpad-friendly). */
  sensitivity?: number;
}

export class Camera {
  zoom = $state(1);
  panX = $state(0);
  panY = $state(0);

  #min: number;
  #max: number;
  #sens: number;
  // Active pan bookkeeping (pointer start px + pan origin), or null when not panning.
  #pan: { px: number; py: number; x0: number; y0: number } | null = null;

  constructor(opts: CameraOpts = {}) {
    this.zoom = opts.zoom ?? 1;
    this.#min = opts.minZoom ?? 0.2;
    this.#max = opts.maxZoom ?? 3;
    this.#sens = opts.sensitivity ?? 0.0015;
  }

  /** The surface transform: pan then scale, from a top-left transform-origin (so surface-local (0,0)
   *  maps to the pan offset — the invariant `clientToSurface` and cursor-anchored zoom rely on). */
  get transform(): string {
    return `translate(${this.panX}px, ${this.panY}px) scale(${this.zoom})`;
  }

  /** True while a drag-pan is in flight (e.g. to suppress a click that ends a pan). */
  get panning(): boolean {
    return this.#pan !== null;
  }

  /** Cursor-anchored wheel zoom: scale by an amount proportional to the scroll distance (not a fixed
   *  step per event, which makes a trackpad explode), keeping the surface point under the cursor fixed by
   *  adjusting pan. `viewport` supplies the client origin the cursor is measured against. */
  wheelZoom(e: WheelEvent, viewport: HTMLElement): void {
    e.preventDefault();
    // Normalize line-mode deltas (deltaMode 1) to px.
    const px = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    const next = Math.min(this.#max, Math.max(this.#min, this.zoom * Math.exp(-px * this.#sens)));
    if (next === this.zoom) return;
    // The surface maps a local point s to viewport coords as v = pan + zoom·s, so v stays put under the
    // cursor when pan' = v − (next/zoom)(v − pan).
    const rect = viewport.getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    this.panX = cx - (next / this.zoom) * (cx - this.panX);
    this.panY = cy - (next / this.zoom) * (cy - this.panY);
    this.zoom = next;
  }

  /** Begin a drag-to-pan from a backdrop pointerdown: capture the pointer on the grabbed element and
   *  track it to release (self-contained — no window handlers). Left button only. */
  startPan(e: PointerEvent): void {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    this.#pan = { px: e.clientX, py: e.clientY, x0: this.panX, y0: this.panY };
    const move = (ev: PointerEvent): void => {
      if (!this.#pan) return;
      this.panX = this.#pan.x0 + (ev.clientX - this.#pan.px);
      this.panY = this.#pan.y0 + (ev.clientY - this.#pan.py);
    };
    const up = (): void => {
      this.#pan = null;
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  /** A viewport client point → surface-local coords: subtract the (post-transform) surface origin and
   *  divide out the zoom. transform-origin is top-left, so the surface's client top-left is surface-local
   *  (0,0). `surface` is the transformed element; returns (0,0) if it isn't mounted yet. */
  clientToSurface(
    surface: HTMLElement | null | undefined,
    clientX: number,
    clientY: number,
  ): { x: number; y: number } {
    if (!surface) return { x: 0, y: 0 };
    const r = surface.getBoundingClientRect();
    return { x: (clientX - r.left) / this.zoom, y: (clientY - r.top) / this.zoom };
  }

  /** Fit + center a surface-space bounding box in a viewport, with `padding` px of breathing room. The
   *  fit-to-content framing (WorldView frames its gear on a space switch; the bench frames its stack). */
  frame(
    viewportW: number,
    viewportH: number,
    bbox: { left: number; top: number; right: number; bottom: number },
    padding = 0,
  ): void {
    const w = bbox.right - bbox.left;
    const h = bbox.bottom - bbox.top;
    if (w <= 0 || h <= 0) return;
    const z = Math.min(
      this.#max,
      Math.max(this.#min, Math.min((viewportW - 2 * padding) / w, (viewportH - 2 * padding) / h)),
    );
    this.zoom = z;
    this.panX = viewportW / 2 - z * ((bbox.left + bbox.right) / 2);
    this.panY = viewportH / 2 - z * ((bbox.top + bbox.bottom) / 2);
  }
}
