<script lang="ts">
  // The shared device chassis: the bezel, the floating corner name, and the front/back 3-D flip — the
  // skeuomorphic frame every faceplate sits in. A device (the generic `Panel`, or a bespoke per-device
  // component) fills the two faces via the `front`/`back` snippets and never re-implements the flip.
  // Chassis also publishes the `DeviceHandle` to context, so the bound widgets composed inside those
  // snippets bind to the live engine by id (see device-handle.ts). Extracted from `Panel` in Story 5.7
  // so custom faceplates and the generic fallback share one flip implementation.
  import { type Snippet, untrack } from "svelte";
  import { setDeviceHandle, type DeviceHandle } from "../device-handle";
  import type { Finish } from "../skin";

  interface Props {
    /** The live-engine bridge, published to descendants for the bound widgets. */
    handle: DeviceHandle;
    /** Whether the back face is turned toward the operator (driven by the world layer). */
    flipped?: boolean;
    /** Faceplate finish — sets the front-face gradient + engraved-ink tokens. A bespoke device can
     *  still paint its own front background in its snippet; the ink tokens keep its labels legible. */
    finish?: Finish;
    /** Optional brand accent, exposed as `--ae-accent` for a faceplate to use (border/chassis colour). */
    accent?: string;
    /** Floating corner name; omitted ⇒ no name badge. */
    name?: string;
    /** The front face's contents. */
    front: Snippet;
    /** The back face's contents. */
    back: Snippet;
  }
  let { handle, flipped = false, finish = "slate", accent, name, front, back }: Props = $props();

  // Publish the handle once. It's a stable object whose methods read reactive props at call time, so we
  // deliberately capture the initial reference (untrack documents the intent and silences the warning).
  setDeviceHandle(untrack(() => handle));
</script>

<section class="panel" data-finish={finish} style:--ae-accent={accent}>
  <!-- Device name floats in the top-left corner (out of layout flow) so it never steals height from a
       thin 1U chassis. It reads on both faces (it's outside the flipper). -->
  {#if name}<span class="name">{name}</span>{/if}

  <!-- Flip card: both faces share one grid cell, so the flipper sizes to the taller face — no manual
       height sync. preserve-3d + per-face backface-visibility hides whichever face is turned away. -->
  <div class="flipper" class:flipped>
    <div class="face front" data-face="front" aria-hidden={flipped}>{@render front()}</div>
    <div class="face back" data-face="back" aria-hidden={!flipped}>{@render back()}</div>
  </div>
</section>

<style>
  .panel {
    /* In the spatial world the panel fills its chassis box (sized to the device's footprint); zoom in
       to operate it. box-sizing keeps the padding inside the footprint. */
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    /* Chassis outline: a device's brand accent when it sets one (Focusrite red), else the neutral edge. */
    border: 1px solid var(--ae-accent, var(--ae-line-hard));
    border-radius: 8px;
    /* Dark chassis; the faceplate finish lives on the front face (below), so the
       panel's own background reads as the thin bezel around the faceplate. */
    background: var(--ae-bg-panel-2);
    box-shadow:
      var(--ae-bevel-top),
      var(--ae-shadow-card);
    /* Bezel inset in **mm** (the panel is 1 px/mm; the world/bench zoom scales it). */
    padding: 2px 3px;
    display: flex;
    flex-direction: column;
    position: relative;
  }
  /* Floating name: pinned top-left, out of flow, non-interactive — so the flipper gets the full chassis
     height and a 1U unit's jacks stop clipping. */
  .name {
    position: absolute;
    top: 2px;
    left: 2px;
    z-index: 2;
    font-family: var(--ae-font-display);
    font-size: 4px;
    font-weight: 700;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    /* Engraved ink; the color tracks the faceplate finish (dark ink on the light
       grey face, light ink on slate/black). The name floats over the front face. */
    color: var(--ae-text-primary);
    pointer-events: none;
    white-space: nowrap;
    max-width: 92%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .panel[data-finish="grey"] .name {
    color: #26261f;
  }

  .flipper {
    display: grid;
    grid-template: 1fr / 1fr; /* one cell filling the flipper, so both faces fill the chassis height */
    flex: 1;
    min-height: 0;
    transform-style: preserve-3d;
    transition: transform 0.45s ease;
  }
  .flipper.flipped {
    transform: rotateY(180deg);
  }
  .face {
    grid-area: 1 / 1;
    min-height: 0;
    overflow: hidden;
    backface-visibility: hidden;
  }
  .back {
    transform: rotateY(180deg);
  }

  /* Front faceplate: one 165° finish gradient + a subtle lit-top / shaded-bottom bevel. Each finish
     also sets --ae-faceplate-ink / -muted; control labels read those, so their engraved text
     automatically contrasts with the face. Custom properties pierce Svelte's style scoping, so
     descendants (a device's face content) inherit them across components. */
  .front {
    padding: 2px;
    border-radius: var(--ae-radius-panel);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.25),
      inset 0 -3px 7px rgba(0, 0, 0, 0.18);
  }
  .panel[data-finish="grey"] .front {
    background: linear-gradient(
      165deg,
      var(--ae-finish-grey-1),
      var(--ae-finish-grey-2) 55%,
      var(--ae-finish-grey-3)
    );
    --ae-faceplate-ink: #26261f;
    --ae-faceplate-ink-muted: #55554a;
  }
  .panel[data-finish="slate"] .front {
    background: linear-gradient(
      165deg,
      var(--ae-finish-slate-1),
      var(--ae-finish-slate-2) 55%,
      var(--ae-finish-slate-3)
    );
    --ae-faceplate-ink: var(--ae-text-primary);
    --ae-faceplate-ink-muted: var(--ae-text-secondary);
  }
  .panel[data-finish="black"] .front {
    background: linear-gradient(
      165deg,
      var(--ae-finish-black-1),
      var(--ae-finish-black-2) 55%,
      var(--ae-finish-black-3)
    );
    --ae-faceplate-ink: var(--ae-text-primary);
    --ae-faceplate-ink-muted: var(--ae-text-secondary);
  }

  .back {
    /* Rear panel: dark sheet metal, distinct from the finished front face. */
    background: linear-gradient(var(--ae-bg-panel), var(--ae-bg-panel-2));
    border-radius: 5px;
    padding: 2px;
  }
</style>
