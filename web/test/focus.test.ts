import { describe, expect, it } from "vitest";
import type { DeviceDescriptor, PortDescriptor } from "../src/catalog";
import { isFocusable } from "../src/focus";

const eventsIn: PortDescriptor = {
  id: 0,
  label: "MIDI In",
  direction: "input",
  domain: "events",
  kind: "midi",
  connector: "din5",
};
const analogIn: PortDescriptor = {
  id: 0,
  label: "In",
  direction: "input",
  domain: "analog",
  kind: "line",
  connector: "quarterInch",
};

function device(typeId: string, ports: PortDescriptor[]): DeviceDescriptor {
  return {
    typeId,
    name: typeId,
    formFactor: { kind: "desktop", widthMm: 100, heightMm: 100, depthMm: 100 },
    params: [],
    ports,
    readouts: [],
  };
}

describe("isFocusable", () => {
  it("is true for any device with an events input (playable/operable) — derived, not listed", () => {
    // Both the synth and the standalone controller have an events input, so both are focusable with no
    // per-type entry needed; even an unknown future events-in device is focusable for free. (Whether a
    // focus surface actually shows a keybed is a per-surface choice now — see MidiControllerFocus.)
    expect(isFocusable(device("synth_voice", [eventsIn]))).toBe(true);
    expect(isFocusable(device("midi_controller", [eventsIn]))).toBe(true);
    expect(isFocusable(device("some_new_synth", [eventsIn]))).toBe(true);
  });

  it("is true for a type with a dedicated focus surface (channel_strip → console)", () => {
    expect(isFocusable(device("channel_strip", [analogIn]))).toBe(true);
  });

  it("is false for a device with no deep-control surface", () => {
    expect(isFocusable(device("speaker", [analogIn]))).toBe(false);
    expect(isFocusable(device("ad_converter", [analogIn]))).toBe(false);
  });
});
