// The bench debug watch-list's searchable set: enumerating every watchable audio parameter in the rig
// (each device's params, configs, and readouts) and matching them against the filter text. Pure given
// its inputs (no Svelte) — the DebugPanel builds the `$derived` results from these, and unit tests pin
// the enumeration + matching. The pin *set* itself (persistence, toggle) lives in scene-ops.

import { type DeviceDescriptor, descriptorFor } from "./catalog";
import type { BenchWatch, Scene } from "./scene-store";

/** One searchable/pinnable item — a device instance's param, structural config, or readout, flattened
 *  with the metadata the panel needs to display it (label, unit, and whether it recompiles). The `id` is
 *  the stringified param/readout id or the config key (matching {@link BenchWatch}). */
export interface Watchable {
  device: string;
  deviceName: string;
  kind: BenchWatch["kind"];
  id: string;
  label: string;
  /** Readout/param unit ("V", "ms", "dBFS", …); "" for a config (a toggle) and unitless params. */
  unit: string;
  /** True for a config: changing it recompiles the patch (the config-vs-param distinction, made visible). */
  recompile: boolean;
}

/** Enumerate every watchable in the scene: each device's params, configs, and readouts, in that order.
 *  Devices whose type isn't in the catalog (a stale pin after a Rust edit) are simply skipped. */
export function watchables(scene: Scene, catalog: DeviceDescriptor[]): Watchable[] {
  const out: Watchable[] = [];
  for (const dev of scene.patch.devices) {
    const desc = descriptorFor(catalog, dev.typeId);
    if (!desc) continue;
    for (const p of desc.params)
      out.push({
        device: dev.id,
        deviceName: desc.name,
        kind: "param",
        id: String(p.id),
        label: p.label,
        unit: p.unit,
        recompile: false,
      });
    for (const c of desc.configs)
      out.push({
        device: dev.id,
        deviceName: desc.name,
        kind: "config",
        id: c.key,
        label: c.label,
        unit: "",
        recompile: true,
      });
    for (const r of desc.readouts)
      out.push({
        device: dev.id,
        deviceName: desc.name,
        kind: "readout",
        id: String(r.id),
        label: r.label,
        unit: r.unit,
        recompile: false,
      });
  }
  return out;
}

/** Whether a watchable matches the filter text — case-insensitive substring across device name, label,
 *  kind, and id (so "cross", "matrix", "8", or a device name all narrow the 206-param DUT). An empty
 *  query matches nothing: the list is a filter-to-pin surface, not a dump. */
export function matchesQuery(w: Watchable, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return false;
  return (
    `${w.deviceName} ${w.label}`.toLowerCase().includes(q) ||
    w.kind.includes(q) ||
    w.id.toLowerCase().includes(q)
  );
}
