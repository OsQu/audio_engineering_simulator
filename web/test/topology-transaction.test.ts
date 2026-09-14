import { describe, expect, it } from "vitest";
import {
  clearAccepted,
  deadlineElapsed,
  initialTopologyState,
  invalidateGeneration,
  receive,
  recover,
  submit,
  TRANSACTION_DEADLINE_MS,
  type TopologyProposal,
} from "../src/topology-transaction";

const proposal: TopologyProposal = {
  label: "Connect synth to interface",
  delta: { kind: "connection-added", payload: { from: "synth", to: "interface" } },
};

const submitted = (nowMs = 1_000) => submit(initialTopologyState(7, 3), proposal, nowMs);

describe("topology transactions", () => {
  it("serializes requests and correlates compiler acceptance to a scoped delta", () => {
    const first = submitted();
    expect(first.request).toMatchObject({ id: 1, generation: 7, baseRevision: 3, ...proposal });
    expect(submit(first.state, proposal, 1_001)).toEqual({ state: first.state });

    const accepted = receive(first.state, { kind: "accepted", generation: 7, id: 1 }, 1_100);
    expect(accepted).toMatchObject({
      generation: 7,
      revision: 4,
      outcome: { kind: "idle" },
      accepted: { revision: 4, request: first.request },
    });
    expect(clearAccepted(accepted).accepted).toBeNull();
  });

  it.each([
    [{ kind: "rejected", generation: 7, id: 1, message: "invalid cable" }, "invalid cable"],
    [{ kind: "busy", generation: 7, id: 1 }, "engine is busy"],
  ] as const)("keeps a rejected or busy proposal retryable", (reply, message) => {
    const first = submitted();
    const rejected = receive(first.state, reply, 1_100);
    expect(rejected.outcome).toMatchObject({ kind: "rejected", request: first.request, message });
    expect(submit(rejected, proposal, 1_200).request).toMatchObject({ id: 2, generation: 7 });
  });

  it("retains an unavailable proposal without pretending it was rejected", () => {
    const first = submitted();
    const unavailable = receive(first.state, { kind: "unavailable", generation: 7, id: 1 }, 1_100);
    expect(unavailable.outcome).toEqual({ kind: "unavailable", proposal });
    expect(submit(unavailable, proposal, 1_200).request).toMatchObject({ id: 2 });
  });

  it("becomes unknown exactly at five seconds, not rejected", () => {
    const first = submitted();
    expect(deadlineElapsed(first.state, 1_000 + TRANSACTION_DEADLINE_MS - 1)).toBe(first.state);
    const timedOut = deadlineElapsed(first.state, 1_000 + TRANSACTION_DEADLINE_MS);
    expect(timedOut.outcome).toMatchObject({ kind: "unknown", reason: "timeout", request: first.request });
    expect(submit(timedOut, proposal, 6_001)).toEqual({ state: timedOut });
  });

  it("recovers by advancing generation before replacement and ignores its late reply", () => {
    const first = submitted();
    const timedOut = deadlineElapsed(first.state, 6_000);
    const replacement = recover(timedOut, 6_100);
    expect(replacement.request).toMatchObject({ id: 2, generation: 8, baseRevision: 3 });
    expect(receive(replacement.state, { kind: "accepted", generation: 7, id: 1 }, 6_200)).toBe(
      replacement.state,
    );
    expect(receive(replacement.state, { kind: "accepted", generation: 8, id: 2 }, 6_200).revision).toBe(4);
  });

  it("invalidates outstanding replies and preserves the accepted topology revision across generations", () => {
    const first = submitted();
    const invalidated = invalidateGeneration(first.state);
    expect(invalidated).toMatchObject({ generation: 8, revision: 3, outcome: { kind: "idle" } });
    expect(receive(invalidated, { kind: "accepted", generation: 7, id: 1 }, 1_100)).toBe(invalidated);
  });

  it("turns an explicit processor failure into unknown and ignores malformed stale correlations", () => {
    const first = submitted();
    const failed = receive(first.state, { kind: "processor-failure", generation: 7, id: 1 }, 1_100);
    expect(failed.outcome).toMatchObject({ kind: "unknown", reason: "processor-failure" });
    expect(receive(first.state, { kind: "accepted", generation: 7, id: 99 }, 1_100)).toBe(first.state);
  });
});
