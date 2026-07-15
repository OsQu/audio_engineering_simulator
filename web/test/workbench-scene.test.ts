import { describe, expect, it } from "vitest";
import type { DeviceDescriptor, PortDescriptor } from "../src/catalog";
import {
  analogOutputPort,
  BENCH_DEVICE,
  BOOTSTRAP_TYPE,
  benchScene,
  bootstrapScene,
  defaultBenchTap,
  SPEAKER_DEVICE,
} from "../src/workbench-scene";

// A minimal descriptor with the given typeId + ports — the scene builders only read `typeId` and `ports`.
function descWith(typeId: string, ports: Partial<PortDescriptor>[]): DeviceDescriptor {
  return {
    typeId,
    name: typeId,
    formFactor: { kind: "rackmount", rackUnits: 1 },
    params: [],
    ports: ports as PortDescriptor[],
    readouts: [],
    configs: [],
  } as DeviceDescriptor;
}

// A speaker descriptor with an analog output "Tap" at id 0 — the monitor terminus the fallback tap uses.
const speaker = descWith("speaker", [
  { id: 0, direction: "input", domain: "analog" },
  { id: 0, direction: "output", domain: "analog" },
]);

describe("bootstrapScene", () => {
  it("is a lone synth_voice tapped at port 0, no connections", () => {
    const { patch } = bootstrapScene();
    expect(patch.devices).toEqual([{ id: BENCH_DEVICE, typeId: BOOTSTRAP_TYPE }]);
    expect(patch.connections).toEqual([]);
    expect(patch.output).toEqual({ device: BENCH_DEVICE, port: 0 });
  });
});

describe("analogOutputPort", () => {
  it("picks the first ANALOG output, skipping an earlier digital output (the 8i6 USB-send trap)", () => {
    // The 8i6 shape: analog inputs, a *digital* USB-send output at id 0, then the analog Line Out at id 2.
    // The tap is rendered as a voltage, so it must resolve to the analog output (2), not the digital one.
    const desc = descWith("unit", [
      { id: 0, direction: "input", domain: "analog" },
      { id: 0, direction: "output", domain: "digital" },
      { id: 2, direction: "output", domain: "analog" },
    ]);
    expect(analogOutputPort(desc)).toBe(2);
  });

  it("is undefined for a digital-only-output device (e.g. the computer)", () => {
    const desc = descWith("unit", [{ id: 1, direction: "output", domain: "digital" }]);
    expect(analogOutputPort(desc)).toBeUndefined();
  });

  it("is undefined for an output-less device", () => {
    const desc = descWith("unit", [{ id: 0, direction: "input", domain: "analog" }]);
    expect(analogOutputPort(desc)).toBeUndefined();
  });
});

describe("defaultBenchTap", () => {
  it("taps the DUT's own analog output when it has one", () => {
    const dut = descWith("amp", [
      { id: 0, direction: "input", domain: "analog" },
      { id: 0, direction: "output", domain: "analog" },
    ]);
    expect(defaultBenchTap(dut, speaker)).toEqual({ device: BENCH_DEVICE, port: 0 });
  });

  it("falls back to the speaker's analog tap for a digital-only-output device (benchable via the DA)", () => {
    const dut = descWith("computer", [{ id: 0, direction: "output", domain: "digital" }]);
    expect(defaultBenchTap(dut, speaker)).toEqual({ device: SPEAKER_DEVICE, port: 0 });
  });
});

describe("benchScene", () => {
  it("builds the DUT, tapped at the DUT", () => {
    const dut = descWith("amp", [
      { id: 0, direction: "input", domain: "analog" },
      { id: 0, direction: "output", domain: "analog" },
    ]);
    const scene = benchScene(dut, [dut, speaker]);
    expect(scene?.patch.devices).toEqual([{ id: BENCH_DEVICE, typeId: "amp" }]);
    expect(scene?.patch.connections).toEqual([]);
    expect(scene?.patch.output).toEqual({ device: BENCH_DEVICE, port: 0 });
  });

  it("is undefined when the catalog has no speaker (a catalog regression, not a normal state)", () => {
    const dut = descWith("amp", [{ id: 0, direction: "output", domain: "analog" }]);
    expect(benchScene(dut, [dut])).toBeUndefined();
  });
});
