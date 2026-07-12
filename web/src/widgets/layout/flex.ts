// ============================================================================
// Layout primitives — flex style builder
// ----------------------------------------------------------------------------
// The pure logic behind <Row>/<Col>. Faceplate authoring is *composition*: you
// place already-styled widgets (Knob/Socket/…) inside these primitives and never
// hand-name a flex container or round-trip to a <style> block for layout. The API
// shape follows Radix Themes (typed gap/align/justify props) + Braid (spacing is
// a value, not a class); it is domain-fitted to this repo — **lengths are in mm**
// (the faceplate lays out at 1 px/mm; the world/bench zoom scales it), and the
// primitives forward `style`/`class` so a face's inherited --ae-*/size vars ride
// through.
//
// Kept as a plain function (not baked into the .svelte files) so it is trivially
// unit-testable — see web/test/layout.test.ts — matching the repo's "unit-test
// each side" habit.
// ============================================================================

/** Cross-axis alignment (maps to `align-items`). */
export type Align = "start" | "center" | "end" | "stretch" | "baseline";
/** Main-axis distribution (maps to `justify-content`). */
export type Justify = "start" | "center" | "end" | "between" | "around" | "evenly";

export interface FlexProps {
  /** Gap between children, in mm. */
  gap?: number;
  /** Cross-axis alignment of this container's *children* (maps to `align-items`). Default `center`. */
  align?: Align;
  /** Cross-axis alignment of *this* item within its own parent (maps to `align-self`). `"stretch"`
   *  fills the item to its wrap-line's cross size — the room `mt="auto"` needs to push a child to the
   *  far end. Prefer this over `fill`'s `height:100%`: it is wrap-safe and needs no definite parent height. */
  alignSelf?: Align;
  /** Main-axis distribution. Default `start`. */
  justify?: Justify;
  /** Allow children to wrap onto multiple lines. */
  wrap?: boolean;
  /** Fill the parent (height:100% + border-box) — for a face/section container. */
  fill?: boolean;
  /** Establish a positioning context so descendant <Place> anchors to this box. */
  relative?: boolean;
  /** Padding on all edges, in mm (overridden by px/py, then per-edge). */
  p?: number;
  /** Horizontal padding (left+right), in mm. */
  px?: number;
  /** Vertical padding (top+bottom), in mm. */
  py?: number;
  pt?: number;
  pr?: number;
  pb?: number;
  pl?: number;
  /** Margin on all edges, in mm — or `"auto"` to absorb free space (overridden by mx/my, then per-edge). */
  m?: Length;
  /** Horizontal margin (left+right). */
  mx?: Length;
  /** Vertical margin (top+bottom). */
  my?: Length;
  mt?: Length;
  mr?: Length;
  mb?: Length;
  ml?: Length;
}

/**
 * A length in mm, or the string `"auto"`. An `auto` margin is the flexbox idiom for
 * pushing an item (and its following siblings) to the far end of the container —
 * e.g. `mt="auto"` in a Col drops the item to the bottom while earlier items stay
 * at the top. Needs surplus space to distribute, so the parent must stretch/size
 * the item's cross-axis (`align="stretch"`) or give it a height.
 */
export type Length = number | "auto";

const ALIGN: Record<Align, string> = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
  stretch: "stretch",
  baseline: "baseline",
};

const JUSTIFY: Record<Justify, string> = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
  between: "space-between",
  around: "space-around",
  evenly: "space-evenly",
};

/** 1 px ≡ 1 mm on the faceplate; `"auto"` passes through; `undefined` emits nothing. */
const mm = (v: Length | undefined): string | null =>
  v == null ? null : v === "auto" ? "auto" : `${v}px`;

/**
 * Build the inline `style` declaration string for a flex container in the given
 * `dir`. Padding and margin each resolve in precedence order: per-edge → axis
 * (x/y) → all. Margins accept `"auto"` (the push-to-far-end flexbox idiom).
 */
export function flexStyle(p: FlexProps, dir: "row" | "column"): string {
  const decls: (string | null)[] = [
    "display:flex",
    `flex-direction:${dir}`,
    `align-items:${ALIGN[p.align ?? "center"]}`,
    p.alignSelf != null ? `align-self:${ALIGN[p.alignSelf]}` : null,
    `justify-content:${JUSTIFY[p.justify ?? "start"]}`,
    p.gap != null ? `gap:${mm(p.gap)}` : null,
    p.wrap ? "flex-wrap:wrap" : null,
    p.fill ? "height:100%" : null,
    p.fill ? "box-sizing:border-box" : null,
    p.relative ? "position:relative" : null,
  ];

  const pad = (specific?: number, axis?: number): string | null => mm(specific ?? axis ?? p.p);
  const mrg = (specific?: Length, axis?: Length): string | null => mm(specific ?? axis ?? p.m);
  const box: Record<string, string | null> = {
    "padding-top": pad(p.pt, p.py),
    "padding-right": pad(p.pr, p.px),
    "padding-bottom": pad(p.pb, p.py),
    "padding-left": pad(p.pl, p.px),
    "margin-top": mrg(p.mt, p.my),
    "margin-right": mrg(p.mr, p.mx),
    "margin-bottom": mrg(p.mb, p.my),
    "margin-left": mrg(p.ml, p.mx),
  };
  for (const [prop, val] of Object.entries(box)) {
    if (val != null) decls.push(`${prop}:${val}`);
  }

  return decls.filter((d): d is string => d != null).join(";");
}
