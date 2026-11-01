# Story 10.4 — Explicit faces & face-local socket geometry — program design

> Companion to the Story 10.4 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Task 10.4.1 — Pure face-presentation model  · done

`FaceInstanceId` identifies a rendered chassis slot. It deliberately does not identify only a device or a
side: a studio/support slot remains stable while its selected side changes, the workbench DUT has two slots,
and each focus window has its own non-patchable slot. The geometry model introduced in Task 10.4.3 keys a
side within that slot as `(FaceInstanceId, DeviceFacing)`.

File-tree diff:

```text
web/src/
  view-resolution.ts             face identity, resolution, and presentation policy
  presentation-state.ts          pure generation-guarded flip transitions
web/test/
  view-resolution.test.ts        studio/workbench/focus/top/rack face matrix
  presentation-state.test.ts     finish/cancel/reversal/unmount/reduced-motion oracles
```

Key types and signatures:

```ts
type FaceInstanceId = string;

type PresentationStage =
  | { kind: "studio" }
  | { kind: "workbench"; role: "dut-front" | "dut-back" | "support" }
  | { kind: "focus"; windowId: string };

interface ResolvedFace {
  id: FaceInstanceId;
  objectId: string;
  face: DeviceFacing;
  state: "presented" | "transitioning" | "hidden";
  interaction: "enabled" | "disabled";
}

interface FlipTransition {
  generation: number;
  from: DeviceFacing;
  to: DeviceFacing;
}

interface PresentationState {
  readonly flips: Readonly<Record<string, FlipTransition>>;
  readonly generations: Readonly<Record<string, number>>;
}

interface FlipStart {
  state: PresentationState;
  generation: number;
}

function resolveFaces(input: FaceResolutionInput): ResolvedFace[];
function beginFlip(
  state: PresentationState,
  objectId: string,
  to: DeviceFacing,
): FlipStart;
function finishFlip(
  state: PresentationState,
  objectId: string,
  generation: number,
): PresentationState;
function cancelFlip(
  state: PresentationState,
  objectId: string,
  generation: number,
): PresentationState;
function unmountPresentation(state: PresentationState, objectId: string): PresentationState;
```

The transition generation is a monotonically increasing token assigned on every start or reversal, not a
time, frame number, topology revision, or persisted scene field. Finish/cancel events captured by an older
transition are ignored. Unmount invalidates the active transition. A zero-duration or reduced-motion flip
uses the same start record and settles its generation synchronously after layout.

Resolution rules:

- a wall-view studio device exposes one stable studio slot; a mounted device's selected side and transition
  state inherit from its rack;
- top view exposes no device faces or physical leads;
- the workbench DUT exposes independent `dut-front` and `dut-back` slots; a support device exposes one stable
  `support` slot whose selected side follows its bench facing;
- every focus window includes its window ID and has disabled physical interaction, so duplicate faceplates
  cannot register as stage patch targets;
- both `from` and `to` sides are transitioning until the current generation settles, and any physical lead
  touching the device or an occupant of a transitioning rack is suppressed by resolved presentation policy.

## Task 10.4.2 — Face lifecycle integration  · done

