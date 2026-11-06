import { describe, expect, it } from "vitest";
import { dbfsAttr, debugRequested, peakDbfs, resolveEngineState } from "../src/live-status";

describe("resolveEngineState", () => {
  const base = { fault: null, started: true, ready: true, audioState: "running" as const };

  it("walks idle → booting → suspended → playing", () => {
    expect(resolveEngineState({ ...base, started: false, ready: false, audioState: null })).toBe(
      "idle",
    );
    expect(resolveEngineState({ ...base, ready: false, audioState: "suspended" })).toBe("booting");
    expect(resolveEngineState({ ...base, audioState: "suspended" })).toBe("suspended");
    expect(resolveEngineState(base)).toBe("playing");
  });

  it("a closed context is not playing", () => {
    expect(resolveEngineState({ ...base, audioState: "closed" })).toBe("suspended");
  });

  it("a fault wins over every other state", () => {
    expect(resolveEngineState({ ...base, fault: "worklet error: boom" })).toBe("error");
    expect(resolveEngineState({ ...base, fault: "x", started: false })).toBe("error");
  });
});

describe("peakDbfs / dbfsAttr", () => {
  it("full scale is 0 dBFS, half scale ≈ −6.0", () => {
    expect(dbfsAttr(peakDbfs(1))).toBe("0.0");
    // 20·log10(0.5) = −6.0206
    expect(peakDbfs(0.5)).toBeCloseTo(-6.0206, 3);
    expect(dbfsAttr(peakDbfs(0.5))).toBe("-6.0");
  });

  it("silence (≤ 1e-4) is −∞ and serializes as -inf", () => {
    expect(peakDbfs(0)).toBe(Number.NEGATIVE_INFINITY);
    expect(peakDbfs(1e-4)).toBe(Number.NEGATIVE_INFINITY);
    expect(dbfsAttr(peakDbfs(0))).toBe("-inf");
  });
});

describe("debugRequested", () => {
  it("is on only for debug=1", () => {
    expect(debugRequested("?debug=1")).toBe(true);
    expect(debugRequested("?s=abc&debug=1")).toBe(true);
    expect(debugRequested("")).toBe(false);
    expect(debugRequested("?debug=0")).toBe(false);
    expect(debugRequested("?debug")).toBe(false);
  });
});
