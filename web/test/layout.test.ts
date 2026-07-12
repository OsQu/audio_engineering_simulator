import { describe, expect, it } from "vitest";
import { flexStyle } from "../src/widgets/layout/flex";

// Parse the `;`-joined declaration string into a lookup, so assertions read a
// property regardless of emission order.
function decls(style: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const d of style.split(";")) {
    const i = d.indexOf(":");
    if (i > 0) out[d.slice(0, i)] = d.slice(i + 1);
  }
  return out;
}

describe("flexStyle", () => {
  it("sets the flex direction and sensible defaults", () => {
    const row = decls(flexStyle({}, "row"));
    expect(row.display).toBe("flex");
    expect(row["flex-direction"]).toBe("row");
    expect(row["align-items"]).toBe("center"); // default align
    expect(row["justify-content"]).toBe("flex-start"); // default justify
    expect(decls(flexStyle({}, "column"))["flex-direction"]).toBe("column");
  });

  it("emits gap in mm (px == mm on the faceplate)", () => {
    expect(decls(flexStyle({ gap: 2.5 }, "row")).gap).toBe("2.5px");
    expect(flexStyle({}, "row")).not.toContain("gap:"); // omitted when unset
  });

  it("maps align + justify tokens to their CSS values", () => {
    const s = decls(flexStyle({ align: "end", justify: "between" }, "row"));
    expect(s["align-items"]).toBe("flex-end");
    expect(s["justify-content"]).toBe("space-between");
    expect(decls(flexStyle({ justify: "around" }, "row"))["justify-content"]).toBe("space-around");
  });

  it("emits align-self only when set, mapping the token (the wrap-safe fill for mt:auto)", () => {
    expect(flexStyle({}, "column")).not.toContain("align-self");
    expect(decls(flexStyle({ alignSelf: "stretch" }, "column"))["align-self"]).toBe("stretch");
    expect(decls(flexStyle({ alignSelf: "end" }, "column"))["align-self"]).toBe("flex-end");
  });

  it("adds wrap / fill / relative only when flagged", () => {
    expect(flexStyle({}, "row")).not.toContain("flex-wrap");
    expect(flexStyle({}, "row")).not.toContain("position");
    const s = decls(flexStyle({ wrap: true, fill: true, relative: true }, "row"));
    expect(s["flex-wrap"]).toBe("wrap");
    expect(s.height).toBe("100%");
    expect(s["box-sizing"]).toBe("border-box");
    expect(s.position).toBe("relative");
  });

  it("resolves padding precedence: per-edge > axis (px/py) > all (p)", () => {
    // `p` fills every edge…
    const all = decls(flexStyle({ p: 3 }, "row"));
    expect([
      all["padding-top"],
      all["padding-right"],
      all["padding-bottom"],
      all["padding-left"],
    ]).toEqual(["3px", "3px", "3px", "3px"]);
    // …px/py override their axis…
    const axis = decls(flexStyle({ p: 3, px: 5 }, "row"));
    expect(axis["padding-left"]).toBe("5px");
    expect(axis["padding-right"]).toBe("5px");
    expect(axis["padding-top"]).toBe("3px");
    // …and a per-edge value wins outright.
    const edge = decls(flexStyle({ p: 3, px: 5, pl: 1 }, "row"));
    expect(edge["padding-left"]).toBe("1px");
    expect(edge["padding-right"]).toBe("5px");
  });

  it("emits no padding declarations when none are given", () => {
    expect(flexStyle({ gap: 1 }, "row")).not.toContain("padding");
  });

  it("emits margins in mm and passes 'auto' through (the push-to-far-end idiom)", () => {
    expect(decls(flexStyle({ mt: 2 }, "column"))["margin-top"]).toBe("2px");
    expect(decls(flexStyle({ mt: "auto" }, "column"))["margin-top"]).toBe("auto");
    // Same precedence as padding: per-edge > axis (mx/my) > all (m).
    const s = decls(flexStyle({ m: 1, my: 2, mb: "auto" }, "column"));
    expect(s["margin-top"]).toBe("2px"); // from my
    expect(s["margin-bottom"]).toBe("auto"); // per-edge wins
    expect(s["margin-left"]).toBe("1px"); // from m
    expect(flexStyle({ gap: 1 }, "row")).not.toContain("margin");
  });
});
