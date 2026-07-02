<script lang="ts">
  // The analog VU face: a cream glass in a dark bezel, an inked scale arc with a red over-level zone,
  // and a pivoting needle whose angle comes from a plain 0..1 `fraction`. Pure presentation — the
  // caller maps its own quantity (the master monitor's dBFS, or a device's VU-unit reading) onto the
  // fraction and supplies the formatted readout text. Skin painted from the --ae-vu-* tokens.
  interface Props {
    /** Needle position: 0 = hard left / silence, 1 = full right. Values outside 0..1 are clamped. */
    fraction: number;
    /** Left-hand cap label (e.g. "OUT" for the master monitor, "VU" for a meter device). */
    label: string;
    /** Formatted numeric readout shown to the right (e.g. "-6 dBFS", "-2.0 VU", "—" for silence). */
    readout: string;
    /** Shrink the face to fit a thin (1U) device front. The header monitor uses the full size. */
    compact?: boolean;
  }
  let { fraction, label, readout, compact = false }: Props = $props();

  // The needle sweeps a 100° arc, -50°..+50° from vertical, across the 0..1 fraction.
  const needle = $derived(Math.max(0, Math.min(1, fraction)) * 100 - 50);
  const ticks = [-50, -25, 0, 25, 50];
</script>

<div class="vu" class:compact>
  <span class="cap">{label}</span>
  <div class="bezel">
    <div class="glass">
      <svg class="face" viewBox="0 0 150 92" aria-hidden="true">
        <!-- scale arc + the over-level red segment near the top-right -->
        <path class="arc" d="M 26 45 A 64 64 0 0 1 124 45" />
        <path class="arc red" d="M 107 31 A 64 64 0 0 1 124 45" />
        {#each ticks as t (t)}
          <line class="tick" x1="75" y1="24" x2="75" y2="30" transform={`rotate(${t} 75 86)`} />
        {/each}
        <text class="mark" x="75" y="78" text-anchor="middle">VU</text>
        <line class="needle" x1="75" y1="86" x2="75" y2="28" transform={`rotate(${needle} 75 86)`} />
        <circle class="pivot" cx="75" cy="86" r="5" />
      </svg>
    </div>
  </div>
  <span class="readout">{readout}</span>
</div>

<style>
  .vu {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .cap {
    font-family: var(--ae-font-ui);
    font-size: 0.65rem;
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
  }
  .bezel {
    padding: 5px;
    border-radius: var(--ae-radius-panel);
    background: linear-gradient(to bottom, var(--ae-vu-bezel-1), var(--ae-vu-bezel-2));
    box-shadow:
      0 2px 5px rgba(0, 0, 0, 0.6),
      inset 0 0 0 1px #000;
  }
  .glass {
    width: 8rem;
    border-radius: 3px;
    overflow: hidden;
    background: linear-gradient(to bottom, var(--ae-vu-face-top), var(--ae-vu-face-bot));
  }
  .face {
    display: block;
    width: 100%;
    height: auto;
  }
  .arc {
    fill: none;
    stroke: var(--ae-vu-ink);
    stroke-width: 2;
  }
  .arc.red {
    stroke: var(--ae-vu-red);
    stroke-width: 3;
  }
  .tick {
    stroke: var(--ae-vu-ink);
    stroke-width: 1.5;
    stroke-linecap: round;
  }
  .needle {
    stroke: var(--ae-vu-needle);
    stroke-width: 2;
    stroke-linecap: round;
  }
  .pivot {
    fill: var(--ae-vu-pivot);
  }
  .mark {
    fill: var(--ae-vu-ink);
    font-family: var(--ae-font-ui);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }
  .readout {
    width: 4.5rem;
    font-family: var(--ae-font-ui);
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
  }

  /* Compact (1U device front): scale the face down so its height fits a thin rack unit. Width drives
     height (the face keeps its aspect), so a narrower glass is a shorter meter. */
  .vu.compact {
    gap: 0.3rem;
  }
  .vu.compact .bezel {
    padding: 3px;
  }
  .vu.compact .glass {
    width: 3rem;
  }
  .vu.compact .cap {
    font-size: 0.55rem;
  }
  .vu.compact .readout {
    width: 2.75rem;
    font-size: 0.6rem;
  }
</style>
