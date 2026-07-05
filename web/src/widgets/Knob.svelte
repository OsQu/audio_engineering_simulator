<script lang="ts">
  // A rotary knob: a brushed-metal collar around a turnable cap, with an 11-mark tick ring and a
  // pointer that rotates over a 270° sweep mapped to the param range. Drag vertically to turn
  // (Shift = fine), double-click to reset to the default, arrow keys to nudge.
  //
  // Skin: the visuals are the design-system knob recipe (see styles/components.css), copied into the
  // scoped block below and reading the global --ae-* tokens. The interaction contract is unchanged.
  import type { ParamDescriptor } from "../catalog";
  import { keyStep, verticalDrag } from "./drag";
  import { formatParam } from "./format";

  interface Props {
    param: ParamDescriptor;
    value: number;
    onChange: (v: number) => void;
    /** Cap finish — a skin-only choice; the mechanics are identical. Defaults to the dark cap. */
    cap?: "dark" | "chrome" | "red" | "blue" | "cream";
    /** Physical dial diameter in **mm** (the faceplate is laid out at 1 px/mm and the world/bench zoom
     *  scales it). When given, the knob is a fixed physical size — the real-gear model. When omitted it
     *  keeps the legacy container-relative sizing (`min(rem, cqh)`), so un-migrated callers are unchanged. */
    size?: number;
  }
  let { param, value, onChange, cap = "dark", size }: Props = $props();

  // Physical sizing driven off the mm dial diameter: the column tracks the dial, gaps + label/value type
  // scale with it, so a small gain knob and a big monitor knob read at true proportions. `null` ⇒ the CSS
  // falls back to the legacy container-relative values.
  const sizeVars = $derived(
    size === undefined
      ? undefined
      : `--dial: ${size}px; --knob-gap: ${(size * 0.08).toFixed(2)}px; ` +
        `--knob-font: ${(size * 0.26).toFixed(2)}px`,
  );

  // -135°..+135° (a 270° sweep) across min→max.
  const angle = $derived(-135 + ((value - param.min) / (param.max - param.min || 1)) * 270);

  // 11 evenly-spaced tick marks across the same 270° sweep (27° apart).
  const ticks = Array.from({ length: 11 }, (_, i) => -135 + i * 27);

  function onKey(e: KeyboardEvent): void {
    const next = keyStep(e, value, param.min, param.max);
    if (next !== null) {
      e.preventDefault();
      onChange(next);
    }
  }
</script>

<div class="knob" style={sizeVars}>
  <div
    class="dial"
    role="slider"
    tabindex="0"
    aria-label={param.label}
    aria-valuemin={param.min}
    aria-valuemax={param.max}
    aria-valuenow={value}
    onpointerdown={(e) => verticalDrag(e, { value, min: param.min, max: param.max, onChange })}
    ondblclick={() => onChange(param.default)}
    onkeydown={onKey}
  >
    <div class="collar"></div>
    <div class="cap" data-cap={cap}></div>
    <svg class="face" viewBox="0 0 100 100" aria-hidden="true">
      {#each ticks as t (t)}
        <line class="tick" x1="50" y1="7" x2="50" y2="13" transform={`rotate(${t} 50 50)`} />
      {/each}
      <line class="pointer" x1="50" y1="50" x2="50" y2="25" transform={`rotate(${angle} 50 50)`} />
    </svg>
  </div>
  <span class="label">{param.label}</span>
  <span class="value">{formatParam(param, value)}</span>
</div>

<style>
  /* Sizes scale with the chassis height (cqh, against the WorldView `.content` size container) but are
     capped at their natural rem, so a normal/desktop panel is unchanged while a thin 1U rack unit
     shrinks the knob to fit instead of clipping. min() also means no container ⇒ the rem cap wins. */
  /* When a physical `--dial` (mm) is supplied the knob is a fixed size the world/bench zoom scales; the
     `var(--x, legacy)` fallbacks keep un-migrated callers on the old container-relative sizing. */
  .knob {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--knob-gap, min(5px, 4cqh));
    width: var(--dial, min(4.5rem, 92cqh));
  }
  .dial {
    position: relative;
    width: var(--dial, min(3.25rem, 56cqh));
    height: var(--dial, min(3.25rem, 56cqh));
    cursor: ns-resize;
    touch-action: none;
    outline: none;
  }
  .collar {
    position: absolute;
    inset: 13%;
    border-radius: 50%;
    background: radial-gradient(
      circle at 50% 30%,
      var(--ae-metal-collar-1),
      var(--ae-metal-collar-2) 48%,
      var(--ae-metal-collar-3) 78%,
      var(--ae-metal-collar-4) 100%
    );
    box-shadow:
      var(--ae-shadow-knob),
      inset 0 1px 1px rgba(255, 255, 255, 0.6),
      inset 0 -2px 3px rgba(0, 0, 0, 0.4);
  }
  .cap {
    position: absolute;
    inset: 28%;
    border-radius: 50%;
    /* default = dark cap; overridden via [data-cap] below */
    background: radial-gradient(circle at 50% 28%, var(--ae-cap-dark-top), var(--ae-cap-dark-bot) 74%);
    box-shadow:
      inset 0 2px 3px rgba(255, 255, 255, 0.18),
      inset 0 -3px 5px rgba(0, 0, 0, 0.55),
      0 1px 2px rgba(0, 0, 0, 0.4);
  }
  .cap[data-cap="chrome"] {
    background: radial-gradient(circle at 50% 28%, var(--ae-cap-chrome-top), var(--ae-cap-chrome-bot) 74%);
  }
  .cap[data-cap="red"] {
    background: radial-gradient(circle at 50% 28%, var(--ae-cap-red-top), var(--ae-cap-red-bot) 74%);
  }
  .cap[data-cap="blue"] {
    background: radial-gradient(circle at 50% 28%, var(--ae-cap-blue-top), var(--ae-cap-blue-bot) 74%);
  }
  .cap[data-cap="cream"] {
    background: radial-gradient(circle at 50% 28%, var(--ae-cap-cream-top), var(--ae-cap-cream-bot) 74%);
  }

  .face {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .tick {
    stroke: var(--ae-knob-tick);
    stroke-width: 1.6;
    stroke-linecap: round;
  }
  .pointer {
    stroke: var(--ae-cap-dark-pointer);
    stroke-width: 3.4;
    stroke-linecap: round;
  }
  /* pointer color tracks the cap finish it sits on */
  .cap[data-cap="chrome"] ~ .face .pointer,
  .cap[data-cap="cream"] ~ .face .pointer {
    stroke: var(--ae-cap-chrome-pointer);
  }
  .cap[data-cap="red"] ~ .face .pointer,
  .cap[data-cap="blue"] ~ .face .pointer {
    stroke: #ffffff;
  }

  .dial:focus-visible .collar {
    box-shadow:
      var(--ae-shadow-knob),
      0 0 0 2px var(--ae-signal-mic-lit);
  }

  .label {
    font-family: var(--ae-font-ui);
    font-weight: var(--ae-label-weight);
    font-size: var(--knob-font, min(var(--ae-label-size), 17cqh));
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    /* On a device faceplate the engraved ink is set by the panel finish; standalone
       (no faceplate) it falls back to the light UI text so it reads on the dark room. */
    color: var(--ae-faceplate-ink, var(--ae-text-strong));
    text-align: center;
    line-height: 1.15;
  }
  .value {
    font-family: var(--ae-font-ui);
    font-weight: 500;
    font-size: var(--knob-font, min(var(--ae-value-size), 17cqh));
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
    font-variant-numeric: tabular-nums;
  }
</style>
