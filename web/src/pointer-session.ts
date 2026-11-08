// The shared primary-pointer lifecycle every owned stage gesture — patching, device move,
// pan, cable select — runs on. One session per stage root at a time:
//   - only a primary pointer's primary button begins one; presses by other pointers are ignored while it
//     runs (multi-pointer), as are their moves and releases;
//   - movement past a per-pointer-type threshold turns a click into a drag;
//   - a normal release is distinct from a cancel (`pointercancel`, lost capture, window blur, unmount),
//     which must never commit;
//   - after a release, ownership is cleared *before* capture is released, so the resulting
//     `lostpointercapture` cannot undo the committed gesture.
//
// `step` is the pure reducer (unit-tested); `beginPointerSession` is the thin DOM driver around it.

export type Pt = { readonly x: number; readonly y: number };

/** Client px a press may travel before it stops being a click. Touch jitters more. */
export const MOVE_THRESHOLD_PX: Readonly<Record<string, number>> = { mouse: 4, pen: 4, touch: 8 };
const thresholdFor = (pointerType: string): number => MOVE_THRESHOLD_PX[pointerType] ?? 4;

export type SessionState =
  | { readonly kind: "idle" }
  | {
      readonly kind: "active";
      readonly pointerId: number;
      readonly start: Pt;
      readonly moved: boolean;
      readonly threshold: number;
    };

export type SessionInput =
  | {
      readonly kind: "down";
      readonly pointerId: number;
      readonly button: number;
      readonly isPrimary: boolean;
      readonly pointerType: string;
      readonly at: Pt;
    }
  | { readonly kind: "move" | "up"; readonly pointerId: number; readonly at: Pt }
  /** pointercancel / lostpointercapture carry an id; blur and unmount don't (`null`). */
  | { readonly kind: "cancel"; readonly pointerId: number | null };

export type SessionEffect = "begin" | "move" | "release" | "cancel" | "ignore";

export interface SessionStep {
  readonly state: SessionState;
  readonly effect: SessionEffect;
  /** Whether the session had moved past its threshold (meaningful for move/release). */
  readonly moved: boolean;
}

export const IDLE: SessionState = { kind: "idle" };

export function step(state: SessionState, input: SessionInput): SessionStep {
  const ignore: SessionStep = {
    state,
    effect: "ignore",
    moved: state.kind === "active" && state.moved,
  };
  if (state.kind === "idle") {
    if (input.kind !== "down" || input.button !== 0 || !input.isPrimary) return ignore;
    return {
      state: {
        kind: "active",
        pointerId: input.pointerId,
        start: input.at,
        moved: false,
        threshold: thresholdFor(input.pointerType),
      },
      effect: "begin",
      moved: false,
    };
  }
  switch (input.kind) {
    case "down":
      return ignore;
    case "move": {
      if (input.pointerId !== state.pointerId) return ignore;
      const moved =
        state.moved ||
        Math.hypot(input.at.x - state.start.x, input.at.y - state.start.y) > state.threshold;
      return { state: moved === state.moved ? state : { ...state, moved }, effect: "move", moved };
    }
    case "up":
      if (input.pointerId !== state.pointerId) return ignore;
      return { state: IDLE, effect: "release", moved: state.moved };
    case "cancel":
      if (input.pointerId !== null && input.pointerId !== state.pointerId) return ignore;
      return { state: IDLE, effect: "cancel", moved: state.moved };
  }
}

// --- DOM driver ---------------------------------------------------------------------------------------

export interface SessionHandlers {
  /** A move of the owning pointer; `moved` is whether it has passed the click threshold. */
  move?: (e: PointerEvent, moved: boolean) => void;
  /** Normal release of the owning pointer. Ownership and capture are already cleared. */
  release: (e: PointerEvent, moved: boolean) => void;
  /** Cancelled (pointercancel, lost capture, blur, unmount) — must not commit. */
  cancel: () => void;
}

export interface PointerSession {
  /** End the session as a cancel (e.g. the stage unmounts mid-gesture). */
  cancel(): void;
}

const active = new WeakMap<Element, PointerSession>();

/** Is a gesture already running on this stage root (so another pointer's press must be ignored)? */
export const sessionActive = (root: Element): boolean => active.has(root);

/** The session running on this root, if any (the stage cancels it on unmount). */
export const activeSession = (root: Element): PointerSession | undefined => active.get(root);

const at = (e: PointerEvent): Pt => ({ x: e.clientX, y: e.clientY });

/** Begin a session for `down` on `root`, capturing the pointer there (move/up/cancel are read on window). Returns null (and does nothing)
 *  when the press isn't a primary pointer's primary button or a session is already running. */
export function beginPointerSession(
  root: Element,
  down: PointerEvent,
  handlers: SessionHandlers,
): PointerSession | null {
  if (active.has(root)) return null;
  let state = IDLE;
  const first = step(state, {
    kind: "down",
    pointerId: down.pointerId,
    button: down.button,
    isPrimary: down.isPrimary,
    pointerType: down.pointerType,
    at: at(down),
  });
  if (first.effect !== "begin") return null;
  state = first.state;

  const finish = (): void => {
    // Clear ownership first, then release capture: the lostpointercapture this fires finds no listener.
    active.delete(root);
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onCancelEvent);
    root.removeEventListener("lostpointercapture", onCancelEvent as EventListener);
    window.removeEventListener("blur", onBlur);
    try {
      if (root.hasPointerCapture(down.pointerId)) root.releasePointerCapture(down.pointerId);
    } catch {
      // The pointer is already gone; nothing to release.
    }
  };
  const onMove = (e: PointerEvent): void => {
    const next = step(state, { kind: "move", pointerId: e.pointerId, at: at(e) });
    state = next.state;
    if (next.effect === "move") handlers.move?.(e, next.moved);
  };
  const onUp = (e: PointerEvent): void => {
    const next = step(state, { kind: "up", pointerId: e.pointerId, at: at(e) });
    state = next.state;
    if (next.effect !== "release") return;
    finish();
    handlers.release(e, next.moved);
  };
  const cancelWith = (pointerId: number | null): void => {
    const next = step(state, { kind: "cancel", pointerId });
    state = next.state;
    if (next.effect !== "cancel") return;
    finish();
    handlers.cancel();
  };
  const onCancelEvent = (e: PointerEvent): void => cancelWith(e.pointerId);
  const onBlur = (): void => cancelWith(null);

  const session: PointerSession = { cancel: () => cancelWith(null) };
  active.set(root, session);
  try {
    root.setPointerCapture(down.pointerId);
  } catch {
    // Synthetic or already-released pointer: the session still tracks root-level events.
  }
  // Move/up/cancel on window: captured events bubble there, and a release outside the stage (or one with
  // no final move) still ends the session. Lost capture is reported on the capturing element itself.
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onCancelEvent);
  root.addEventListener("lostpointercapture", onCancelEvent as EventListener);
  window.addEventListener("blur", onBlur);
  return session;
}
