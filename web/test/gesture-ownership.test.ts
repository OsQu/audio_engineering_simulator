// Press ownership priority.
import { describe, expect, it } from "vitest";
import { classifyPress, type PressFacts } from "../src/gesture-ownership";
import type { SocketPick, SocketTarget } from "../src/patch-targets";

const target: SocketTarget = {
  id: "a:output:0",
  aliases: ["a:output:0"],
  objectId: "a",
  point: { x: 0, y: 0 },
  radius: 1,
  order: 0,
};
const near: SocketPick = { kind: "near", target };
const miss: SocketPick = { kind: "none", reason: "miss" };
const facts = (patch: Partial<PressFacts>): PressFacts => ({
  inControl: false,
  socket: miss,
  cableKey: null,
  objectId: null,
  ...patch,
});

describe("classifyPress", () => {
  it.each<[string, Partial<PressFacts>, string]>([
    [
      "a control beats a socket under it",
      { inControl: true, socket: near, objectId: "a" },
      "control",
    ],
    ["a near socket beats the device body it sits on", { socket: near, objectId: "a" }, "socket"],
    ["a socket beats its own cable's hit path", { socket: near, cableKey: "k" }, "socket"],
    ["a cable beats the device under it", { cableKey: "k", objectId: "a" }, "cable"],
    ["a device body without a socket nearby moves", { objectId: "a" }, "device"],
    ["anything else pans", {}, "pan"],
    [
      "a blocked socket search falls through",
      { socket: { kind: "none", reason: "blocked" } },
      "pan",
    ],
  ])("%s", (_name, patch, kind) => {
    expect(classifyPress(facts(patch)).kind).toBe(kind);
  });

  it("carries the picked socket and the ids it owns", () => {
    expect(classifyPress(facts({ socket: near }))).toEqual({ kind: "socket", pick: near });
    expect(classifyPress(facts({ cableKey: "k" }))).toEqual({ kind: "cable", key: "k" });
    expect(classifyPress(facts({ objectId: "a" }))).toEqual({ kind: "device", objectId: "a" });
  });
});
