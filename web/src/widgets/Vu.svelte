<script lang="ts">
  // The master-output level meter. In Story 4.2 it reads the **already-exposed output buffer** (the host
  // monitor level the worklet posts as a peak each ~8 quanta) — an honest signal, but the host monitor
  // level, *not* a simulated meter device. A voltage-native, calibrated `VuMeter` node (with real
  // ballistics) + a node→host readout lane land in Story 4.5; this widget repoints onto it then.
  //
  // Presentation is the shared analog `VuFace`; here we only map the linear monitor peak onto its 0..1
  // needle fraction (−60..0 dBFS) and format the dBFS readout.
  import VuFace from "./VuFace.svelte";

  interface Props {
    /** Linear peak of the output block (±1.0 = monitor full scale). */
    level: number;
  }
  let { level }: Props = $props();

  // dBFS scale, -60..0 mapped across the sweep; silence reads as empty (needle hard left).
  const dbfs = $derived(level > 1e-4 ? 20 * Math.log10(level) : Number.NEGATIVE_INFINITY);
  const fraction = $derived((dbfs + 60) / 60);
  const readout = $derived(dbfs === Number.NEGATIVE_INFINITY ? "—" : `${dbfs.toFixed(0)} dBFS`);
</script>

<VuFace {fraction} label="OUT" {readout} />
