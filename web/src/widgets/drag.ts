// Shared pointer-drag + keyboard logic for the continuous widgets (Knob, Fader). A real knob/fader is
// operated by dragging vertically; we map vertical travel onto the param's value range, with Shift for
// fine control, and use pointer capture so the drag keeps tracking outside the element.

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

interface DragOptions {
  /** Value at the start of the drag. */
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
  /** Pixels of vertical travel that span the full min→max range (default 180). */
  travelPx?: number;
}

/** Begin a vertical pointer-drag from `e` on its target: drag up to increase, Shift for fine control. */
export function verticalDrag(e: PointerEvent, opts: DragOptions): void {
  const el = e.currentTarget as Element;
  const startY = e.clientY;
  const start = opts.value;
  const span = opts.max - opts.min || 1;
  const travel = opts.travelPx ?? 180;
  el.setPointerCapture(e.pointerId);

  const move = (ev: PointerEvent): void => {
    const dy = startY - ev.clientY; // up = increase
    const fine = ev.shiftKey ? 0.2 : 1;
    opts.onChange(clamp(start + (dy / travel) * span * fine, opts.min, opts.max));
  };
  const up = (): void => {
    el.removeEventListener("pointermove", move as EventListener);
    el.removeEventListener("pointerup", up);
  };
  el.addEventListener("pointermove", move as EventListener);
  el.addEventListener("pointerup", up);
}

/** New clamped value for an arrow-key press (Shift = finer), or `null` if `e` isn't an arrow key. */
export function keyStep(e: KeyboardEvent, value: number, min: number, max: number): number | null {
  const span = max - min || 1;
  const step = (e.shiftKey ? 0.01 : 0.04) * span;
  if (e.key === "ArrowUp" || e.key === "ArrowRight") return clamp(value + step, min, max);
  if (e.key === "ArrowDown" || e.key === "ArrowLeft") return clamp(value - step, min, max);
  return null;
}
