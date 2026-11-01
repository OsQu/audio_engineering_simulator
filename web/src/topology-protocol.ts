// The UI half of the structural worklet protocol. `processor-impl.js` deliberately repeats the two
// constants: it is concatenated into a standalone AudioWorklet bundle and cannot import TypeScript.

export const TOPOLOGY_PROTOCOL_VERSION = 1;
export const TOPOLOGY_BUILD_VERSION = "topology-transactions-v1";

/** Fields which identify a structural request on the UI ↔ worklet boundary. */
export interface TopologyRequestMetadata {
  readonly generation: number;
  readonly id: number;
  readonly baseRevision: number;
  readonly protocolVersion: number;
  readonly buildVersion: string;
}

export const protocolVersionsMatch = (actual: number, expected: number): boolean =>
  actual === expected;
