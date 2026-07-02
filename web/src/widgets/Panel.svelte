<script lang="ts">
  // A device's panel, laid out generically from its descriptor. The **front** carries one control
  // widget per param (chosen by `kind`); the **back** carries the I/O jacks (inputs then outputs). The
  // CSS 3-D flip between them is **controlled** by the `flipped` prop — the world layer drives it (a
  // direct front↔back toggle), so the panel no longer flips itself. Jacks are drag-to-connect patch
  // points (Story 4.4).
  import type { Snippet } from "svelte";
  import type { ParamDescriptor, PortDescriptor, ReadoutDescriptor } from "../catalog";
  import Fader from "./Fader.svelte";
  import Jack from "./Jack.svelte";
  import Knob from "./Knob.svelte";
  import Meter from "./Meter.svelte";
  import Switch from "./Switch.svelte";

  interface Props {
    /** Device instance id — tags the jacks so the cable layer can locate them (Story 4.4). */
    device: string;
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

  const inputs = $derived(ports.filter((p) => p.direction === "input"));
  const outputs = $derived(ports.filter((p) => p.direction === "output"));
</script>

<section class="panel">
  <!-- Device name floats in the top-left corner (out of layout flow) so it never steals height from a
       thin 1U chassis. A proper skin / label design comes later. -->
  <span class="name">{name}</span>

  <div class="flipper" class:flipped>
    <div class="face front" aria-hidden={flipped}>
      {#if params.length > 0}
        <div class="controls">
          {#each params as p (p.id)}
            {#if p.kind === "fader"}
              <Fader param={p} value={valueFor(p.id)} onChange={(v) => onParam(p, v)} />
            {:else if p.kind === "switch"}
              <Switch param={p} value={valueFor(p.id)} onChange={(v) => onParam(p, v)} />
            {:else}
              <Knob param={p} value={valueFor(p.id)} onChange={(v) => onParam(p, v)} />
            {/if}
          {/each}
        </div>
      {:else if readouts.length === 0}
        <p class="empty">no front-panel controls</p>
      {/if}
      {#if readouts.length > 0}
        <!-- Meter screen: one bar per readout, driven live by the node→host lane (Story 4.5). -->
        <div class="meters">
          {#each readouts as r (r.id)}
            <Meter label={r.label} unit={r.unit} value={readingFor?.(r.id) ?? -120} />
          {/each}
        </div>
      {/if}
      {#if children}
        <div class="screen-slot">{@render children()}</div>
      {/if}
    </div>

    <div class="face back" aria-hidden={!flipped}>
      <div class="jacks">
        {#if inputs.length > 0}
          <div class="group">
            <span class="group-label">In</span>
            <div class="row">
              {#each inputs as p (`in-${p.id}`)}
                <Jack {device} port={p} />
              {/each}
            </div>
          </div>
        {/if}
        {#if outputs.length > 0}
          <div class="group">
            <span class="group-label">Out</span>
            <div class="row">
              {#each outputs as p (`out-${p.id}`)}
                <Jack {device} port={p} />
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>
</section>

<style>
  .panel {
    /* In the spatial world the panel fills its chassis box (sized to the device's footprint); zoom in
       to operate it. box-sizing keeps the padding inside the footprint. Sizes scale with the chassis
       (`cqh`/`cqw` against the `.content` size container) but are capped at the original rem, so large
       devices look unchanged while a thin 1U rack unit shrinks its content to fit instead of clipping. */
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    border: 1px solid #bbb;
    border-radius: 8px;
    background: linear-gradient(#fafafa, #ececec);
    box-shadow:
      inset 0 1px 0 #fff,
      0 1px 3px rgba(0, 0, 0, 0.12);
    padding: clamp(2px, 6cqh, 0.6rem) clamp(3px, 2cqw, 0.9rem);
    display: flex;
    flex-direction: column;
    position: relative;
  }
  /* Floating name: pinned top-left, out of flow, non-interactive — so the flipper gets the full chassis
     height and a 1U unit's jacks stop clipping. It reads on both faces (it's outside the flipper). */
  .name {
    position: absolute;
    top: clamp(1px, 3cqh, 0.45rem);
    left: clamp(2px, 2cqw, 0.7rem);
    z-index: 2;
    font-size: clamp(5px, 22cqh, 0.8rem);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #555;
    pointer-events: none;
    white-space: nowrap;
    max-width: 92%;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Flip card: both faces share one grid cell, so the flipper sizes to the taller face — no manual
     height sync. preserve-3d + per-face backface-visibility hides whichever face is turned away. */
  .flipper {
    display: grid;
    grid-template: 1fr / 1fr; /* one cell filling the flipper, so both faces fill the chassis height */
    flex: 1;
    min-height: 0;
    transform-style: preserve-3d;
    transition: transform 0.45s ease;
  }
  .face {
    min-height: 0;
    overflow: hidden;
  }
  .flipper.flipped {
    transform: rotateY(180deg);
  }
  .face {
    grid-area: 1 / 1;
    backface-visibility: hidden;
  }
  .back {
    transform: rotateY(180deg);
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 0.4rem;
    align-items: flex-start;
  }
  .empty {
    font-size: clamp(5px, 20cqh, 0.75rem);
    color: #999;
    font-style: italic;
    margin: 0;
  }
  .screen-slot {
    margin-top: 0.6rem;
  }
  .meters {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    margin-top: 0.2rem;
  }

  .back {
    background: #e4e4e4;
    border-radius: 5px;
    padding: clamp(2px, 4cqh, 0.5rem);
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
    font-size: clamp(4px, 16cqh, 0.6rem);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #888;
  }
  .row {
    display: flex;
    flex-direction: row;
    flex-wrap: nowrap;
    gap: clamp(2px, 1.5cqw, 0.5rem);
  }
</style>
