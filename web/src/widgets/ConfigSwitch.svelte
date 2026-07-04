<script lang="ts">
  // A toggle bound to a device **structural config** key (not a param): it reads/writes the config via
  // the ambient `DeviceHandle`, and flipping it edits the scene and rebuilds the engine (a recompile,
  // like repatching) rather than a smoothed param set. Used for INST/hi-Z, which changes the preamp's
  // input impedance — baked into the loading divider at compile, so it's structural, not a knob.
  import { getDeviceHandle } from "../device-handle";

  interface Props {
    /** Structural config key (matches a `ConfigDescriptor.key`). */
    key: string;
    /** Override the descriptor's label. */
    label?: string;
  }
  let { key, label }: Props = $props();

  const handle = getDeviceHandle();
  const desc = $derived(handle.configDesc(key));
  const on = $derived(handle.config(key) >= 0.5);
  const text = $derived(label ?? desc?.label ?? key);
</script>

{#if desc}
  <div class="config-switch">
    <button
      type="button"
      class:on
      role="switch"
      aria-checked={on}
      aria-label={text}
      onclick={() => handle.setConfig(key, on ? 0 : 1)}
    >
      <span class="led"></span>
    </button>
    <span class="label">{text}</span>
  </div>
{/if}

<style>
  /* A compact software-style toggle: a lamp in a rounded pill that lights green when engaged. Smaller
     than the hardware `Switch` (this lives in the Focusrite Control focus surface, not on metal). */
  .config-switch {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.2rem;
  }
  button {
    position: relative;
    width: 2.4rem;
    height: 1.3rem;
    padding: 0;
    border: 1px solid var(--ae-jack-edge);
    border-radius: 0.7rem;
    background: var(--ae-bg-chip);
    box-shadow: var(--ae-bevel-press);
    cursor: pointer;
  }
  .led {
    position: absolute;
    top: 50%;
    left: 22%;
    transform: translate(-50%, -50%);
    width: 0.7rem;
    aspect-ratio: 1;
    border-radius: 50%;
    background: var(--ae-led-red-off);
    transition: left 0.1s ease;
  }
  button.on .led {
    left: 78%;
    background: radial-gradient(circle at 40% 35%, var(--ae-led-green-lit), var(--ae-led-green));
    box-shadow:
      0 0 5px var(--ae-led-green-glow),
      inset 0 0 2px rgba(255, 255, 255, 0.7);
  }
  .label {
    font-family: var(--ae-font-ui);
    font-weight: var(--ae-label-weight);
    font-size: var(--ae-label-size);
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    color: var(--ae-faceplate-ink, var(--ae-text-strong));
    line-height: 1.1;
  }
</style>
