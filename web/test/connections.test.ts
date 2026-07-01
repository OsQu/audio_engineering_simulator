import { describe, expect, it } from "vitest";
import type { Endpoint } from "../src/connections";
import {
  cableAllowed,
  cableControlPoints,
  cablePathData,
  distanceToCable,
  evaluateConnection,
  isPointNearCable,
} from "../src/connections";
import type { Connection } from "../src/scene";

// Endpoint builders keep the legality cases terse.
const out = (device: string, port = 0, domain: Endpoint["domain"] = "analog"): Endpoint => ({
  device,
  port,
  direction: "output",
  domain,
});
const inp = (device: string, port = 0, domain: Endpoint["domain"] = "analog"): Endpoint => ({
  device,
  port,
  direction: "input",
  domain,
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
    const v = evaluateConnection(out("ad", 0, "digital"), inp("da", 0, "digital"), []);
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

describe("cableAllowed", () => {
  it("permits a cable on analog edges only", () => {
    expect(cableAllowed("analog")).toBe(true);
    expect(cableAllowed("digital")).toBe(false);
    expect(cableAllowed("events")).toBe(false);
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
