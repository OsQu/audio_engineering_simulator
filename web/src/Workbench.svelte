<script lang="ts">
  // The device workbench — the second view root (Epic 6): a focused single-device development view at
  // /devices/<typeId>. It constructs its own SceneSession (the shared engine consumer surface), boots it
  // **suspended** on a known-good bootstrap so the catalog arrives before any gesture, then resolves the
  // requested typeId against that catalog — hot-swapping to the device (device-only, silent — the rig is
  // 6.3) or falling back to the catalog index. Audio resumes on the first interaction. The bench stage
  // (grid + both faces + live params/meters) lands in Task 6.2.4; for now a known device shows a stub.

  import BenchStage from "./BenchStage.svelte";
  import { SceneSession } from "./session.svelte";
  import { analogOutputPort, bootstrapScene, deviceScene } from "./workbench-scene";

  interface Props {
    /** The requested device type id from the URL, or "" for the bare /devices index. */
    typeId: string;
    /** Route to a new path (History API push), shared from the router. */
    navigate: (path: string) => void;
  }

  let { typeId, navigate }: Props = $props();

  // This view root's own session, booted on the bootstrap scene so the worklet builds + posts the catalog
  // even for an unknown typeId. Boot on mount (suspended); start() is idempotent so the effect is safe.
  const session = new SceneSession(bootstrapScene());
  $effect(() => {
    void session.start(() => {});
  });

  // Autoplay: the context is suspended until the first user interaction (a click/keypress anywhere).
  let resumed = false;
  function resumeOnce(): void {
    if (resumed) return;
    resumed = true;
    void session.resume();
  }

  // The requested device's descriptor once the catalog is in — null for the bare index or an unknown id.
  const requested = $derived(
    typeId ? (session.catalog.find((d) => d.typeId === typeId) ?? null) : null,
  );
  // The analog output port to tap, or undefined if the device has none. A digital-only-output device
  // (e.g. the computer) can't be tapped without a DA — that's the 6.3 monitor chain — so the bench refuses
  // it here rather than building a digital-tap scene, which would fault the (shared, session-fatal) engine.
  const tapPort = $derived(requested ? analogOutputPort(requested) : undefined);

  // Once the engine is up and the requested device is known *and* tappable, make it the live scene (a
  // one-time hot-swap off the bootstrap, and again if the route's typeId changes). The guard on the
  // currently-loaded device id keeps this from re-swapping every re-run.
  $effect(() => {
    if (!session.ready || !requested || tapPort === undefined) return;
    if (session.scene.patch.devices[0]?.typeId === requested.typeId) return;
    session.scene = deviceScene(requested.typeId, tapPort);
    session.hotSwap();
  });
</script>

<svelte:window onpointerdown={resumeOnce} onkeydown={resumeOnce} />

<main class="workbench">
  {#if !session.ready}
    <p class="booting">Booting engine… <span class="muted">{session.status}</span></p>
  {:else if requested && tapPort !== undefined}
    <header class="head">
      <span class="name">{requested.name}</span>
      <span class="muted">{requested.typeId}</span>
      <button type="button" class="back" onclick={() => navigate("/")}>← scene view</button>
    </header>
    <BenchStage {session} desc={requested} />
  {:else if requested}
    <!-- Known device, but no analog output to tap: needs a DA / monitor chain (Story 6.3). -->
    <header class="head">
      <span class="name">{requested.name}</span>
      <span class="muted">{requested.typeId}</span>
      <button type="button" class="back" onclick={() => navigate("/")}>← scene view</button>
    </header>
    <p class="muted">
      This device has no analog output to tap — a monitor chain (DA) is needed to bench it. That lands
      in Story 6.3.
    </p>
  {:else}
    <!-- Catalog index: the bare /devices route, or an unknown typeId. -->
    <header class="head">
      <span class="name">Device workbench</span>
      {#if typeId}<span class="muted">unknown device “{typeId}”</span>{/if}
      <button type="button" class="back" onclick={() => navigate("/")}>← scene view</button>
    </header>
    <ul class="index">
      {#each session.catalog as d (d.typeId)}
        <li>
          <button type="button" class="dev" onclick={() => navigate(`/devices/${d.typeId}`)}>
            {d.name} <span class="muted">{d.typeId}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</main>

<style>
  .workbench {
    padding: 1.5rem 2rem;
    font: 15px/1.5 var(--ae-font-ui);
    color: var(--ae-text-secondary);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 0.8rem;
    margin-bottom: 1rem;
  }
  .name {
    font-size: 1.2rem;
    color: var(--ae-text-strong);
  }
  .muted {
    color: var(--ae-text-muted);
    font-size: 0.85rem;
  }
  .back {
    margin-left: auto;
    font: inherit;
    padding: 0.4em 1em;
    cursor: pointer;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  .index {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    max-width: 30rem;
  }
  .dev {
    width: 100%;
    text-align: left;
    font: inherit;
    padding: 0.5em 0.9em;
    cursor: pointer;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  .dev:hover {
    background: var(--ae-bg-panel);
  }
</style>
