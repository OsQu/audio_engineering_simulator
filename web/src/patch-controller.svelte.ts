// The patch controller: the pointer/drag interaction machinery for patching cables, factored beside the
// SceneSession so a second view root (the 6.2 workbench) drives the identical glue instead of
// re-implementing the pointer bookkeeping. A thin reactive shell — all decision logic stays in the pure,
// node-tested modules (patching.ts transitions, scene-ops.ts edits, jack-anchors.ts measurement); this
// class holds the in-flight drag state + the measured anchor store and adapts DOM pointer events to the
// pure transitions. Bound to a SceneSession, whose scene/catalog/cables it reads and whose engine it
// hot-swaps after each committing edit. The layout-dependent *measurement trigger* stays view-side (an
// `$effect` with the scene-view dep list); the controller just exposes `measure(worldApi)` + the store.

import type { JackAnchor } from "./cable-view";
import type { ConnectVerdict } from "./connections";
import { measureJacks as measureJacksDom } from "./jack-anchors";
import type { JackHit, PatchState } from "./patching";
import * as patching from "./patching";
import type { Connection } from "./scene";
import * as sceneOps from "./scene-ops";
import type { SceneSession } from "./session.svelte";
import type { WorldApi } from "./world-api";

export class PatchController {
  #session: SceneSession;

  // Measured jack-connector centres in surface space, keyed "device:direction:port" (from each jack's
  // `data-jack` attribute), each tagged with the chassis face it sits on. Populated by `measure` after
  // layout; lets a cable anchor at the real socket when its jack is on the shown face, falling back to a
  // chassis-centre estimate otherwise.
  jackAnchors = $state<Record<string, JackAnchor>>({});

  // An in-progress patch from a source jack: the source end + its anchor, the moving free end (surface
  // coords), and — when hovering a candidate jack — the verdict so we can colour the cable + commit.
  // `mode` is "drag" while the pointer is held (same-view patching), or "pending" after a *click* — a
  // held cable that survives a wall/room switch, so you can complete a **cross-view** patch by clicking
  // the source jack, turning to the other wall/room, and clicking the destination jack.
  dragCable = $state<PatchState>(null);
  // Pointer-down bookkeeping to tell a click (→ pending pick) from a drag (moved past a small threshold).
  #cableDown = { x: 0, y: 0, moved: false };

  constructor(session: SceneSession) {
    this.#session = session;
  }

  // Re-measure jack anchors into surface space (the DOM work lives in jack-anchors.ts). The view root's
  // `$effect` schedules this on layout changes; the controller just owns the resulting store.
  measure(worldApi: WorldApi | undefined): void {
    if (worldApi) this.jackAnchors = measureJacksDom(worldApi);
  }

  // Resolve a `data-jack` key into the JackHit the pure transitions need (endpoint + measured anchor),
  // or null if it doesn't name a real, resolvable port.
  #jackHitOf(key: string | undefined | null): JackHit | null {
    if (!key) return null;
    const endpoint = patching.endpointFromJackKey(this.#session.scene, this.#session.catalog, key);
    return endpoint ? { key, endpoint, anchor: this.jackAnchors[key] ?? null } : null;
  }

  #patchDeps(): { connections: Connection[] } {
    return { connections: this.#session.scene.patch.connections };
  }

  // The patching handlers are thin adapters: they read the DOM into a JackHit + surface coords, keep the
  // click-vs-drag threshold bookkeeping, call the pure transition, assign its state, and on a returned
  // `commit` verdict commit the cable (which hot-swaps). See patching.ts for the transition semantics.
  pointerDown(e: PointerEvent, worldApi: WorldApi | undefined): void {
    if (!worldApi) return;
    // A pending cable's second press: only record the point; pointerUp resolves click-vs-pan.
    if (this.dragCable?.mode === "pending") {
      this.#cableDown = { x: e.clientX, y: e.clientY, moved: false };
      return;
    }
    const hit = this.#jackHitOf(
      (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-jack]")?.dataset.jack,
    );
    if (!hit?.anchor) return; // only a measured jack can start a drag
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
    const srcAnchor = this.jackAnchors[patching.jackKeyOf(this.dragCable.source)] ?? null;
    const hit = this.#jackHitOf(
      document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-jack]")?.dataset
        .jack,
    );
    this.dragCable = patching.pointerMove(
      this.dragCable,
      hit,
      cursor,
      srcAnchor,
      this.#patchDeps(),
    );
  }

  pointerUp(e: PointerEvent): void {
    if (!this.dragCable) return;
    // The pending second-click needs the jack under the release point; drag-release uses the last verdict.
    const hit =
      this.dragCable.mode === "pending" && !this.#cableDown.moved
        ? this.#jackHitOf(
            document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-jack]")
              ?.dataset.jack,
          )
        : null;
    const res = patching.pointerUp(this.dragCable, hit, !this.#cableDown.moved, this.#patchDeps());
    if (res.commit) this.commitCable(res.commit);
    this.dragCable = res.state;
  }

  // Cancel an in-progress patch (drag or pending) — Esc, or the cross-view banner's Cancel button.
  cancel(): void {
    this.dragCable = patching.cancel();
  }

  // Commit a hovered/clicked connection verdict into the scene, then hot-swap the engine.
  commitCable(v: ConnectVerdict): void {
    sceneOps.commitCable(this.#session.scene, this.#session.catalog, this.#session.cables, v);
    this.#session.hotSwap();
  }

  // Remove a connection, then hot-swap. The view wraps this to also clear its cable-inspector selection.
  disconnect(c: Connection): void {
    sceneOps.disconnect(this.#session.scene, c);
    this.#session.hotSwap();
  }

  // Set (or clear, `""` ⇒ ideal wire) the cable type on a connection, then hot-swap — the cable's R·C is
  // baked into the edge at compile, so changing it rebuilds the engine.
  setCableType(c: Connection, typeId: string): void {
    sceneOps.setCableType(this.#session.scene, this.#session.cables, c, typeId);
    this.#session.hotSwap();
  }
}
