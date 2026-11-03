// The patch controller: the pointer/drag interaction machinery for patching cables, factored beside the
// SceneSession so a second view root (the 6.2 workbench) drives the identical glue instead of
// re-implementing the pointer bookkeeping. A thin reactive shell — all decision logic stays in the pure,
// node-tested modules (patching.ts transitions, scene-ops.ts edits, socket-geometry.ts registration and
// projection); this class holds the in-flight drag state + the socket-geometry index and adapts DOM
// pointer events to the pure transitions. Bound to a SceneSession, whose scene/catalog/cables it reads and
// whose engine it hot-swaps after each committing edit. Each face slot registers its own geometry through
// the `GeometryRegistrar` methods; the view root binds the projection (`project`) of its slots' frames.

import type { JackAnchor } from "./cable-view";
import type { CableType } from "./catalog";
import type { ConnectVerdict } from "./connections";
import type { GeometryRegistrar } from "./face-geometry";
import { patchableJackKey } from "./jack-anchors";
import type { PointerKind, SocketPick, SocketTarget } from "./patch-targets";
import type { JackHit, PatchState } from "./patching";
import * as patching from "./patching";
import { beginPointerSession, type PointerSession } from "./pointer-session";
import type { Connection } from "./scene";
import * as sceneOps from "./scene-ops";
import type { SceneSession } from "./session.svelte";
import {
  createSocketGeometryIndex,
  type FaceGeometry,
  type GeometrySlot,
  type ProjectedSlot,
  projectAnchors,
  type RegisterOutcome,
  registerGeometry,
  requestGeometry,
  unregisterGeometry,
} from "./socket-geometry";
import { pointerKind } from "./stage-gestures";
import type { WorldApi } from "./world-api";

/** What a view root binds so the controller can pick sockets and place the free end (Story 10.7). */
export interface PatchStage {
  clientToSurface: (clientX: number, clientY: number) => { x: number; y: number };
  /** The socket a client point means on this stage right now (its targets + client frame). */
  pick: (client: { x: number; y: number }, pointerType: PointerKind) => SocketPick;
}

export class PatchController implements GeometryRegistrar {
  #session: SceneSession;

  // Face-local socket geometry (Story 10.4), registered per (face instance, side) by each face slot's
  // `faceGeometry` action at its current revision. Camera and placement never touch it.
  geometry = $state.raw(createSocketGeometryIndex());
  // The view root's current projection of its registered slots onto the stage surface (frames + which
  // side faces the operator). Pure inputs; moving a device or the camera only changes this.
  #slots = $state.raw<() => readonly ProjectedSlot[]>(() => []);

