<script lang="ts">
  // A master-output level meter. In Story 4.2 it reads the **already-exposed output buffer** (the host
  // monitor level the worklet posts as a peak each ~8 quanta) — an honest signal, but the host monitor
  // level, *not* a simulated meter device. A voltage-native, calibrated `VuMeter` node (with real
  // ballistics) + a node→host readout lane land in Story 4.5; this widget repoints onto it then.
  //
  // Skin: an analog VU face (cream glass in a dark bezel, an inked arc with a red over-level zone, and a
  // pivoting needle) painted from the --ae-vu-* tokens. The needle angle comes from the level reading.
  interface Props {
    /** Linear peak of the output block (±1.0 = monitor full scale). */
    level: number;
  }
  let { level }: Props = $props();

  // dBFS scale, -60..0 mapped across the sweep; silence reads as empty (needle hard left).
  const dbfs = $derived(level > 1e-4 ? 20 * Math.log10(level) : Number.NEGATIVE_INFINITY);
  const pct = $derived(Math.max(0, Math.min(100, ((dbfs + 60) / 60) * 100)));
  const readout = $derived(dbfs === Number.NEGATIVE_INFINITY ? "—" : `${dbfs.toFixed(0)} dBFS`);
  // The needle sweeps a 100° arc, -50°..+50° from vertical, across the -60..0 dBFS range.
  const needle = $derived(pct - 50);
  const ticks = [-50, -25, 0, 25, 50];
</script>

<div class="vu">
  <span class="cap">OUT</span>
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
    color: var(--ae-text-muted);
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
    color: var(--ae-text-muted);
  }
</style>
