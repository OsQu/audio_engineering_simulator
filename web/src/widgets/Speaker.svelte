<script lang="ts">
  // The speaker's faceplate — a compact desktop monitor (the graph's analog terminus). No params: the
  // front is a decorative driver cone + a "Monitor" legend, the back carries the line In (fed by the DA)
  // and the voltage Tap output. Exposed ids from the Rust `speaker` entry: input 0 = In, output 0 = Tap.
  import { untrack } from "svelte";
  import type { DeviceUiProps } from "../device-ui";
  import { makeHandle } from "../device-handle";
  import { skinFor } from "../skin";
  import Chassis from "./Chassis.svelte";
  import Col from "./layout/Col.svelte";
  import Legend from "./Legend.svelte";
  import Socket from "./Socket.svelte";

  let props: DeviceUiProps = $props();
  const skin = $derived(skinFor(props.typeId));
  const handle = makeHandle(untrack(() => props));

  const faceVars = "--legend: 3.5px; --jack: 10px; --jack-font: 3.5px; --jack-gap: 1.2px";
</script>

<Chassis {handle} flipped={props.flipped} finish={skin.finish} accent={skin.accent}>
  {#snippet front()}
    <Col fill align="center" justify="center" gap={6} style={faceVars}>
      <div class="cone" aria-hidden="true"><span class="dust"></span></div>
      <Legend text="Monitor" />
    </Col>
  {/snippet}

  {#snippet back()}
    <Col fill align="center" justify="center" gap={8} style={faceVars}>
      <Col gap={1.5} align="center">
        <Legend text="In" />
        <Socket dir="input" id={0} />
      </Col>
      <Col gap={1.5} align="center">
        <Legend text="Tap" />
        <Socket dir="output" id={0} />
      </Col>
    </Col>
  {/snippet}
</Chassis>

<style>
  /* Decorative driver cone (mm; the panel is 1 px/mm). Not a control — the speaker has no params. */
  .cone {
    width: 80px;
    height: 80px;
    border-radius: 50%;
    background: radial-gradient(circle at 50% 38%, #34383d, #0c0d10 72%);
    border: 3px solid #17191c;
    box-shadow:
      inset 0 3px 8px rgba(0, 0, 0, 0.65),
      inset 0 -2px 4px rgba(255, 255, 255, 0.05),
      0 1px 3px rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .dust {
    /* the centre dust cap */
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: radial-gradient(circle at 50% 35%, #3d4247, #1a1c1f 78%);
    box-shadow: inset 0 1px 2px rgba(255, 255, 255, 0.12);
  }
</style>
