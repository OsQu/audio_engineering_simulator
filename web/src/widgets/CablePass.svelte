<script lang="ts">
  // Paint one cable pass inside its resolver-supplied clip chain. Each clip is a nested <g>,
  // so the chain intersects; the children (stroke + hit path) are clipped for paint *and* hit testing.
  import type { Snippet } from "svelte";
  import { type CablePass, passClipIds } from "../cable-resolution";

  interface Props {
    pass: CablePass;
    /** The SVG's `SilhouetteClips` prefix. */
    prefix: string;
    children: Snippet;
  }
  let { pass, prefix, children }: Props = $props();
  const ids = $derived(passClipIds(pass, prefix));
</script>

{#snippet clipped(i: number)}
  {#if i < ids.length}
    <g clip-path={`url(#${ids[i]})`}>{@render clipped(i + 1)}</g>
  {:else}
    {@render children()}
  {/if}
{/snippet}

<g class="cable-pass" data-pass={pass.kind}>{@render clipped(0)}</g>
