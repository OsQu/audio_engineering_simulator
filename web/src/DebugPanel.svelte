<script lang="ts">
  // The bench debug surface (Story 6.4) — an *audio-parameter* inspector over the rig, reading the same
  // shared `SceneSession` the scene view does (nothing forks the engine/patch plumbing). Two parts:
  //   1. an always-on header (the few-enough things: master output peak + monitored tap, signal-path
  //      latency, and the per-connection loading losses) — no filtering needed at these counts;
  //   2. a filter + pin watch-list over the rig's params/configs/readouts — the DUT can expose hundreds
  //      (the 8i6 alone has 206), so it's a searchable watch-list, never a flat dump. Pins persist in
  //      `scene.ui.benchWatch` (URL round-trip), so a watched value re-renders after a `wasm:watch` reload.
  // The panel is read-only: the faceplate knobs remain the editor; this surfaces the exact numbers,
  // ranges, and ids the knobs don't. Engine-internal health (overruns/render-ms/drops) + the seed
  // control are deliberately *not* here — they stay on `session.health` / the pinned SEED.

  import { type Watchable, matchesQuery, watchables } from "./bench-watch";
  import { type DeviceDescriptor, descriptorFor } from "./catalog";
  import type { BenchWatch } from "./scene-store";
  import { isWatched, toggleWatch, watchKey } from "./scene-ops";
  import type { SceneSession } from "./session.svelte";
  import { formatParam } from "./widgets/format";

  interface Props {
    session: SceneSession;
  }
  let { session }: Props = $props();

  // A device instance's display name (its catalog `name`), falling back to the instance id.
  function deviceName(id: string): string {
    const dev = session.scene.patch.devices.find((d) => d.id === id);
    return (dev && session.catalog.find((c) => c.typeId === dev.typeId)?.name) ?? id;
  }

  // The catalog descriptor for a device instance (or undefined if its type left the catalog — a stale pin).
  function descOf(id: string): DeviceDescriptor | undefined {
    const dev = session.scene.patch.devices.find((d) => d.id === id);
    return dev && descriptorFor(session.catalog, dev.typeId);
  }

  // --- Header ---------------------------------------------------------------------------------------
  // Master output peak as dBFS (the level message is linear ±1.0 = full scale); silence → "−∞".
  const peakDbfs = $derived(
    session.level <= 1e-4 ? "−∞" : (20 * Math.log10(session.level)).toFixed(1),
  );

  // The monitored tap (patch.output): the device + analog-output-port label it renders from.
  const tap = $derived.by(() => {
    const out = session.scene.patch.output;
    const desc = descOf(out.device);
    const port = desc?.ports.find((p) => p.direction === "output" && p.id === out.port);
    return `${desc?.name ?? out.device} · ${port?.label ?? `out ${out.port}`}`;
  });

  // Per-connection loading losses (dB) — index-aligned with `session.losses`; digital/event edges (null)
  // are skipped (ideal, no resistive loading), matching the scene view's global losses panel.
  const losses = $derived(
    session.scene.patch.connections
      .map((c, i) => ({ c, loss: session.losses[i] }))
      .filter((x) => x.loss !== undefined && x.loss !== null)
      .map((x) => ({
        from: deviceName(x.c.from.device),
        to: deviceName(x.c.to.device),
        loss: x.loss as number,
      })),
  );

  // --- Watch-list -----------------------------------------------------------------------------------
  const MAX_RESULTS = 40; // cap the results list; a note surfaces when the filter matched more (no silent cap).

  let query = $state("");

  // The full searchable set (every device's params/configs/readouts) and the filtered results.
  const all = $derived(watchables(session.scene, session.catalog));
  const matched = $derived(query.trim() ? all.filter((w) => matchesQuery(w, query)) : []);
  const results = $derived(matched.slice(0, MAX_RESULTS));

  // A watchable's live value + secondary detail (range/recompile), resolved read-only through the session.
  // Stale pins (device/param gone after a Rust edit) degrade to an "unavailable" row rather than crashing.
  interface Row {
    key: string;
    item: BenchWatch;
    device: string;
    label: string;
    kind: BenchWatch["kind"];
    value: string;
    detail: string;
    available: boolean;
  }
  function rowFor(w: BenchWatch): Row {
    const base = { key: watchKey(w), item: w, kind: w.kind, device: w.device };
    const desc = descOf(w.device);
    const gone = (label: string): Row => ({
      ...base,
      device: deviceName(w.device),
      label,
      value: "—",
      detail: "unavailable",
      available: false,
    });
    if (!desc) return gone(`${w.kind} ${w.id}`);
    if (w.kind === "param") {
      const p = desc.params.find((d) => String(d.id) === w.id);
      if (!p) return gone(`param ${w.id}`);
      return {
        ...base,
        device: desc.name,
        label: p.label,
        value: formatParam(p, session.paramValue(w.device, desc, p.id)),
        detail: `${p.min}–${p.max}${p.unit ? ` ${p.unit}` : ""} · default ${p.default}`,
        available: true,
      };
    }
    if (w.kind === "config") {
      const c = desc.configs.find((d) => d.key === w.id);
      if (!c) return gone(`config ${w.id}`);
      return {
        ...base,
        device: desc.name,
        label: c.label,
        value: session.configValue(w.device, desc, c.key) >= 0.5 ? "on" : "off",
        detail: "recompiles",
        available: true,
      };
    }
    const r = desc.readouts.find((d) => String(d.id) === w.id);
    if (!r) return gone(`readout ${w.id}`);
    const v = session.readingFor(w.device, r.id);
    return {
      ...base,
      device: desc.name,
      label: r.label,
      value: `${v <= -55 ? "—" : v.toFixed(1)}${r.unit ? ` ${r.unit}` : ""}`,
      detail: "readout",
      available: true,
    };
  }
  const pinned = $derived((session.scene.ui.benchWatch ?? []).map(rowFor));

  const asItem = (w: Watchable): BenchWatch => ({ device: w.device, kind: w.kind, id: w.id });
  const pinnedNow = (w: Watchable): boolean => isWatched(session.scene, asItem(w));
