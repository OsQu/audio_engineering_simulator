<script lang="ts">
  // A rotary knob: an SVG dial whose pointer-notch rotates over a 270° sweep mapped to the param range.
  // Drag vertically to turn (Shift = fine), double-click to reset to the default, arrow keys to nudge.
  import type { ParamDescriptor } from "../catalog";
  import { keyStep, verticalDrag } from "./drag";
  import { formatParam } from "./format";

  interface Props {
    param: ParamDescriptor;
    value: number;
    onChange: (v: number) => void;
  }
  let { param, value, onChange }: Props = $props();

  // -135°..+135° (a 270° sweep) across min→max.
  const angle = $derived(-135 + ((value - param.min) / (param.max - param.min || 1)) * 270);

  function onKey(e: KeyboardEvent): void {
    const next = keyStep(e, value, param.min, param.max);
    if (next !== null) {
      e.preventDefault();
      onChange(next);
    }
  }
</script>

<div class="knob">
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <svg
    viewBox="0 0 48 48"
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
    <circle class="ring" cx="24" cy="24" r="20" />
    <circle class="cap" cx="24" cy="24" r="15" />
    <g transform={`rotate(${angle} 24 24)`}>
      <line class="notch" x1="24" y1="24" x2="24" y2="11" />
    </g>
  </svg>
  <span class="label">{param.label}</span>
  <span class="value">{formatParam(param, value)}</span>
</div>

<style>
  .knob {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 4.5rem;
    gap: 0.15rem;
  }
  svg {
    width: 3rem;
    height: 3rem;
    cursor: ns-resize;
    touch-action: none;
    outline: none;
  }
  svg:focus-visible .ring {
    stroke: #4a90d9;
  }
  .ring {
    fill: none;
    stroke: #ccc;
    stroke-width: 2;
  }
  .cap {
    fill: #2b2b2b;
    stroke: #111;
    stroke-width: 1;
  }
  .notch {
    stroke: #e8e8e8;
    stroke-width: 2.5;
    stroke-linecap: round;
  }
  .label {
    font-size: 0.7rem;
    color: #444;
    text-align: center;
  }
  .value {
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
    color: #777;
  }
</style>
