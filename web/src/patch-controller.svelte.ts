// The patch controller: the pointer/drag interaction machinery for patching cables, factored beside the
// SceneSession so a second view root (the 6.2 workbench) drives the identical glue instead of
// re-implementing the pointer bookkeeping. A thin reactive shell — all decision logic stays in the pure,
// node-tested modules (patching.ts transitions, scene-ops.ts edits, socket-geometry.ts registration and
// projection); this class holds the in-flight drag state + the socket-geometry index and adapts DOM
// pointer events to the pure transitions. Presses reach it only through a stage arbiter
// (stage-gestures.ts, Story 10.7), which hands it socket presses (`begin`); the view root binds the
// stage it picks sockets on (`bindStage`) and feeds a pending pick's cursor-follow (`hover`). Bound to a SceneSession, whose scene/catalog/cables it reads and
// whose engine it hot-swaps after each committing edit. Each face slot registers its own geometry through
// the `GeometryRegistrar` methods.

import type { CableType } from "./catalog";
import type { ConnectVerdict } from "./connections";
import type { GeometryRegistrar } from "./face-geometry";
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
  type RegisterOutcome,
  registerGeometry,
  requestGeometry,
  unregisterGeometry,
} from "./socket-geometry";
import { pointerKind } from "./stage-gestures";

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
  // An in-progress patch from a source jack: the logical source end, the moving free end (surface
  // coords), and — when hovering a candidate jack — the verdict so we can colour the cable + commit.
  // `mode` is "drag" while the pointer is held (same-view patching), or "pending" after a *click* — a
  // held cable that survives a wall/room switch, so you can complete a **cross-view** patch by clicking
  // the source jack, turning to the other wall/room, and clicking the destination jack.
  dragCable = $state<PatchState>(null);
  // The stage the arbiter-driven gesture picks on, and the session of the gesture in flight.
  #stage: PatchStage | null = null;
  #gesture: PointerSession | null = null;

  constructor(session: SceneSession) {
    this.#session = session;
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

  // A picked physical socket as the JackHit the pure transitions need: the alias that complements the
  // held source (duplex), its Endpoint, and its projected centre as the anchor.
  #hitOfTarget(target: SocketTarget): JackHit | null {
    const key = patching.aliasFor(target.aliases, this.dragCable?.source ?? null);
    if (!key) return null;
    const endpoint = patching.endpointFromJackKey(this.#session.descriptorOf, key);
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

  // Cancel an in-progress patch (drag or pending) — Esc, or the cross-view banner's Cancel button.
  cancel(): void {
    this.#gesture?.cancel();
    this.dragCable = patching.cancel();
  }

  // The Rust compiler decides whether a cable is schedulable; its edit is applied only after acceptance.
  commitCable(v: ConnectVerdict): void {
    if (!v.ok) return;
    this.#session.proposeTopology("connect cable", (scene) => {
      sceneOps.commitCable(scene, this.#session.descriptorOf, this.#session.cables, v);
      sceneOps.enumerateComputerUsb(scene, this.#session.descriptorOf, v.connection);
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
      sceneOps.enumerateComputerUsb(scene, this.#session.descriptorOf, connection);
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
