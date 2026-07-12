<script lang="ts">
  // A vertical flex container — the twin of Row (see it for the prop/forwarding notes).
  import type { Snippet } from "svelte";
  import { type FlexProps, flexStyle } from "./flex";

  interface Props extends FlexProps {
    children?: Snippet;
    class?: string;
    style?: string;
  }
  let { children, class: klass, style: styleIn, ...flex }: Props = $props();
  const style = $derived([flexStyle(flex, "column"), styleIn].filter(Boolean).join(";"));
</script>

<div class={klass} {style}>{@render children?.()}</div>
