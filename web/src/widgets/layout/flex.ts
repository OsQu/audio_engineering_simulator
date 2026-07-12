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
  /** Cross-axis alignment. Default `center`. */
  align?: Align;
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
}

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

/** 1 px ≡ 1 mm on the faceplate; `undefined` emits nothing. */
const mm = (v: number | undefined): string | null => (v == null ? null : `${v}px`);

/**
 * Build the inline `style` declaration string for a flex container in the given
 * `dir`. Padding resolves in precedence order: per-edge → axis (px/py) → all (p).
 */
export function flexStyle(p: FlexProps, dir: "row" | "column"): string {
  const decls: (string | null)[] = [
    "display:flex",
    `flex-direction:${dir}`,
    `align-items:${ALIGN[p.align ?? "center"]}`,
    `justify-content:${JUSTIFY[p.justify ?? "start"]}`,
    p.gap != null ? `gap:${mm(p.gap)}` : null,
    p.wrap ? "flex-wrap:wrap" : null,
    p.fill ? "height:100%" : null,
    p.fill ? "box-sizing:border-box" : null,
    p.relative ? "position:relative" : null,
  ];

  const edge = (specific?: number, axis?: number): string | null => mm(specific ?? axis ?? p.p);
  const pad: Record<string, string | null> = {
    "padding-top": edge(p.pt, p.py),
    "padding-right": edge(p.pr, p.px),
    "padding-bottom": edge(p.pb, p.py),
    "padding-left": edge(p.pl, p.px),
  };
  for (const [prop, val] of Object.entries(pad)) {
    if (val != null) decls.push(`${prop}:${val}`);
  }

  return decls.filter((d): d is string => d != null).join(";");
}
