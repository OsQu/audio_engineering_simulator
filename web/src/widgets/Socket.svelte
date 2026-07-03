<script lang="ts">
  // A patch jack bound to a device port **by direction + id**: it resolves the port descriptor from the
  // ambient `DeviceHandle` and renders the shared `Jack` (styled by connector kind/domain, carrying the
  // `data-jack` attribute the cable layer measures). A faceplate places these wherever it wants the port
  // to appear — on either face — so port-to-face assignment is simply where the markup is written.
  import type { PortDirection } from "../catalog";
  import { getDeviceHandle } from "../device-handle";
  import Jack from "./Jack.svelte";

  interface Props {
    /** Which direction of port to bind. */
    dir: PortDirection;
    /** Port id within its direction. */
    id: number;
  }
  let { dir, id }: Props = $props();

  const handle = getDeviceHandle();
  const port = $derived(handle.port(dir, id));
</script>

{#if port}
  <Jack device={handle.device} {port} />
{/if}
