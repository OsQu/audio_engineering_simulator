import { describe, expect, it } from "vitest";
import type { ParamDescriptor, ParamKind, ParamTaper } from "../src/catalog";
import { formatParam } from "../src/widgets/format";

// A terse param-descriptor builder — only the fields `formatParam` reads (kind/unit/taper) matter here.
const param = (unit: string, kind: ParamKind = "knob", taper?: ParamTaper): ParamDescriptor => ({
  id: 0,
  label: "x",
  unit,
  kind,
  taper,
  min: 0,
  max: 1,
  default: 0,
});

describe("formatParam", () => {
  it("shows on/off for a switch regardless of unit", () => {
    expect(formatParam(param("", "switch"), 1)).toBe("on");
    expect(formatParam(param("", "switch"), 0)).toBe("off");
  });

  it('formats a "dB" (voltage-gain) unit as 20·log₁₀ dB with a sign, −∞ at zero', () => {
    expect(formatParam(param("dB"), 1)).toBe("+0.0 dB"); // unity gain
    expect(formatParam(param("dB"), 2)).toBe("+6.0 dB"); // 20·log10(2) = 6.02
    expect(formatParam(param("dB"), 0.5)).toBe("-6.0 dB"); // 20·log10(0.5) = −6.02
    expect(formatParam(param("dB"), 0)).toBe("−∞ dB");
  });

  it('formats a "dB SPL" unit as 20·log₁₀(p / 20µPa), so 1 Pa reads 94 dB SPL', () => {
    expect(formatParam(param("dB SPL"), 1)).toBe("94.0 dB SPL"); // 20·log10(1/20e-6) = 93.98
    expect(formatParam(param("dB SPL"), 20)).toBe("120.0 dB SPL"); // 20·log10(20/20e-6) = 120
    expect(formatParam(param("dB SPL"), 0)).toBe("−∞ dB SPL"); // silence
  });

  it("shows the raw number + unit for any other unit — a log-taper Hz knob reads Hz, not dB", () => {
    expect(formatParam(param("Hz", "knob", "log"), 220)).toBe("220 Hz");
    expect(formatParam(param("V"), 1.23)).toBe("1.23 V");
    expect(formatParam(param("ms"), 5)).toBe("5 ms");
  });

  it("shows a unitless value with no suffix", () => {
    expect(formatParam(param(""), 5)).toBe("5");
    expect(formatParam(param(""), 0.75)).toBe("0.75");
  });
});
