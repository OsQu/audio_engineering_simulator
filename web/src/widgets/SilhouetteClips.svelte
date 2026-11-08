<script lang="ts">
  // One inside and one outside clip per chassis silhouette, defined once per SVG under a
  // prefix. `CablePass` nests these by id. The outside clip is a huge rect minus the rounded silhouette
  // (evenodd), so each clip removes exactly one silhouette and nesting removes their union.
  import {
    insideClipId,
    outsideClipId,
    roundedRectPath,
    type SurfaceSilhouette,
  } from "../cable-resolution";

  interface Props {
    prefix: string;
    silhouettes: ReadonlyMap<string, SurfaceSilhouette>;
  }
  let { prefix, silhouettes }: Props = $props();

  const FAR = 1e7;
  const everywhere = `M ${-FAR} ${-FAR} H ${FAR} V ${FAR} H ${-FAR} Z`;
</script>

<defs>
  {#each [...silhouettes.values()] as s (s.objectId)}
    <clipPath id={insideClipId(prefix, s.objectId)} clipPathUnits="userSpaceOnUse">
      <path d={roundedRectPath(s)} />
    </clipPath>
    <clipPath id={outsideClipId(prefix, s.objectId)} clipPathUnits="userSpaceOnUse">
      <path d={`${everywhere} ${roundedRectPath(s)}`} clip-rule="evenodd" />
    </clipPath>
  {/each}
</defs>
