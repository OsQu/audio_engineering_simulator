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

## Task 10.4.2 — Face lifecycle integration  · planned

_(program design drafted at pickup)_

## Task 10.4.3 — Revisioned socket-geometry model  · planned

_(program design drafted at pickup)_

## Task 10.4.4 — Face-local DOM registration and projection  · planned

_(program design drafted at pickup)_

## Task 10.4.5 — Browser matrix and legacy cleanup  · planned

_(program design drafted at pickup)_
