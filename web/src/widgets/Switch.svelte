<script lang="ts">
  // A toggle switch (the device power switch): clicking flips the param between 0 and 1. The smoothed
  // `powered` param on the engine side de-clicks the transition; here it's just a 0/1 control.
  import type { ParamDescriptor } from "../catalog";

  interface Props {
    param: ParamDescriptor;
    value: number;
    onChange: (v: number) => void;
  }
  let { param, value, onChange }: Props = $props();

  const on = $derived(value >= 0.5);
</script>

<div class="switch">
  <button
    type="button"
    class:on
    role="switch"
    aria-checked={on}
    aria-label={param.label}
    onclick={() => onChange(on ? 0 : 1)}
  >
    <span class="led"></span>
    <span class="cap"></span>
  </button>
  <span class="label">{param.label}</span>
</div>

<style>
  .switch {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 4.5rem;
    gap: 0.25rem;
  }
  button {
    position: relative;
    width: 2.4rem;
    height: 3rem;
    padding: 0;
    border: 1px solid #111;
    border-radius: 4px;
    background: #2b2b2b;
    cursor: pointer;
  }
  .led {
    position: absolute;
    top: 0.3rem;
    left: 50%;
    transform: translateX(-50%);
    width: 0.45rem;
    height: 0.45rem;
    border-radius: 50%;
    background: #5a1a1a;
  }
  button.on .led {
    background: #36d36b;
    box-shadow: 0 0 5px #36d36b;
  }
  .cap {
    position: absolute;
    left: 4px;
    right: 4px;
    bottom: 4px;
    height: 1.4rem;
    border-radius: 3px;
    background: linear-gradient(#555, #3a3a3a);
    transition: transform 0.08s ease;
  }
  button.on .cap {
    transform: translateY(-0.5rem);
    background: linear-gradient(#6a6a6a, #4a4a4a);
  }
  .label {
    font-size: 0.7rem;
    color: #444;
  }
</style>
