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

## Task 10.4.2 — Face lifecycle integration  · design drafted (awaiting review)

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
  Workbench.svelte          owns the bench PresentationState (passed to BenchStage), focus-window slots
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

## Task 10.4.3 — Revisioned socket-geometry model  · planned

_(program design drafted at pickup)_

## Task 10.4.4 — Face-local DOM registration and projection  · planned

_(program design drafted at pickup)_

## Task 10.4.5 — Browser matrix and legacy cleanup  · planned

_(program design drafted at pickup)_
