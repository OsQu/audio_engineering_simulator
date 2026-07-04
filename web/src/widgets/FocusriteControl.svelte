<script lang="ts">
  // The 8i6's "Focusrite Control" focus surface: the software page for the preamp switches and the
  // routing matrix. INST/AIR/PAD are software-controlled on the real 2nd-gen 8i6 (the front panel has
  // only indicator LEDs); the routing matrix is Focusrite Control's mixer, drawn by the shared
  // `RoutingGrid`. It publishes the `DeviceHandle` itself (no `Chassis` bezel — this is an app window,
  // not the metal box) so the bound widgets work: INST is a structural `ConfigSwitch` (recompiles),
  // PAD/AIR are runtime `Control` switches, and each matrix cell is a `Control` knob bound to a
  // crosspoint gain.
  //
  // The **preamp switches** are placed with literal ids so the guardrail can statically confirm this
  // surface covers Pad/Air (1/2/4/5). The **matrix grid** is data-rendered by `RoutingGrid` — it derives
  // its rows/cols and per-cell param ids from the crosspoint params (those whose label reads "input →
  // output"), so its ids aren't literal in the source; the guardrail covers them via a declared range
  // instead (Story 5.7.6's declared-coverage mechanism). Exposed ids from the Rust entry (full 8i6):
  // 0 Gain1 · 1 Pad1 · 2 Air1 · 3 Gain2 · 4 Pad2 · 5 Air2 · 6 Phones1 · 7 Phones2 · 8–203 crosspoints ·
  // 204 Monitor · 205 Power.
  import { untrack } from "svelte";
  import { makeHandle, setDeviceHandle } from "../device-handle";
  import type { DeviceUiProps } from "../device-ui";
  import ConfigSwitch from "./ConfigSwitch.svelte";
  import Control from "./Control.svelte";
  import RoutingGrid from "./RoutingGrid.svelte";

  let props: DeviceUiProps = $props();
  // The handle reads reactive props at call time, so capture the (stable) props object once.
  setDeviceHandle(makeHandle(untrack(() => props)));

  const hasRouting = $derived((props.params ?? []).some((p) => p.label.includes("→")));
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

  {#if hasRouting}
    <section>
      <span class="section-title">Routing</span>
      <p class="hint">Each cell mixes an input into an output — turn it up to route.</p>
      <RoutingGrid params={props.params} />
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
</style>
