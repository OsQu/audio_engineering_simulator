import { describe, expect, it } from "vitest";
import {
  SCARLETT_GAIN_HALO_THRESHOLDS_DBU,
  scarlettGainHaloState,
} from "../src/widgets/Scarlett8i6/gain-halo";

describe("Scarlett 8i6 input-gain halo", () => {
  const { green, amber, red } = SCARLETT_GAIN_HALO_THRESHOLDS_DBU;

  it("translates the documented -24 / -6 / 0 dBFS boundaries through the 1 V-peak ADC calibration", () => {
    // 0 dBFS is 1 V peak = 20·log10(1 / 0.7746) = +2.22 dBu. Subtracting the documented dBFS
    // thresholds gives -21.78 dBu (green) and -3.78 dBu (amber).
    expect(red).toBeCloseTo(2.22, 2);
    expect(green).toBeCloseTo(-21.78, 2);
    expect(amber).toBeCloseTo(-3.78, 2);
  });

  it("changes state exactly at each inclusive threshold", () => {
    expect(scarlettGainHaloState(green - 0.01)).toBe("off");
    expect(scarlettGainHaloState(green)).toBe("green");

    expect(scarlettGainHaloState(amber - 0.01)).toBe("green");
    expect(scarlettGainHaloState(amber)).toBe("amber");

    expect(scarlettGainHaloState(red - 0.01)).toBe("amber");
    expect(scarlettGainHaloState(red)).toBe("red");
  });

  it("stays dark for silence and invalid readouts", () => {
    expect(scarlettGainHaloState(Number.NEGATIVE_INFINITY)).toBe("off");
    expect(scarlettGainHaloState(Number.NaN)).toBe("off");
    expect(scarlettGainHaloState(Number.POSITIVE_INFINITY)).toBe("off");
  });
});
