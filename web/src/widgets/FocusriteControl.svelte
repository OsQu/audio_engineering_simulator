<script lang="ts">
  // The 8i6's "Focusrite Control" focus surface: the software page for the preamp switches and the
  // routing matrix. INST/AIR/PAD are software-controlled on the real 2nd-gen 8i6 (the front panel has
  // only indicator LEDs); the routing matrix is Focusrite Control's mixer, here a crosspoint grid. It
  // publishes the `DeviceHandle` itself (no `Chassis` bezel — this is an app window, not the metal box)
  // so the bound widgets work: INST is a structural `ConfigSwitch` (recompiles), PAD/AIR are runtime
  // `Control` switches, and each matrix cell is a `Control` knob bound to a crosspoint gain.
  //
  // The **preamp switches** are placed with literal ids so the guardrail can statically confirm this
  // surface covers Pad/Air (1/2/4/5). The **matrix grid** is data-rendered — it derives its rows/cols
  // and per-cell param ids from the crosspoint params (those whose label reads "input → output"), so
  // its ids aren't literal in the source; the guardrail covers them via a declared range instead
  // (Story 5.7.6's declared-coverage mechanism, first exercised here). Exposed ids from the Rust entry:
  // 0 Gain1 · 1 Pad1 · 2 Air1 · 3 Gain2 · 4 Pad2 · 5 Air2 · 6–14 crosspoints · 15 Monitor · 16 Phones ·
  // 17 Power.
  import { untrack } from "svelte";
  import { makeHandle, setDeviceHandle } from "../device-handle";
  import type { DeviceUiProps } from "../device-ui";
  import ConfigSwitch from "./ConfigSwitch.svelte";
  import Control from "./Control.svelte";

  let props: DeviceUiProps = $props();
  // The handle reads reactive props at call time, so capture the (stable) props object once.
  setDeviceHandle(makeHandle(untrack(() => props)));

  // The routing matrix, derived from the crosspoint params — those whose label reads "input → output"
  // (authored by the Rust catalog). Rows are the unique inputs, columns the unique outputs, in first-
  // seen order; each cell carries its crosspoint's exposed param id. Deriving the grid from the labels
  // keeps it robust to id shifts — the structure comes from the data, not hard-coded positions.
  interface Crosspoint {
    id: number;
    input: string;
    output: string;
  }
  const crosspoints = $derived<Crosspoint[]>(
    (props.params ?? [])
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

<div class="focusrite">
  <section>
    <span class="section-title">Preamps</span>
    <p class="hint">INST rewires the input (recompiles); AIR/PAD are live.</p>
    <div class="channels">
      <div class="channel">
        <span class="channel-name">Channel 1</span>
        <div class="switches">
          <ConfigSwitch key="inst1" label="Inst" />
          <Control id={1} as="switch" />
          <Control id={2} as="switch" />
        </div>
      </div>
      <div class="channel">
        <span class="channel-name">Channel 2</span>
        <div class="switches">
          <ConfigSwitch key="inst2" label="Inst" />
          <Control id={4} as="switch" />
          <Control id={5} as="switch" />
        </div>
      </div>
    </div>
  </section>

  {#if crosspoints.length > 0}
    <section>
      <span class="section-title">Routing</span>
      <p class="hint">Each cell mixes an input into an output — turn it up to route.</p>
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
    </section>
  {/if}
</div>

<style>
  .focusrite {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    padding: 1rem;
    min-width: 28rem;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .section-title {
    font-family: var(--ae-font-display);
    font-weight: 700;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    font-size: var(--ae-legend-size, 0.8rem);
    color: var(--ae-accent, var(--ae-text-strong));
  }
  .hint {
    margin: 0;
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    color: var(--ae-text-secondary, var(--ae-text-primary));
  }
  .channels {
    display: flex;
    flex-direction: row;
    gap: 1.5rem;
  }
  .channel {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.6rem;
    padding: 0.75rem 1rem;
    border: 1px solid color-mix(in srgb, var(--ae-accent, var(--ae-line-panel)) 40%, transparent);
    border-radius: var(--ae-radius-control);
    background: var(--ae-bg-chip);
  }
  .channel-name {
    font-family: var(--ae-font-display);
    font-weight: 700;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    font-size: var(--ae-legend-size, 0.75rem);
    color: var(--ae-text-strong);
  }
  .switches {
    display: flex;
    flex-direction: row;
    gap: 1rem;
    align-items: flex-start;
  }
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
