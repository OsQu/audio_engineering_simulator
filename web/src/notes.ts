// The computer-keyboard → MIDI-note mapping as pure functions — no DOM, no Svelte. Lifted out of the
// old global keyboard listener (engine.ts) so the note layout is unit-testable and can be shared by
// both the physical-key capture and the on-screen keybed surface (Story 4.8).
//
// One octave of a piano over the QWERTY home rows: white keys on A–K, black keys on the W/E/T/Y/U row
// above — the de-facto layout (Ableton, many soft synths). Z/X transpose the base octave.

/** Semitone offset (from the base-octave C) for each mapped key. */
const KEY_SEMITONES: Record<string, number> = {
  a: 0, // C
  w: 1, // C#
  s: 2, // D
  e: 3, // D#
  d: 4, // E
  f: 5, // F
  t: 6, // F#
  g: 7, // G
  y: 8, // G#
  h: 9, // A
  u: 10, // A#
  j: 11, // B
  k: 12, // C (octave up)
};

/** MIDI note for the base octave's C (C4 = 60). */
export const BASE_C = 60;
/** The lowest / highest octave transposition Z/X can reach (± this many octaves from the base). */
export const OCTAVE_MIN = -3;
export const OCTAVE_MAX = 3;
/** Fixed velocity for computer-keyboard notes (no aftertouch on a QWERTY key). */
export const DEFAULT_VELOCITY = 100;

/** The MIDI note a key plays at the given octave transposition, or `null` if the key isn't a note key.
 *  Case-insensitive. */
export function noteForKey(key: string, octave: number): number | null {
  const semis = KEY_SEMITONES[key.toLowerCase()];
  return semis === undefined ? null : BASE_C + 12 * octave + semis;
}

/** The octave step a key triggers — `-1` for Z (down), `+1` for X (up), `null` for any other key. */
export function octaveShiftFor(key: string): -1 | 1 | null {
  const k = key.toLowerCase();
  if (k === "z") return -1;
  if (k === "x") return 1;
  return null;
}

/** Clamp an octave transposition into the playable range [{@link OCTAVE_MIN}, {@link OCTAVE_MAX}]. */
export function clampOctave(octave: number): number {
  return Math.max(OCTAVE_MIN, Math.min(OCTAVE_MAX, octave));
}
