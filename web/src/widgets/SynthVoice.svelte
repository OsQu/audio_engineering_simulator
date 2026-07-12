<script lang="ts">
  // The synth voice's faceplate — a compact desktop synth module (sized to sit with the 8i6). Composed
  // from the shared bound widgets via the layout primitives (Row/Col), so authoring is placement, not
  // CSS: front = Level fader + the four ADSR knobs + the signature envelope Screen + the Power switch;
  // back = the MIDI input and the instrument output. Exposed ids from the Rust `synth_voice` entry:
  // 0 Level · 1 Attack · 2 Decay · 3 Sustain · 4 Release · 5 Power; input 0 = MIDI, output 0 = Out.
  // The ADSR Screen is pure presentation computed from the live param values (no engine tap).
  import { untrack } from "svelte";
  import type { DeviceUiProps } from "../device-ui";
  import { makeHandle } from "../device-handle";
  import { skinFor } from "../skin";
  import Chassis from "./Chassis.svelte";
  import Control from "./Control.svelte";
  import Col from "./layout/Col.svelte";
  import Row from "./layout/Row.svelte";
  import Legend from "./Legend.svelte";
  import Screen from "./Screen.svelte";
  import Socket from "./Socket.svelte";

  let props: DeviceUiProps = $props();
  const skin = $derived(skinFor(props.typeId));
  const handle = makeHandle(untrack(() => props));

  // Physical control sizes in **mm** (the panel is 1 px/mm; the world/bench zoom scales it).
  const faceVars = "--legend: 2.6px; --jack: 8px; --jack-font: 3px; --jack-gap: 1px";
</script>

<Chassis {handle} flipped={props.flipped} finish={skin.finish} accent={skin.accent}>
  {#snippet front()}
    <Row fill align="center" justify="around" gap={3} style={faceVars}>
      <Col gap={1} align="center">
        <Control id={0} as="fader" size={40} />
        <Legend text="Level" />
      </Col>
      <Row gap={2.5} align="end">
        <Col gap={1} align="center">
          <Control id={1} cap="cream" size={9} />
          <Legend text="A" />
        </Col>
        <Col gap={1} align="center">
          <Control id={2} cap="cream" size={9} />
          <Legend text="D" />
        </Col>
        <Col gap={1} align="center">
          <Control id={3} cap="cream" size={9} />
          <Legend text="S" />
        </Col>
        <Col gap={1} align="center">
          <Control id={4} cap="cream" size={9} />
          <Legend text="R" />
        </Col>
      </Row>
      <Screen
        attackMs={handle.value(1)}
        decayMs={handle.value(2)}
        sustain={handle.value(3)}
        releaseMs={handle.value(4)}
      />
      <Col gap={1} align="center">
        <Control id={5} size={6} />
        <Legend text="Power" />
      </Col>
    </Row>
  {/snippet}

  {#snippet back()}
    <Row fill align="center" justify="around" gap={4} style={faceVars}>
      <Col gap={1.5} align="center">
        <Legend text="MIDI" />
        <Socket dir="input" id={0} size={12} />
      </Col>
      <Col gap={1.5} align="center">
        <Legend text="Out" />
        <Socket dir="output" id={0} />
      </Col>
    </Row>
  {/snippet}
</Chassis>
