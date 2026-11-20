import { describe, expect, it } from "vitest";
import type { ConfigDescriptor } from "../src/catalog";
import { makeHandle, resolveConfigWidget } from "../src/device-handle";

// The bound config widgets (ConfigSwitch/ConfigButton/ConfigSelect) render from `resolveConfigWidget`:
// a hit renders the control, a miss renders a dev-only ⚠ placeholder carrying `problem` (and logs it).
// The runner is node-only, so the branch is covered here at the resolver rather than by mounting.

const phantom: ConfigDescriptor = { key: "phantom", label: "48V", kind: "toggle", default: 0 };
const program: ConfigDescriptor = {
  key: "program",
  label: "Program",
  kind: "select",
  default: 0,
  options: [{ value: 0, label: "Piano" }],
};

const handle = makeHandle({
  device: "iface-1",
  params: [],
  ports: [],
  configs: [phantom, program],
  valueFor: () => 0,
  onParam: () => {},
});

describe("resolveConfigWidget", () => {
  it("returns the descriptor for a key the device exposes", () => {
    expect(resolveConfigWidget(handle, "phantom")).toEqual({ desc: phantom });
    expect(resolveConfigWidget(handle, "program", "select")).toEqual({ desc: program });
  });

  it("names the device and key when the key is missing from the descriptor", () => {
    const r = resolveConfigWidget(handle, "phantm");
    expect(r.desc).toBeUndefined();
    expect(r.problem).toContain('"phantm"');
    expect(r.problem).toContain('"iface-1"');
    expect(r.problem).toMatch(/stale WASM/);
  });

  it("treats a device with no configs at all as a miss", () => {
    const bare = makeHandle({
      device: "bare",
      params: [],
      ports: [],
      valueFor: () => 0,
      onParam: () => {},
    });
    expect(resolveConfigWidget(bare, "phantom").problem).toContain('"phantom"');
  });

  it("reports a kind mismatch when the widget needs a specific kind", () => {
    const r = resolveConfigWidget(handle, "phantom", "select");
    expect(r.desc).toBeUndefined();
    expect(r.problem).toContain("is a toggle, not a select");
  });
});
