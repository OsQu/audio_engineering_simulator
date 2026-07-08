<script lang="ts">
  // The shared **device hover header**: a slim toolbar that floats just above a device's top edge,
  // hidden until the device is hovered, holding that device's chrome controls (the scene view's
  // open/flip/space/remove; the bench's rotate). One place, used by both view roots — the scene
  // (WorldView) and the flat bench (BenchStage) — so a device's controls read the same everywhere and
  // never clutter the faceplate.
  //
  // This component owns the bar's *appearance* + its hidden/transition default; the **host** device
  // container owns the *reveal trigger*, because the thing you hover differs per view (WorldView's
  // `.device` vs the bench's `.device-group`). A host reveals the bar with, e.g.:
  //   .device:hover :global(.device-chrome), .device:focus-within :global(.device-chrome) {
  //     opacity: 1; transform: none; pointer-events: auto;
  //   }
  // The host must be `position: relative` (the bar is absolutely positioned against it) and must not clip
  // its top overflow (the bar sits above the chassis).
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();
</script>

<div class="device-chrome">{@render children()}</div>

<style>
  .device-chrome {
    position: absolute;
    bottom: 100%; /* sit flush above the device's top edge (no gap ⇒ no hover flicker crossing onto it) */
    left: 0;
    right: 0;
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 4px;
    background: var(--ae-bg-panel);
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-control) var(--ae-radius-control) 0 0;
    box-shadow: 0 -4px 12px rgba(0, 0, 0, 0.4);
    /* Hidden until the host reveals it (see the header comment); the host's rule wins on specificity. */
    opacity: 0;
    transform: translateY(6px);
    pointer-events: none;
    transition:
      opacity 0.12s ease,
      transform 0.12s ease;
  }
</style>
