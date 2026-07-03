<script lang="ts">
  // A control bound to a device param **by id**: it resolves the param descriptor from the ambient
  // `DeviceHandle` (put in context by `Chassis`) and renders the widget its `kind` implies — or an
  // explicit override. Drives the live engine through the handle. A faceplate places one of these per
  // control it wants to show; it never touches the worklet or the param plumbing.
  import type { CapFinish } from "../skin";
  import { getDeviceHandle } from "../device-handle";
  import Fader from "./Fader.svelte";
  import Knob from "./Knob.svelte";
  import Switch from "./Switch.svelte";

  interface Props {
    /** Exposed param id (its position in the device's exposed param list). */
    id: number;
    /** Override the widget kind; defaults to the descriptor's `kind`. */
    as?: "knob" | "fader" | "switch";
    /** Knob cap finish (ignored by fader/switch). */
    cap?: CapFinish;
  }
  let { id, as, cap = "dark" }: Props = $props();

  const handle = getDeviceHandle();
  const param = $derived(handle.param(id));
  const kind = $derived(as ?? param?.kind ?? "knob");
</script>

{#if param}
  {#if kind === "fader"}
    <Fader {param} value={handle.value(id)} onChange={(v) => handle.set(id, v)} />
  {:else if kind === "switch"}
    <Switch {param} value={handle.value(id)} onChange={(v) => handle.set(id, v)} />
  {:else}
    <Knob {param} value={handle.value(id)} onChange={(v) => handle.set(id, v)} {cap} />
  {/if}
{/if}
