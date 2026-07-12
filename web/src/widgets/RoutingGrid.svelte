<script lang="ts">
  // A data-driven **crosspoint routing list** — the shared mixer widget for any device with a routing
  // `Matrix` (the 8i6's Focusrite Control mixer; the computer's DAW loopback). It derives its structure
  // from the crosspoint params — those whose label reads "input → output" (authored by the Rust catalog
  // via `GridSpec`): each crosspoint's exposed param id is a gain (0 = not routed, up to +12 dB), and a
  // route "exists" precisely when its gain is above zero. Deriving the routes/ids from the labels keeps it
  // robust to id shifts — the structure comes from the data, not hard-coded positions — so the guardrail
  // covers the cells via a **declared range** (they aren't literal ids in any surface's source).
  //
  // Rather than a full N×M wall of knobs (the 8i6 matrix alone is 14×14 = 196 crosspoints, nearly all
  // idle), it shows **only the live routes** as a compact list — one row per active crosspoint (`input →
  // output`, a gain `Control`, and a remove ×) — plus an **add-route picker** (input → output over the
  // full vocabulary) that brings any crosspoint live at unity. All of this is pure param movement — no
  // recompile — because routing lives inside the `Matrix` node behind smoothed gains.
  //
  // It renders nothing but the list and expects an ambient `DeviceHandle` in context (its inner
  // `Control`s bind through it, and it reads/sets crosspoint gains through it), so it must be used inside
  // a surface that publishes one (`Chassis`, or a focus surface calling `setDeviceHandle`).
  import type { ParamDescriptor } from "../catalog";
  import { getDeviceHandle } from "../device-handle";
  import Control from "./Control.svelte";

  interface Props {
    /** The device's exposed params; the crosspoints are those whose label contains "→". */
    params: ParamDescriptor[];
  }
  let { params = [] }: Props = $props();

  const handle = getDeviceHandle();

  // A crosspoint carries signal — is a live route — when its gain sits above ~0 (0 = muted/unrouted).
  const EPS = 1e-4;
  // Adding a route brings its crosspoint to unity gain (0 dB) — pass-through, the sensible default level.
  const UNITY = 1.0;

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
  // The full input/output vocabulary (first-seen order) — what the add-route picker offers.
  const allInputs = $derived([...new Set(crosspoints.map((c) => c.input))]);
  const allOutputs = $derived([...new Set(crosspoints.map((c) => c.output))]);

  const cell = (input: string, output: string): Crosspoint | undefined =>
    crosspoints.find((c) => c.input === input && c.output === output);

  // The live routes, in crosspoint order (input-major). Reading gains through the handle keeps this
  // reactive to routing edits — a route appears/disappears as its gain crosses zero.
  const routes = $derived(crosspoints.filter((c) => handle.value(c.id) > EPS));

  // Add-route picker selection; kept valid against the (possibly shifting) vocabulary.
  let pickInput = $state("");
  let pickOutput = $state("");
  $effect(() => {
    if (!allInputs.includes(pickInput)) pickInput = allInputs[0] ?? "";
  });
  $effect(() => {
    if (!allOutputs.includes(pickOutput)) pickOutput = allOutputs[0] ?? "";
  });

  function addRoute(input: string, output: string): void {
    const c = cell(input, output);
    if (c) handle.set(c.id, UNITY);
  }
  function removeRoute(c: Crosspoint): void {
    handle.set(c.id, 0);
  }
</script>

{#if crosspoints.length > 0}
  {#if routes.length > 0}
    <ul class="routes">
      {#each routes as c (c.id)}
        <li class="route">
          <span class="ends">
            <span class="end input">{c.input}</span>
            <span class="arrow">→</span>
            <span class="end output">{c.output}</span>
          </span>
          <Control id={c.id} cap="dark" size={20} />
          <button
            type="button"
            class="remove"
            title={`Remove ${c.input} → ${c.output}`}
            aria-label={`Remove route ${c.input} → ${c.output}`}
            onclick={() => removeRoute(c)}>×</button
          >
        </li>
      {/each}
    </ul>
  {:else}
    <p class="empty">No routes yet — add one below.</p>
  {/if}

  <!-- Add any crosspoint (over the full input/output vocabulary) at unity gain. -->
  <div class="add-route">
    <label>
      <span>In</span>
      <select bind:value={pickInput}>
        {#each allInputs as inp (inp)}
          <option value={inp}>{inp}</option>
        {/each}
      </select>
    </label>
    <span class="arrow">→</span>
    <label>
      <span>Out</span>
      <select bind:value={pickOutput}>
        {#each allOutputs as out (out)}
          <option value={out}>{out}</option>
        {/each}
      </select>
    </label>
    <button
      type="button"
      class="add"
      disabled={!pickInput || !pickOutput}
      onclick={() => addRoute(pickInput, pickOutput)}>Add route</button
    >
  </div>
{/if}

<style>
  /* The routing list: one row per live crosspoint. */
  .routes {
    list-style: none;
    margin: 0;
    padding: 0.4rem 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    border: 1px solid color-mix(in srgb, var(--ae-accent, var(--ae-line-panel)) 40%, transparent);
    border-radius: var(--ae-radius-control);
    background: var(--ae-bg-chip);
    width: fit-content;
    max-width: 100%;
    /* A busy matrix can carry many routes; let the list scroll rather than push the layout tall/wide. */
    max-height: 20rem;
    overflow: auto;
  }
  .route {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 0.6rem;
    padding: 0.15rem 0.2rem;
  }
  .ends {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 0.4rem;
    /* Keep the endpoint column a stable width so the knobs line up down the list. */
    flex: 1 1 auto;
    min-width: 12rem;
  }
  .end {
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    white-space: nowrap;
  }
  .end.input {
    color: var(--ae-text-secondary, var(--ae-text-primary));
  }
  .end.output {
    color: var(--ae-text-strong);
  }
  .arrow {
    color: var(--ae-text-muted);
  }
  .remove {
    appearance: none;
    border: none;
    background: none;
    cursor: pointer;
    padding: 0 0.2rem;
    line-height: 1;
    font-size: 1rem;
    color: var(--ae-text-muted);
    margin-left: auto;
  }
  .remove:hover {
    color: var(--ae-accent, var(--ae-text-strong));
  }
  .empty {
    margin: 0;
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    color: var(--ae-text-muted);
  }
  /* The add-route picker bar. */
  .add-route {
    display: flex;
    flex-direction: row;
    align-items: flex-end;
    flex-wrap: wrap;
    gap: 0.6rem;
    margin-top: 0.6rem;
  }
  .add-route label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    color: var(--ae-text-muted);
  }
  .add-route .arrow {
    align-self: center;
  }
  .add-route select {
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    padding: 0.25rem 0.4rem;
    border-radius: var(--ae-radius-control);
    border: 1px solid color-mix(in srgb, var(--ae-line-panel) 60%, transparent);
    background: var(--ae-bg-chip);
    color: var(--ae-text-primary);
  }
  .add {
    appearance: none;
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    padding: 0.35rem 0.7rem;
    border-radius: var(--ae-radius-control);
    border: 1px solid color-mix(in srgb, var(--ae-accent, var(--ae-line-panel)) 55%, transparent);
    background: color-mix(in srgb, var(--ae-accent, var(--ae-text-strong)) 15%, transparent);
    color: var(--ae-text-strong);
    cursor: pointer;
  }
  .add:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .add:not(:disabled):hover {
    background: color-mix(in srgb, var(--ae-accent, var(--ae-text-strong)) 30%, transparent);
  }
</style>
