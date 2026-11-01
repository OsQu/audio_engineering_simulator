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

/** Exactly the wire fields of a request. A UI-side `TopologyRequest` also carries its proposal — whose
 *  delta payload is the edit *closure* — and is a structural superset of this type, so it type-checks as
 *  metadata but cannot be structured-cloned across the MessagePort (DataCloneError). */
export const wireTransaction = (request: TopologyRequestMetadata): TopologyRequestMetadata => ({
  generation: request.generation,
  id: request.id,
  baseRevision: request.baseRevision,
  protocolVersion: request.protocolVersion,
  buildVersion: request.buildVersion,
});

export const protocolVersionsMatch = (actual: number, expected: number): boolean =>
  actual === expected;
