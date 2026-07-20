<script lang="ts">
  // A static waveform thumbnail for a DAW take (Story 5.11.6) — display only. Draws the host-computed
  // per-bucket peak magnitudes as centred vertical bars in an SVG scaled to the container. The peaks are
  // a filesystem read the host renders (see waveform.ts); the simulation still owns all audio.
  interface Props {
    /** Per-bucket peak magnitudes (0..~1), left to right. Empty ⇒ nothing drawn. */
    peaks: number[];
    /** Transport playhead as a fraction of the take (0..1) — draws a scrolling cursor line. Omit /
     *  out of range ⇒ no cursor (stopped past the take, or no transport yet). */
    position?: number;
  }
  let { peaks, position }: Props = $props();

  // The cursor's x in viewBox units (bucket space), when the playhead is within the take.
  const cursorX = $derived(
    position !== undefined && position >= 0 && position <= 1 ? position * peaks.length : null,
  );
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
    {#if cursorX !== null}
      <line class="cursor" x1={cursorX} x2={cursorX} y1="0" y2="100" />
    {/if}
  </svg>
{/if}

<style>
  .wave {
    width: 100%;
    /* Default thumbnail height; the arrangement overrides `--wave-h: 100%` so a lane's clip fills it. */
    height: var(--wave-h, 2.4rem);
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
  .cursor {
    stroke: var(--ae-text-strong, #fff);
    stroke-width: 1.5;
    opacity: 0.85;
  }
</style>
