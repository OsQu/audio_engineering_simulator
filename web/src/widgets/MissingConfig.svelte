<script lang="ts">
  // Stand-in for a bound config widget whose key didn't resolve (see `resolveConfigWidget`):
  // a visible ⚠ chip naming the key where the widget should be, plus a console warning with the full
  // reason — so a typo'd key or a stale WASM artifact can't hide as an invisible zero-width element.
  interface Props {
    /** The config key the widget asked for. */
    key: string;
    /** Why it didn't resolve (names the device and key). */
    problem: string;
  }
  let { key, problem }: Props = $props();

  $effect(() => {
    console.warn(`[config widget] ${problem}`);
  });
</script>

<span class="missing-config" role="alert" title={problem}>⚠ {key}</span>

<style>
  .missing-config {
    display: inline-block;
    padding: 0 0.3em;
    border: 1px dashed var(--ae-led-red, #d33);
    border-radius: 0.2em;
    color: var(--ae-led-red, #d33);
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    white-space: nowrap;
  }
</style>
