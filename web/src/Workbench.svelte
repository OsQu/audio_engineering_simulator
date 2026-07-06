<script lang="ts">
  // The device workbench — the second view root (Epic 6): a focused single-device development view at
  // /devices/<typeId>. It constructs its own SceneSession (the shared engine consumer surface), boots it
  // **suspended** on a known-good bootstrap so the catalog arrives before any gesture, then resolves the
  // requested typeId against that catalog — hot-swapping to the bench scene (the device-under-test plus a
  // fixed supporting cast: synth source + DA + speaker, unwired) or falling back to the catalog index.
  // Audio resumes on the first interaction; the user patches source→DUT→monitor by hand (Story 6.3).

  import BenchStage from "./BenchStage.svelte";
  import { PatchController } from "./patch-controller.svelte";
  import { SceneSession } from "./session.svelte";
  import type { WorldApi } from "./world-api";
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
    session.scene = benchScene(requested, session.catalog) ?? session.scene;
    session.hotSwap();
  });

  // --- Patching -------------------------------------------------------------------------------------
  // The shared patch controller (identical machinery to the scene view) + the bench stage's coordinate
  // seam, bound back from BenchStage so the window pointer handlers + jack measurement can use it.
  const patch = new PatchController(session);
  let benchApi = $state<WorldApi | undefined>();

  // Re-measure jack anchors when the layout that determines them changes (engine ready, the api mounting,
  // the scene's devices/connections, the catalog). Pan/zoom needn't trigger it — surface-local coords are
  // invariant. Measure after paint (rAF) and again shortly after (fonts/layout settle).
  $effect(() => {
    void session.ready;
    void benchApi;
    void session.catalog.length;
    JSON.stringify(session.scene.patch.devices);
    JSON.stringify(session.scene.patch.connections);
    const raf = requestAnimationFrame(() => patch.measure(benchApi));
    const settle = setTimeout(() => patch.measure(benchApi), 120);
    return () => {
      cancelAnimationFrame(raf);
      clearTimeout(settle);
    };
  });

  // Patching feels **identical to the scene view** — the same PatchController flow, so both click-to-pick
  // (click a source jack, then a destination jack) and drag work. The pointer handlers just delegate; the
  // monitored tap is chosen from the "Listen" selector in the header, not by a jack click.
  function onPointerDown(e: PointerEvent): void {
    resumeOnce();
    patch.pointerDown(e, benchApi);
  }

  function onKeyDown(e: KeyboardEvent): void {
    resumeOnce();
    if (e.key === "Escape") patch.cancel(); // abandon an in-progress pick/drag
  }

  // The analog outputs across all bench devices, for the "Listen" selector — each is a monitorable tap (the
  // output is rendered as a voltage, so only analog ports qualify; a digital out is heard via the DA→speaker).
  const analogOutputs = $derived(
    session.scene.patch.devices.flatMap((d) => {
      const desc = session.catalog.find((c) => c.typeId === d.typeId);
      return (desc?.ports ?? [])
        .filter((p) => p.direction === "output" && p.domain === "analog")
        .map((p) => ({ device: d.id, port: p.id, key: `${d.id}:${p.id}`, label: `${desc?.name} · ${p.label}` }));
    }),
  );
  // The selector's current value ("device:port") — the scene's output tap.
  const tapKey = $derived(`${session.scene.patch.output.device}:${session.scene.patch.output.port}`);
  function setTap(key: string): void {
    const tap = analogOutputs.find((o) => o.key === key);
    if (!tap) return;
    session.scene.patch.output = { device: tap.device, port: tap.port };
    session.hotSwap();
  }
</script>

<svelte:window
  onpointerdown={onPointerDown}
  onpointermove={(e) => patch.pointerMove(e, benchApi)}
  onpointerup={(e) => patch.pointerUp(e)}
  onkeydown={onKeyDown}
/>

<main class="workbench">
  {#if !session.ready}
    <p class="booting">Booting engine… <span class="muted">{session.status}</span></p>
  {:else if requested}
    <header class="head">
      <span class="name">{requested.name}</span>
      <span class="muted">{requested.typeId}</span>
      <!-- Listen: which analog output feeds the monitor (the audible tap). Patching is jack-only, so this
           picks the tap without overloading a jack click. -->
      <label class="listen">
        Listen
        <select
          aria-label="monitored output"
          value={tapKey}
          onchange={(e) => setTap(e.currentTarget.value)}
        >
          {#each analogOutputs as o (o.key)}
            <option value={o.key}>{o.label}</option>
          {/each}
        </select>
      </label>
      <button type="button" class="back" onclick={() => navigate("/")}>← scene view</button>
    </header>
    <BenchStage {session} desc={requested} {patch} bind:api={benchApi} />
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
  /* "Listen" tap selector — pushed to the right, beside the back button. */
  .listen {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 0.4em;
    font-size: 0.85rem;
    color: var(--ae-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--ae-legend-spacing);
  }
  .listen select {
    font: inherit;
    text-transform: none;
    letter-spacing: normal;
    padding: 0.3em 0.5em;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  .back {
    font: inherit;
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
