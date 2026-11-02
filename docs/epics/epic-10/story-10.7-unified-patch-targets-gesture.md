# Story 10.7 — Unified patch targets & gesture lifecycle — program design

> Companion to the Story 10.7 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Landed baseline (gap pass, 2026-11-02, against the AES-69 / 10.6 tip `0003465`)

What the code does today, which this Story changes:

- **Targets are DOM-exact.** `PatchController` resolves a socket only when the event target, or
  `document.elementFromPoint`, lands on the small `[data-jack]` span (`patchableJackKey`). There is no
  screen-space tolerance. At the minimum zoom a physical socket is a few pixels across.
- **Gesture starts compete.** Four independent `pointerdown` starts exist, and DOM order decides between
  them:
  - `draggable` on each device body, which excludes only exact `[data-jack]`/control hits;
  - `camera.startPan` on the studio backdrop and the bench viewport;
  - the patch controller on `svelte:window` (App and Workbench);
  - `onclick` on each `.cable-hit` path, plus the portal chip's own drag.

  A near-miss press lands on the panel and starts a device move. This is known regression
  `near-socket-press-starts-device-drag`.
- **Drag release replays a cached verdict.** `patching.pointerUp` commits `state.verdict` from the last
  `pointermove`. Only the pending second click hit-tests the release point. This is known regression
  `drag-release-target-differs-from-click-target`.
- **No pointer lifecycle on patch gestures.** There is no primary-button guard, no pointer ID, no capture,
  and no `pointercancel` / `lostpointercapture` / blur handling. `draggable` does split `onCancel` from
  `onEnd`, but it never handles `lostpointercapture` or blur. `camera.startPan` treats cancel as up, which is
  harmless for a pan.
- **Illegal reasons are invisible.** `ConnectVerdict.reason` exists (`connections.ts`), but nothing renders
  it. An illegal drag release silently drops the lead.
- **Focus windows are already non-patchable.** Their faces carry `data-interaction="disabled"`. That is
  the right policy, but `elementFromPoint` still searches the whole window, and a window is no blocker for
  any geometric search.
- **Already in place and reused:**
  - `CableResolutionInput`, which carries preview-aware silhouettes with total order, projected slots,
    the geometry index, membership and U1 suppression;
  - `resolveEndpoint` and `socketFor`/`projectSocket`, plus `SocketShape.r` (face-local radius) and
    `aliases` (one duplex socket has many endpoint keys);
  - `silhouetteContains`, `evaluateConnection`, and the `Camera` (`zoom`, `panX`, `panY`).

## Settled decisions (approved by Oskari as-is, I-222, 2026-11-02)

1. **Targets are derived from the cable-resolution input.** `CableResolutionInput` already holds exactly
   what socket eligibility needs, so no new resolver input is introduced. A socket is a target iff:
   - its owner is `local`, not suppressed, and its face is presented;
   - its geometry is `current`;
   - its centre is **exposed**: no silhouette ordered above the owner contains it.

   Hidden-side sockets (exits), off-view, pending, mid-flip and top-view sockets therefore produce no
   targets. One physical socket gives one target that carries every alias. The target's surface radius is
   `r × frameScale`.
2. **Picking happens in client pixels after the camera.** A stage adapter supplies the camera transform
   (`zoom`, surface client origin), the viewport client rect and the blocker client rects. Tolerances are
   named constants: `max(8 px, rendered r)` for mouse/pen and `max(12 px, rendered r)` for touch. The
   order is:
   1. Pointer outside the viewport, or inside a blocker → none.
   2. Each target's tolerance disc is clipped:
      - centre outside the viewport or inside a blocker → ineligible;
      - pointer's surface point inside a silhouette ordered above the owner → ineligible for this press,
        which is the partial-exposure clip.
   3. **Exact physical hit** (pointer within rendered r) wins. Among several, the higher paint order and
      then the stable socket ID decide.
   4. Otherwise the **nearest centre** within tolerance wins, with ties broken by paint order and then
      socket ID.

   The picker is legality-blind. The caller evaluates the nearest socket's verdict and reports it, so an
   illegal nearest socket is **never skipped** for a farther legal one.
