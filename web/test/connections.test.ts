import { describe, expect, it } from "vitest";
import type { CableType } from "../src/catalog";
import type { Endpoint } from "../src/connections";
import {
  cableAllowed,
  cableControlPoints,
  cablePathData,
  cableSpec,
  cableTypeIdFor,
  distanceToCable,
  evaluateConnection,
  isPointNearCable,
  wouldCreateCycle,
} from "../src/connections";
import type { Connection } from "../src/scene";

// Endpoint builders keep the legality cases terse. Connector defaults to ¼" so same-connector cases
// (the common ones) stay terse; pass it explicitly to exercise a connector mismatch.
const out = (
  device: string,
  port = 0,
  domain: Endpoint["domain"] = "analog",
  connector: Endpoint["connector"] = "quarterInch",
): Endpoint => ({
  device,
  port,
  direction: "output",
  domain,
  connector,
});
const inp = (
  device: string,
  port = 0,
  domain: Endpoint["domain"] = "analog",
  connector: Endpoint["connector"] = "quarterInch",
): Endpoint => ({
  device,
  port,
  direction: "input",
  domain,
  connector,
});
const conn = (fromDev: string, fromPort: number, toDev: string, toPort: number): Connection => ({
  from: { device: fromDev, port: fromPort },
  to: { device: toDev, port: toPort },
});

describe("evaluateConnection — legality", () => {
  it("accepts an output → input of the same domain, oriented from=output to=input", () => {
    const v = evaluateConnection(out("synth"), inp("ad"), []);
    expect(v.ok).toBe(true);
    if (v.ok) {
      expect(v.connection).toEqual({
        from: { device: "synth", port: 0 },
        to: { device: "ad", port: 0 },
      });
      expect(v.replaces).toBeNull();
    }
  });

  it("orients regardless of drag direction (input dragged onto output)", () => {
    const v = evaluateConnection(inp("ad"), out("synth"), []);
    expect(v.ok).toBe(true);
    if (v.ok) {
      expect(v.connection).toEqual({
        from: { device: "synth", port: 0 },
        to: { device: "ad", port: 0 },
      });
    }
  });

  it("rejects output → output", () => {
    const v = evaluateConnection(out("a"), out("b"), []);
    expect(v.ok).toBe(false);
  });

  it("rejects input → input", () => {
    const v = evaluateConnection(inp("a"), inp("b"), []);
    expect(v.ok).toBe(false);
  });

  it("rejects a cross-domain edge (analog → digital)", () => {
    const v = evaluateConnection(out("synth", 0, "analog"), inp("eq", 0, "digital"), []);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.reason).toMatch(/domain/);
  });

  it("accepts a matching digital edge", () => {
    const v = evaluateConnection(
      out("ad", 0, "digital", "digital"),
      inp("da", 0, "digital", "digital"),
      [],
    );
    expect(v.ok).toBe(true);
  });

  it('rejects same-domain ports with incompatible connectors (XLR into ¼")', () => {
    const v = evaluateConnection(
      out("mic", 0, "analog", "xlr"),
      inp("pre", 0, "analog", "quarterInch"),
      [],
    );
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.reason).toMatch(/connector/);
  });

  it('accepts same-connector analog ports despite differing signal-class (instrument → line, both ¼")', () => {
    const v = evaluateConnection(
      out("synth", 0, "analog", "quarterInch"),
      inp("gain", 0, "analog", "quarterInch"),
      [],
    );
    expect(v.ok).toBe(true);
  });

  it("rejects a device patched to itself (self-cycle)", () => {
    const v = evaluateConnection(out("strip", 0), inp("strip", 0), []);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.reason).toMatch(/itself/);
  });

  it("rejects an exact duplicate connection", () => {
    const existing = [conn("synth", 0, "ad", 0)];
    const v = evaluateConnection(out("synth"), inp("ad"), existing);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.reason).toMatch(/already/);
  });

  it("replaces the existing cable when the target input is already driven (fan-in is illegal)", () => {
    // 'ad' input 0 is already fed by 'gain'; wiring 'synth' into it must replace that edge.
    const existing = [conn("gain", 0, "ad", 0)];
    const v = evaluateConnection(out("synth"), inp("ad"), existing);
    expect(v.ok).toBe(true);
    if (v.ok) {
      expect(v.replaces).toEqual(conn("gain", 0, "ad", 0));
      expect(v.connection.from.device).toBe("synth");
    }
  });

  it("allows fan-out: a second cable from the same output to a different input is fine", () => {
    const existing = [conn("synth", 0, "ad", 0)];
    const v = evaluateConnection(out("synth"), inp("spk"), existing);
    expect(v.ok).toBe(true);
    if (v.ok) expect(v.replaces).toBeNull();
  });

  it("distinguishes ports by id within a device (input 0 vs output 0 are different ports)", () => {
    // An input already driven at port 0 doesn't block a different input port 1.
    const existing = [conn("gain", 0, "strip", 0)];
    const v = evaluateConnection(out("synth"), inp("strip", 1), existing);
    expect(v.ok).toBe(true);
    if (v.ok) expect(v.replaces).toBeNull();
  });
});

