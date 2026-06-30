import type { ParamDescriptor } from "../catalog";

/** Format a param value for a widget's readout: on/off for switches, else the number + unit. */
export function formatParam(p: ParamDescriptor, value: number): string {
  if (p.kind === "switch") return value >= 0.5 ? "on" : "off";
  const text = Number.isInteger(value) ? String(value) : value.toFixed(2);
  return p.unit ? `${text} ${p.unit}` : text;
}
