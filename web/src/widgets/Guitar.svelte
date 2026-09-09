<script lang="ts">
  // The two guitar faces share their panel: one physical MIDI input, one output whose carrier is declared
  // by the catalog (instrument volts for electric, air for acoustic), a structural GM-preset selector,
  // and the DA/monitor controls that actually exist on each chassis.
  import { untrack } from "svelte";
  import type { DeviceUiProps } from "../device-ui";
  import { makeHandle } from "../device-handle";
  import { skinFor } from "../skin";
  import Chassis from "./Chassis.svelte";
  import ConfigSelect from "./ConfigSelect.svelte";
  import Control from "./Control.svelte";
  import Col from "./layout/Col.svelte";
  import Legend from "./Legend.svelte";
  import Row from "./layout/Row.svelte";
  import Socket from "./Socket.svelte";

  let props: DeviceUiProps = $props();
  const skin = $derived(skinFor(props.typeId));
  const handle = makeHandle(untrack(() => props));
  const acoustic = $derived(props.typeId === "acoustic_guitar");
  const faceVars = "--legend: 2.8px; --jack: 10px; --jack-font: 3.3px; --jack-gap: 1.2px";
</script>

<Chassis {handle} flipped={props.flipped} finish={skin.finish} accent={skin.accent} name={props.name}>
  {#snippet front()}
    <Row fill align="center" justify="around" gap={4} style={faceVars}>
      <Col gap={1} align="center">
        <Legend text={acoustic ? "Acoustic" : "Electric"} />
        <ConfigSelect key="preset" />
      </Col>
      <Col gap={1} align="center">
        <Control id={0} size={18} />
        <Legend text="Power" />
      </Col>
      {#if acoustic}
        <Col gap={1} align="center">
          <Control id={1} size={22} />
          <Legend text="Volume" />
        </Col>
      {/if}
    </Row>
  {/snippet}

  {#snippet back()}
    <Row fill align="center" justify="around" gap={4} style={faceVars}>
      <Col gap={1.5} align="center">
        <Legend text="MIDI" />
        <Socket dir="input" id={0} size={12} />
      </Col>
      <Col gap={1.5} align="center">
        <Legend text={acoustic ? "Air" : "Out"} />
        <Socket dir="output" id={0} size={12} />
      </Col>
    </Row>
  {/snippet}
</Chassis>
