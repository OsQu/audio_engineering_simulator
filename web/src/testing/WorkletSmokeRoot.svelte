<script lang="ts">
  // An intentionally tiny test-only composition root. Unlike BrowserFixtureRoot, this one boots the
  // production engine and generated worklet so the browser smoke proves the patch protocol boundary.
  import { onDestroy } from "svelte";
  import { startEngine, type ControlMessage, type EngineControl } from "../engine";
  import { defaultScene } from "../scene-store";
  import { TOPOLOGY_BUILD_VERSION, TOPOLOGY_PROTOCOL_VERSION } from "../topology-protocol";

  type WorkletSmokeState =
    "idle" | "starting" | "ready" | "accepted" | "installed" | "rejected" | "mismatch" | "error";

  const request = {
    generation: 41,
    id: 10103,
    baseRevision: 7,
    protocolVersion: TOPOLOGY_PROTOCOL_VERSION,
    buildVersion: TOPOLOGY_BUILD_VERSION,
  };
  const patch = defaultScene().patch;
  let smokeState: WorkletSmokeState = $state("idle");
  let detail = $state("");
  let events = $state<string[]>([]);
  let send: ((message: ControlMessage) => void) | undefined;
  let control: EngineControl | undefined;
  let destroyed = false;

  async function start(): Promise<void> {
    if (smokeState !== "idle") return;
    smokeState = "starting";
    try {
      const next = await startEngine(
        patch,
        {
          onStatus: (message) => {
            detail = message;
          },
          onHealth: () => {},
          onLevel: () => {},
          onReady: (_ready, nextSend) => {
            send = nextSend;
            smokeState = "ready";
          },
          onTopologyAccepted: (transaction) => {
            if (transaction.id === request.id) {
              events = [...events, "accepted"];
              smokeState = "accepted";
            }
          },
          onTopologyInstalled: (transaction) => {
            if (transaction.id === request.id) {
              events = [...events, "installed"];
              smokeState = "installed";
            }
          },
          onTopologyRejected: (transaction, message) => {
            if (transaction.id === request.id + 1) {
              smokeState = "rejected";
              detail = message;
            }
          },
        },
        0,
        location.search.includes("mismatch=1")
          ? { topologyProtocolVersion: TOPOLOGY_PROTOCOL_VERSION + 1 }
          : {},
      );
      control = next;
      if (destroyed) await control.close();
      else await control.resume();
    } catch (error) {
      smokeState = "error";
      detail = error instanceof Error ? error.message : String(error);
    }
  }

  function submitCompatiblePatch(): void {
    if (!send || smokeState !== "ready") return;
    send({ type: "loadPatch", transaction: request, patch });
  }

  function submitRejectedPatch(): void {
    if (!send || smokeState !== "ready") return;
    send({
      type: "loadPatch",
      transaction: { ...request, id: request.id + 1 },
      patch: { ...patch, output: { device: "missing-device", port: 0 } },
    });
  }

  onDestroy(() => {
    destroyed = true;
    void control?.close();
  });
</script>

<main data-testid="worklet-smoke">
  <output data-testid="worklet-state" data-state={smokeState} data-events={events.join(",")}
    >{smokeState}</output
  >
  {#if detail}
    <output data-testid="worklet-detail">{detail}</output>
  {/if}
  <button type="button" onclick={start} disabled={smokeState !== "idle"}>Start real worklet</button>
  <button type="button" onclick={submitCompatiblePatch} disabled={smokeState !== "ready"}>
    Submit compatible patch
  </button>
  <button type="button" onclick={submitRejectedPatch} disabled={smokeState !== "ready"}>
    Submit rejected patch
  </button>
</main>
