<script lang="ts">
  // A device meter readout: a labelled horizontal bar + numeric value for one scalar reading from the
  // engine's node→host lane (a VU/dBu/dBFS level). Unlike the master `Vu` (which reads the host monitor
  // buffer), this shows a *simulated meter node's* reading, addressed by (device, readout id). The bar's
  // range is chosen per unit so the scale reads sensibly (a VU face, a dBFS bar, …).

  interface Props {
    /** Readout label (e.g. "VU", "Peak"). */
    label: string;
    /** Unit string (e.g. "VU", "dBu", "dBFS") — also picks the bar's range. */
    unit: string;
    /** The live reading. A very low value (the engine's floor) reads as empty / "—". */
    value: number;
  }
  let { label, unit, value }: Props = $props();

  // Per-unit bar range (min..max), so each meter's scale matches its quantity. VU centres on 0 VU;
  // dBu spans a wide analog range; dBFS runs up to the 0 dBFS ceiling.
  const RANGES: Record<string, { min: number; max: number }> = {
    VU: { min: -20, max: 3 },
    dBu: { min: -40, max: 24 },
    dBFS: { min: -60, max: 0 },
  };
  const range = $derived(RANGES[unit] ?? { min: -60, max: 0 });
  // Below ~this the reading is treated as silence (the meter floors read −60/−120).
  const OFF_SCALE = -55;

  const pct = $derived(
    Math.max(0, Math.min(100, ((value - range.min) / (range.max - range.min)) * 100)),
  );
  const readout = $derived(value <= OFF_SCALE ? "—" : `${value.toFixed(1)} ${unit}`);
</script>

<div class="meter-widget" title={`${label} (${unit})`}>
  <span class="cap">{label}</span>
  <div class="bar">
    <div class="unlit" style={`left: ${pct}%`}></div>
  </div>
  <span class="readout">{readout}</span>
</div>

<style>
  .meter-widget {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.7rem;
  }
  .cap {
    min-width: 2.2rem;
    letter-spacing: 0.05em;
    color: #666;
    text-transform: uppercase;
  }
  .bar {
    position: relative;
    flex: 1;
    min-width: 4rem;
    height: 0.6rem;
    border-radius: 3px;
    overflow: hidden;
    /* Green up to ~75% of the range, amber, then red near the top (clip). */
    background: linear-gradient(to right, #36d36b 0%, #36d36b 65%, #e0a93c 85%, #d6453c 97%);
  }
  .unlit {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    background: #d4d4d4;
    transition: left 0.08s linear;
  }
  .readout {
    min-width: 4.5rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: #777;
  }
</style>
