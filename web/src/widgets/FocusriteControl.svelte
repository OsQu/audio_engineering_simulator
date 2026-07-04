<script lang="ts">
  // The 8i6's "Focusrite Control" focus surface: the software page where the preamp switches live
  // (INST/AIR/PAD are software-controlled on the real 2nd-gen 8i6 — the front panel has only indicator
  // LEDs). v1 shows the two channels' preamp switches; Story 5.7.9 grows this same surface with the
  // routing matrix. It publishes the `DeviceHandle` itself (no `Chassis` bezel — this is an app window,
  // not the metal box) so the bound widgets work: INST is a structural `ConfigSwitch` (recompiles),
  // PAD/AIR are runtime param switches (`Control`).
  import { untrack } from "svelte";
  import { makeHandle, setDeviceHandle } from "../device-handle";
  import type { DeviceUiProps } from "../device-ui";
  import ConfigSwitch from "./ConfigSwitch.svelte";
  import Control from "./Control.svelte";

  let props: DeviceUiProps = $props();
  // The handle reads reactive props at call time, so capture the (stable) props object once.
  setDeviceHandle(makeHandle(untrack(() => props)));

  // The switches are placed with **literal** ids/keys (not a data loop) so the faceplate guardrail can
  // statically confirm this surface covers Pad/Air — the exposed ids from the Rust entry are
  // 0 Gain1 · 1 Pad1 · 2 Air1 · 3 Gain2 · 4 Pad2 · 5 Air2 · 6 Monitor · 7 Phones · 8 Power.
</script>

<div class="focusrite">
  <p class="hint">Preamp switches — INST rewires the input (recompiles); AIR/PAD are live.</p>
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
</div>

<style>
  .focusrite {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1rem;
    min-width: 24rem;
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
