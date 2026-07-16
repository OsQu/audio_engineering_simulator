<script lang="ts">
  // A single **duplex** patch jack (USB-C, Ethernet): one physical connector carrying both directions.
  // It binds a device's paired output + input ports (declared by the catalog's `duplex_links`, surfaced
  // as `PortDescriptor.duplexPartner`) and renders **one** shared `Jack`. The output side is the
  // canonical `data-jack` key; the input side rides along as `data-jack-alt`, so the cable layer can
  // anchor either leg of the link at this one jack, and a drag from it authors a `duplex` connection.
  // A faceplate places it once, where the real single USB/Ethernet jack sits.
  import { getDeviceHandle } from "../device-handle";
  import Jack from "./Jack.svelte";

  interface Props {
    /** Output port id — the send side of the duplex jack. */
    outId: number;
    /** Input port id — the return side (the output's `duplexPartner`). */
    inId: number;
    /** Physical connector diameter in mm (real-gear sizing, scaled by the world/bench zoom). */
    size?: number;
  }
  let { outId, inId, size }: Props = $props();

  const handle = getDeviceHandle();
  const port = $derived(handle.port("output", outId));
</script>

{#if port}
  <Jack device={handle.device} {port} {size} alt={{ direction: "input", id: inId }} />
{/if}
