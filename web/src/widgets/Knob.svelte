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
  }
  let { param, value, onChange, cap = "dark" }: Props = $props();

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

<div class="knob">
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
  .knob {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    width: 4.5rem;
  }
  .dial {
    position: relative;
    width: 3.25rem;
    height: 3.25rem;
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
    font-size: var(--ae-label-size);
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    color: var(--ae-text-strong);
    text-align: center;
    line-height: 1.15;
  }
  .value {
    font-family: var(--ae-font-ui);
    font-weight: 500;
    font-size: var(--ae-value-size);
    color: var(--ae-text-muted);
    font-variant-numeric: tabular-nums;
  }
</style>
