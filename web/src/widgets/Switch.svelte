<script lang="ts">
  // A toggle switch (the device power switch): clicking flips the param between 0 and 1. The smoothed
  // `powered` param on the engine side de-clicks the transition; here it's just a 0/1 control.
  import type { ParamDescriptor } from "../catalog";

  interface Props {
    param: ParamDescriptor;
    value: number;
    onChange: (v: number) => void;
  }
  let { param, value, onChange }: Props = $props();

  const on = $derived(value >= 0.5);
</script>

<div class="switch">
  <button
    type="button"
    class:on
    role="switch"
    aria-checked={on}
    aria-label={param.label}
    onclick={() => onChange(on ? 0 : 1)}
  >
    <span class="led"></span>
    <span class="cap"></span>
  </button>
  <span class="label">{param.label}</span>
</div>

<style>
  /* A bat-toggle: recessed barrel (jack tokens) + a metal nub that slides up when on, with an LED lamp
     above it (led tokens). Internals are proportional (%), and the button scales with the chassis height
     (cqh, capped at its rem) so a 1U rack unit shrinks it instead of clipping — same approach as Knob. */
  .switch {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: min(4.5rem, 92cqh);
    gap: min(0.25rem, 3cqh);
  }
  button {
    position: relative;
    width: min(2.2rem, 40cqh);
    height: min(3rem, 58cqh);
    padding: 0;
    border: 1px solid var(--ae-jack-edge);
    border-radius: 4px;
    background: radial-gradient(circle at 50% 32%, var(--ae-jack-top), var(--ae-jack-bot));
    box-shadow: var(--ae-bevel-press);
    cursor: pointer;
  }
  .led {
    position: absolute;
    top: 9%;
    left: 50%;
    transform: translateX(-50%);
    width: 20%;
    aspect-ratio: 1;
    border-radius: 50%;
    background: var(--ae-led-red-off);
    box-shadow: var(--ae-bevel-press);
  }
  button.on .led {
    background: radial-gradient(circle at 40% 35%, var(--ae-led-green-lit), var(--ae-led-green));
    box-shadow:
      0 0 6px var(--ae-led-green-glow),
      inset 0 0 2px rgba(255, 255, 255, 0.7);
  }
  .cap {
    position: absolute;
    left: 12%;
    right: 12%;
    bottom: 8%;
    height: 46%;
    border-radius: 3px;
    background: linear-gradient(to bottom, var(--ae-fader-cap-top), var(--ae-fader-cap-bot));
    box-shadow: var(--ae-bevel-top);
    transition: transform 0.08s ease;
  }
  button.on .cap {
    transform: translateY(-34%);
    background: linear-gradient(to bottom, var(--ae-metal-collar-3), var(--ae-fader-cap-top));
  }
  .label {
    font-family: var(--ae-font-ui);
    font-weight: var(--ae-label-weight);
    font-size: min(var(--ae-label-size), 17cqh);
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    color: var(--ae-faceplate-ink, var(--ae-text-strong));
    text-align: center;
    line-height: 1.15;
  }
</style>
