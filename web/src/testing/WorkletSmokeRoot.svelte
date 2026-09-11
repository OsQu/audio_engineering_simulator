<script lang="ts">
  // An intentionally tiny test-only composition root. Unlike BrowserFixtureRoot, this one boots the
  // production engine and generated worklet so the browser smoke proves the patch protocol boundary.
  import { onDestroy } from "svelte";
  import { startEngine, type ControlMessage, type EngineControl } from "../engine";
  import { defaultScene } from "../scene-store";

  type WorkletSmokeState = "idle" | "starting" | "ready" | "accepted" | "rejected" | "error";

  const requestId = 10103;
  const patch = defaultScene().patch;
  let smokeState: WorkletSmokeState = $state("idle");
  let detail = $state("");
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
          onPatchAccepted: (id) => {
            if (id === requestId) smokeState = "accepted";
          },
          onPatchRejected: (id, message) => {
            if (id === requestId) {
              smokeState = "rejected";
              detail = message;
            }
          },
        },
        0,
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
    send({ type: "loadPatch", requestId, patch });
  }

  onDestroy(() => {
    destroyed = true;
    void control?.close();
  });
</script>

<main data-testid="worklet-smoke">
  <output data-testid="worklet-state" data-state={smokeState}>{smokeState}</output>
  {#if detail}
    <output data-testid="worklet-detail">{detail}</output>
  {/if}
  <button type="button" onclick={start} disabled={smokeState !== "idle"}>Start real worklet</button>
  <button type="button" onclick={submitCompatiblePatch} disabled={smokeState !== "ready"}>
    Submit compatible patch
  </button>
</main>