  // Socket centres in surface space, keyed "device:direction:port" (every endpoint alias of each socket),
  // each tagged with the side it sits on — the contract cable routing and patching consume. Derived from
  // the geometry index × the projected slots; lets a cable anchor at the real socket when its side is
  // shown, falling back to a chassis-centre estimate otherwise.
  jackAnchors = $derived<Record<string, JackAnchor>>(projectAnchors(this.geometry, this.#slots()));

  // An in-progress patch from a source jack: the logical source end, the moving free end (surface
  // coords), and — when hovering a candidate jack — the verdict so we can colour the cable + commit.
  // `mode` is "drag" while the pointer is held (same-view patching), or "pending" after a *click* — a
  // held cable that survives a wall/room switch, so you can complete a **cross-view** patch by clicking
  // the source jack, turning to the other wall/room, and clicking the destination jack.
  dragCable = $state<PatchState>(null);
  // Pointer-down bookkeeping to tell a click (→ pending pick) from a drag — legacy window path only
  // (the workbench until it adopts the stage arbiter); the studio uses the shared pointer session.
  #cableDown = { x: 0, y: 0, moved: false };
  // The stage the arbiter-driven gesture picks on, and the session of the gesture in flight.
  #stage: PatchStage | null = null;
  #gesture: PointerSession | null = null;

  constructor(session: SceneSession) {
    this.#session = session;
  }

  /** Bind the view root's slot projection (read reactively by `jackAnchors`). */
  project(slots: () => readonly ProjectedSlot[]): void {
    this.#slots = slots;
  }

  // GeometryRegistrar — the face slots' `faceGeometry` action drives these.
  request(slot: GeometrySlot, revision: string): number | null {
    const req = requestGeometry(this.geometry, slot, revision);
    if (req.index !== this.geometry) this.geometry = req.index;
    return req.measure ? req.generation : null;
  }
  register(slot: GeometrySlot, generation: number, geometry: FaceGeometry): RegisterOutcome {
    const res = registerGeometry(this.geometry, slot, generation, geometry);
    this.geometry = res.index;
    return res.outcome;
  }
  unregister(slot: GeometrySlot): void {
    this.geometry = unregisterGeometry(this.geometry, slot);
  }

  // Resolve a `data-jack` key into the JackHit the pure transitions need (endpoint + measured anchor),
  // or null if it doesn't name a real, resolvable port.
  #jackHitOf(key: string | undefined | null): JackHit | null {
    if (!key) return null;
    const endpoint = patching.endpointFromJackKey(this.#session.scene, this.#session.catalog, key);
    return endpoint ? { key, endpoint, anchor: this.jackAnchors[key] ?? null } : null;
  }

  // A picked physical socket as the JackHit the pure transitions need: the alias that complements the
  // held source (duplex), its Endpoint, and its projected centre as the anchor.
  #hitOfTarget(target: SocketTarget): JackHit | null {
    const key = patching.aliasFor(target.aliases, this.dragCable?.source ?? null);
    if (!key) return null;
    const endpoint = patching.endpointFromJackKey(this.#session.scene, this.#session.catalog, key);
    return endpoint ? { key, endpoint, anchor: target.point, aliases: [...target.aliases] } : null;
  }

  #hitAt(e: PointerEvent): JackHit | null {
    const pick = this.#stage?.pick({ x: e.clientX, y: e.clientY }, pointerKind(e));
    return pick && pick.kind !== "none" ? this.#hitOfTarget(pick.target) : null;
  }

  #patchDeps(): { connections: Connection[]; cables: CableType[] } {
    return { connections: this.#session.scene.patch.connections, cables: this.#session.cables };
  }

  /** Bind the stage the arbiter-driven gestures pick on (the view root's targets + client frame). */
  bindStage(stage: PatchStage): void {
    this.#stage = stage;
  }

  /** A press the stage arbiter assigned to a socket (Story 10.7). A fresh press starts a drag from it; a
   *  press while a pick is pending resolves on release. Either way the gesture runs on the shared pointer
   *  session: release re-picks its own target, a cancel never commits. */
  begin(e: PointerEvent, pick: { target: SocketTarget }, root: HTMLElement): void {
    if (this.dragCable?.mode !== "pending") {
      const hit = this.#hitOfTarget(pick.target);
      const next = patching.pointerDown(null, hit).state;
      if (!next) return;
      this.dragCable = next;
    }
    this.#gesture = beginPointerSession(root, e, {
      move: (ev) => this.#track(ev),
      release: (ev, moved) => {
        this.#gesture = null;
        const res = patching.pointerUp(this.dragCable, this.#hitAt(ev), !moved, this.#patchDeps());
        if (res.commit) this.commitCable(res.commit);
        this.dragCable = res.state;
      },
      cancel: () => {
        this.#gesture = null;
        this.dragCable = patching.pointerCancel(this.dragCable);
      },
    });
    // No session (not a primary press, or one already running): never leave a drag without a release.
    if (!this.#gesture) this.dragCable = patching.pointerCancel(this.dragCable);
  }

  /** Window pointermove: a pending pick's free end follows the cursor (buttonless hover). An owned
   *  gesture is driven by its session instead. */
  hover(e: PointerEvent): void {
    if (this.#gesture || !this.dragCable || !this.#stage) return;
    this.#track(e);
  }

  #track(e: PointerEvent): void {
    if (!this.dragCable || !this.#stage) return;
    const cursor = this.#stage.clientToSurface(e.clientX, e.clientY);
    this.dragCable = patching.pointerMove(
      this.dragCable,
      this.#hitAt(e),
      cursor,
      this.#patchDeps(),
    );
  }

  // LEGACY (deleted in Task 10.7.4, once the workbench adopts the stage arbiter): window handlers that
  // resolve exact DOM jack hits. The patching handlers are thin adapters: they read the DOM into a JackHit + surface coords, keep the
  // click-vs-drag threshold bookkeeping, call the pure transition, assign its state, and on a returned
  // `commit` verdict commit the cable (which hot-swaps). See patching.ts for the transition semantics.
  pointerDown(e: PointerEvent, worldApi: WorldApi | undefined): void {
    if (!worldApi) return;
    // A pending cable's second press: only record the point; pointerUp resolves click-vs-pan.
    if (this.dragCable?.mode === "pending") {
      this.#cableDown = { x: e.clientX, y: e.clientY, moved: false };
      return;
    }
    const hit = this.#jackHitOf(patchableJackKey(e.target as Element | null));
    if (!hit?.anchor) return; // only a jack with current geometry can start a drag
    e.preventDefault();
    this.#cableDown = { x: e.clientX, y: e.clientY, moved: false };
    this.dragCable = patching.pointerDown(this.dragCable, hit).state;
  }

  pointerMove(e: PointerEvent, worldApi: WorldApi | undefined): void {
    if (!this.dragCable || !worldApi) return;
    // Track whether the active press has moved past the click threshold — but only while a button is
    // held (`e.buttons`), so a pending cable's buttonless cursor-follow is never mistaken for a pan.
    if (e.buttons !== 0 && !this.#cableDown.moved) {
      if (Math.hypot(e.clientX - this.#cableDown.x, e.clientY - this.#cableDown.y) > 4) {
        this.#cableDown.moved = true;
      }
    }
    const cursor = worldApi.clientToSurface(e.clientX, e.clientY);
    const hit = this.#jackHitOf(patchableJackKey(document.elementFromPoint(e.clientX, e.clientY)));
    this.dragCable = patching.pointerMove(this.dragCable, hit, cursor, this.#patchDeps());
  }

  pointerUp(e: PointerEvent): void {
    if (!this.dragCable) return;
    // Release re-resolves the jack under the release point in both modes (Story 10.7).
    const hit = this.#jackHitOf(patchableJackKey(document.elementFromPoint(e.clientX, e.clientY)));
    const res = patching.pointerUp(this.dragCable, hit, !this.#cableDown.moved, this.#patchDeps());
    if (res.commit) this.commitCable(res.commit);
    this.dragCable = res.state;
  }

  // Cancel an in-progress patch (drag or pending) — Esc, or the cross-view banner's Cancel button.
  cancel(): void {
    this.#gesture?.cancel();
    this.dragCable = patching.cancel();
  }

  // The Rust compiler decides whether a cable is schedulable; its edit is applied only after acceptance.
  commitCable(v: ConnectVerdict): void {
    if (!v.ok) return;
    this.#session.proposeTopology("connect cable", (scene) => {
      sceneOps.commitCable(scene, this.#session.catalog, this.#session.cables, v);
      sceneOps.enumerateComputerUsb(scene, this.#session.catalog, v.connection);
    });
  }

  // Remove a connection, then hot-swap. Unplugging a USB link from a computer re-enumerates it back to
  // its built-in 2×2. The view wraps this to also clear its cable-inspector selection.
  disconnect(c: Connection): void {
    const key = sceneOps.connKey(c);
    this.#session.proposeTopology("disconnect cable", (scene) => {
      const connection = scene.patch.connections.find((x) => sceneOps.connKey(x) === key);
      if (!connection) return;
      sceneOps.disconnect(scene, connection);
      sceneOps.enumerateComputerUsb(scene, this.#session.catalog, connection);
    });
  }

  // Set (or clear, `""` ⇒ ideal wire) the cable type on a connection, then hot-swap — the cable's R·C is
  // baked into the edge at compile, so changing it rebuilds the engine.
  setCableType(c: Connection, typeId: string): void {
    const key = sceneOps.connKey(c);
    this.#session.proposeTopology("change cable type", (scene) => {
      const connection = scene.patch.connections.find((x) => sceneOps.connKey(x) === key);
      if (connection) sceneOps.setCableType(scene, this.#session.cables, connection, typeId);
    });
  }
}
