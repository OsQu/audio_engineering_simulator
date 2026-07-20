// The control law that maps a continuous widget's **travel** (a normalized 0..1 position — knob
// rotation or fader throw) onto its **value**, per the param's `taper`. This is a UI concern only:
// the value produced is exactly what the engine stores (a linear voltage-gain multiplier for a gain
// knob), unchanged. The taper just decides *where along the travel* each value sits.
//
// Why it matters: a voltage-gain param spans a huge linear range (`min=0, max=1000` ≈ 0×→+60 dB). A
// *linear* knob crams the whole usable range into the first sliver of travel — a quarter-turn is
// already +48 dB. A `"log"` taper maps travel **dB-linearly** (equal rotation = equal dB step), the
// way a real gain pot is marked, so a quarter-turn lands near unity and the low end is controllable.

import type { ParamDescriptor } from "../catalog";

/** The dB span a `log` knob's rotation sweeps below its `max`. Chosen so a gain knob whose `max` is
 *  +60 dB (the 1000× voltage-gain ceiling) and whose default is unity (0 dB) sits at ~quarter travel
 *  and round-trips: at 60 dB the default would pin to the bottom (colliding with silence), at 80 dB
 *  it sits interior. Curve for a +60 dB knob: ¼ ≈ 0 dB, ½ ≈ +20, ¾ ≈ +40, full = +60. */
const LOG_SWEEP_DB = 80;

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/** The param's value → normalized travel (0 = fully counter-clockwise/bottom, 1 = full). Inverse of
 *  {@link fromNorm}; used to place the pointer/cap for a given value. */
export function toNorm(p: ParamDescriptor, value: number): number {
  const span = p.max - p.min || 1;
  if (p.taper !== "log") return clamp((value - p.min) / span, 0, 1);
  // Geometric between a positive floor and max (a control that never mutes).
  if (p.min > 0) {
    if (value <= p.min) return 0;
    return clamp(Math.log(value / p.min) / Math.log(p.max / p.min), 0, 1);
  }
  // Audio taper: dB-linear over the top LOG_SWEEP_DB of travel, snapping to silence at the very
  // bottom (min === 0, i.e. a level control that reaches true zero).
  if (value <= 0) return 0;
  return clamp(1 + (20 * Math.log10(value / p.max)) / LOG_SWEEP_DB, 0, 1);
}

/** Normalized travel (0..1) → the param's value. Inverse of {@link toNorm}; the widget calls this on
 *  every drag/keystep to turn a new position into the value it sends to the engine. */
export function fromNorm(p: ParamDescriptor, norm: number): number {
  const n = clamp(norm, 0, 1);
  if (p.taper !== "log") return p.min + n * (p.max - p.min);
  if (p.min > 0) return p.min * (p.max / p.min) ** n;
  if (n <= 0) return 0;
  return p.max * 10 ** (((n - 1) * LOG_SWEEP_DB) / 20);
}
