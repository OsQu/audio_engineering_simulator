import { describe, expect, it } from "vitest";
import {
  BASE_C,
  clampOctave,
  noteForKey,
  OCTAVE_MAX,
  OCTAVE_MIN,
  octaveShiftFor,
} from "../src/notes";

describe("noteForKey", () => {
  it("maps the home row to a chromatic octave from the base C", () => {
    // A = C (base), then the mapped keys climb by their semitone offsets to K = C an octave up.
    expect(noteForKey("a", 0)).toBe(BASE_C); // C4 = 60
    expect(noteForKey("w", 0)).toBe(BASE_C + 1); // C#4
    expect(noteForKey("k", 0)).toBe(BASE_C + 12); // C5 (octave up)
    expect(noteForKey("h", 0)).toBe(BASE_C + 9); // A4 = 69 (the 440 Hz reference)
  });

  it("is case-insensitive", () => {
    expect(noteForKey("A", 0)).toBe(noteForKey("a", 0));
  });

  it("transposes by whole octaves", () => {
    expect(noteForKey("a", 1)).toBe(BASE_C + 12);
    expect(noteForKey("a", -2)).toBe(BASE_C - 24);
  });

  it("returns null for keys that aren't note keys", () => {
    expect(noteForKey("z", 0)).toBeNull(); // octave-shift key, not a note
    expect(noteForKey("x", 0)).toBeNull();
    expect(noteForKey("1", 0)).toBeNull();
    expect(noteForKey(" ", 0)).toBeNull();
  });
});

describe("octaveShiftFor", () => {
  it("reads Z as down, X as up, and nothing else", () => {
    expect(octaveShiftFor("z")).toBe(-1);
    expect(octaveShiftFor("x")).toBe(1);
    expect(octaveShiftFor("Z")).toBe(-1); // case-insensitive
    expect(octaveShiftFor("a")).toBeNull();
  });
});

describe("clampOctave", () => {
  it("clamps to the playable range", () => {
    expect(clampOctave(0)).toBe(0);
    expect(clampOctave(OCTAVE_MAX + 5)).toBe(OCTAVE_MAX);
    expect(clampOctave(OCTAVE_MIN - 5)).toBe(OCTAVE_MIN);
  });
});
