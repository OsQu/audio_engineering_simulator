import { describe, expect, it } from "vitest";
import {
  protocolVersionsMatch,
  TOPOLOGY_BUILD_VERSION,
  TOPOLOGY_PROTOCOL_VERSION,
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
