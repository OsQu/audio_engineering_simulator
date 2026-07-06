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
    /** Physical fader travel (track height) in **mm** (real-gear sizing, scaled by the world/bench zoom).
     *  When omitted, keeps the legacy container-relative sizing. */
    size?: number;
  }
  let { param, value, onChange, size }: Props = $props();

  // Physical sizing off the mm track height; the width/cap/type scale with it. `null` ⇒ legacy sizing.
  const sizeVars = $derived(
    size === undefined
      ? undefined
      : `--fader-h: ${size}px; --fader-col: ${(size * 0.55).toFixed(2)}px; ` +
        `--fader-track: ${Math.max(2, size * 0.12).toFixed(2)}px; --fader-cap-w: ${(size * 0.3).toFixed(2)}px; ` +
        `--fader-cap-h: ${(size * 0.15).toFixed(2)}px; --fader-font: ${(size * 0.11).toFixed(2)}px; ` +
        `--fader-gap: ${(size * 0.04).toFixed(2)}px`,
  );

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

<div class="fader" style={sizeVars}>
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
    <div class="cap" style={`bottom: ${pct}%`}></div>
  </div>
  <span class="label">{param.label}</span>
  <span class="value">{formatParam(param, value)}</span>
</div>

<style>
  /* Sizes scale with the chassis height (cqh, against the WorldView `.content` size container), capped
     at their natural rem, so a normal panel is unchanged while a thin 1U unit shrinks the fader to fit. */
  .fader {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: var(--fader-col, min(4.5rem, 92cqh));
    gap: var(--fader-gap, min(0.15rem, 2cqh));
  }
  .track {
    position: relative;
    width: var(--fader-track, min(0.9rem, 12cqh));
    height: var(--fader-h, min(7.5rem, 68cqh));
    border-radius: 8px;
    /* Recessed metal slot with a dark centre index groove. */
    background: linear-gradient(
      to right,
      var(--ae-fader-slot-edge),
      var(--ae-fader-slot-mid) 50%,
      var(--ae-fader-slot-edge)
    );
    box-shadow: inset 0 0 4px #000;
    cursor: ns-resize;
    touch-action: none;
    outline: none;
  }
  .track::before {
    content: "";
    position: absolute;
    left: 50%;
    top: 8%;
    bottom: 8%;
    width: 2px;
    transform: translateX(-50%);
    background: #000;
    border-radius: 2px;
  }
  .track:focus-visible {
    box-shadow:
      inset 0 0 4px #000,
      0 0 0 2px var(--ae-signal-mic-lit);
  }
  .cap {
    position: absolute;
    left: 50%;
    /* `bottom` is set inline from value→%; the transform keeps the cap centred on that point at any size. */
    transform: translate(-50%, 50%);
    width: var(--fader-cap-w, min(2.2rem, 30cqh));
    height: var(--fader-cap-h, min(1.1rem, 15cqh));
    border-radius: 3px;
    background: linear-gradient(to bottom, var(--ae-fader-cap-top), var(--ae-fader-cap-bot));
    box-shadow:
      var(--ae-shadow-control),
      var(--ae-bevel-top);
    border: 1px solid #000;
  }
  .cap::after {
    /* the white throw index line across the cap */
    content: "";
    position: absolute;
    left: 12%;
    right: 12%;
    top: 50%;
    height: 2px;
    background: var(--ae-fader-index);
    transform: translateY(-50%);
    box-shadow: 0 0 4px rgba(255, 255, 255, 0.4);
  }
  .label {
    font-family: var(--ae-font-ui);
    font-weight: var(--ae-label-weight);
    font-size: var(--fader-font, min(var(--ae-label-size), 17cqh));
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
    color: var(--ae-faceplate-ink, var(--ae-text-strong));
    text-align: center;
  }
  .value {
    font-family: var(--ae-font-ui);
    font-size: var(--fader-font, min(var(--ae-value-size), 17cqh));
    font-variant-numeric: tabular-nums;
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
  }
</style>
