import { describe, expect, it } from "vitest";
import type { DeviceDescriptor, PortDescriptor } from "../src/catalog";
import {
  analogOutputPort,
  BENCH_DEVICE,
  BOOTSTRAP_TYPE,
  bootstrapScene,
  deviceScene,
} from "../src/workbench-scene";

// A minimal descriptor with the given ports — deviceScene only reads `ports`.
function descWith(ports: Partial<PortDescriptor>[]): DeviceDescriptor {
  return {
    typeId: "unit",
    name: "Unit",
    formFactor: { kind: "rackmount", rackUnits: 1 },
    params: [],
    ports: ports as PortDescriptor[],
    readouts: [],
    configs: [],
  } as DeviceDescriptor;
}

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
    const desc = descWith([
      { id: 0, direction: "input", domain: "analog" },
      { id: 0, direction: "output", domain: "digital" },
      { id: 2, direction: "output", domain: "analog" },
    ]);
    expect(analogOutputPort(desc)).toBe(2);
  });

  it("is undefined for a digital-only-output device (e.g. the computer) — can't be tapped without a DA", () => {
    const desc = descWith([{ id: 1, direction: "output", domain: "digital" }]);
    expect(analogOutputPort(desc)).toBeUndefined();
  });

  it("is undefined for an output-less device", () => {
    const desc = descWith([{ id: 0, direction: "input", domain: "analog" }]);
    expect(analogOutputPort(desc)).toBeUndefined();
  });
});

describe("deviceScene", () => {
  it("builds a lone device tapped at the given (analog) output port, no connections", () => {
    const { patch } = deviceScene("scarlett_8i6", 2);
    expect(patch.devices).toEqual([{ id: BENCH_DEVICE, typeId: "scarlett_8i6" }]);
    expect(patch.connections).toEqual([]);
    expect(patch.output).toEqual({ device: BENCH_DEVICE, port: 2 });
  });
});