</script>

<div class="debug">
  <section class="header">
    <div class="stat">
      <span class="k">Output peak</span>
      <span class="v">{peakDbfs} <span class="u">dBFS</span></span>
    </div>
    <div class="stat">
      <span class="k">Monitoring</span>
      <span class="v">{tap}</span>
    </div>
    <div class="stat">
      <span class="k">Signal-path latency</span>
      <span class="v">{session.latencyMs.toFixed(2)} <span class="u">ms</span></span>
    </div>
    <div class="stat losses">
      <span class="k">Connection losses</span>
      {#if losses.length === 0}
        <span class="v muted">— none</span>
      {:else}
        <ul>
          {#each losses as l (l.from + "→" + l.to)}
            <li>
              {l.from} → {l.to} <strong>{l.loss.toFixed(2)}</strong> <span class="u">dB</span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </section>

  <section class="watch">
    {#if pinned.length > 0}
      <ul class="pins">
        {#each pinned as row (row.key)}
          <li class:gone={!row.available}>
            <span class="kind kind-{row.kind}">{row.kind}</span>
            <span class="lbl"><span class="dev">{row.device}</span> · {row.label}</span>
            <span class="val">{row.value}</span>
            <span class="det">{row.detail}</span>
            <button
              type="button"
              class="unpin"
              title="Unpin"
              onclick={() => toggleWatch(session.scene, row.item)}>✕</button
            >
          </li>
        {/each}
      </ul>
    {/if}

    <div class="filter">
      <input
        type="text"
        placeholder="Filter params, configs, readouts… ({all.length} total)"
        bind:value={query}
        aria-label="filter watchable parameters"
      />
    </div>

    {#if query.trim()}
      {#if results.length === 0}
        <p class="hint">No match for “{query}”.</p>
      {:else}
        <ul class="results">
          {#each results as w (watchKey(w))}
            <li>
              <button
                type="button"
                class="pin"
                class:on={pinnedNow(w)}
                title={pinnedNow(w) ? "Unpin" : "Pin"}
                onclick={() => toggleWatch(session.scene, asItem(w))}
                >{pinnedNow(w) ? "★" : "☆"}</button
              >
              <span class="kind kind-{w.kind}">{w.kind}</span>
              <span class="lbl"><span class="dev">{w.deviceName}</span> · {w.label}</span>
              <span class="rid">{w.id}</span>
            </li>
          {/each}
        </ul>
        {#if matched.length > results.length}
          <p class="hint">
            Showing {results.length} of {matched.length} matches — narrow the filter to see the rest.
          </p>
        {/if}
      {/if}
    {:else if pinned.length === 0}
      <p class="hint">
        Type to filter the rig's params, configs, and readouts, then pin what you're watching.
      </p>
    {/if}
  </section>
</div>

<style>
  .debug {
    font: 13px/1.5 var(--ae-font-ui);
    color: var(--ae-text-secondary);
    font-variant-numeric: tabular-nums;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .header {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 0.5rem 2rem;
    padding: 0.6rem 0.9rem;
    background: var(--ae-bg-panel);
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-control);
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .k {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: var(--ae-legend-spacing);
    color: var(--ae-text-muted);
  }
  .v {
    color: var(--ae-text-strong);
  }
  .u {
    color: var(--ae-text-muted);
    font-size: 0.85em;
  }
  .muted {
    color: var(--ae-text-muted);
  }
  .losses ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }
  .losses li strong {
    color: var(--ae-text-strong);
  }

  /* --- Watch-list --- */
  .watch {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .filter input {
    width: 100%;
    box-sizing: border-box;
    font: inherit;
    padding: 0.4em 0.6em;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  .pins,
  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .pins li,
  .results li {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.2em 0.5em;
    border-radius: var(--ae-radius-control);
  }
  .pins li {
    background: var(--ae-bg-panel);
    border: 1px solid var(--ae-line-panel);
  }
  .results li:hover {
    background: var(--ae-bg-panel);
  }
  .pins li.gone {
    opacity: 0.55;
  }
  .kind {
    flex: 0 0 auto;
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: var(--ae-legend-spacing);
    color: var(--ae-text-muted);
    min-width: 4.5em;
  }
  .kind-config {
    color: var(--ae-warn, #c98a2b);
  }
  .lbl {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dev {
    color: var(--ae-text-strong);
  }
  .val {
    flex: 0 0 auto;
    color: var(--ae-text-strong);
  }
  .det,
  .rid {
    flex: 0 0 auto;
    font-size: 0.78rem;
    color: var(--ae-text-muted);
  }
  .pin,
  .unpin {
    flex: 0 0 auto;
    cursor: pointer;
    font: inherit;
    line-height: 1;
    padding: 0.1em 0.3em;
    color: var(--ae-text-muted);
    background: transparent;
    border: none;
  }
  .pin.on {
    color: var(--ae-text-strong);
  }
  .unpin:hover,
  .pin:hover {
    color: var(--ae-text-strong);
  }
  .hint {
    margin: 0;
    font-size: 0.82rem;
    color: var(--ae-text-muted);
  }
</style>
