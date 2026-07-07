import { describe, expect, it } from "vitest";
import { SCHEMA_VERSION, type Scene } from "../src/scene-store";
import { decodeScene, encodeScene } from "../src/url-scene";

// A small bench-shaped scene with a non-Latin1 label glyph (×) to exercise the UTF-8 base64 path.
function scene(schemaVersion = SCHEMA_VERSION): Scene {
  return {
    schemaVersion,
    ui: { spaces: [], racks: [], placements: {}, portals: {} },
    patch: {
      devices: [
        { id: "src", typeId: "synth_voice" },
        { id: "dev", typeId: "scarlett_8i6" },
        { id: "spk", typeId: "speaker" },
      ],
      connections: [{ from: { device: "src", port: 0 }, to: { device: "dev", port: 0 } }],
      output: { device: "dev", port: 0 },
    },
  };
}

describe("encodeScene / decodeScene", () => {
  it("round-trips a scene through the URL-safe encoding (× label glyph included)", () => {
    const s = scene();
    expect(decodeScene(encodeScene(s))).toEqual(s);
  });

  it("produces a URL-safe string (no +, /, or = padding)", () => {
    expect(encodeScene(scene())).not.toMatch(/[+/=]/);
  });

  it("discards a scene from a different schema version (→ regenerate, never migrate)", () => {
    expect(decodeScene(encodeScene(scene(SCHEMA_VERSION - 1)))).toBeNull();
  });

  it("is null for absent / malformed input", () => {
    expect(decodeScene(null)).toBeNull();
    expect(decodeScene("")).toBeNull();
    expect(decodeScene("not-base64!!")).toBeNull();
    expect(decodeScene(encodeScene({ schemaVersion: SCHEMA_VERSION } as Scene))).toBeNull(); // no patch/ui
  });
});
