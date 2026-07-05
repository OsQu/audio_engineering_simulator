<script lang="ts">
  // The device workbench — the second view root (Epic 6): a focused single-device development view at
  // /devices/<typeId>. It constructs its own SceneSession (the shared engine consumer surface), boots it
  // **suspended** on a known-good bootstrap so the catalog arrives before any gesture, then resolves the
  // requested typeId against that catalog — hot-swapping to the bench scene (the device-under-test plus a
  // fixed supporting cast: synth source + DA + speaker, unwired) or falling back to the catalog index.
  // Audio resumes on the first interaction; the user patches source→DUT→monitor by hand (Story 6.3).

  import BenchStage from "./BenchStage.svelte";
  import { SceneSession } from "./session.svelte";
  import { BENCH_DEVICE, benchScene, bootstrapScene } from "./workbench-scene";

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

  // Once the engine is up and the requested device is known, make the bench scene (DUT + supporting cast)
  // the live scene — a one-time hot-swap off the bootstrap, and again if the route's typeId changes. The
  // guard on the currently-loaded DUT instance keeps this from re-swapping every re-run. `benchScene`
  // returns undefined only on a catalog regression (no speaker / no analog tap), which leaves the bootstrap.
  $effect(() => {
    if (!session.ready || !requested) return;
    const loaded = session.scene.patch.devices.find((d) => d.id === BENCH_DEVICE)?.typeId;
    if (loaded === requested.typeId) return;
    const scene = benchScene(requested, session.catalog);
    if (!scene) return;
    session.scene = scene;
    session.hotSwap();
  });
</script>

<svelte:window onpointerdown={resumeOnce} onkeydown={resumeOnce} />

<main class="workbench">
  {#if !session.ready}
    <p class="booting">Booting engine… <span class="muted">{session.status}</span></p>
  {:else if requested}
    <header class="head">
      <span class="name">{requested.name}</span>
      <span class="muted">{requested.typeId}</span>
      <button type="button" class="back" onclick={() => navigate("/")}>← scene view</button>
    </header>
    <BenchStage {session} desc={requested} />
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
