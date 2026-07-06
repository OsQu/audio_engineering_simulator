<script lang="ts">
  // The **generic**, descriptor-driven faceplate — the fallback for any device that doesn't register its
  // own (Story 5.7). The **front** carries one bound `Control` per param (widget chosen by `kind`) plus a
  // `Reading` per readout; the **back** carries the I/O `Socket`s (inputs then outputs). The shared
  // `Chassis` owns the bezel + 3-D flip (driven by `flipped`) and publishes the `DeviceHandle` to context,
  // so the bound widgets bind to the live engine by id — this component just arranges them.
  import { type Snippet, untrack } from "svelte";
  import type { ParamDescriptor, PortDescriptor, ReadoutDescriptor } from "../catalog";
  import { makeHandle } from "../device-handle";
  import { capFor, skinFor } from "../skin";
  import Chassis from "./Chassis.svelte";
  import Control from "./Control.svelte";
  import Reading from "./Reading.svelte";
  import Socket from "./Socket.svelte";

  interface Props {
    /** Device instance id — tags the jacks so the cable layer can locate them. */
    device: string;
    /** Catalog device-type id — selects the visual skin (faceplate finish + cap finish). */
    typeId: string;
    name: string;
    params: ParamDescriptor[];
    ports: PortDescriptor[];
    /** Scalar readouts the device meters (node→host lane); empty for a non-metering device. */
    readouts?: ReadoutDescriptor[];
    /** Whether the back panel faces the operator (controlled by the world layer, gated by clearance). */
    flipped?: boolean;
    /** Current value for a device-local param id. */
    valueFor: (id: number) => number;
    /** Current live reading for a device-local readout id (from the node→host lane). */
    readingFor?: (id: number) => number;
    /** Apply a new value to a param. */
    onParam: (p: ParamDescriptor, value: number) => void;
    /** Optional per-device front-panel embellishment (e.g. the synth's ADSR screen). */
    children?: Snippet;
  }
  // Kept as a whole (not destructured) so the reactive object flows into makeHandle, whose methods read
  // it at call time — the handle stays live without re-creating it.
  let props: Props = $props();

  const skin = $derived(skinFor(props.typeId));
  const inputs = $derived(props.ports.filter((p) => p.direction === "input"));
  const outputs = $derived(props.ports.filter((p) => p.direction === "output"));
  const readouts = $derived(props.readouts ?? []);
  // makeHandle reads the (stable) props object lazily, so capture it once; untrack documents that.
  const handle = makeHandle(untrack(() => props));
</script>

<Chassis {handle} flipped={props.flipped} finish={skin.finish} name={props.name}>
  {#snippet front()}
    {#if props.params.length > 0}
      <div class="controls">
        {#each props.params as p (p.id)}
          <Control id={p.id} cap={capFor(skin, p.id)} size={12} />
        {/each}
      </div>
    {:else if readouts.length === 0}
      <p class="empty">no front-panel controls</p>
    {/if}
    {#if readouts.length > 0}
      <!-- Meter screen: one bar per readout, driven live by the node→host lane. -->
      <div class="meters">
        {#each readouts as r (r.id)}
          <Reading id={r.id} />
        {/each}
      </div>
    {/if}
    {#if props.children}
      <div class="screen-slot">{@render props.children()}</div>
    {/if}
  {/snippet}

  {#snippet back()}
    <div class="jacks">
      {#if inputs.length > 0}
        <div class="group">
          <span class="group-label">In</span>
          <div class="row">
            {#each inputs as p (`in-${p.id}`)}
              <Socket dir="input" id={p.id} size={8} />
            {/each}
          </div>
        </div>
      {/if}
      {#if outputs.length > 0}
        <div class="group">
          <span class="group-label">Out</span>
          <div class="row">
            {#each outputs as p (`out-${p.id}`)}
              <Socket dir="output" id={p.id} size={8} />
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/snippet}
</Chassis>

<style>
  /* This component styles only the *face content* (controls / meters / jacks); the bezel, flip, and
     finish live in Chassis. Svelte scopes these rules to elements declared here, so they still apply
     when Chassis renders the snippets. */
  /* All spacing in **mm** (the panel is 1 px/mm; the world/bench zoom scales it), matching the physical
     control sizes the faceplate sets via the size props. */
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 3px 2.5px;
    align-items: flex-start;
  }
  .empty {
    font-size: 3px;
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
    font-style: italic;
    margin: 0;
  }
  .screen-slot {
    margin-top: 2px;
  }
  .meters {
    /* Readouts sit side by side across the (wide) rack front — Peak next to RMS, the VU needle next
       to its Peak — instead of stacking, which keeps the front short enough for a thin rack unit. */
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-top: 1px;
  }

  /* Rear panel: In and Out groups laid out **horizontally** in one row (how a real 1U rear panel looks),
     centered and filling the face height — so it fits a thin rack unit instead of stacking too tall. */
  .jacks {
    display: flex;
    flex-direction: row;
    flex-wrap: nowrap;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 100%;
    box-sizing: border-box;
  }
  .group {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 2.5px;
    min-width: 0;
  }
  .group-label {
    flex: none;
    font-family: var(--ae-font-ui);
    font-size: 3px;
    text-transform: uppercase;
    letter-spacing: var(--ae-label-spacing);
    color: var(--ae-text-muted);
  }
  .row {
    display: flex;
    flex-direction: row;
    flex-wrap: nowrap;
    gap: 2.5px;
  }
</style>
