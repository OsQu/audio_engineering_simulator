<script lang="ts">
  // The `computer`'s focus surface — a minimal "DAW mixer": the send meters plus the loopback routing
  // matrix (the `Matrix` node's crosspoints, defaulting to send 1→return 1, send 2→return 2). It's an app
  // window, not the metal box, so it publishes the `DeviceHandle` itself (no `Chassis`) and composes the
  // shared bound widgets. The routing grid is data-rendered by `RoutingGrid` (rows = sends, cols =
  // returns, each cell a crosspoint knob); its ids aren't literal in the source, so the guardrail covers
  // them via a **declared range** (0–47). A real DAW UI (transport, tracks, pan/solo) is future work —
  // this exists so the loopback is visible and adjustable. Exposed ids: params 0–47 crosspoints, readouts
  // 0–15 the 8 send meters.
  import { untrack } from "svelte";
  import { makeHandle, setDeviceHandle } from "../device-handle";
  import type { DeviceUiProps } from "../device-ui";
  import Reading from "./Reading.svelte";
  import RoutingGrid from "./RoutingGrid.svelte";

  let props: DeviceUiProps = $props();
  setDeviceHandle(makeHandle(untrack(() => props)));
  const meters = $derived((props.readouts ?? []).map((r) => r.id));
</script>

<div class="mixer">
  <section>
    <span class="section-title">USB Sends</span>
    <p class="hint">The per-lane levels the DAW is recording.</p>
    <div class="meters">
      {#each meters as id (id)}
        <Reading {id} />
      {/each}
    </div>
  </section>

  <section>
    <span class="section-title">Routing</span>
    <p class="hint">
      Each cell mixes a send into a return — the loopback the interface plays back.
    </p>
    <RoutingGrid params={props.params} />
  </section>
</div>

<style>
  .mixer {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    padding: 1rem;
    min-width: 28rem;
    max-width: 100%;
    box-sizing: border-box;
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
  .meters {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
    gap: clamp(0.4rem, 2cqw, 1.2rem);
  }
</style>
