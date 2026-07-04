import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// Guardrail for bespoke faceplates: a device that authors its own component(s) must place **every**
// exposed param and port (else a control/jack is unreachable) and reference **only** valid ids (a stray
// id would render nothing). The vitest runner is node-only, so it can't mount the Svelte component;
// instead this statically scans the surface source(s) for the ids its bound widgets
// (Control/Socket/Reading/ConfigSwitch) reference and checks them against the device's exposed face.
//
// **Declared coverage across surfaces (Story 5.7.6):** a param may be placed on the in-world faceplate
// *or* on the device's focus surface (e.g. the 8i6's PAD/AIR live in Focusrite Control, not on the
// panel). So a device declares its set of surfaces here and the test **unions** the ids scanned from
// all of them, still demanding the union cover the full param face. Ports stay faceplate-only. Structural
// config keys (INST) are covered via `ConfigSwitch key="…"` and checked against the declared keys.
//
// The expected faces mirror the Rust catalog entries (guarded on the Rust side by
// `catalog_aligns_with_exposed_face`). A literal-id convention is assumed (`id={N}`, `key="…"`), which
// the bespoke surfaces follow; a future data-driven surface (the 5.7.9 matrix) would declare its ids
// explicitly instead.

/** Param ids a source places via Control/Reading (bound param/readout widgets), sorted. */
function paramIds(src: string): number[] {
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

const uniqSorted = (ns: number[]): number[] => [...new Set(ns)].sort((a, b) => a - b);

describe("Scarlett 8i6 surfaces place its full exposed face", () => {
  // The registered surfaces for the 8i6: the in-world faceplate and the Focusrite Control focus surface.
  const faceplate = surfaceSource("Scarlett8i6.svelte");
  const focus = surfaceSource("FocusriteControl.svelte");

  it("covers every param across faceplate ∪ focus surface (9: gains, pad/air, monitor, phones, power)", () => {
    // Faceplate: gains 0/3, monitor 6, phones 7, power 8. Focus: pad/air 1,2,4,5.
    const covered = uniqSorted([...paramIds(faceplate), ...paramIds(focus)]);
    expect(covered).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
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
