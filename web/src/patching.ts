// The cable patching state machine as pure transitions — no DOM, no Svelte. This is the trickiest UI
// logic in the app (drag-to-patch, plus a click-to-pick "pending" mode that survives a wall/room switch
// so a cross-view patch completes with two clicks), so pulling it into pure functions makes every
// transition unit-testable.
//
// The DOM facts a transition needs — which jack is under the pointer (as a JackHit), the surface-space
// cursor, the pressed jack's projected anchor, and whether the press was a click or a drag — are resolved by
// the App adapter and passed in. A transition returns the next PatchState; the ones that can complete a
// patch return a `commit` verdict alongside, and the caller commits it + hot-swaps.

import type { CableType, Describe } from "./catalog";
import { type ConnectVerdict, type Endpoint, evaluateConnection } from "./connections";
import type { Connection } from "./scene";

export type Pt = { x: number; y: number };

// An in-progress patch from a source jack: the logical source end (never a cached anchor — renderers
// re-resolve it every frame through cable-resolution `resolveHeldCable`), the moving free end
// (surface coords), and — when hovering a candidate jack — the verdict so the cable can be coloured.
// `mode` is "drag" while the pointer is held (same-view patching), or "pending" after a *click* — a held
// cable that survives a wall/room switch, so a cross-view patch completes by clicking the source jack,
// turning to the other wall/room, and clicking the destination. `rejection` is the reason the last
// release on an illegal socket didn't connect: it stays visible until the pointer leaves that
// socket, so the user can choose another destination.
export type PatchState = {
  source: Endpoint;
  free: Pt;
  over: boolean;
  legal: boolean;
  verdict: ConnectVerdict | null;
  mode: "drag" | "pending";
  rejection: Rejection | null;
} | null;

/** Why a release on a socket didn't connect: the socket's key and the verdict's reason. */
export type Rejection = { key: string; reason: string };

// The socket under the pointer, resolved by the adapter from a picked target (patch-targets.ts): the key
// of the endpoint it stands for, every alias of its physical socket (a duplex socket has two), the
// Endpoint, and its projected anchor (null when the socket has no current geometry in this view).
export type JackHit = { key: string; endpoint: Endpoint; anchor: Pt | null; aliases?: string[] };
export type PatchDeps = { connections: Connection[]; cables?: CableType[] };
// A transition result; `commit` present ⇒ the caller should commit that verdict and hot-swap.
export type PatchResult = { state: PatchState; commit?: ConnectVerdict };
// The "device:direction:port" key of an endpoint's jack (matches jackKey in App / the data-jack attr).
export const jackKeyOf = (e: Endpoint): string => `${e.device}:${e.direction}:${e.port}`;

// Resolve a `data-jack` value ("device:direction:port") to a full Endpoint (with the port's domain from
// the descriptor), or null if it doesn't name a real port.
export function endpointFromJackKey(describe: Describe, key: string): Endpoint | null {
  const [device, direction, portStr] = key.split(":");
  if (!device || (direction !== "input" && direction !== "output")) return null;
  const port = Number(portStr);
  const desc = describe(device);
  const pd = desc?.ports.find((p) => p.direction === direction && p.id === port);
  if (!pd) return null;
  return {
    device,
    port,
    direction,
    domain: pd.domain,
    connector: pd.connector,
    channels: pd.channels,
    duplexPartner: pd.duplexPartner,
  };
}

/** Which alias of a physical socket a press means: for a duplex socket, the direction complementing the
 *  held source (so output → input reads as one connection), else the socket's canonical alias. */
export function aliasFor(aliases: readonly string[], source: Endpoint | null): string | undefined {
  if (source) {
    const want = source.direction === "output" ? ":input:" : ":output:";
    const complement = aliases.find((a) => a.includes(want));
    if (complement) return complement;
  }
  return aliases[0];
}

/** Does this hit stand on the source's own physical socket (any alias)? */
const isSource = (state: NonNullable<PatchState>, hit: JackHit): boolean => {
  const key = jackKeyOf(state.source);
  return hit.key === key || (hit.aliases?.includes(key) ?? false);
};

// Pointer-down on a jack. A pending cable is untouched here — its second interaction resolves on
// pointer-up. Otherwise, a jack with a measured anchor starts a fresh drag from it; anything else leaves
// the state unchanged.
export function pointerDown(state: PatchState, hit: JackHit | null): PatchResult {
  if (state?.mode === "pending") return { state };
  if (!hit?.anchor) return { state };
  return {
    state: {
      source: hit.endpoint,
      free: hit.anchor,
      over: false,
      legal: false,
      verdict: null,
      mode: "drag",
      rejection: null,
    },
  };
}

// Track the free end while dragging or holding a pending cable. If the pointer is over another jack,
// evaluate legality and snap the free end to it (a magnetic feel); else follow the cursor. This is
// feedback only — release re-resolves its own target. A shown rejection survives only while
// the pointer stays on the socket that was rejected.
export function pointerMove(
  state: PatchState,
  hit: JackHit | null,
  cursor: Pt,
  deps: PatchDeps,
): PatchState {
  if (!state) return null;
  const rejection = hit && state.rejection?.key === hit.key ? state.rejection : null;
  if (hit && !isSource(state, hit)) {
    const verdict = evaluateConnection(state.source, hit.endpoint, deps.connections, deps.cables);
    return {
      ...state,
      free: hit.anchor ?? cursor,
      over: true,
      legal: verdict.ok,
      verdict,
      rejection,
    };
  }
  return { ...state, free: cursor, over: false, legal: false, verdict: null, rejection };
}

// Release. `hit` is the socket picked at the release point *now* — never the last move's verdict — and
// `clickNotDrag` is true when the press never moved past the threshold. Both modes evaluate a destination
// the same way, so click + click and drag + drop return the same verdict:
//  - a legal destination commits;
//  - an illegal destination keeps (or turns the drag into) a **pending** pick carrying the reason;
//  - the source itself: a click promotes a drag to pending and cancels a pending pick.
// Otherwise (no socket under the release): a drag click promotes to pending, a real drag drops the lead,
// and a pending pick is kept — a click on empty space or a non-jack (the view/space switcher, which is how
// a cross-view patch completes) must not end it. Cancel otherwise is via Esc or the banner's Cancel.
export function pointerUp(
  state: PatchState,
  hit: JackHit | null,
  clickNotDrag: boolean,
  deps: PatchDeps,
): PatchResult {
  if (!state) return { state: null };
  const pending = (rejection: Rejection | null = null): PatchResult => ({
    state: { ...state, mode: "pending", over: false, legal: false, verdict: null, rejection },
  });

  if (hit && isSource(state, hit)) {
    if (state.mode === "drag") return clickNotDrag ? pending() : { state: null };
    return clickNotDrag ? { state: null } : { state };
  }
  if (hit) {
    const verdict = evaluateConnection(state.source, hit.endpoint, deps.connections, deps.cables);
    if (verdict.ok) return { state: null, commit: verdict };
    return pending({ key: hit.key, reason: verdict.reason });
  }
  if (state.mode === "drag") return clickNotDrag ? pending() : { state: null };
  return { state };
}

// The gesture was cancelled (pointercancel, lost capture, blur, unmount) — never a commit. A held *drag*
// drops its lead; a pending pick survives (the cancelled gesture wasn't the pick).
export function pointerCancel(state: PatchState): PatchState {
  return state?.mode === "pending" ? state : null;
}

// Esc (or any explicit cancel) drops an in-progress patch.
export function cancel(): PatchState {
  return null;
}
