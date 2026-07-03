// The cable patching state machine as pure transitions — no DOM, no Svelte. This is the trickiest UI
// logic in the app (drag-to-patch, plus a click-to-pick "pending" mode that survives a wall/room switch
// so a cross-view patch completes with two clicks), so pulling it into pure functions makes every
// transition unit-testable.
//
// The DOM facts a transition needs — which jack is under the pointer (as a JackHit), the surface-space
// cursor, the source jack's live anchor, and whether the press was a click or a drag — are resolved by
// the App adapter and passed in. A transition returns the next PatchState; the ones that can complete a
// patch return a `commit` verdict alongside, and the caller commits it + hot-swaps.

import { type DeviceDescriptor, descriptorFor } from "./catalog";
import { type ConnectVerdict, type Endpoint, evaluateConnection } from "./connections";
import { deviceById } from "./projection";
import type { Connection } from "./scene";
import type { Scene } from "./scene-store";

export type Pt = { x: number; y: number };

// An in-progress patch from a source jack: the source end + its anchor, the moving free end (surface
// coords), and — when hovering a candidate jack — the verdict so the cable can be coloured + committed.
// `mode` is "drag" while the pointer is held (same-view patching), or "pending" after a *click* — a held
// cable that survives a wall/room switch, so a cross-view patch completes by clicking the source jack,
// turning to the other wall/room, and clicking the destination.
export type PatchState = {
  source: Endpoint;
  srcPoint: Pt;
  free: Pt;
  over: boolean;
  legal: boolean;
  verdict: ConnectVerdict | null;
  mode: "drag" | "pending";
} | null;

// The jack under the pointer, resolved by the adapter: its data-jack key, the Endpoint it names, and its
// measured anchor (null when the jack isn't currently measured / in view).
export type JackHit = { key: string; endpoint: Endpoint; anchor: Pt | null };
export type PatchDeps = { connections: Connection[] };
// A transition result; `commit` present ⇒ the caller should commit that verdict and hot-swap.
export type PatchResult = { state: PatchState; commit?: ConnectVerdict };

// The "device:direction:port" key of an endpoint's jack (matches jackKey in App / the data-jack attr).
export const jackKeyOf = (e: Endpoint): string => `${e.device}:${e.direction}:${e.port}`;

// Resolve a `data-jack` value ("device:direction:port") to a full Endpoint (with the port's domain from
// the descriptor), or null if it doesn't name a real port.
export function endpointFromJackKey(
  scene: Scene,
  catalog: DeviceDescriptor[],
  key: string,
): Endpoint | null {
  const [device, direction, portStr] = key.split(":");
  if (!device || (direction !== "input" && direction !== "output")) return null;
  const port = Number(portStr);
  const dev = deviceById(scene, device);
  const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
  const pd = desc?.ports.find((p) => p.direction === direction && p.id === port);
  if (!pd) return null;
  return { device, port, direction, domain: pd.domain, connector: pd.connector };
}

// Pointer-down on a jack. A pending cable is untouched here — its second interaction resolves on
// pointer-up (the adapter only records the press point so a click can be told from a pan). Otherwise, a
// jack with a measured anchor starts a fresh drag from it; anything else leaves the state unchanged.
export function pointerDown(state: PatchState, hit: JackHit | null): PatchResult {
  if (state?.mode === "pending") return { state };
  if (!hit?.anchor) return { state };
  return {
    state: {
      source: hit.endpoint,
      srcPoint: hit.anchor,
      free: hit.anchor,
      over: false,
      legal: false,
      verdict: null,
      mode: "drag",
    },
  };
}

// Track the free end while dragging or holding a pending cable. `srcAnchor` is the source jack's live
// anchor (or null if it's off-view — the lead then draws as a floating end). If the pointer is over
// another jack, evaluate legality and snap the free end to it (a magnetic feel); else follow the cursor.
export function pointerMove(
  state: PatchState,
  hit: JackHit | null,
  cursor: Pt,
  srcAnchor: Pt | null,
  deps: PatchDeps,
): PatchState {
  if (!state) return null;
  const srcPoint = srcAnchor ?? state.srcPoint;
  if (hit && hit.key !== jackKeyOf(state.source)) {
    const verdict = evaluateConnection(state.source, hit.endpoint, deps.connections);
    return {
      ...state,
      srcPoint,
      free: hit.anchor ?? cursor,
      over: true,
      legal: verdict.ok,
      verdict,
    };
  }
  return { ...state, srcPoint, free: cursor, over: false, legal: false, verdict: null };
}

// Release. `clickNotDrag` is true when the press never moved past the drag threshold.
//  - **drag**: a release over a legal jack commits; a click (no move) promotes to a **pending** pick
//    that survives a view switch; a real drag released over nothing cancels.
//  - **pending**: a press-and-drag is a pan → keep the pick. A *click* resolves it only in the two ways
//    that must end the patch: a legal destination jack commits, and re-clicking the source cancels.
//    Everything else keeps the pick — a click on empty space, on a non-jack (e.g. the view/space
//    switcher, which is how a cross-view patch is completed), or on an illegal jack. Cancel otherwise is
//    via Esc or the banner's Cancel button.
export function pointerUp(
  state: PatchState,
  hit: JackHit | null,
  clickNotDrag: boolean,
  deps: PatchDeps,
): PatchResult {
  if (!state) return { state: null };

  if (state.mode === "drag") {
    if (state.verdict?.ok) return { state: null, commit: state.verdict };
    if (clickNotDrag)
      return { state: { ...state, mode: "pending", over: false, legal: false, verdict: null } };
    return { state: null };
  }

  if (!clickNotDrag) return { state }; // a press-and-drag is a pan — keep the pick
  if (!hit) return { state }; // empty space / a non-jack (e.g. the switcher) — keep it through the switch
  if (hit.key === jackKeyOf(state.source)) return { state: null }; // re-clicking the source cancels
  const verdict = evaluateConnection(state.source, hit.endpoint, deps.connections);
  if (verdict.ok) return { state: null, commit: verdict }; // a legal destination commits
  return { state }; // an illegal jack — keep the pick so another target can be tried
}

// Esc (or any explicit cancel) drops an in-progress patch.
export function cancel(): PatchState {
  return null;
}