Wire the 10.4.1 model into the three view roots without threading new props through every faceplate
widget. Each root owns one `PresentationState`; the DOM reports flip completion through the Web Animations
API object for the exact CSS transition a generation started, never through a timer or bubbled
`transitionend` (a reversal's stale `transitioncancel` would otherwise settle the new generation).

File-tree diff:

```text
web/src/
  view-resolution.ts        + transitionOwner(scene, objectId)  (rack id for mounted gear, else object id)
  face-lifecycle.ts         NEW  DOM adapter: awaitFlip(root, owner, generation, settle, cancel)
  jack-anchors.ts           + patchableJackKey(el)  (null under a data-interaction="disabled" slot)
  patch-controller.svelte.ts  jack lookups go through patchableJackKey
  widgets/Chassis.svelte    + data-flipper on .flipper; reduced-motion ⇒ transition: none
  App.svelte                presentation state, resolveFaces(studio), face-slot wrappers, flip wiring,
                            lead suppression, focus-window slots, measure on settle
  BenchStage.svelte         workbench resolveFaces, dut-front/dut-back/support slots, support flip wiring,
                            lead suppression
  Workbench.svelte          focus-window slot (BenchStage owns the bench PresentationState: only it flips)
web/test-browser/
  face-lifecycle.spec.ts    flip settle, rapid reversal, reduced motion, focus jacks not patchable
```

Key shapes:

```ts
// face-lifecycle.ts — after tick(): collect CSSTransitions on [data-flipper] under
// [data-flip-owner="<owner>"]; none (reduced motion, zero duration, off-view, empty rack) ⇒ settle(gen)
// synchronously; otherwise first `anim.finished` ⇒ settle(gen), rejection ⇒ cancel(gen). The generation is
// captured in the closure, so a superseded transition's promise is a no-op through the 10.4.1 guard.
export function awaitFlip(
  root: ParentNode,
  owner: string,
  generation: number,
  settle: (generation: number) => void,
  cancel: (generation: number) => void,
): void;

// jack-anchors.ts
export function patchableJackKey(el: Element | null): string | null;
```

Face slot wrapper (studio item, bench face column, focus window) — `display: contents`, so layout is untouched:

```svelte
<div class="face-slot" data-face-instance={face.id} data-face-state={face.state}
     data-interaction={face.interaction} data-flip-owner={transitionOwner(scene, face.objectId)}>
  <Faceplate … flipped={durableFacing === "back"} />
</div>
```

Flow: a flip chip calls the existing scene-op (durable facing changes at once, so the CSS transform animates
toward it), then `beginFlip(owner, to)` and `awaitFlip(...)`. While transitioning the resolved face keeps its
settled `from` side, `interaction` is disabled (jacks not pickable), and every lead touching the owner or its
occupants — ordinary cable, tip patch, portal stub, held rubber-band/pending end — is filtered by
`physicalLeadsSuppressed`; the pending pick and its banner stay. On settle the root re-measures jacks once
(an event, replacing the flip-specific half of the 480 ms timer; the generic font/layout timer stays until
10.4.4 replaces the sweep). Item teardown (view/space switch, removal, eject) calls
`unmountPresentation(owner)` for owners whose item unmounts.

Decisions:

- Wrapper element + data attributes instead of new `Chassis`/faceplate props — nine faceplate widgets pass
  `flipped` through; adding lifecycle props to each is churn that 10.4.4 would touch again.
- WAAPI `CSSTransition.finished` over `transitionend`/`transitioncancel` — binds one promise to one
  generation, so reversal ordering can't mis-settle.
- Focus slots are `interaction="disabled"`: they still operate controls, but their jacks can no longer start
  or complete a patch with the stage anchor of the same key (a latent collision today).
- DUT front/back slots never transition (the DUT shows both sides and never flips).

As built: the studio face slot also carries `data-face-side` (the resolved settled side) for tests; the
generation settles once *all* of an owner's visible transitions finish (a rack animates every occupant)
and cancels only if none did. The focus-window patch-gate is unit-free DOM; its browser proof lands with
the 10.4.5 matrix (the fixtures have no focusable device yet).

## Task 10.4.3 — Revisioned socket-geometry model  · done

A pure index of where each physical socket sits on a presented side, in that side's natural millimetres,
keyed by `(FaceInstanceId, DeviceFacing)`. No DOM, camera, placement, or time.

File-tree diff:

```text
web/src/socket-geometry.ts     index, revision/generation guard, lookup, projection
web/test/socket-geometry.test.ts
```

Key shapes:

```ts
interface SocketShape { id: string; aliases: readonly string[]; x: number; y: number; r: number }
interface FaceGeometry { width: number; height: number; sockets: readonly SocketShape[] }
interface GeometrySlot { face: FaceInstanceId; side: DeviceFacing }
type GeometryEntry =
  | { status: "pending"; revision: string; generation: number }
  | { status: "current"; revision: string; generation: number; geometry: FaceGeometry };
type RegisterOutcome = "current" | "stale" | "unmounted" | "empty" | "duplicate";

function requestGeometry(index, slot, revision): { index; generation; measure: boolean };
function registerGeometry(index, slot, generation, geometry): { index; outcome: RegisterOutcome };
function unregisterGeometry(index, slot): SocketGeometryIndex;
function socketFor(index, slot, endpoint): { status: "current"; socket; geometry } | { status: "pending" } | { status: "absent" };
function projectSocket(geometry, socket, frame: SurfaceFrame, presented: boolean): { x: number; y: number };
const geometryRevision: (parts: readonly (string | number | boolean)[]) => string;
```

Rules:

- an unchanged revision returns the same index object (a current registration survives untouched); a new or
  changed revision goes `pending` under a fresh generation from one index-wide monotonic counter, so a
  re-mounted slot never reuses a token held by an obsolete measurement;
- `registerGeometry` applies only the slot's current generation (`stale` otherwise), answers `unmounted`
  for an unregistered slot, and rejects a zero-size frame (`empty`) or any endpoint key named twice
  (`duplicate`), leaving the slot pending rather than overwriting good geometry;
- a duplex connector is one `SocketShape` whose `aliases` list both endpoint keys;
- `projectSocket` maps face-local mm into a surface frame; a see-through side (the hidden half of a flip
  card) mirrors about the face width, matching how rotateY(180deg) places it.

## Task 10.4.4 — Face-local DOM registration and projection  · done

Replace the global `[data-jack]` sweep (rect of every jack through `clientToSurface`, rAF + 480/120 ms
timers) with a per-face registration adapter and a pure projection that produces the same
`Record<jackKey, JackAnchor>` contract `cable-view` already consumes — so cable routing and patching are
unchanged in shape.

File-tree diff:

```text
web/src/face-geometry.ts        NEW  DOM adapter: measureFace(frame, side, presented) → FaceGeometry
                                     (rect deltas ÷ frame rect × frame layout size: camera-invariant;
                                     hidden side un-mirrored), the `faceGeometry` action, geometryContent
web/src/socket-geometry.ts      + projectAnchors(index, slots) → Record<jackKey, JackAnchor>,
                                     + optional FaceGeometry.home (bench un-offset origin)
web/src/patch-controller.svelte.ts   owns the SocketGeometryIndex + frames; jackAnchors becomes $derived
                                     projection; register(slot, element, revision) / unregister(slot)
web/src/jack-anchors.ts         sweep removed (patchableJackKey stays)
web/src/App.svelte              studio frame = cable-view deviceSurfaceRect (pure); register per face slot
web/src/BenchStage.svelte       bench frame = measured home origin (registration) + offsetFor (pure)
web/src/Workbench.svelte        timer-driven measure effect removed
```

Revision per slot: `[typeId, ports, configs values, footprint, skin finish, layout box w×h, font revision]`.
Camera, placement, rack moves, and bench offsets are not inputs; they move the *frame* only, so geometry is
reused and projection follows immediately. Measurement runs only when a slot is `presented` (not mid-flip),
after layout (rAF), and again when `document.fonts` finishes loading (a revision bump, not a timer).

As built: the studio slot became a real box (`.face-slot.frame`, 100% × 100% of the chassis item) so it is
the measurement frame; focus slots stay `display: contents`. Each slot reflects `data-geometry`
(`pending`/`current`) and `data-geometry-generation` (the last accepted measurement), which the browser
fixtures wait on instead of the old 550 ms sleep and which the zoom/placement tests assert is unchanged.
`PatchController.jackAnchors` is now `$derived` from `geometry × project(slots)`; `measure()` is gone.

## Task 10.4.5 — Browser matrix and legacy cleanup  · done

Browser matrix (`web/test-browser/face-lifecycle.spec.ts`, `socket-geometry.spec.ts`):

| Case | Test |
| --- | --- |
| back→front flip, leads suppressed until settle | `a flip suppresses its leads until its transition settles` |
| front→back flip, reduced motion settles synchronously | `reduced motion settles a flip without waiting for an animation` |
| rapid reversal | `a rapid reversal settles on the latest generation only` |
| rack inheritance | `a rack flip transitions its mounted occupant and settles it to the rack's side` |
| cancellation / unmount mid-flip | `leaving the wall mid-flip invalidates it; returning shows the settled durable side` |
| workbench DUT duplicate slots | `the workbench DUT exposes independent front and back slots` |
| focus isolation | `a focus-window copy cannot start a patch from its duplicate jack` (mutation-checked: fails without the `patchableJackKey` gate) |
| zoom + placement reuse geometry (studio) | `zoom and placement re-project registered geometry without remeasuring` |
| zoom reuses geometry (workbench) | `the workbench projects its DUT sockets through zoom without remeasuring` |

Duplex aliases and differently configured same-type instances are covered by the pure
`socket-geometry.test.ts` oracles: the browser fixtures have no duplex device, and they never start the
engine, so a second configured instance can't be built there.

Legacy removed: the global `measureJacks` DOM sweep, every rAF + 480/120 ms measure timer (studio, bench,
workbench), the flip-settle remeasure, and the fixtures' 550 ms sleep (now `geometrySettled`).
`WorldApi.measureRoot` became `stageRoot` (it now only scopes the flip lifecycle's stage queries), and
comments that described "measured jack anchors" now describe projected socket anchors.
