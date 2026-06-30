<script lang="ts">
  // A master-output level meter. In Story 4.2 it reads the **already-exposed output buffer** (the host
  // monitor level the worklet posts as a peak each ~8 quanta) — an honest signal, but the host monitor
  // level, *not* a simulated meter device. A voltage-native, calibrated `VuMeter` node (with real
  // ballistics) + a node→host readout lane land in Story 4.5; this widget repoints onto it then.
  interface Props {
    /** Linear peak of the output block (±1.0 = monitor full scale). */
    level: number;
  }
  let { level }: Props = $props();

  // dBFS scale, -60..0 mapped across the bar; silence reads as empty.
  const dbfs = $derived(level > 1e-4 ? 20 * Math.log10(level) : Number.NEGATIVE_INFINITY);
  const pct = $derived(Math.max(0, Math.min(100, ((dbfs + 60) / 60) * 100)));
  const readout = $derived(dbfs === Number.NEGATIVE_INFINITY ? "—" : `${dbfs.toFixed(0)} dBFS`);
</script>

<div class="vu">
  <span class="cap">OUT</span>
  <div class="meter">
    <div class="unlit" style={`left: ${pct}%`}></div>
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
    font-size: 0.65rem;
    letter-spacing: 0.05em;
    color: #888;
  }
  .meter {
    position: relative;
    width: 12rem;
    height: 0.8rem;
    border-radius: 3px;
    overflow: hidden;
    /* Fixed colour zones: green to ~-12 dB, amber to ~-3 dB, red near clip. */
    background: linear-gradient(to right, #36d36b 0%, #36d36b 60%, #e0a93c 80%, #d6453c 95%);
  }
  /* Covers the un-lit part from the level rightward, revealing the gradient up to the peak. */
  .unlit {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    background: #d4d4d4;
    transition: left 0.08s linear;
  }
  .readout {
    width: 4.5rem;
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
    color: #777;
  }
</style>
