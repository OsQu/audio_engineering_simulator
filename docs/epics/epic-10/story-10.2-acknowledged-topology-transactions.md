# Story 10.2 — Acknowledged topology transactions — program design

> Companion to the Story 10.2 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Task 10.2.1 — Pure topology transaction state machine  · planned

The state machine owns correlation, the five-second deadline, and stale-reply exclusion; it is pure and
clock-driven so it neither imports Svelte nor reaches into `SceneSession`. A proposal carries a scoped patch
change rather than a full `Scene` snapshot. On acceptance the session applies that delta to its current scene,
which preserves concurrent non-topology placement, parameter, track, and UI changes. The "accepted" reply is
compiler acceptance only; installation is a separate protocol event.

File-tree diff:

```text
web/src/
  topology-transaction.ts             pure state transition / request correlation module
web/test/
  topology-transaction.test.ts        table-driven fake transport and fake-clock matrix
```

Key types and signatures:

```ts
type EngineGeneration = number;
type TopologyRevision = number;
type TransactionId = number;

type TopologyOutcome =
  | { kind: "idle" }
  | { kind: "submitting"; request: TopologyRequest; deadlineMs: number }
  | { kind: "rejected"; request: TopologyRequest; message: string }
  | { kind: "unknown"; request: TopologyRequest; reason: "timeout" | "processor-failure" }
  | { kind: "unavailable"; proposal: TopologyProposal };

interface TopologyProposal {
  readonly delta: TopologyDelta;       // apply to latest accepted scene, never a Scene snapshot
  readonly label: string;
}

interface TopologyRequest extends TopologyProposal {
  readonly generation: EngineGeneration;
  readonly id: TransactionId;
  readonly baseRevision: TopologyRevision;
}

function submit(state: TopologyState, proposal: TopologyProposal, nowMs: number): SubmitResult;
function receive(state: TopologyState, reply: TopologyReply, nowMs: number): TopologyState;
function deadlineElapsed(state: TopologyState, nowMs: number): TopologyState;
function invalidateGeneration(state: TopologyState): TopologyState;
```

The fake-clock oracle documents that a request sent at `1_000 ms` becomes `unknown` exactly at `6_000 ms`,
not `rejected`; recovery increments the generation before it sends a replacement request, so an acceptance from
generation `g` cannot alter state in `g + 1`. Tables cover accepted, compiler-rejected, busy, unavailable,
deadline, recovery, late replies, concurrent non-topology edit preservation, and malformed/protocol-mismatched
replies.

## Task 10.2.2 — Versioned worklet acknowledgement lifecycle  · planned

_(program design drafted at pickup)_

## Task 10.2.3 — Staged session and gesture migration  · planned

_(program design drafted at pickup)_

## Task 10.2.4 — Browser and real-worklet transaction proof  · planned

_(program design drafted at pickup)_
