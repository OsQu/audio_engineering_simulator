// Press ownership (Story 10.7): every stage press is classified exactly once, synchronously, before any
// gesture adapter captures the pointer or mutates anything. Pure — the stage adapter reads the DOM facts
// (is the target a control? which socket is picked? is it on a cable or a device?) and this decides.
//
// Priority: control/chrome → socket → visible cable → device move → backdrop pan. A control (knob, fader,
// switch, toolbar, focus window, banner, portal chip) keeps its own gesture and also blocks geometric
// socket search behind it; a picked socket beats the cable or device body it sits on (so a near-socket
// press can never start a device move, and a connected socket beats its own cable's hit path).

import type { SocketPick } from "./patch-targets";

export type Ownership =
  | { readonly kind: "control" }
  | { readonly kind: "socket"; readonly pick: Extract<SocketPick, { kind: "exact" | "near" }> }
  | { readonly kind: "cable"; readonly key: string }
  | { readonly kind: "device"; readonly objectId: string }
  | { readonly kind: "pan" };

/** What the stage adapter read from the press. */
export interface PressFacts {
  /** The target is inside a control, toolbar, focus window, banner, or other self-gesturing UI. */
  readonly inControl: boolean;
  /** The socket picked at the press point (already clipped by viewport, blockers and exposure). */
  readonly socket: SocketPick;
  /** The `data-cable` key of a visible cable hit path under the press, if any. */
  readonly cableKey: string | null;
  /** The `data-stage-object` id of the movable object under the press, if any. */
  readonly objectId: string | null;
}

export function classifyPress(facts: PressFacts): Ownership {
  if (facts.inControl) return { kind: "control" };
  if (facts.socket.kind !== "none") return { kind: "socket", pick: facts.socket };
  if (facts.cableKey !== null) return { kind: "cable", key: facts.cableKey };
  if (facts.objectId !== null) return { kind: "device", objectId: facts.objectId };
  return { kind: "pan" };
}
