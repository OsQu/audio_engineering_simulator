<script lang="ts">
  // Absolute placement at physical coordinates — the coordinate idiom alongside Row/Col's
  // flow. Pin a child to any edge of the nearest positioned ancestor: x/y = left/top,
  // r/b = right/bottom, all in mm. The ancestor must establish a positioning context —
  // put `relative` on the enclosing Row/Col. Use for silkscreen pins, wordmarks, badges.
  import type { Snippet } from "svelte";

  interface Props {
    /** Distance from the left edge, in mm. */
    x?: number;
    /** Distance from the top edge, in mm. */
    y?: number;
    /** Distance from the right edge, in mm (pins to the right instead of the left). */
    r?: number;
    /** Distance from the bottom edge, in mm (pins to the bottom instead of the top). */
    b?: number;
    children?: Snippet;
    class?: string;
    style?: string;
  }
  let { x, y, r, b, children, class: klass, style: styleIn }: Props = $props();
  const mm = (v: number | undefined): string | null => (v == null ? null : `${v}px`);
  const style = $derived(
    [
      "position:absolute",
      x != null ? `left:${mm(x)}` : null,
      y != null ? `top:${mm(y)}` : null,
      r != null ? `right:${mm(r)}` : null,
      b != null ? `bottom:${mm(b)}` : null,
      styleIn,
    ]
      .filter(Boolean)
      .join(";"),
  );
</script>

<div class={klass} {style}>{@render children?.()}</div>
