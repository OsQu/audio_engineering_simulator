<script lang="ts">
  // A horizontal flex container. Compose widgets inside it; tune layout with typed
  // mm props (gap/align/justify/wrap/…) instead of a hand-named class. `style` and
  // `class` forward to the root so a face's inherited --ae-* vars ride through and
  // global/utility classes still work. (Parent-*scoped* classes won't reach here —
  // that's Svelte's per-component scoping; keep decoration in the parent's markup.)
  import type { Snippet } from "svelte";
  import { type FlexProps, flexStyle } from "./flex";

  interface Props extends FlexProps {
    children?: Snippet;
    class?: string;
    style?: string;
  }
  let { children, class: klass, style: styleIn, ...flex }: Props = $props();
  const style = $derived([flexStyle(flex, "row"), styleIn].filter(Boolean).join(";"));
</script>

<div class={klass} {style}>{@render children?.()}</div>