describe("wouldCreateCycle / feedback-loop rejection", () => {
  // A → B → C already wired.
  const chain = [conn("a", 0, "b", 0), conn("b", 0, "c", 0)];

  it("detects a loop closing the chain (C → A)", () => {
    expect(wouldCreateCycle("c", "a", chain)).toBe(true);
  });

  it("allows an edge that doesn't close a loop (A → C is a forward skip)", () => {
    expect(wouldCreateCycle("a", "c", chain)).toBe(false);
  });

  it("treats a self edge as a cycle", () => {
    expect(wouldCreateCycle("a", "a", [])).toBe(true);
  });

  it("evaluateConnection rejects a drag that would feed back (C.out → A.in)", () => {
    const v = evaluateConnection(out("c"), inp("a"), chain);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.reason).toMatch(/loop/);
  });
});

describe("cableAllowed", () => {
  it("permits a cable on analog edges only", () => {
    expect(cableAllowed("analog")).toBe(true);
    expect(cableAllowed("digital")).toBe(false);
    expect(cableAllowed("events")).toBe(false);
  });
});

describe("cable spec ↔ type-id round-trip", () => {
  const cables: CableType[] = [
    {
      typeId: "patch_short",
      label: "Patch",
      kind: "line",
      connector: "quarterInch",
      lengthM: 0.5,
      resistanceOhms: 0.05,
      capacitanceFarads: 5e-11,
    },
    {
      typeId: "instrument_6m",
      label: "Instr 6m",
      kind: "instrument",
      connector: "quarterInch",
      lengthM: 6,
      resistanceOhms: 0.3,
      capacitanceFarads: 6e-10,
    },
  ];

  it("cableSpec extracts just the R·C the engine reads", () => {
    expect(cableSpec(cables[1])).toEqual({ resistanceOhms: 0.3, capacitanceFarads: 6e-10 });
  });

  it("cableTypeIdFor recovers the preset id from a stored spec", () => {
    expect(cableTypeIdFor(cables, cableSpec(cables[1]))).toBe("instrument_6m");
  });

  it("returns '' for no cable (ideal) or an unmatched custom spec", () => {
    expect(cableTypeIdFor(cables, undefined)).toBe("");
    expect(cableTypeIdFor(cables, { resistanceOhms: 999, capacitanceFarads: 1 })).toBe("");
  });
});

describe("cable geometry", () => {
  const p0 = { x: 0, y: 100 };
  const p3 = { x: 300, y: 100 };

  it("places control points a third of the way in horizontally", () => {
    const [a, c1, c2, b] = cableControlPoints(p0, p3);
    expect(a).toEqual(p0);
    expect(b).toEqual(p3);
    expect(c1.x).toBeCloseTo(100);
    expect(c2.x).toBeCloseTo(200);
  });

  it("droops the control points downward (+y) below the endpoints", () => {
    const [, c1, c2] = cableControlPoints(p0, p3);
    expect(c1.y).toBeGreaterThan(p0.y);
    expect(c2.y).toBeGreaterThan(p3.y);
  });

  it("sags more for a longer cable, but is clamped", () => {
    const [, near] = cableControlPoints({ x: 0, y: 0 }, { x: 40, y: 0 });
    const [, far] = cableControlPoints({ x: 0, y: 0 }, { x: 2000, y: 0 });
    expect(far.y).toBeGreaterThan(near.y);
    expect(far.y).toBeLessThanOrEqual(220 + 1e-9); // MAX_SAG clamp
  });

  it("emits a well-formed SVG cubic path", () => {
    const d = cablePathData(p0, p3);
    expect(d).toMatch(/^M [\d.-]+ [\d.-]+ C [\d.-]+ [\d.-]+ [\d.-]+ [\d.-]+ [\d.-]+ [\d.-]+$/);
  });
});

describe("cable hit-testing", () => {
  const p0 = { x: 0, y: 100 };
  const p3 = { x: 300, y: 100 };

  it("reports ~zero distance at an endpoint", () => {
    expect(distanceToCable(p0, p3, p0)).toBeCloseTo(0, 5);
  });

  it("finds the drooping midpoint below the straight line, not on it", () => {
    // The curve sags, so the midpoint of the straight chord (y=100) is above the curve → some distance.
    const mid = { x: 150, y: 100 };
    expect(distanceToCable(p0, p3, mid)).toBeGreaterThan(5);
    // A point on the sagged curve (below the chord) is close.
    const onCurve = { x: 150, y: 100 + 0.2 * 300 * 0.75 }; // roughly on the droop
    expect(distanceToCable(p0, p3, onCurve)).toBeLessThan(30);
  });

  it("isPointNearCable respects the threshold", () => {
    const near = { x: 0, y: 104 };
    const far = { x: 150, y: 500 };
    expect(isPointNearCable(p0, p3, near, 8)).toBe(true);
    expect(isPointNearCable(p0, p3, far, 8)).toBe(false);
  });
});