3. **One capture-phase arbiter per stage, with direct dispatch.** A `stageGestures` action sits on each
   stage viewport (studio `WorldView`, bench `BenchStage`) and listens in the capture phase. It classifies
   the press synchronously, before any adapter can capture or mutate anything. The priority is:
   1. **control/chrome:** DOM target inside a control, toolbar, focus window, banner or portal chip;
   2. **socket:** a picked target, or any press while a pick is pending;
   3. **visible cable:** a `[data-cable]` target;
   4. **device move:** `[data-stage-object]`;
   5. **pan:** anything else.

   A control press is left to propagate untouched. For every other kind the arbiter calls the owning
   adapter (`patch.begin`, `beginMove`, `camera.beginPan`, or cable select) and stops propagation. The
   adapters lose their own `pointerdown` starts:
   - `draggable` becomes a move adapter, no longer an action listening for presses;
   - the backdrop `onpointerdown` and the window-level `patch.pointerDown` are deleted;
   - the cable `onclick` becomes a release-without-move select. Keyboard select stays.

   *Rejected:* an ownership marker that each adapter reads in its own bubble listener. That keeps four
   competing listeners alive and makes the ordering implicit again.
4. **One shared pointer session.** `pointer-session.ts` tracks one primary pointer per stage:
   - it applies the pointer ID, primary-button and `isPrimary` guards, and the move threshold (4 px
     mouse/pen, 8 px touch);
   - it captures the pointer on the stage root;
   - it reports outcomes `release | cancel`, where cancel covers `pointercancel`, `lostpointercapture`,
     window blur and unmount;
   - presses from other pointers are ignored while a session is active;
   - after a normal release it clears ownership **before** releasing capture, so the resulting
     `lostpointercapture` cannot undo a committed gesture.

   Device, patch and pan all run on it. A device cancel discards the preview and never calls the commit
   path.
5. **Release always re-resolves.** `patching.pointerUp` takes the release-point pick for both modes and
   recomputes legality. It never reads `state.verdict` to commit. A move event only drives the
   magnetic/colour feedback.
6. **Illegal targets keep the pick.** A drag released on an illegal socket becomes a
   **pending** pick that carries the rejection reason. This is the same outcome as click-click on that
   socket, so the two modes return identical verdicts. The reason renders beside the held lead's free end
   and in the pick banner, and clears on the next target or move. A real drag released over no socket
   still drops the lead, as today.
7. **Cancellation semantics.** A cancelled patch *drag* drops the lead to the null state. A pending pick
   survives the cancel of any other gesture (pan, device move) and survives toolbar or view navigation.
   Esc and the banner's Cancel button remain the explicit ways to drop it.
8. **Focus-window policy: not patchable, and windows block.** This keeps the existing explicit policy:
   - focus-window faces produce no targets;
   - each focus window registers as a blocker, so the stage's geometric search never reaches through it.

   *Rejected:* patching through focus windows, which would need a coordinate mapping to the stage
   sockets, and no user has asked for it.

## Task 10.7.1 — Pure socket targets & picker  · done

This task adds one pure module and wires nothing yet.

File-tree diff:

```text
web/src/
+ patch-targets.ts        resolveSocketTargets, pickSocketTarget, tolerance constants
web/test/
+ patch-targets.test.ts   eligibility matrix, exposure, exact-vs-nearest, ties, zoom, blockers, duplex
```

Key types & signatures:

```ts
/** One physical, patchable socket in surface coords (one per socket, not per alias). */
export interface SocketTarget {
  readonly id: string;                 // SocketShape.id — stable tie-breaker
  readonly aliases: readonly string[]; // every endpoint key ("device:direction:port")
  readonly objectId: string;
  readonly point: SurfacePoint;        // presented-side centre
  readonly radius: number;             // surface units: r × frame scale
  readonly order: number;              // owner's paint order (−1 if no silhouette, e.g. the bench DUT)
}

/** Only presented, current, exposed sockets of local, unsuppressed owners. Deterministic order. */
export function resolveSocketTargets(input: CableResolutionInput): SocketTarget[]

export const MOUSE_TOLERANCE_PX = 8;
export const TOUCH_TOLERANCE_PX = 12;

export interface PickContext {
  readonly zoom: number;                      // client px per surface unit
  readonly origin: { x: number; y: number };  // surface (0,0) in client px
  readonly viewport: ClientRect;              // clip
  readonly blockers: readonly ClientRect[];   // focus windows, toolbars, banners
  readonly silhouettes: ReadonlyMap<string, SurfaceSilhouette>; // partial-exposure clip
  readonly pointerType: "mouse" | "pen" | "touch";
}

export type SocketPick =
  | { readonly kind: "exact" | "near"; readonly target: SocketTarget }
  | { readonly kind: "none"; readonly reason: "outside" | "blocked" | "miss" };

export function pickSocketTarget(
  targets: readonly SocketTarget[], client: { x: number; y: number }, ctx: PickContext,
): SocketPick
```

The tests are hand-calculated, table-driven matrices:

- **Eligibility** for presented, hidden, transitioning, pending, off-view, top-view and focus faces. The
  focus case is absent by construction, because focus faces are never in the stage's slots.
