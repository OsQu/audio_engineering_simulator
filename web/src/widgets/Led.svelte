<script lang="ts">
  // A passive indicator LED with a small legend beneath — lit purely from state, no interaction. Used
  // on faceplates for switches that live in software (the 8i6's INST/AIR/PAD, toggled in Focusrite
  // Control): the front panel shows only whether they're on. Presentational; the parent computes `on`.
  interface Props {
    /** Whether the lamp is lit. */
    on: boolean;
    /** Short legend under the lamp (e.g. "INST", "AIR", "PAD"). */
    label: string;
    /** Physical lamp diameter in **mm** (real-gear sizing, scaled by the world/bench zoom). When omitted,
     *  keeps the legacy container-relative sizing. */
    size?: number;
  }
  let { on, label, size }: Props = $props();

  const sizeVars = $derived(
    size === undefined
      ? undefined
      : `--led: ${size}px; --led-font: ${(size * 0.85).toFixed(2)}px; --led-gap: ${(size * 0.25).toFixed(2)}px`,
  );
</script>

<div class="led-indicator" style={sizeVars}>
  <span class="lamp" class:on></span>
  <span class="label">{label}</span>
</div>

<style>
  .led-indicator {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--led-gap, min(0.15rem, 2cqh));
  }
  .lamp {
    width: var(--led, min(0.6rem, 12cqh));
    aspect-ratio: 1;
    border-radius: 50%;
    background: var(--ae-led-red-off);
    box-shadow: var(--ae-bevel-press);
  }
  .lamp.on {
    /* The 8i6's indicators glow green (matching the Switch LED lit state). */
    background: radial-gradient(circle at 40% 35%, var(--ae-led-green-lit), var(--ae-led-green));
    box-shadow:
      0 0 5px var(--ae-led-green-glow),
      inset 0 0 2px rgba(255, 255, 255, 0.7);
  }
  .label {
    font-family: var(--ae-font-ui);
    font-weight: var(--ae-label-weight);
    font-size: var(--led-font, min(0.5rem, 11cqh));
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    color: var(--ae-faceplate-ink, var(--ae-text-primary));
    line-height: 1;
  }
</style>
