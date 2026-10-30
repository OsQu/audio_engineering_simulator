// The face-geometry DOM adapter (Story 10.4): measures one face slot's sockets into face-local mm and
// registers them in the pure socket-geometry index. It replaces the old global `[data-jack]` sweep, which
// read every jack through the camera transform on a rAF + fixed settle timer and so coupled socket truth
// to pan/zoom/placement/animation state.
//
// Measurement is a *ratio*: each jack's client-rect offset from the slot frame's client rect, scaled by
// the frame's layout size over its client size. Any uniform camera scale and every translation (pan,
// placement, bench offset, rack move) divides out, so a registration survives them untouched. A face is
// only measured while it is settled (`presented`, never mid-flip, which would foreshorten it); the side
// turned away inside a flip card measures mirrored and is un-mirrored here, so both sides are stored as
// seen when presented. A new measurement is requested only when the slot's revision changes: descriptor/
// config, skin, the frame's layout size (ResizeObserver — layout boxes ignore transforms), or fonts
// finishing loading.

import type { DeviceFacing } from "./scene-store";
import type {
  FaceGeometry,
  GeometrySlot,
  RegisterOutcome,
  SocketShape,
} from "./socket-geometry";
import { geometryRevision } from "./socket-geometry";

/** Measure one side of a slot frame. `presented` is the side currently turned toward the operator. */
export function measureFace(
  frame: HTMLElement,
  side: DeviceFacing,
  presented: boolean,
  homeOf?: (box: DOMRect) => { x: number; y: number },
): FaceGeometry {
  const box = frame.getBoundingClientRect();
  const width = frame.offsetWidth;
  const height = frame.offsetHeight;
  if (!(box.width > 0 && box.height > 0 && width > 0 && height > 0)) {
    return { width: 0, height: 0, sockets: [] };
  }
  const kx = width / box.width;
  const ky = height / box.height;
  const sockets: SocketShape[] = [];
  for (const el of frame.querySelectorAll<HTMLElement>(`[data-face="${side}"] [data-jack]`)) {
    const id = el.dataset.jack;
    const r = el.getBoundingClientRect();
    if (!id || (r.width === 0 && r.height === 0)) continue; // not laid out on this side
    const local = (r.left + r.width / 2 - box.left) * kx;
    const alt = el.dataset.jackAlt;
    sockets.push({
      id,
      aliases: alt ? [id, alt] : [id],
      x: presented ? local : width - local,
      y: (r.top + r.height / 2 - box.top) * ky,
      r: (r.width / 2) * kx,
    });
  }
  return { width, height, sockets, ...(homeOf ? { home: homeOf(box) } : {}) };
}

/** What the registration adapter needs from its owner (the patch controller). */
export interface GeometryRegistrar {
  /** Ask for `revision`; returns a generation when a measurement is needed, else null. */
  request(slot: GeometrySlot, revision: string): number | null;
  register(slot: GeometrySlot, generation: number, geometry: FaceGeometry): RegisterOutcome;
  unregister(slot: GeometrySlot): void;
}

export interface FaceGeometryParams {
  registrar: GeometryRegistrar;
  face: string;
  /** The sides this slot lays out: both for a flip card, one for a DUT column. */
  sides: readonly DeviceFacing[];
  /** The side turned toward the operator (its DOM orientation, i.e. the durable facing). */
  presented: DeviceFacing;
  /** False while the face is mid-flip: a rotating face is never measured. */
  settled: boolean;
  /** Everything non-DOM that moves a socket within the face (descriptor/config, skin…). */
  content: readonly (string | number | boolean)[];
  /** The bench stack's un-offset origin; omitted on stages with a pure frame. */
  homeOf?: (box: DOMRect) => { x: number; y: number };
}

// Fonts arriving re-lay text and can move sockets; a load completion bumps every slot's revision.
let fontRevision = 0;
const fontListeners = new Set<() => void>();
if (typeof document !== "undefined" && document.fonts) {
  document.fonts.addEventListener("loadingdone", () => {
    fontRevision += 1;
    for (const listener of fontListeners) listener();
  });
}

/** Svelte action: keep a face slot's geometry registered at its current revision. */
export function faceGeometry(node: HTMLElement, initial: FaceGeometryParams) {
  let params = initial;
  let frame = 0;
  let registered: GeometrySlot[] = [];
  node.dataset.geometry = "pending";

  const sync = (): void => {
    cancelAnimationFrame(frame);
    const slots = params.sides.map((side) => ({ face: params.face, side }));
    for (const old of registered) {
      if (!slots.some((s) => s.face === old.face && s.side === old.side)) {
        params.registrar.unregister(old);
      }
    }
    registered = slots;
    const revision = geometryRevision([
      ...params.content,
      node.offsetWidth,
      node.offsetHeight,
      fontRevision,
    ]);
    const due = slots
      .map((slot) => ({ slot, generation: params.registrar.request(slot, revision) }))
      .filter((d): d is { slot: GeometrySlot; generation: number } => d.generation !== null);
    if (due.length === 0) {
      node.dataset.geometry = "current";
      return;
    }
    node.dataset.geometry = "pending";
    if (!params.settled) return; // measured once the flip settles (params update)
    // After layout: the generation was captured now, so a later revision makes this measurement stale.
    frame = requestAnimationFrame(() => {
      const outcomes = due.map(({ slot, generation }) =>
        params.registrar.register(
          slot,
          generation,
          measureFace(node, slot.side, slot.side === params.presented, params.homeOf),
        ),
      );
      // `empty`/`duplicate` leave the slot pending; the next resize or revision retries.
      if (outcomes.every((outcome) => outcome === "current")) node.dataset.geometry = "current";
    });
  };

  const resize = new ResizeObserver(sync);
  resize.observe(node);
  fontListeners.add(sync);
  sync();

  return {
    update(next: FaceGeometryParams): void {
      params = next;
      sync();
    },
    destroy(): void {
      cancelAnimationFrame(frame);
      resize.disconnect();
      fontListeners.delete(sync);
      for (const slot of registered) params.registrar.unregister(slot);
    },
  };
}
