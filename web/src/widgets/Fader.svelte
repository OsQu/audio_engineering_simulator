<script lang="ts">
  // A vertical fader: a cap riding a track, its height mapped to the param range. Drag vertically
  // (Shift = fine), double-click to reset to the default, arrow keys to nudge.
  import type { ParamDescriptor } from "../catalog";
  import { keyStep, verticalDrag } from "./drag";
  import { formatParam } from "./format";

  interface Props {
    param: ParamDescriptor;
    value: number;
    onChange: (v: number) => void;
  }
  let { param, value, onChange }: Props = $props();

  // 0%..100% of the track height, bottom = min.
  const pct = $derived(((value - param.min) / (param.max - param.min || 1)) * 100);

  function onKey(e: KeyboardEvent): void {
    const next = keyStep(e, value, param.min, param.max);
    if (next !== null) {
      e.preventDefault();
      onChange(next);
    }
  }
</script>

<div class="fader">
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="track"
    role="slider"
    tabindex="0"
    aria-label={param.label}
    aria-valuemin={param.min}
    aria-valuemax={param.max}
    aria-valuenow={value}
    onpointerdown={(e) => verticalDrag(e, { value, min: param.min, max: param.max, onChange, travelPx: 120 })}
    ondblclick={() => onChange(param.default)}
    onkeydown={onKey}
  >
    <div class="cap" style={`bottom: calc(${pct}% - 7px)`}></div>
  </div>
  <span class="label">{param.label}</span>
  <span class="value">{formatParam(param, value)}</span>
</div>

<style>
  .fader {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 4.5rem;
    gap: 0.15rem;
  }
  .track {
    position: relative;
    width: 0.5rem;
    height: 7.5rem;
    background: #d8d8d8;
    border-radius: 3px;
    cursor: ns-resize;
    touch-action: none;
    outline: none;
  }
  .track:focus-visible {
    box-shadow: 0 0 0 2px #4a90d9;
  }
  .cap {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    width: 1.6rem;
    height: 14px;
    background: #2b2b2b;
    border: 1px solid #111;
    border-radius: 3px;
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
