<script lang="ts">
  // The master-output level meter. In Story 4.2 it reads the **already-exposed output buffer** (the host
  // monitor level the worklet posts as a peak each ~8 quanta) — an honest signal, but the host monitor
  // level, *not* a simulated meter device. A voltage-native, calibrated `VuMeter` node (with real
  // ballistics) + a node→host readout lane land in Story 4.5; this widget repoints onto it then.
  //
  // Presentation is the shared analog `VuFace`; here we only map the linear monitor peak onto its 0..1
  // needle fraction (−60..0 dBFS) and format the dBFS readout.
  //
  // The wrapper carries the `output-peak` live-verification hook (see `EngineHealth.svelte`): the current
  // and held peak as dBFS (`-inf` = silence).
  import { dbfsAttr, peakDbfs } from "../live-status";
  import VuFace from "./VuFace.svelte";

  interface Props {
    /** Linear peak of the output block (±1.0 = monitor full scale). */
    level: number;
    /** The highest `level` since the engine started (exposed as a hook only; the needle shows `level`). */
    hold?: number;
  }
  let { level, hold = 0 }: Props = $props();

  // dBFS scale, -60..0 mapped across the sweep; silence reads as empty (needle hard left).
  const dbfs = $derived(peakDbfs(level));
  const fraction = $derived((dbfs + 60) / 60);
  const readout = $derived(dbfs === Number.NEGATIVE_INFINITY ? "—" : `${dbfs.toFixed(0)} dBFS`);
</script>

<span
  class="hook"
  data-testid="output-peak"
  data-dbfs={dbfsAttr(dbfs)}
  data-hold-dbfs={dbfsAttr(peakDbfs(hold))}
  aria-label="output peak {dbfsAttr(dbfs)} dBFS"
>
  <VuFace {fraction} label="OUT" {readout} />
</span>

<style>
  .hook {
    display: contents;
  }
</style>
