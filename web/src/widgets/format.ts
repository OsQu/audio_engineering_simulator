import type { ParamDescriptor } from "../catalog";

/** 0 dB SPL reference pressure: 20 µPa (the threshold of hearing) — mirrors the engine's `SPL_REF_PA`. */
const SPL_REF_PA = 20e-6;

/** Format a param value for a widget's readout. Display is keyed on the **unit**, not the taper (taper
 *  governs knob *travel*, unit governs *display*): a switch reads on/off; a `"dB"` unit is a linear
 *  voltage-gain shown as `20·log₁₀` dB; a `"dB SPL"` unit is a linear pressure in Pascals shown as
 *  `20·log₁₀(p / 20µPa)` dB SPL (so the vox loudness knob reads in the engineer's unit); everything else
 *  is the raw number + unit (so a log-taper Hz knob reads Hz, not dB). */
export function formatParam(p: ParamDescriptor, value: number): string {
  if (p.kind === "switch") return value >= 0.5 ? "on" : "off";
  if (p.unit === "dB") {
    if (value <= 0) return "−∞ dB";
    const db = 20 * Math.log10(value);
    return `${db >= 0 ? "+" : ""}${db.toFixed(1)} dB`;
  }
  if (p.unit === "dB SPL") {
    if (value <= 0) return "−∞ dB SPL";
    return `${(20 * Math.log10(value / SPL_REF_PA)).toFixed(1)} dB SPL`;
  }
  const text = Number.isInteger(value) ? String(value) : value.toFixed(2);
  return p.unit ? `${text} ${p.unit}` : text;
}
