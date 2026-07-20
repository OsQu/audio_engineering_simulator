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

/** Input / output port ids a source places, by direction. A plain `Socket` places one directed port;
 *  a `DuplexSocket` (USB-C — one connector, both directions) places an output (`outId`) *and* an input
 *  (`inId`) at once, so it counts toward whichever direction is asked. */
function portIds(src: string, dir: "input" | "output"): number[] {
  const re = new RegExp(`<Socket\\b[^>]*?\\bdir="${dir}"[^>]*?\\bid=\\{(\\d+)\\}`, "g");
  const ids = [...src.matchAll(re)].map((m) => Number(m[1]));
  const dupAttr = dir === "output" ? "outId" : "inId";
  const dupRe = new RegExp(`<DuplexSocket\\b[^>]*?\\b${dupAttr}=\\{(\\d+)\\}`, "g");
  ids.push(...[...src.matchAll(dupRe)].map((m) => Number(m[1])));
  return ids.sort((a, b) => a - b);
}

/** Structural config keys a source places via ConfigSwitch (focus surfaces) or ConfigButton
 *  (hardware faceplates), sorted. */
function configKeys(src: string): string[] {
  return [...src.matchAll(/<Config(?:Switch|Button)\b[^>]*?\bkey="([^"]+)"/g)]
    .map((m) => m[1])
    .sort();
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
  // The full-8i6 206-param face: 0 Gain1 · 1 Pad1 · 2 Air1 · 3 Gain2 · 4 Pad2 · 5 Air2 · 6 Phones1 ·
  // 7 Phones2 · 8–203 matrix crosspoints (14×14) · 204 Monitor · 205 Power.
  const CROSSPOINTS = range(8, 203); // rendered as a data grid (RoutingGrid) in the focus surface

  it("covers every param across faceplate ∪ focus (literal) ∪ the declared crosspoint grid", () => {
    const covered = uniqSorted([
      ...literalParamIds(faceplate), // gains 0/3, phones 6/7, monitor 204, power 205
      ...literalParamIds(focus), // pad/air 1,2,4,5
      ...CROSSPOINTS, // 8–203, declared (data-rendered grid)
    ]);
    expect(covered).toEqual(range(0, 205));
  });

  it("places every input exactly once on the faceplate (2 combo, 4 line, S/PDIF, USB, MIDI)", () => {
    expect(portIds(faceplate, "input")).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
  });

  it("places every output exactly once on the faceplate (USB, S/PDIF, 4 line, 2 phones, MIDI)", () => {
    expect(portIds(faceplate, "output")).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
  });

  it("covers every structural config key: INST in the focus surface, the global 48V on the faceplate", () => {
    // INST is software-controlled (Focusrite Control); 48V is a real front-panel button, so the
    // faceplate carries its interactive `ConfigButton` — one key, both preamps (the Rust entry's
    // shared `phantom`). The union is the 8i6's full config face.
    expect(configKeys(focus)).toEqual(["inst1", "inst2"]);
    expect(configKeys(faceplate)).toEqual(["phantom"]);
    expect([...configKeys(faceplate), ...configKeys(focus)].sort()).toEqual([
      "inst1",
      "inst2",
      "phantom",
    ]);
  });
});

describe("Computer surfaces place its full exposed face", () => {
  const faceplate = surfaceSource("Computer.svelte");
  const focus = surfaceSource("ComputerMixer.svelte");
  // The computer's param face is entirely the track → return crossbar crosspoints (T×M, config-sized),
  // **data-rendered** by RoutingGrid on the DAW mixer focus surface — so neither surface places a literal
  // param control (the faceplate is meters + jacks; the mixer is transport + track strips + the grid).
  // The exact crosspoint count is config-driven, so it isn't asserted here — the Rust side
  // (`catalog_aligns_with_exposed_face`) owns the face; this guards only against stray literal ids.

  it("places no literal param controls — every param is a crossbar crosspoint via RoutingGrid", () => {
    expect(literalParamIds(faceplate)).toEqual([]);
    expect(literalParamIds(focus)).toEqual([]);
  });

  it("places the USB input (the send bus) on the faceplate", () => {
    expect(portIds(faceplate, "input")).toEqual([0]);
  });

  it("places the USB output (the return bus) on the faceplate", () => {
    expect(portIds(faceplate, "output")).toEqual([0]);
  });
});
