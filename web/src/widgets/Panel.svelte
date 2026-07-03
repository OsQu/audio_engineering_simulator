<script lang="ts">
  // The **generic**, descriptor-driven faceplate — the fallback for any device that doesn't register its
  // own (Story 5.7). The **front** carries one bound `Control` per param (widget chosen by `kind`) plus a
  // `Reading` per readout; the **back** carries the I/O `Socket`s (inputs then outputs). The shared
  // `Chassis` owns the bezel + 3-D flip (driven by `flipped`) and publishes the `DeviceHandle` to context,
  // so the bound widgets bind to the live engine by id — this component just arranges them.
  import type { Snippet } from "svelte";
  import type { ParamDescriptor, PortDescriptor, ReadoutDescriptor } from "../catalog";
  import type { DeviceHandle } from "../device-handle";
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
  let {
    device,
    typeId,
    name,
    params,
    ports,
    readouts = [],
    flipped = false,
    valueFor,
    readingFor,
    onParam,
    children,
  }: Props = $props();

  const skin = $derived(skinFor(typeId));
  const inputs = $derived(ports.filter((p) => p.direction === "input"));
  const outputs = $derived(ports.filter((p) => p.direction === "output"));

  // The live-engine bridge the bound widgets (Control/Socket/Reading) read from context. Methods read
  // the reactive props at call time, so the handle object is built once yet always current; `device`
  // is a getter for the same reason. Ids are the descriptor's exposed ids.
  const handle: DeviceHandle = {
    get device() {
      return device;
    },
    value: (id) => valueFor(id),
    set: (id, v) => {
      const p = params.find((x) => x.id === id);
      if (p) onParam(p, v);
    },
    reading: (id) => readingFor?.(id) ?? -120,
    param: (id) => params.find((x) => x.id === id),
    port: (dir, id) => ports.find((x) => x.direction === dir && x.id === id),
    readout: (id) => readouts.find((x) => x.id === id),
  };
</script>

<Chassis {handle} {flipped} finish={skin.finish} {name}>
  {#snippet front()}
    {#if params.length > 0}
      <div class="controls">
        {#each params as p (p.id)}
          <Control id={p.id} cap={capFor(skin, p.id)} />
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
    {#if children}
      <div class="screen-slot">{@render children()}</div>
    {/if}
  {/snippet}

  {#snippet back()}
    <div class="jacks">
      {#if inputs.length > 0}
        <div class="group">
          <span class="group-label">In</span>
          <div class="row">
            {#each inputs as p (`in-${p.id}`)}
              <Socket dir="input" id={p.id} />
            {/each}
          </div>
        </div>
      {/if}
      {#if outputs.length > 0}
        <div class="group">
          <span class="group-label">Out</span>
          <div class="row">
            {#each outputs as p (`out-${p.id}`)}
              <Socket dir="output" id={p.id} />
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
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 0.4rem;
    align-items: flex-start;
  }
  .empty {
    font-size: clamp(5px, 20cqh, 0.75rem);
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
    font-style: italic;
    margin: 0;
  }
  .screen-slot {
    margin-top: 0.6rem;
  }
  .meters {
    /* Readouts sit side by side across the (wide) rack front — Peak next to RMS, the VU needle next
       to its Peak — instead of stacking, which keeps the front short enough for a thin rack unit. */
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
    gap: clamp(0.5rem, 3cqw, 1.5rem);
    margin-top: 0.2rem;
  }

  /* Rear panel: In and Out groups laid out **horizontally** in one row (how a real 1U rear panel looks),
     centered and filling the face height — so it fits a thin rack unit instead of stacking too tall. */
  .jacks {
    display: flex;
    flex-direction: row;
    flex-wrap: nowrap;
    align-items: center;
    justify-content: center;
    gap: clamp(4px, 5cqw, 1.5rem);
    height: 100%;
    box-sizing: border-box;
  }
  .group {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: clamp(2px, 1.5cqw, 0.5rem);
    min-width: 0;
  }
  .group-label {
    flex: none;
    font-family: var(--ae-font-ui);
    font-size: clamp(4px, 16cqh, 0.6rem);
    text-transform: uppercase;
    letter-spacing: var(--ae-label-spacing);
    color: var(--ae-text-muted);
  }
  .row {
    display: flex;
    flex-direction: row;
    flex-wrap: nowrap;
    gap: clamp(2px, 1.5cqw, 0.5rem);
  }
</style>
