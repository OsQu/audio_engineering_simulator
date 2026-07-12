<script lang="ts">
  // The MIDI controller's faceplate — a compact thru box (no sound, no params). It is *played* through
  // the focus keybed (its open host-fed MIDI input); on the panel the front is just a brand legend and
  // the back carries the DIN MIDI In/Out. Exposed ids from the Rust `midi_controller` entry: input 0 =
  // MIDI In, output 0 = MIDI Out.
  import { untrack } from "svelte";
  import type { DeviceUiProps } from "../device-ui";
  import { makeHandle } from "../device-handle";
  import { skinFor } from "../skin";
  import Chassis from "./Chassis.svelte";
  import Col from "./layout/Col.svelte";
  import Row from "./layout/Row.svelte";
  import Legend from "./Legend.svelte";
  import Socket from "./Socket.svelte";

  let props: DeviceUiProps = $props();
  const skin = $derived(skinFor(props.typeId));
  const handle = makeHandle(untrack(() => props));

  const faceVars = "--legend: 3px; --jack: 11px; --jack-font: 3.5px; --jack-gap: 1.2px";

  // A purely decorative mini keybed (non-interactive — real playing is the focus keybed). Two octaves of
  // white keys with the black keys overlaid at the usual C#/D#/F#/G#/A# boundaries. Sizes in mm (the
  // panel is 1 px/mm); box-sizing keeps each white key exactly `whiteW` wide so black keys land on the
  // boundaries. `blackAfterInOctave` is the white-key index (0=C..6=B) that each black key sits after.
  const OCTAVES = 2;
  const whiteW = 14;
  const blackW = 8;
  const whites: number[] = Array.from({ length: OCTAVES * 7 }, (_, i) => i);
  const blackAfterInOctave = [0, 1, 3, 4, 5];
  const blackLefts: number[] = [];
  for (let oct = 0; oct < OCTAVES; oct++) {
    for (const i of blackAfterInOctave) {
      blackLefts.push((oct * 7 + i + 1) * whiteW - blackW / 2);
    }
  }
</script>

<Chassis {handle} flipped={props.flipped} finish={skin.finish} accent={skin.accent}>
  {#snippet front()}
    <Col fill align="center" justify="center" gap={2} style={faceVars}>
      <Legend text="Keys · MIDI" />
      <div class="keybed" aria-hidden="true">
        {#each whites as _w (_w)}
          <span class="wkey"></span>
        {/each}
        {#each blackLefts as left (left)}
          <span class="bkey" style="left: {left}px"></span>
        {/each}
      </div>
    </Col>
  {/snippet}

  {#snippet back()}
    <Row fill align="center" justify="around" gap={4} style={faceVars}>
      <Col gap={1.5} align="center">
        <Legend text="In" />
        <Socket dir="input" id={0} />
      </Col>
      <Col gap={1.5} align="center">
        <Legend text="Out" />
        <Socket dir="output" id={0} />
      </Col>
    </Row>
  {/snippet}
</Chassis>

<style>
  /* Decorative mini keybed (mm; the panel is 1 px/mm). Non-interactive silkscreen-ish rendering. */
  .keybed {
    position: relative;
    display: flex;
    height: 28px;
    border-radius: 0 0 1.5px 1.5px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
  }
  .wkey {
    box-sizing: border-box;
    width: 14px;
    height: 100%;
    background: linear-gradient(to bottom, #f4f1ea, #d9d5cb);
    border: 0.4px solid #14161a;
    border-radius: 0 0 1.2px 1.2px;
  }
  .bkey {
    position: absolute;
    top: 0;
    width: 8px;
    height: 17px;
    background: linear-gradient(to bottom, #2a2c30, #0b0c0e);
    border-radius: 0 0 1px 1px;
    box-shadow:
      inset 0 -1px 1px rgba(255, 255, 255, 0.12),
      0 1px 1px rgba(0, 0, 0, 0.6);
  }
</style>
