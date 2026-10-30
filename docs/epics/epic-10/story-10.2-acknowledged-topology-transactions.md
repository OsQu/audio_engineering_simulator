# Story 10.2 — Acknowledged topology transactions — program design

> Companion to the Story 10.2 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Task 10.2.1 — Pure topology transaction state machine  · done

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

*As built:* the module also exposes `recover(state, nowMs)` (invalidate the generation, then resubmit the
retained proposal) and `clearAccepted(state)`. Acceptance increments the topology revision and parks the
request in `state.accepted` until the session has applied its delta, so the state machine never needs to
know what a `Scene` is. Busy replies collapse into the retryable `rejected` outcome with a default message.

## Task 10.2.2 — Versioned worklet acknowledgement lifecycle  · done

`topology-protocol.ts` owns `TOPOLOGY_PROTOCOL_VERSION` / `TOPOLOGY_BUILD_VERSION` and the
`TopologyRequestMetadata` (generation, id, base revision, protocol and build version) carried on the UI ↔
worklet boundary. `processor-impl.js` repeats both constants because it is concatenated into a standalone
worklet bundle and cannot import TypeScript.

```text
UI                                   worklet
loadPatch { transaction, patch }  →  load_patch (compile, queue swap)
                                  ←  topologyAccepted { transaction }      compiler queued it
                                  ←  topologyRejected { transaction, msg } compile threw; live scene untouched
   … next render quantum installs the scene …
                                  ←  losses, deviceDescriptors, topologyInstalled { transaction }
```

- Readiness fails closed: the processor posts `protocolMismatch` instead of building the engine when
  `processorOptions.topologyProtocolVersion` differs, and `startEngine` refuses `onReady` if the `ready`
  message reports a different version. A mismatched per-request version is also refused.
- Per-instance `deviceDescriptors` moved from "on load" to "after install", so engine-derived data refreshes
  only once the new schedule is live.
- The legacy `requestId` / `patchAccepted` / `patchRejected` path remains for untransacted `loadPatch`.

## Task 10.2.3 — Staged session and gesture migration  · done

`SceneSession` holds `topology: TopologyState` and one deadline timer. Every structural producer goes through

```ts
proposeTopology(label: string, edit: (scene: Scene) => void): boolean
```

which applies `edit` to a detached snapshot to compile, sends it with the request metadata, and keeps `edit`
itself as the scoped delta. On `topologyAccepted` the session replays `edit` against the *then-current*
scene (so concurrent placement/param/track/UI edits survive) and reseeds param values; on
`topologyInstalled` it re-applies track state and refreshes waveforms. Producers migrated: cable
connect/disconnect/cable-type (`PatchController`, which re-finds the connection by key inside the edit),
config toggles, DAW track count, studio add/remove device, workbench DUT load / monitored-tap / bench gear,
scene load, and reload. `replaceTopology(label, scene)` is the whole-scene producer for load paths;
`hotSwap()` and `submitPatchCandidate()` survive only as thin bridges onto the same channel, so there is no
unacknowledged `loadPatch` left in production code. The studio status bar offers **Retry topology change**
on rejection and **Recover topology** on an unknown outcome; recovery rebuilds the candidate from the latest
accepted scene.

## Task 10.2.4 — Browser and real-worklet transaction proof  · done

- Pure (`topology-transaction.test.ts`, `topology-protocol.test.ts`): accepted, compiler-rejected, busy,
  unavailable, exact five-second deadline → unknown, recovery with generation bump, late/stale/mis-correlated
  replies, processor failure, concurrent non-topology edit preservation, and protocol version stamping /
  matching.
- Real worklet (`worklet-patch-acceptance.spec.ts`): a compatible transaction yields exactly
  `accepted,installed`; an invalid output reference is rejected with no install event; a protocol mismatch
  never reaches readiness.
- `TestSceneSession` overrides `proposeTopology` to accept synchronously, so the Story 10.1 studio fixture
  keeps exercising gestures without audio.

## Residual notes (carried forward, not blocking)

- The worklet compiles synchronously on its single thread, so it never emits `busy` or `unavailable`; the
  state machine supports both for a future off-thread compiler. Generic processor `error` messages are not yet
  mapped to `processor-failure` (a missing reply still becomes `unknown` at the deadline).
- The session arms the deadline with `setTimeout(5_000)` rather than `TRANSACTION_DEADLINE_MS`, and the
  `unavailable` outcome has no dedicated UI affordance (a new proposal resubmits).
- Browser gesture coverage of "pending action retained until a known outcome" is pure-level only: the studio
  fixture session auto-accepts; the real-worklet spec drives the protocol directly, not `SceneSession`.
