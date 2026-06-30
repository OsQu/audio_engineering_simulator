<script lang="ts">
  // A device's panel, laid out generically from its descriptor. The **front** carries one control
  // widget per param (chosen by `kind`); the **back** carries the I/O jacks (inputs then outputs). A
  // per-panel button flips between them with a CSS 3-D transform — self-contained here, so Story 4.3
  // can later *gate* the flip behind a physical clearance action (pull-from-rack / roll-off-wall).
  // Jacks are display-only; drag-to-connect patching is Story 4.4.
  import type { ParamDescriptor, PortDescriptor } from "../catalog";
  import Fader from "./Fader.svelte";
  import Jack from "./Jack.svelte";
  import Knob from "./Knob.svelte";
  import Switch from "./Switch.svelte";

  interface Props {
    name: string;
    params: ParamDescriptor[];
    ports: PortDescriptor[];
    /** Current value for a device-local param id. */
    valueFor: (id: number) => number;
    /** Apply a new value to a param. */
    onParam: (p: ParamDescriptor, value: number) => void;
  }
  let { name, params, ports, valueFor, onParam }: Props = $props();

  let flipped = $state(false);

  const inputs = $derived(ports.filter((p) => p.direction === "input"));
  const outputs = $derived(ports.filter((p) => p.direction === "output"));
</script>

<section class="panel">
  <header>
    <span class="name">{name}</span>
    <button class="flip" type="button" onclick={() => (flipped = !flipped)}>
      {flipped ? "front ▸" : "back ▸"}
    </button>
  </header>

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
      {:else}
        <p class="empty">no front-panel controls</p>
      {/if}
    </div>

    <div class="face back" aria-hidden={!flipped}>
      <div class="jacks">
        {#if inputs.length > 0}
          <div class="group">
            <span class="group-label">In</span>
            <div class="row">
              {#each inputs as p (`in-${p.id}`)}
                <Jack port={p} />
              {/each}
            </div>
          </div>
        {/if}
        {#if outputs.length > 0}
          <div class="group">
            <span class="group-label">Out</span>
            <div class="row">
              {#each outputs as p (`out-${p.id}`)}
                <Jack port={p} />
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
    border: 1px solid #bbb;
    border-radius: 8px;
    background: linear-gradient(#fafafa, #ececec);
    box-shadow:
      inset 0 1px 0 #fff,
      0 1px 3px rgba(0, 0, 0, 0.12);
    padding: 0.6rem 0.9rem 0.8rem;
    min-width: 9rem;
  }
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.6rem;
  }
  .name {
    font-size: 0.8rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #555;
  }
  .flip {
    font: inherit;
    font-size: 0.65rem;
    padding: 0.1rem 0.4rem;
    border: 1px solid #bbb;
    border-radius: 4px;
    background: #f4f4f4;
    color: #555;
    cursor: pointer;
  }

  /* Flip card: both faces share one grid cell, so the flipper sizes to the taller face — no manual
     height sync. preserve-3d + per-face backface-visibility hides whichever face is turned away. */
  .flipper {
    display: grid;
    transform-style: preserve-3d;
    transition: transform 0.45s ease;
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
    font-size: 0.75rem;
    color: #999;
    font-style: italic;
    margin: 0;
  }

  .back {
    background: #e4e4e4;
    border-radius: 5px;
    padding: 0.5rem;
  }
  .jacks {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .group-label {
    width: 1.6rem;
    flex: none;
    font-size: 0.6rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #888;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
</style>
