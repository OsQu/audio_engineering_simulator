// Machine-readable engine status for the live-verification hooks. The Debug surfaces render prose for
// people ("▶ playing @ 48000 Hz · …"); agents and browser specs read these values instead, from `data-*`
// attributes on stable `data-testid`s (see `EngineHealth.svelte`, `Vu.svelte`, `Meter.svelte`), so a
// wording change in the prose never breaks a check.

/** The engine's lifecycle as one word. `suspended` = the worklet is built but the AudioContext isn't
 *  running yet (the browser autoplay policy: it needs a user gesture); `playing` = audio is flowing. */
export type EngineState = "idle" | "booting" | "suspended" | "playing" | "error";

export interface EngineStateInputs {
  /** A fatal engine fault message (worklet crash/error, protocol mismatch, setup failure), or null. */
  fault: string | null;
  started: boolean;
  ready: boolean;
  /** The AudioContext's state, or null before the context exists. */
  audioState: AudioContextState | null;
}

export function resolveEngineState(s: EngineStateInputs): EngineState {
  if (s.fault !== null) return "error";
  if (!s.started) return "idle";
  if (!s.ready) return "booting";
  return s.audioState === "running" ? "playing" : "suspended";
}

/** Below this linear peak the monitor reads as silence (−80 dBFS). */
const SILENCE = 1e-4;

/** A linear output peak (±1.0 = full scale) as dBFS; silence is `-Infinity`. */
export function peakDbfs(linear: number): number {
  return linear > SILENCE ? 20 * Math.log10(linear) : Number.NEGATIVE_INFINITY;
}

/** A dBFS value as a `data-*` attribute: one decimal, or `-inf` for silence. */
export function dbfsAttr(dbfs: number): string {
  return Number.isFinite(dbfs) ? dbfs.toFixed(1) : "-inf";
}

/** Whether the URL asks for the Debug surface open on load (`?debug=1`) — so a check (or a person
 *  sharing a link) lands with the engine health visible instead of hunting for the menu. */
export function debugRequested(search: string): boolean {
  return new URLSearchParams(search).get("debug") === "1";
}
