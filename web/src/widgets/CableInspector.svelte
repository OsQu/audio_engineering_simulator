<script lang="ts">
  // The shared cable inspector — the floating panel shown when a patch cable is selected, in both the
  // scene view and the workbench bench (one mechanism, no divergence). It changes the cable **type**
  // (ideal wire vs an R·C preset that physically fits the connector) and disconnects the lead; only analog
  // links carry a cable, so a digital/event link reads as ideal. Everything derives from the session +
  // the selected connection; edits go through the shared PatchController (which hot-swaps the engine).
  import { cableTypeIdFor } from "../connections";
  import type { Connection } from "../scene";
  import * as sceneOps from "../scene-ops";
  import type { PatchController } from "../patch-controller.svelte";
  import type { SceneSession } from "../session.svelte";

  interface Props {
    session: SceneSession;
    patch: PatchController;
    /** The selected connection (kept fresh by the parent's `selectedConn` derivation). */
    conn: Connection;
    /** Clear the parent's selection (also called after a disconnect). */
    onClose: () => void;
  }
  let { session, patch, conn, onClose }: Props = $props();

  const domain = $derived(sceneOps.connectionDomain(session.scene, session.catalog, conn));
  // The cable presets that physically fit this link (matching connector) — so you can't put an XLR cable
  // on a ¼" link. The current type id resolves the connection's stored R·C back to a preset (else ideal).
  const cables = $derived(sceneOps.cablesFor(session.scene, session.catalog, session.cables, conn));
  const currentType = $derived(cableTypeIdFor(session.cables, conn.cable));
  // The loading loss (§5.3 divider) in dB, from the static per-connection losses — null for a digital link.
  const loss = $derived.by((): number | null => {
    const i = session.scene.patch.connections.findIndex(
      (c) => sceneOps.connKey(c) === sceneOps.connKey(conn),
    );
    return i >= 0 ? (session.losses[i] ?? null) : null;
  });

  function disconnect(): void {
    patch.disconnect(conn);
    onClose();
  }
</script>

<div class="cable-inspector">
  <span class="ci-label">
    Cable <strong>{conn.from.device}</strong> → <strong>{conn.to.device}</strong>
  </span>
  {#if domain === "analog"}
    <label class="ci-type">
      Type
      <select value={currentType} onchange={(e) => patch.setCableType(conn, e.currentTarget.value)}>
        <option value="">Ideal wire</option>
        {#each cables as ct (ct.typeId)}
          <option value={ct.typeId}>{ct.label}</option>
        {/each}
      </select>
    </label>
    <!-- Static impedance loading loss (how far the loaded input sits below the source's open-circuit V). -->
    <span class="ci-loss">loading {loss !== null ? `${loss.toFixed(2)} dB` : "—"}</span>
  {:else}
    <span class="ci-ideal">digital link — ideal (no cable)</span>
  {/if}
  <button type="button" onclick={disconnect}>Disconnect</button>
  <button type="button" class="ci-close" onclick={onClose}>Close</button>
</div>

<style>
  .cable-inspector {
    position: absolute;
    left: 50%;
    bottom: 1rem;
    transform: translateX(-50%);
    z-index: 6;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.6rem;
    padding: 0.4rem 0.75rem;
    background: var(--ae-bg-panel);
    color: var(--ae-text-strong);
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-panel);
    box-shadow: var(--ae-shadow-card);
    font-size: 0.8rem;
  }
  .ci-type,
  .ci-ideal {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    color: #b8bcc2;
  }
  select {
    font: inherit;
    font-size: 0.75rem;
  }
  button {
    font: inherit;
    font-size: 0.72rem;
    padding: 0.2rem 0.7rem;
  }
  .ci-loss {
    color: #8fd0a0;
    font-variant-numeric: tabular-nums;
  }
  .ci-close {
    margin-left: auto;
  }
</style>