- **Exposure:**
  - a front device over a rear socket;
  - a later front over an earlier front;
  - partial cover with the centre exposed — eligible, but a pointer inside the cover misses it.
- **Picking:**
  - exact beats a nearer centre that is not exact;
  - nearest centre wins, with paint order and then ID breaking ties;
  - one target carries both duplex aliases;
  - the tolerance stays 8/12 px across minimum, default and maximum zoom;
  - pointers outside the viewport and inside a blocker miss.
- **Invariant:** the result is the same for the same input, and only presented sockets ever appear.

## Task 10.7.2 — Pure gesture classification, pointer session & release re-resolution  · done

File-tree diff:

```text
web/src/
+ gesture-ownership.ts    classifyPress (pure priority), PressFacts, Ownership
+ pointer-session.ts      primary-pointer lifecycle: pure `step` reducer + `beginPointerSession` DOM driver
  patching.ts             pointerUp re-resolves on release; illegal release → pending + `rejection`;
                          pointerCancel; aliasFor; JackHit gains `aliases`
web/test/
+ gesture-ownership.test.ts, pointer-session.test.ts; patching.test.ts updated
```

Key types & signatures (as landed):

```ts
export type Ownership =
  | { kind: "control" }
  | { kind: "socket"; pick: Extract<SocketPick, { kind: "exact" | "near" }> }
  | { kind: "cable"; key: string }
  | { kind: "device"; objectId: string }
  | { kind: "pan" };
export interface PressFacts { inControl: boolean; socket: SocketPick; cableKey: string | null; objectId: string | null }
export function classifyPress(f: PressFacts): Ownership

// pointer-session
export const MOVE_THRESHOLD_PX = { mouse: 4, pen: 4, touch: 8 };
export function step(s: SessionState, i: SessionInput): { state; effect: "begin"|"move"|"release"|"cancel"|"ignore"; moved }
export function beginPointerSession(root: Element, down: PointerEvent, h: SessionHandlers): PointerSession | null
export const sessionActive: (root: Element) => boolean

// patching.ts
export type JackHit = { key: string; endpoint: Endpoint; anchor: Pt | null; aliases?: string[] };
export type PatchState = { …; rejection: { key: string; reason: string } | null } | null;
export function pointerUp(state, hit: JackHit | null /* picked at release */, clickNotDrag, deps): PatchResult
export function pointerCancel(state): PatchState   // drag → null; pending survives
export function aliasFor(aliases, source): string | undefined  // duplex: the complementary direction
```

Deviations from the drafted shape (small, recorded as assumptions):

- **No `pending` press fact.** A pending pick owns only presses that pick a socket. Presses elsewhere fall
  through to cable, device or pan, and a pan during a pending pick keeps working as it does today. A click
  on empty space or a non-jack never ended a pending pick anyway, so the patch controller doesn't need to
  see those presses.
- **`JackHit` is kept, not renamed to `SocketHit`.** It gains `aliases`, so a click on any alias of the
  source's duplex socket counts as "the source". `aliasFor` chooses which alias a picked target stands for.
- **The session reads move/up/cancel on `window`.** Captured events bubble there, so a release outside
  the stage still ends the session. A release with no final move ends it too: the known regression
  dispatches `pointerup` on `window`. Lost capture is read on the root.
- **Pending + drag released over nothing keeps the pick.** Pending + drag released on a socket resolves
  like a click on that socket.

The tests are table-driven:

- the classification priority, including a socket beating the device body and its own cable;
- the session reducer: other pointers ignored, non-primary button ignored, the 4/8 px threshold, release
  vs cancel, and a lost-capture cancel ignored after a release;
- `pointerUp` equivalence: drag+drop and click+click give the same commit and state for legal and illegal
  targets, and need no final move;
- the rejection lifecycle and duplex source aliases.

## Task 10.7.3 — Studio arbiter & adapters  · planned
_(program design drafted at pickup)_

This task:

- adds the `stageGestures` capture-phase action on the `WorldView` viewport;
- adds `PatchController.begin` / session wiring, which deletes the window `pointerdown` and the
  `patchableJackKey`/`elementFromPoint` path;
- turns `draggable` into `beginMove` and adds `camera.beginPan`;
- moves cable select to release-without-move;
- shows the rejection reason;
- graduates both known regressions.

## Task 10.7.4 — Workbench, blockers & the Story browser matrix  · planned
_(program design drafted at pickup)_

This task:

- runs the same arbiter on `BenchStage`, with the DUT's two simultaneous faces as targets;
- registers focus windows, toolbars and banners as blockers;
- adds the browser matrix: zoom extremes, multi-pointer, cancel, blur, lost capture, view navigation with
  a pending pick, illegal-target feedback, and control precedence;
- updates the manual matrix notes.
