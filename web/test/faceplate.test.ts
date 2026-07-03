import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// Guardrail for bespoke faceplates: a device that authors its own component must place **every** exposed
// param and port (else a control/jack is unreachable) and reference **only** valid ids (a stray id would
// render nothing). The vitest runner is node-only, so it can't mount the Svelte component; instead this
// statically scans the faceplate source for the ids its bound widgets (Control/Socket/Reading) reference
// and checks them against the device's exposed face. The expected faces mirror the Rust catalog entries
// (guarded on the Rust side by `catalog_aligns_with_exposed_face`). A literal-id convention is assumed
// (ids written as `id={N}`), which the bespoke faceplates follow.

/** The exposed ids a faceplate places, parsed from its source. */
function placedIds(src: string): { params: number[]; inputs: number[]; outputs: number[] } {
  const nums = (re: RegExp) => [...src.matchAll(re)].map((m) => Number(m[1])).sort((a, b) => a - b);
  return {
    // Params are placed via Control (knob/fader/switch) or metered via Reading.
    params: nums(/<(?:Control|Reading)\b[^>]*?\bid=\{(\d+)\}/g),
    inputs: nums(/<Socket\b[^>]*?\bdir="input"[^>]*?\bid=\{(\d+)\}/g),
    outputs: nums(/<Socket\b[^>]*?\bdir="output"[^>]*?\bid=\{(\d+)\}/g),
  };
}

function faceplateSource(file: string): string {
  return readFileSync(new URL(`../src/widgets/${file}`, import.meta.url), "utf8");
}

describe("Scarlett 8i6 faceplate places its full exposed face", () => {
  const placed = placedIds(faceplateSource("Scarlett8i6.svelte"));

  it("places every param exactly once (2 preamp gains + monitor + phones, and 4 power switches)", () => {
    expect(placed.params).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
  });
  it("places every input exactly once (2 combo inputs, USB return, MIDI in)", () => {
    expect(placed.inputs).toEqual([0, 1, 2, 3]);
  });
  it("places every output exactly once (2 USB sends, line out, phones, MIDI out)", () => {
    expect(placed.outputs).toEqual([0, 1, 2, 3, 4]);
  });
});
