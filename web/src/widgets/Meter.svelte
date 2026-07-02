<script lang="ts">
  // A device meter readout for one scalar reading from the engine's node→host lane (a VU/dBu/dBFS
  // level). Unlike the master `Vu` (which reads the host monitor buffer), this shows a *simulated meter
  // node's* reading, addressed by (device, readout id). The presentation matches the quantity: a VU
  // reading sweeps the analog `VuFace` needle; a dBu/dBFS level is a horizontal LED bar. The per-unit
  // range lives here (one source), so both presentations map the reading onto the same 0..1 position.
  import VuFace from "./VuFace.svelte";

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

{#if unit === "VU"}
  <!-- A VU reading is the archetypal analog needle, not a bar — share the master monitor's face,
       compact so it fits a thin (1U) device front. -->
  <VuFace fraction={pct / 100} {label} {readout} compact />
{:else}
  <div class="meter-widget" title={`${label} (${unit})`}>
    <span class="cap">{label}</span>
    <div class="bar">
      <div class="unlit" style={`left: ${pct}%`}></div>
    </div>
    <span class="readout">{readout}</span>
  </div>
{/if}

<style>
  /* Sits on a device faceplate, so the engraved text reads the faceplate ink tokens (dark on a light
     finish, light on a dark one); the bar is a recessed screen with an LED-palette fill, dark where
     unlit — matching the app's indicator LEDs (--ae-led-*) and the analog VU face. */
  .meter-widget {
    display: flex;
    align-items: center;
    gap: var(--ae-space-1);
    font-family: var(--ae-font-ui);
    font-size: 0.7rem;
  }
  .cap {
    min-width: 2.2rem;
    letter-spacing: var(--ae-label-spacing);
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
    text-transform: uppercase;
  }
  .bar {
    position: relative;
    flex: 1;
    min-width: 4rem;
    height: 0.6rem;
    border-radius: var(--ae-radius-control);
    overflow: hidden;
    /* Recessed meter well: LED green up to ~65% of the range, amber, then red near clip. */
    background: linear-gradient(
      to right,
      var(--ae-led-green) 0%,
      var(--ae-led-green) 65%,
      var(--ae-led-amber) 85%,
      var(--ae-led-red) 97%
    );
    box-shadow: var(--ae-bevel-press);
  }
  .unlit {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    background: var(--ae-led-neutral-off);
    transition: left 0.08s linear;
  }
  .readout {
    min-width: 4.5rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
  }
</style>
