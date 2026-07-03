<script lang="ts">
  // A meter bound to a device readout **by id**: it resolves the readout descriptor from the ambient
  // `DeviceHandle` and renders the shared `Meter`, fed the live reading off the node→host lane. A
  // faceplate places one per readout it wants to show.
  import { getDeviceHandle } from "../device-handle";
  import Meter from "./Meter.svelte";

  interface Props {
    /** Exposed readout id (its position in the device's exposed readout list). */
    id: number;
  }
  let { id }: Props = $props();

  const handle = getDeviceHandle();
  const readout = $derived(handle.readout(id));
</script>

{#if readout}
  <Meter label={readout.label} unit={readout.unit} value={handle.reading(id)} />
{/if}
