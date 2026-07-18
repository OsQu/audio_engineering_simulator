<script lang="ts">
  // The `computer` faceplate (Story 5.7.8; config-driven since 5.10) — the interface's USB peer, drawn as
  // a plain box: it exists so the multichannel USB cluster has a legal partner and the interface can be
  // played end-to-end. The **front** is the DAW's per-lane input meters (one per metered "send" lane it
  // records); the **back** carries the one USB connector split into its input (send) and output (return)
  // jacks. The DAW routing itself (the loopback matrix's crosspoints) lives on the focus surface
  // (ComputerMixer.svelte) — a full mixing UI is future work, so this is deliberately minimal.
  //
  // Everything is **data-driven from the props** (the *per-instance* descriptor): the computer has no
  // fixed channel count — it adopts the attached interface's published USB shape (default 2×2, the 8i6
  // makes it 8 sends × 6 returns). So the meter strip renders whatever `readouts` it's handed, and the
  // USB jacks carry the descriptor's lane counts.
  import { untrack } from "svelte";
  import type { DeviceUiProps } from "../device-ui";
  import { makeHandle } from "../device-handle";
  import { skinFor } from "../skin";
  import Chassis from "./Chassis.svelte";
  import DuplexSocket from "./DuplexSocket.svelte";
  import Legend from "./Legend.svelte";
  import Reading from "./Reading.svelte";

  let props: DeviceUiProps = $props();
  const skin = $derived(skinFor(props.typeId));
  const handle = makeHandle(untrack(() => props));
  // The send-meter readout ids in exposed order (Peak, RMS per lane) — all of them, laid out in a strip.
  const meters = $derived((props.readouts ?? []).map((r) => r.id));

  // Physical control sizes in **mm** (panel is 1 px/mm; the world/bench zoom scales it), inherited by the
  // legends + the multi-lane USB jacks. Sized down for the compact (240 × 28 mm) chassis.
  const faceVars =
    "--legend: 2.4px; --jack: 9px; --jack-font: 3px; --jack-gap: 1px; --jack-lane-font: 2.4px";
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
          <DuplexSocket outId={0} inId={0} />
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
    font-size: 3.5px;
    letter-spacing: var(--ae-legend-spacing);
    color: var(--ae-faceplate-ink, var(--ae-text-primary));
  }
</style>
