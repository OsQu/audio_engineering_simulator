// The stage gesture arbiter: one capture-phase `pointerdown` listener per stage viewport
// classifies every press *before* any gesture adapter captures the pointer or mutates anything, then
// dispatches it to exactly one owner — control/chrome, socket (patching), visible cable (select), device
// move, or backdrop pan (gesture-ownership.ts decides). A control press propagates untouched to its own
// widget; every other press is consumed here (preventDefault + stopPropagation) so no competing
// bubble-phase start can also fire. While a gesture's pointer session runs, presses by other pointers are
// swallowed (multi-pointer).
//
// Also the DOM half of socket picking: `readStageFrame` reads the camera's client mapping, the viewport
// clip, and the UI blockers (`[data-pointer-blocker]` that currently take pointer hits — focus windows,
// open toolbars, banners) that `pickSocketTarget` consumes.

import { classifyPress, type Ownership } from "./gesture-ownership";
import type { ClientRect, PointerKind, SocketPick } from "./patch-targets";
import { activeSession, sessionActive } from "./pointer-session";

/** Elements whose press keeps its own gesture and blocks socket search behind it: native controls,
 *  ARIA sliders/switches (knobs, faders), portal chips, and anything marked as a pointer blocker. */
export const CONTROL_SELECTOR =
  'button, input, select, textarea, a, [role="slider"], [role="switch"], [data-portal], [data-pointer-blocker], [data-control]';

/** The client-side frame a stage is seen through right now: what picking needs besides the targets. */
export interface StageFrame {
  /** Client px per surface unit. */
  readonly zoom: number;
  /** Surface (0, 0) in client px. */
  readonly origin: { readonly x: number; readonly y: number };
  readonly viewport: ClientRect;
  readonly blockers: readonly ClientRect[];
}

const rectOf = (r: DOMRect): ClientRect => ({
  left: r.left,
  top: r.top,
  right: r.right,
  bottom: r.bottom,
});

/** Every `[data-pointer-blocker]` that currently takes pointer hits and has a box. */
export function collectBlockers(): ClientRect[] {
  const out: ClientRect[] = [];
  for (const el of document.querySelectorAll<HTMLElement>("[data-pointer-blocker]")) {
    const box = el.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0) continue;
    if (getComputedStyle(el).pointerEvents === "none") continue;
    out.push(rectOf(box));
  }
  return out;
}

/** Read a stage's frame: the transformed surface's client origin, the camera zoom, the viewport clip. */
export function readStageFrame(
  viewport: HTMLElement | null | undefined,
  surface: HTMLElement | null | undefined,
  zoom: number,
): StageFrame | null {
  if (!viewport || !surface) return null;
  const s = surface.getBoundingClientRect();
  return {
    zoom,
    origin: { x: s.left, y: s.top },
    viewport: rectOf(viewport.getBoundingClientRect()),
    blockers: collectBlockers(),
  };
}

export const pointerKind = (e: PointerEvent): PointerKind =>
  e.pointerType === "touch" ? "touch" : e.pointerType === "pen" ? "pen" : "mouse";

/** The owners a stage wires in. Each receives the press and the stage root its session captures on. */
export interface StageGestureParams {
  /** The socket a press means (the stage's targets + frame). Not called for control presses. */
  pick: (e: PointerEvent) => SocketPick;
  socket: (
    e: PointerEvent,
    pick: Extract<Ownership, { kind: "socket" }>["pick"],
    root: HTMLElement,
  ) => void;
  cable?: (e: PointerEvent, key: string, root: HTMLElement) => void;
  device?: (e: PointerEvent, objectId: string, root: HTMLElement) => void;
  pan: (e: PointerEvent, root: HTMLElement) => void;
}

const MISS: SocketPick = { kind: "none", reason: "miss" };

/** Svelte action: arbitrate every press on this stage viewport in the capture phase. */
export function stageGestures(node: HTMLElement, initial: StageGestureParams) {
  let p = initial;

  function onDown(e: PointerEvent): void {
    if (sessionActive(node)) {
      // Another pointer while a gesture runs: ignore it entirely (it must not start a second gesture).
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (e.button !== 0 || !e.isPrimary) return; // secondary buttons/pointers keep native behaviour
    const target = e.target instanceof Element ? e.target : null;
    const inControl = target?.closest(CONTROL_SELECTOR) != null;
    const own = classifyPress({
      inControl,
      socket: inControl ? MISS : p.pick(e),
      cableKey: target?.closest("[data-cable]")?.getAttribute("data-cable") ?? null,
      objectId: target?.closest("[data-stage-object]")?.getAttribute("data-stage-object") ?? null,
    });
    if (own.kind === "control") return;
    e.preventDefault();
    e.stopPropagation();
    if (own.kind === "socket") p.socket(e, own.pick, node);
    else if (own.kind === "cable") p.cable?.(e, own.key, node);
    else if (own.kind === "device") p.device?.(e, own.objectId, node);
    else p.pan(e, node);
  }

  node.addEventListener("pointerdown", onDown, { capture: true });
  return {
    update(next: StageGestureParams): void {
      p = next;
    },
    destroy(): void {
      node.removeEventListener("pointerdown", onDown, { capture: true });
      activeSession(node)?.cancel(); // unmount mid-gesture is a cancel, never a commit
    },
  };
}
