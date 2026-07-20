import type { ParamDescriptor } from "../catalog";

/** Format a param value for a widget's readout: on/off for switches, dB for a log (voltage-gain)
 *  control — its value is a linear multiplier, shown as `20·log₁₀` dB — else the number + unit. */
export function formatParam(p: ParamDescriptor, value: number): string {
  if (p.kind === "switch") return value >= 0.5 ? "on" : "off";
  if (p.taper === "log") {
    if (value <= 0) return "−∞ dB";
    const db = 20 * Math.log10(value);
    return `${db >= 0 ? "+" : ""}${db.toFixed(1)} dB`;
  }
  const text = Number.isInteger(value) ? String(value) : value.toFixed(2);
  return p.unit ? `${text} ${p.unit}` : text;
}
