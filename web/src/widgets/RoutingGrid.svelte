<script lang="ts">
  // A data-driven **crosspoint routing grid** — the shared mixer widget for any device with a routing
  // `Matrix` (the 8i6's Focusrite Control mixer; the computer's DAW loopback). It derives its structure
  // from the crosspoint params — those whose label reads "input → output" (authored by the Rust catalog
  // via `GridSpec`): rows are the unique inputs, columns the unique outputs, in first-seen order, and
  // each cell is a `Control` knob bound to that crosspoint's exposed param id. Deriving rows/cols/ids
  // from the labels keeps it robust to id shifts — the structure comes from the data, not hard-coded
  // positions — so the guardrail covers the cells via a **declared range** (they aren't literal ids in
  // any surface's source).
  //
  // It renders nothing but the grid and expects an ambient `DeviceHandle` in context (its inner
  // `Control`s bind through it), so it must be used inside a surface that publishes one (`Chassis`, or a
  // focus surface calling `setDeviceHandle`).
  import type { ParamDescriptor } from "../catalog";
  import Control from "./Control.svelte";

  interface Props {
    /** The device's exposed params; the crosspoints are those whose label contains "→". */
    params: ParamDescriptor[];
  }
  let { params = [] }: Props = $props();

  interface Crosspoint {
    id: number;
    input: string;
    output: string;
  }
  const crosspoints = $derived<Crosspoint[]>(
    params
      .filter((p) => p.label.includes("→"))
      .map((p) => {
        const [input, output] = p.label.split("→").map((s) => s.trim());
        return { id: p.id, input, output };
      }),
  );
  const inputs = $derived([...new Set(crosspoints.map((c) => c.input))]);
  const outputs = $derived([...new Set(crosspoints.map((c) => c.output))]);
  const cell = (input: string, output: string): Crosspoint | undefined =>
    crosspoints.find((c) => c.input === input && c.output === output);
</script>

{#if crosspoints.length > 0}
  <!-- rows = inputs, cols = outputs; a leading header column + one column per output. -->
  <div class="grid" style="--cols: {outputs.length}">
    <span class="corner"></span>
    {#each outputs as out (out)}
      <span class="col-head">{out}</span>
    {/each}
    {#each inputs as inp (inp)}
      <span class="row-head">{inp}</span>
      {#each outputs as out (out)}
        {@const c = cell(inp, out)}
        <div class="cell">
          {#if c}<Control id={c.id} cap="dark" />{/if}
        </div>
      {/each}
    {/each}
  </div>
{/if}

<style>
  /* The routing grid: a header column of input names + one column per output. */
  .grid {
    display: grid;
    grid-template-columns: auto repeat(var(--cols), 1fr);
    gap: 0.4rem 0.6rem;
    align-items: center;
    justify-items: center;
    padding: 0.6rem 0.8rem;
    border: 1px solid color-mix(in srgb, var(--ae-accent, var(--ae-line-panel)) 40%, transparent);
    border-radius: var(--ae-radius-control);
    background: var(--ae-bg-chip);
    width: fit-content;
    /* A big matrix can overflow its surface; let it scroll rather than push the layout wide. */
    max-width: 100%;
    overflow: auto;
  }
  .col-head,
  .row-head {
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    color: var(--ae-text-muted);
    white-space: nowrap;
  }
  .row-head {
    justify-self: end;
  }
  .cell {
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
