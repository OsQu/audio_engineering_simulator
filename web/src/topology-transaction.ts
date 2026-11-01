// Pure state transitions for changes that alter the compiled patch topology. This deliberately owns
// neither a Scene nor a transport: the session supplies a scoped delta and applies an accepted delta
// to its then-current scene in a later task.

import {
  TOPOLOGY_BUILD_VERSION,
  TOPOLOGY_PROTOCOL_VERSION,
  type TopologyRequestMetadata,
} from "./topology-protocol";

export type EngineGeneration = number;
export type TopologyRevision = number;
export type TransactionId = number;

/** A scoped structural change. Its payload is interpreted by the session, never by this state machine. */
export interface TopologyDelta {
  readonly kind: string;
  readonly payload: unknown;
}

export interface TopologyProposal {
  readonly delta: TopologyDelta;
  readonly label: string;
}

export interface TopologyRequest extends TopologyProposal, TopologyRequestMetadata {
  readonly generation: EngineGeneration;
  readonly id: TransactionId;
  readonly baseRevision: TopologyRevision;
}

export type TopologyOutcome =
  | { kind: "idle" }
  | { kind: "submitting"; request: TopologyRequest; deadlineMs: number }
  | { kind: "rejected"; request: TopologyRequest; message: string }
  | { kind: "unknown"; request: TopologyRequest; reason: "timeout" | "processor-failure" }
  | { kind: "unavailable"; proposal: TopologyProposal };

/** The accepted event is retained until its consumer has applied the delta to the current scene. */
export interface AcceptedTopologyChange {
  readonly request: TopologyRequest;
  readonly revision: TopologyRevision;
}

export interface TopologyState {
  readonly generation: EngineGeneration;
  readonly revision: TopologyRevision;
  readonly nextId: TransactionId;
  readonly protocolVersion: number;
  readonly buildVersion: string;
  readonly outcome: TopologyOutcome;
  readonly accepted: AcceptedTopologyChange | null;
}

export type TopologyReply =
  | { kind: "accepted"; generation: EngineGeneration; id: TransactionId }
  | { kind: "rejected"; generation: EngineGeneration; id: TransactionId; message: string }
  | { kind: "busy"; generation: EngineGeneration; id: TransactionId; message?: string }
  | { kind: "unavailable"; generation: EngineGeneration; id: TransactionId }
  | { kind: "processor-failure"; generation: EngineGeneration; id: TransactionId };

export interface SubmitResult {
  readonly state: TopologyState;
  /** Present exactly when the caller should send this request to the engine. */
  readonly request?: TopologyRequest;
}

export const TRANSACTION_DEADLINE_MS = 5_000;

export function initialTopologyState(
  generation: EngineGeneration = 0,
  revision: TopologyRevision = 0,
  protocolVersion: number = TOPOLOGY_PROTOCOL_VERSION,
  buildVersion: string = TOPOLOGY_BUILD_VERSION,
): TopologyState {
  return {
    generation,
    revision,
    nextId: 1,
    protocolVersion,
    buildVersion,
    outcome: { kind: "idle" },
    accepted: null,
  };
}

/** Start the sole in-flight topology request, if the channel is in a retryable state. */
export function submit(
  state: TopologyState,
  proposal: TopologyProposal,
  nowMs: number,
): SubmitResult {
  if (state.outcome.kind === "submitting" || state.outcome.kind === "unknown") return { state };
  const request: TopologyRequest = {
    ...proposal,
    generation: state.generation,
    id: state.nextId,
    baseRevision: state.revision,
    protocolVersion: state.protocolVersion,
    buildVersion: state.buildVersion,
  };
  return {
    request,
    state: {
      ...state,
      nextId: state.nextId + 1,
      outcome: { kind: "submitting", request, deadlineMs: nowMs + TRANSACTION_DEADLINE_MS },
      accepted: null,
    },
  };
}

/** Correlate one compiler reply. Replies for any obsolete request or generation are inert. */
export function receive(state: TopologyState, reply: TopologyReply, _nowMs: number): TopologyState {
  const pending = state.outcome.kind === "submitting" ? state.outcome : null;
  if (
    !pending ||
    reply.generation !== state.generation ||
    reply.generation !== pending.request.generation ||
    reply.id !== pending.request.id
  ) {
    return state;
  }

  if (reply.kind === "accepted") {
    const revision = state.revision + 1;
    return {
      ...state,
      revision,
      outcome: { kind: "idle" },
      accepted: { request: pending.request, revision },
    };
  }
  if (reply.kind === "unavailable") {
    const { delta, label } = pending.request;
    return { ...state, outcome: { kind: "unavailable", proposal: { delta, label } } };
  }
  if (reply.kind === "processor-failure") {
    return {
      ...state,
      outcome: { kind: "unknown", request: pending.request, reason: "processor-failure" },
    };
  }
  return {
    ...state,
    outcome: {
      kind: "rejected",
      request: pending.request,
      message: reply.kind === "busy" ? (reply.message ?? "engine is busy") : reply.message,
    },
  };
}

/** Move a request to unknown precisely at its deadline; absence of a reply is never a rejection. */
export function deadlineElapsed(state: TopologyState, nowMs: number): TopologyState {
  if (state.outcome.kind !== "submitting" || nowMs < state.outcome.deadlineMs) return state;
  return {
    ...state,
    outcome: { kind: "unknown", request: state.outcome.request, reason: "timeout" },
  };
}

/** Forget a consumed accepted event after its delta has been applied to the current scene. */
export function clearAccepted(state: TopologyState): TopologyState {
  return state.accepted ? { ...state, accepted: null } : state;
}

/** Invalidate an engine generation so all outstanding replies become stale. */
export function invalidateGeneration(state: TopologyState): TopologyState {
  return {
    ...state,
    generation: state.generation + 1,
    outcome: { kind: "idle" },
    accepted: null,
  };
}

/** Recover from an unknown outcome by invalidating the old engine generation and resubmitting its proposal. */
export function recover(state: TopologyState, nowMs: number): SubmitResult {
  if (state.outcome.kind !== "unknown") return { state };
  const next = invalidateGeneration(state);
  return submit(next, state.outcome.request, nowMs);
}
