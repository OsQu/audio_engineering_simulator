<script lang="ts">
  // The shared patch-cable visual — the single source for how a lead looks, so the scene view and the
  // workbench bench never diverge (both render this; the design-system recipe in styles/components.css is
  // reference only, not imported). A settled lead is three stacked strokes (dark shadow, signal-coloured
  // core, thin lit highlight), coloured by the connection's connector kind; the drag rubber-band is a
  // single dashed lead tinted legal/illegal (and lighter while a click-held "pending" cable, scene view).
  // Widths come from the cable tokens (surface mm — the stage's zoom scales them). Interaction (the click
  // hit-path, portals, selection state) stays with each view; this is purely the stroke recipe.
  import type { PortKind } from "../catalog";

  interface Props {
    /** The SVG path data (a cubic from jack to jack, or to the cursor while dragging). */
    d: string;
    /** Connector kind → core/highlight colour. Omitted for the neutral drag lead. */
    kind?: PortKind;
    /** Fatten the core to read as the inspector's selected lead. */
    selected?: boolean;
    /** Render the dashed rubber-band instead of a settled lead. */
    drag?: boolean;
    /** Drag hovering a compatible / incompatible jack → green / red. */
    legal?: boolean;
    illegal?: boolean;
    /** A click-held (cross-view) pending drag lead — lighter, so it reads as "held" not "dragging". */
    pending?: boolean;
  }
  let {
    d,
    kind,
    selected = false,
    drag = false,
    legal = false,
    illegal = false,
    pending = false,
  }: Props = $props();
</script>

{#if drag}
  <path class="cable-drag" class:legal class:illegal class:pending {d} />
{:else}
  <path class="cable-shadow" {d} />
  <path class="cable-core" data-signal={kind} class:selected {d} />
  <path class="cable-highlight" data-signal={kind} {d} />
{/if}

<style>
  .cable-shadow,
  .cable-core,
  .cable-highlight,
  .cable-drag {
    fill: none;
    stroke-linecap: round;
    pointer-events: none;
  }
  .cable-shadow {
    stroke: var(--ae-cable-shadow);
    stroke-width: var(--ae-cable-shadow-w);
    opacity: 0.5;
  }
  .cable-core {
    stroke: var(--ae-signal-line);
    stroke-width: var(--ae-cable-core-w);
  }
  .cable-highlight {
    stroke: var(--ae-signal-line-lit);
    stroke-width: var(--ae-cable-highlight-w);
    opacity: 0.6;
  }
  .cable-core[data-signal="mic"] {
    stroke: var(--ae-signal-mic);
  }
  .cable-core[data-signal="instrument"] {
    stroke: var(--ae-signal-instrument);
  }
  .cable-core[data-signal="speaker"] {
    stroke: var(--ae-signal-speaker);
  }
  .cable-core[data-signal="digital"] {
    stroke: var(--ae-signal-digital);
  }
  .cable-core[data-signal="midi"] {
    stroke: var(--ae-signal-midi);
  }
  .cable-highlight[data-signal="mic"] {
    stroke: var(--ae-signal-mic-lit);
  }
  .cable-highlight[data-signal="instrument"] {
    stroke: var(--ae-signal-instrument-lit);
  }
  .cable-highlight[data-signal="speaker"] {
    stroke: var(--ae-signal-speaker-lit);
  }
  .cable-highlight[data-signal="digital"] {
    stroke: var(--ae-signal-digital-lit);
  }
  .cable-highlight[data-signal="midi"] {
    stroke: var(--ae-signal-midi-lit);
  }
  /* Selected lead: fatten the core so the inspector target reads clearly. */
  .cable-core.selected {
    stroke-width: calc(var(--ae-cable-core-w) + 3px);
  }
  /* The rubber-band while dragging a new cable — same gauge as a settled core, dashed. */
  .cable-drag {
    stroke: var(--ae-cable-drag, #d98c3c);
    stroke-width: var(--ae-cable-core-w);
    stroke-dasharray: 12 9;
    opacity: 0.85;
  }
  .cable-drag.legal {
    stroke: #4caf50;
  }
  .cable-drag.illegal {
    stroke: #d9534f;
  }
  .cable-drag.pending {
    opacity: 0.6;
  }
</style>
