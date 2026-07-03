import { describe, expect, it } from "vitest";
import type { DeviceDescriptor, PortDescriptor } from "../src/catalog";
import { focusSurfaceFor, isFocusable } from "../src/focus";

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

describe("focusSurfaceFor", () => {
  it("gives any device with an events input an instrument (keybed) surface — derived, not listed", () => {
    // Both the synth and the standalone controller have an events input, so both get a keybed with no
    // per-type entry needed.
    expect(focusSurfaceFor(device("synth_voice", [eventsIn]))).toBe("instrument");
    expect(focusSurfaceFor(device("midi_controller", [eventsIn]))).toBe("instrument");
    // Even an unknown future events-in device gets a keybed for free.
    expect(focusSurfaceFor(device("some_new_synth", [eventsIn]))).toBe("instrument");
  });

  it("gives channel_strip a console surface via the explicit override", () => {
    expect(focusSurfaceFor(device("channel_strip", [analogIn]))).toBe("console");
  });

  it("is null for a device with no deep-control surface", () => {
    expect(focusSurfaceFor(device("speaker", [analogIn]))).toBeNull();
    expect(focusSurfaceFor(device("ad_converter", [analogIn]))).toBeNull();
  });
});

describe("isFocusable", () => {
  it("is true exactly when there is a surface to show", () => {
    expect(isFocusable(device("synth_voice", [eventsIn]))).toBe(true);
    expect(isFocusable(device("channel_strip", [analogIn]))).toBe(true);
    expect(isFocusable(device("speaker", [analogIn]))).toBe(false);
  });
});
