<script lang="ts">
  // A static waveform thumbnail for a DAW take (Story 5.11.6) — display only. Draws the host-computed
  // per-bucket peak magnitudes as centred vertical bars in an SVG scaled to the container. The peaks are
  // a filesystem read the host renders (see waveform.ts); the simulation still owns all audio.
  interface Props {
    /** Per-bucket peak magnitudes (0..~1), left to right. Empty ⇒ nothing drawn. */
    peaks: number[];
  }
  let { peaks }: Props = $props();
</script>

{#if peaks.length > 0}
  <svg
    class="wave"
    viewBox={`0 0 ${peaks.length} 100`}
    preserveAspectRatio="none"
    aria-hidden="true"
  >
    {#each peaks as p, i (i)}
      <line x1={i + 0.5} x2={i + 0.5} y1={50 - p * 48} y2={50 + p * 48} />
    {/each}
  </svg>
{/if}

<style>
  .wave {
    width: 100%;
    height: 2.4rem;
    display: block;
    background: var(--ae-surface-2, #151515);
    border-radius: 0.25rem;
  }
  line {
    stroke: var(--ae-accent, #4a90d9);
    stroke-width: 1;
    /* Keep the bar width constant despite the non-uniform viewBox scale. */
    vector-effect: non-scaling-stroke;
  }
</style>
