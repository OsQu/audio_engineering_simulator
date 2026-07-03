<script lang="ts">
  // A mixing-console focus surface: the SAME device descriptor as the in-rack panel, re-laid-out as a
  // channel strip (Story 4.8). The rack panel lays a device's params out in one horizontal row; a
  // console groups them into signal-flow **sections** (by the leading word of each label — "Input",
  // "Output", …) rendered as side-by-side channel columns, with level controls drawn as tall vertical
  // faders (the console idiom) rather than knobs. It reads values and applies edits through the same
  // `valueFor` / `onParam` props, so a move here and a move on the rack panel are the one control — no
  // engine or catalog change, purely a richer presentation of the same truth.
  import type { ParamDescriptor } from "../catalog";
  import Fader from "./Fader.svelte";
  import Switch from "./Switch.svelte";

  interface Props {
    params: ParamDescriptor[];
    valueFor: (id: number) => number;
    onParam: (p: ParamDescriptor, value: number) => void;
  }
  let { params, valueFor, onParam }: Props = $props();

  // Group params into signal-flow sections by the first word of the label ("Input Gain" → section
  // "Input", control "Gain"). Order is preserved, so sections read top-of-chain to bottom.
  type Section = { name: string; params: ParamDescriptor[] };
  const sections = $derived.by((): Section[] => {
    const out: Section[] = [];
    for (const p of params) {
      const [head, ...rest] = p.label.split(" ");
      const name = rest.length > 0 ? head : ""; // no space ⇒ ungrouped
      const last = out.at(-1);
      if (last && last.name === name) last.params.push(p);
      else out.push({ name, params: [p] });
    }
    return out;
  });

  // The control name shown under a widget, with the section word stripped ("Input Gain" → "Gain").
  const controlName = (label: string, section: string): string =>
    section && label.startsWith(`${section} `) ? label.slice(section.length + 1) : label;
</script>

<div class="console">
  {#each sections as section (section.name)}
    <div class="channel">
      {#if section.name}
        <span class="section-label">{section.name}</span>
      {/if}
      {#each section.params as p (p.id)}
        <div class="control">
          {#if p.kind === "switch"}
            <Switch param={p} value={valueFor(p.id)} onChange={(v) => onParam(p, v)} />
          {:else}
            <!-- Level params (gains) become tall faders — the console channel idiom — even where the
                 descriptor suggests a knob for the rack panel. Same param, richer presentation. -->
            <Fader param={p} value={valueFor(p.id)} onChange={(v) => onParam(p, v)} />
            <span class="control-name">{controlName(p.label, section.name)}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/each}
</div>

<style>
  .console {
    display: flex;
    gap: 1.4rem;
    padding: 1rem 1.4rem;
    background: var(--ae-bg-panel-2);
    border: 1px solid var(--ae-line-hard);
    border-radius: 8px;
    box-shadow: var(--ae-bevel-top), var(--ae-shadow-card);
  }
  /* Each section is a console channel: a vertical column of controls, bottom of chain last. */
  .channel {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.8rem;
    padding: 0 0.9rem;
    border-right: 1px solid var(--ae-line-panel);
  }
  .channel:last-child {
    border-right: none;
  }
  .section-label {
    font-family: var(--ae-font-display);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    color: var(--ae-text-primary);
  }
  .control {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.35rem;
  }
  .control-name {
    font-size: 0.68rem;
    color: var(--ae-text-muted);
  }
</style>
