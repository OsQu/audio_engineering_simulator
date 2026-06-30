<script lang="ts">
  // A device's front panel, laid out generically from its descriptor: one widget per control param,
  // chosen by the param's `kind` (fader / switch / knob). The panel is reused by every device — the
  // descriptor is the single source of layout truth. (Back-panel jacks land in Story 4.2.4.)
  import type { ParamDescriptor } from "../catalog";
  import Fader from "./Fader.svelte";
  import Knob from "./Knob.svelte";
  import Switch from "./Switch.svelte";

  interface Props {
    name: string;
    params: ParamDescriptor[];
    /** Current value for a device-local param id. */
    valueFor: (id: number) => number;
    /** Apply a new value to a param. */
    onParam: (p: ParamDescriptor, value: number) => void;
  }
  let { name, params, valueFor, onParam }: Props = $props();
</script>

<section class="panel">
  <header>{name}</header>
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
    <p class="no-controls">no front-panel controls</p>
  {/if}
</section>

<style>
  .panel {
    border: 1px solid #bbb;
    border-radius: 8px;
    background: linear-gradient(#fafafa, #ececec);
    box-shadow: inset 0 1px 0 #fff, 0 1px 3px rgba(0, 0, 0, 0.12);
    padding: 0.6rem 0.9rem 0.8rem;
    min-width: 9rem;
  }
  header {
    font-size: 0.8rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #555;
    margin-bottom: 0.6rem;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 0.4rem;
    align-items: flex-start;
  }
  .no-controls {
    font-size: 0.75rem;
    color: #999;
    font-style: italic;
    margin: 0;
  }
</style>
