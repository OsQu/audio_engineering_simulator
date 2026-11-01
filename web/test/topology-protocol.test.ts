import { describe, expect, it } from "vitest";
import {
  protocolVersionsMatch,
  TOPOLOGY_BUILD_VERSION,
  TOPOLOGY_PROTOCOL_VERSION,
  wireTransaction,
} from "../src/topology-protocol";
import { initialTopologyState, submit } from "../src/topology-transaction";

describe("topology protocol", () => {
  it("requires an exact protocol version match at readiness", () => {
    expect(protocolVersionsMatch(TOPOLOGY_PROTOCOL_VERSION, TOPOLOGY_PROTOCOL_VERSION)).toBe(true);
    expect(protocolVersionsMatch(TOPOLOGY_PROTOCOL_VERSION + 1, TOPOLOGY_PROTOCOL_VERSION)).toBe(
      false,
    );
  });

  it("stamps every structural request with its protocol and build versions", () => {
    const request = submit(
      initialTopologyState(),
      { label: "Connect", delta: { kind: "connection-added", payload: null } },
      0,
    ).request;
    expect(request).toMatchObject({
      protocolVersion: TOPOLOGY_PROTOCOL_VERSION,
      buildVersion: TOPOLOGY_BUILD_VERSION,
    });
  });
});

describe("topology wire transaction", () => {
  it("strips a request carrying its edit closure down to cloneable metadata", () => {
    // Regression: SceneSession posted the whole request, whose delta payload is the edit function,
    // and postMessage threw DataCloneError on every add/remove/connect.
    const { request } = submit(
      initialTopologyState(),
      { label: "add eq", delta: { kind: "scene-topology", payload: () => {} } },
      0,
    );
    if (!request) throw new Error("expected a request");
    expect(() => structuredClone(request)).toThrow();

    const wire = wireTransaction(request);
    expect(() => structuredClone(wire)).not.toThrow();
    expect(Object.keys(wire).sort()).toEqual(
      ["baseRevision", "buildVersion", "generation", "id", "protocolVersion"].sort(),
    );
    expect(wire).toMatchObject({ id: request.id, generation: request.generation });
  });
});
