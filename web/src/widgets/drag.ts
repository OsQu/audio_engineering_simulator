// Shared pointer-drag + keyboard logic for the continuous widgets (Knob, Fader). A real knob/fader is
// operated by dragging vertically; we map vertical travel onto the widget's **normalized position**
// (0..1), then through the param's taper to its value — so a `log` gain knob moves dB-linearly and a
// `linear` control moves value-linearly. Shift gives fine control, and pointer capture keeps the drag
// tracking outside the element.

import type { ParamDescriptor } from "../catalog";
import { fromNorm, toNorm } from "./taper";

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

interface DragOptions {
  /** Param being controlled (its taper + range map position ↔ value). */
  param: ParamDescriptor;
  /** Value at the start of the drag. */
  value: number;
  onChange: (v: number) => void;
  /** Pixels of vertical travel that span the full 0→1 position range (default 180). */
  travelPx?: number;
}

/** Begin a vertical pointer-drag from `e` on its target: drag up to increase, Shift for fine control. */
export function verticalDrag(e: PointerEvent, opts: DragOptions): void {
  const el = e.currentTarget as Element;
  const startY = e.clientY;
  const startNorm = toNorm(opts.param, opts.value);
  const travel = opts.travelPx ?? 180;
  el.setPointerCapture(e.pointerId);

  const move = (ev: PointerEvent): void => {
    const dy = startY - ev.clientY; // up = increase
    const fine = ev.shiftKey ? 0.2 : 1;
    const norm = clamp(startNorm + (dy / travel) * fine, 0, 1);
    opts.onChange(fromNorm(opts.param, norm));
  };
  const up = (): void => {
    el.removeEventListener("pointermove", move as EventListener);
    el.removeEventListener("pointerup", up);
  };
  el.addEventListener("pointermove", move as EventListener);
  el.addEventListener("pointerup", up);
}

/** New value for an arrow-key press (Shift = finer), or `null` if `e` isn't an arrow key. Steps in
 *  normalized position space, so a `log` knob nudges by even dB steps. */
export function keyStep(e: KeyboardEvent, param: ParamDescriptor, value: number): number | null {
  const norm = toNorm(param, value);
  const step = e.shiftKey ? 0.01 : 0.04;
  if (e.key === "ArrowUp" || e.key === "ArrowRight")
    return fromNorm(param, clamp(norm + step, 0, 1));
  if (e.key === "ArrowDown" || e.key === "ArrowLeft")
    return fromNorm(param, clamp(norm - step, 0, 1));
  return null;
}
