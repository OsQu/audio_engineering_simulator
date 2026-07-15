<script lang="ts">
  // A **hardware** push button bound to a device structural config key — the metal-faceplate
  // counterpart of the focus surface's `ConfigSwitch` (same binding path: flipping it edits the scene
  // and rebuilds the engine, a recompile like repatching — never a smoothed param set). Used for the
  // 8i6's global 48V phantom switch: a latching front-panel button whose integrated lamp lights while
  // engaged. Sized in **mm** (faceplates are 1 px/mm; the world/bench zoom scales it), like Led/Switch.
  import { getDeviceHandle } from "../device-handle";

  interface Props {
    /** Structural config key (matches a `ConfigDescriptor.key`). */
    key: string;
    /** Override the descriptor's label. */
    label?: string;
    /** Physical button width in **mm** (real-gear sizing, scaled by the world/bench zoom). */
    size?: number;
  }
  let { key, label, size = 6 }: Props = $props();

  const handle = getDeviceHandle();
  const desc = $derived(handle.configDesc(key));
  const on = $derived(handle.config(key) >= 0.5);
  const text = $derived(label ?? desc?.label ?? key);
  const sizeVars = $derived(
    `--btn: ${size}px; --btn-font: ${(size * 0.45).toFixed(2)}px; ` +
      `--btn-gap: ${(size * 0.2).toFixed(2)}px`,
  );
</script>

{#if desc}
  <div class="config-button" style={sizeVars}>
    <button
      type="button"
      class:on
      role="switch"
      aria-checked={on}
      aria-label={text}
      onclick={() => handle.setConfig(key, on ? 0 : 1)}
    >
      <span class="lamp"></span>
    </button>
    <span class="label">{text}</span>
  </div>
{/if}

<style>
  /* A recessed square push button (jack bevel tokens) with a centred lamp. The lamp lights **red** when
     engaged — the hardware convention for a 48 V phantom indicator (the green tokens stay with the
     power/INST family). */
  .config-button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--btn-gap, 1px);
  }
  /* Every selector is scoped under `.config-button` so the bare-element `button` rule can never leak to
     other buttons in the app (the shared-`<style>` scope hash is belt; this is braces). */
  .config-button button {
    position: relative;
    width: var(--btn, 6px);
    aspect-ratio: 1;
    padding: 0;
    border: 0.4px solid var(--ae-jack-edge);
    border-radius: 18%;
    background: radial-gradient(circle at 50% 32%, var(--ae-jack-top), var(--ae-jack-bot));
    box-shadow: var(--ae-bevel-press);
    cursor: pointer;
  }
  .config-button .lamp {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 45%;
    aspect-ratio: 1;
    border-radius: 50%;
    background: var(--ae-led-red-off);
    box-shadow: var(--ae-bevel-press);
  }
  .config-button button.on .lamp {
    background: radial-gradient(circle at 40% 35%, var(--ae-led-red-lit), var(--ae-led-red));
    box-shadow:
      0 0 5px var(--ae-led-red-glow),
      inset 0 0 2px rgba(255, 255, 255, 0.7);
  }
  .config-button .label {
    font-family: var(--ae-font-ui);
    font-weight: var(--ae-label-weight);
    font-size: var(--btn-font, 2.6px);
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    color: var(--ae-faceplate-ink, var(--ae-text-strong));
    line-height: 1;
  }
</style>
