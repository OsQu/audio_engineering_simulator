<script lang="ts">
  // A structural enumerated choice — unlike a smoothed param, changing it rebuilds the device through
  // the scene config seam. The choices come from the catalog descriptor, so a faceplate never repeats
  // GM program numbers or labels.
  import { getDeviceHandle } from "../device-handle";

  interface Props {
    key: string;
    label?: string;
  }
  let { key, label }: Props = $props();

  const handle = getDeviceHandle();
  const desc = $derived(handle.configDesc(key));
  const text = $derived(label ?? desc?.label ?? key);
  const value = $derived(handle.config(key));
</script>

{#if desc?.kind === "select"}
  <label class="config-select">
    <span>{text}</span>
    <select
      aria-label={text}
      {value}
      onchange={(event) => handle.setConfig(key, Number(event.currentTarget.value))}
    >
      {#each desc.options ?? [] as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </label>
{/if}

<style>
  .config-select {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    color: var(--ae-faceplate-ink, var(--ae-text-strong));
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    font-weight: var(--ae-label-weight);
    letter-spacing: var(--ae-label-spacing);
    text-transform: uppercase;
  }
  select {
    min-width: 10rem;
    border: 1px solid var(--ae-jack-edge);
    border-radius: 0.3rem;
    padding: 0.3rem 0.45rem;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    font: inherit;
    text-transform: none;
  }
</style>
