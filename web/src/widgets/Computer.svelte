<script lang="ts">
  // The `computer` faceplate (Story 5.7.8) — the 8i6's USB peer, drawn as a plain box: it exists so the
  // multichannel USB cluster has a legal partner and the interface can be played end-to-end. The **front**
  // is the DAW's per-lane input meters (the 8 metered "send" lanes it records); the **back** carries the
  // one USB connector split into its input (8-lane send) and output (6-lane return) jacks. The DAW
  // routing itself (the loopback matrix's crosspoints) lives on the focus surface (ComputerMixer.svelte) —
  // a full mixing UI is future work, so this is deliberately minimal. Exposed face from the Rust
  // `computer` entry: params 0–47 are the 8×6 send→return crosspoints (loopback by default); readouts
  // 0–15 are the 8 send meters (Peak/RMS each); ports are USB In (input 0, 8-lane) and USB Out
  // (output 0, 6-lane).
  import { untrack } from "svelte";
  import type { DeviceUiProps } from "../device-ui";
  import { makeHandle } from "../device-handle";
  import { skinFor } from "../skin";
  import Chassis from "./Chassis.svelte";
  import Legend from "./Legend.svelte";
  import Reading from "./Reading.svelte";
  import Socket from "./Socket.svelte";

  let props: DeviceUiProps = $props();
  const skin = $derived(skinFor(props.typeId));
  const handle = makeHandle(untrack(() => props));
  // The send-meter readout ids in exposed order (Peak, RMS per lane) — all of them, laid out in a strip.
  const meters = $derived((props.readouts ?? []).map((r) => r.id));

  // Physical control sizes in **mm** (panel is 1 px/mm; the world/bench zoom scales it), inherited by the
  // legends + the (large, multi-lane) USB jacks.
  const faceVars = "--legend: 2.8px; --jack: 13px; --jack-font: 4px; --jack-gap: 1.2px; --jack-lane-font: 3px";
</script>

<Chassis {handle} flipped={props.flipped} finish={skin.finish} name={props.name}>
  {#snippet front()}
    <div class="front" style={faceVars}>
      <span class="wordmark">Computer</span>
      <Legend text="USB Sends" />
      <div class="meters">
        {#each meters as id (id)}
          <Reading {id} />
        {/each}
      </div>
    </div>
  {/snippet}

  {#snippet back()}
    <div class="back" style={faceVars}>
      <div class="section">
        <Legend text="USB" />
        <div class="row">
          <Socket dir="input" id={0} />
          <Socket dir="output" id={0} />
        </div>
      </div>
    </div>
  {/snippet}
</Chassis>

<style>
  /* Spacing in **mm** (panel is 1 px/mm; the world/bench zoom scales it). */
  .front {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    height: 100%;
    box-sizing: border-box;
  }
  .back {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: center;
    height: 100%;
    box-sizing: border-box;
  }
  .section {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.5px;
  }
  .row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 2.5px;
  }
  .meters {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
    gap: 3px;
  }
  .wordmark {
    font-family: var(--ae-font-display);
    font-weight: 700;
    font-size: 5px;
    letter-spacing: var(--ae-legend-spacing);
    color: var(--ae-faceplate-ink, var(--ae-text-primary));
  }
</style>
