import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// Guardrail for bespoke faceplates: a device that authors its own component(s) must place **every**
// exposed param and port (else a control/jack is unreachable) and reference **only** valid ids (a stray
// id would render nothing). The vitest runner is node-only, so it can't mount the Svelte component;
// instead this statically scans the surface source(s) for the ids its bound widgets
// (Control/Socket/Reading/ConfigSwitch) reference and checks them against the device's exposed face.
//
// **Declared coverage across surfaces (Story 5.7.6, extended in 5.7.9).** A param may be placed on the
// in-world faceplate *or* on the device's focus surface, so a device declares its surfaces here and the
// test **unions** the ids scanned from all of them. Some surfaces render params from **data** (the
// 5.7.9 routing-matrix grid derives its cells from the crosspoint params), so their ids aren't literal
// in the source; such a surface additionally **declares** the id range it covers (`declaredParams`),
// unioned with the literal scans. The union must still cover the full param face.
//
// The expected faces mirror the Rust catalog entries (guarded on the Rust side by
// `catalog_aligns_with_exposed_face`); positional-id churn means this manifest is updated whenever a
// device's face changes (SCHEMA_VERSION bumps in lockstep).

/** Param ids a source places via Control/Reading (bound param/readout widgets) with a **literal** id. */
function literalParamIds(src: string): number[] {
  return [...src.matchAll(/<(?:Control|Reading)\b[^>]*?\bid=\{(\d+)\}/g)]
    .map((m) => Number(m[1]))
    .sort((a, b) => a - b);
}

/** Input / output port ids a source places via Socket, by direction. */
function portIds(src: string, dir: "input" | "output"): number[] {
  const re = new RegExp(`<Socket\\b[^>]*?\\bdir="${dir}"[^>]*?\\bid=\\{(\\d+)\\}`, "g");
  return [...src.matchAll(re)].map((m) => Number(m[1])).sort((a, b) => a - b);
}

/** Structural config keys a source places via ConfigSwitch, sorted. */
function configKeys(src: string): string[] {
  return [...src.matchAll(/<ConfigSwitch\b[^>]*?\bkey="([^"]+)"/g)].map((m) => m[1]).sort();
}

function surfaceSource(file: string): string {
  return readFileSync(new URL(`../src/widgets/${file}`, import.meta.url), "utf8");
}

const range = (lo: number, hi: number): number[] =>
  Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);
const uniqSorted = (ns: number[]): number[] => [...new Set(ns)].sort((a, b) => a - b);

describe("Scarlett 8i6 surfaces place its full exposed face", () => {
  // The registered surfaces for the 8i6, and the ids each covers.
  const faceplate = surfaceSource("Scarlett8i6.svelte");
  const focus = surfaceSource("FocusriteControl.svelte");
  // The 18-param face: 0 Gain1 · 1 Pad1 · 2 Air1 · 3 Gain2 · 4 Pad2 · 5 Air2 · 6–14 matrix crosspoints
  // · 15 Monitor · 16 Phones · 17 Power.
  const CROSSPOINTS = range(6, 14); // rendered as a data grid in the focus surface (non-literal)

  it("covers every param across faceplate ∪ focus (literal) ∪ the declared crosspoint grid", () => {
    const covered = uniqSorted([
      ...literalParamIds(faceplate), // gains 0/3, monitor 15, phones 16, power 17
      ...literalParamIds(focus), // pad/air 1,2,4,5
      ...CROSSPOINTS, // 6–14, declared (data-rendered grid)
    ]);
    expect(covered).toEqual(range(0, 17));
  });

  it("places every input exactly once on the faceplate (2 combo inputs, USB return, MIDI in)", () => {
    expect(portIds(faceplate, "input")).toEqual([0, 1, 2, 3]);
  });

  it("places every output exactly once on the faceplate (2 USB sends, line out, phones, MIDI out)", () => {
    expect(portIds(faceplate, "output")).toEqual([0, 1, 2, 3, 4]);
  });

  it("covers both INST structural config keys in the focus surface", () => {
    expect(configKeys(focus)).toEqual(["inst1", "inst2"]);
  });
});
