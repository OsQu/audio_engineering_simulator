import { describe, expect, it } from "vitest";
import type { ParamDescriptor } from "../src/catalog";
import { fromNorm, toNorm } from "../src/widgets/taper";

// The control-law math (widgets/taper.ts). Tests are the oracle here: you can't see a knob's taper,
// so the mapping is asserted against hand calcs. dB = 20·log₁₀(multiplier).

const gain = (over: Partial<ParamDescriptor> = {}): ParamDescriptor => ({
  id: 0,
  label: "Gain",
  unit: "dB",
  kind: "knob",
  taper: "log",
  min: 0,
  max: 1000, // +60 dB voltage-gain ceiling — the 8i6 gain knobs
  default: 1,
  ...over,
});

const db = (v: number) => 20 * Math.log10(v);

describe("log (dB) taper — the 8i6 gain knobs", () => {
  it("maps a quarter-turn to ~unity (0 dB), not the old linear +48 dB", () => {
    // Old law (linear in the multiplier): fromNorm(0.25) would be 0.25·1000 = 250× = +48 dB.
    const linear = gain({ taper: "linear" });
    expect(db(fromNorm(linear, 0.25))).toBeCloseTo(db(250), 5); // +47.96 dB — the reported bug

    // dB taper: ¼ ≈ 0 dB, ½ ≈ +20, ¾ ≈ +40, full = +60 (80 dB sweep below the +60 dB max).
    const p = gain();
    expect(db(fromNorm(p, 0.25))).toBeCloseTo(0, 4);
    expect(db(fromNorm(p, 0.5))).toBeCloseTo(20, 4);
    expect(db(fromNorm(p, 0.75))).toBeCloseTo(40, 4);
    expect(db(fromNorm(p, 1))).toBeCloseTo(60, 4);
  });

  it("reaches true silence at the very bottom (min === 0)", () => {
    expect(fromNorm(gain(), 0)).toBe(0);
  });

  it("round-trips the unity default so the pointer sits where the value is", () => {
    // The default (1.0 = 0 dB) must map to a travel position that maps back to 1.0 — else the knob
    // would jump on first touch. 0 dB is 60 dB below the +60 dB max, ¾ of the 80 dB sweep ⇒ ¼ travel.
    const p = gain();
    expect(toNorm(p, p.default)).toBeCloseTo(0.25, 4);
    expect(fromNorm(p, toNorm(p, p.default))).toBeCloseTo(1, 4);
  });

  it("is its own inverse across the dB range", () => {
    const p = gain();
    for (const v of [1, 3.2, 10, 100, 500, 1000]) {
      expect(fromNorm(p, toNorm(p, v))).toBeCloseTo(v, 3);
    }
  });

  it("floors at the minimum gain (no silence) for a positive-floored preamp", () => {
    // A mic preamp bottoms out at +8 dB (min = 10^(8/20) ≈ 2.512×), not −∞: a geometric taper
    // between a positive floor and max is dB-linear, so dB(norm) = 8 + norm·(60 − 8).
    const preamp = gain({ min: 2.5118864, default: 2.5118864 });
    expect(db(fromNorm(preamp, 0))).toBeCloseTo(8, 3); // bottom = +8 dB, never silence
    expect(fromNorm(preamp, 0)).toBeGreaterThan(0);
    for (const norm of [0, 0.25, 0.5, 0.75, 1]) {
      expect(db(fromNorm(preamp, norm))).toBeCloseTo(8 + norm * 52, 3);
    }
    // Default (= floor) round-trips to the fully-CCW position with no first-touch jump.
    expect(toNorm(preamp, preamp.default)).toBeCloseTo(0, 4);
    expect(fromNorm(preamp, toNorm(preamp, preamp.default))).toBeCloseTo(preamp.default, 4);
  });

  it("is a unity-capped attenuator for a monitor/phones volume control", () => {
    // Monitor/phones cap GAIN at unity (max = 1×, 0 dB) and reach silence at the bottom — a level
    // control, not a booster. Default unity sits fully open (top of travel) and round-trips.
    const vol = gain({ max: 1, default: 1 });
    expect(db(fromNorm(vol, 1))).toBeCloseTo(0, 4); // top = unity, never boosts above 0 dB
    expect(fromNorm(vol, 0)).toBe(0); // bottom = silence
    expect(db(fromNorm(vol, 0.75))).toBeCloseTo(-20, 4); // dB(norm) = (norm − 1)·80
    expect(toNorm(vol, vol.default)).toBeCloseTo(1, 4);
    expect(fromNorm(vol, toNorm(vol, vol.default))).toBeCloseTo(1, 4);
  });

  it("leaves a linear param unchanged (0 = min, 1 = max, midpoint = average)", () => {
    const lin: ParamDescriptor = { ...gain({ taper: "linear" }), min: 0, max: 4 };
    expect(fromNorm(lin, 0)).toBe(0);
    expect(fromNorm(lin, 0.5)).toBeCloseTo(2, 6);
    expect(fromNorm(lin, 1)).toBe(4);
    expect(toNorm(lin, 3)).toBeCloseTo(0.75, 6);
  });

  it("treats an absent taper as linear", () => {
    const p: ParamDescriptor = { ...gain(), taper: undefined, min: 0, max: 10 };
    expect(fromNorm(p, 0.5)).toBeCloseTo(5, 6);
  });
});
