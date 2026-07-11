<script lang="ts">
  // The device workbench — the second view root (Epic 6): a focused single-device development view at
  // /devices/<typeId>. It constructs its own SceneSession (the shared engine consumer surface), boots it
  // **suspended** on a known-good bootstrap so the catalog arrives before any gesture, then resolves the
  // requested typeId against that catalog — hot-swapping to the bench scene (the device-under-test plus a
  // fixed supporting cast: synth source + DA + speaker, unwired) or falling back to the catalog index.
  // Audio resumes on the first interaction; the user patches source→DUT→monitor by hand (Story 6.3).

  import { isPlayable } from "./catalog";
  import { focusUi, hasFocusSurface } from "./device-ui";
  import { wireMidi } from "./engine";
  import { isFocusable } from "./focus";
  import { wireKeyboardInput } from "./keyboard-input.svelte";
  import { PatchController } from "./patch-controller.svelte";
  import { eventsInputDriven } from "./scene-ops";
  import { SceneSession } from "./session.svelte";
  import { footprint } from "./spatial";
  import { decodeScene, encodeScene } from "./url-scene";
  import BenchStage from "./BenchStage.svelte";
  import DebugPanel from "./DebugPanel.svelte";
  import Keybed from "./widgets/Keybed.svelte";
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

  // The URL's temp scene, read **once** at init (before any write can overwrite it) — the rebuild→reload
  // restore. `null` when absent / malformed / from an older schema (→ regenerate the default bench).
  const initialUrlScene = decodeScene(new URLSearchParams(location.search).get("s"));

  // The type the bench scene is currently built for — the swap guard (tracking this, not the loaded DUT
  // typeId, avoids a collision when the requested device *is* the bootstrap type, `synth_voice`).
  let benchedFor = $state<string | null>(null);

  // Once the engine is up and the requested device is known, make the bench scene the live scene — a
  // one-time hot-swap off the bootstrap, and again if the route's typeId changes. Prefer the URL-persisted
  // scene when it's for *this* device (reload restore); else the freshly-generated default bench (DUT +
  // supporting cast).
  $effect(() => {
    if (!session.ready || !requested || benchedFor === requested.typeId) return;
    const urlDut = initialUrlScene?.patch.devices.find((d) => d.id === BENCH_DEVICE)?.typeId;
    const scene =
      urlDut === requested.typeId ? initialUrlScene : benchScene(requested, session.catalog);
    if (!scene) return;
    session.scene = scene;
    session.hotSwap();
    benchedFor = requested.typeId;
  });

  // Persist the live bench scene to the URL query (debounced `replaceState` — no history spam), path kept
  // at /devices/<typeId>. Only once the device's bench scene is loaded (never the bootstrap). Reading the
  // scene through `encodeScene` registers the reactive dep, so this re-runs on any patch / param / tap edit.
  $effect(() => {
    if (!requested || benchedFor !== requested.typeId) return;
    const query = encodeScene(session.scene);
    const typeId = requested.typeId;
    const t = setTimeout(() => {
      history.replaceState(history.state, "", `/devices/${typeId}?s=${query}`);
    }, 300);
    return () => clearTimeout(t);
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
    // Esc closes the focus overlay first, else abandons an in-progress pick/drag.
    if (e.key !== "Escape") return;
    if (focusedDevice !== null) focusedDevice = null;
    else patch.cancel();
  }

  // The analog outputs across all bench devices, for the "Listen" selector — each is a monitorable tap (the
  // output is rendered as a voltage, so only analog ports qualify; a digital out is heard via the DA→speaker).
  const analogOutputs = $derived(
    session.scene.patch.devices.flatMap((d) => {
      const desc = session.catalog.find((c) => c.typeId === d.typeId);
      return (desc?.ports ?? [])
        .filter((p) => p.direction === "output" && p.domain === "analog")
        .map((p) => ({
          device: d.id,
          port: p.id,
          key: `${d.id}:${p.id}`,
          label: `${desc?.name} · ${p.label}`,
        }));
    }),
  );
  // The selector's current value ("device:port") — the scene's output tap.
  const tapKey = $derived(
    `${session.scene.patch.output.device}:${session.scene.patch.output.port}`,
  );
  function setTap(key: string): void {
    const tap = analogOutputs.find((o) => o.key === key);
    if (!tap) return;
    session.scene.patch.output = { device: tap.device, port: tap.port };
    session.hotSwap();
  }

  // --- Keyboard / keybed ----------------------------------------------------------------------------
  // Every bench device with an events (MIDI) input — the keybed's possible targets (the synth source, the
  // DUT if it's an instrument, and anything else added). "Send to" picks which one(s) the notes play.
  function descOf(deviceId: string) {
    const inst = session.scene.patch.devices.find((d) => d.id === deviceId);
    return inst ? session.catalog.find((c) => c.typeId === inst.typeId) : undefined;
  }
  const eventInputs = $derived(
    session.scene.patch.devices
      .map((d) => ({ id: d.id, desc: descOf(d.id) }))
      .filter((d) => d.desc?.ports.some((p) => p.direction === "input" && p.domain === "events"))
      .map((d) => ({ id: d.id, name: d.desc?.name ?? d.id })),
  );
  // "Send to": "all" (every event input) or a single device id. Falls back to "all" if the chosen device
  // goes away (e.g. a route change to a DUT without a MIDI input).
  let sendTo = $state<string>("all");
  $effect(() => {
    if (sendTo !== "all" && !eventInputs.some((d) => d.id === sendTo)) sendTo = "all";
  });
  // A cable-driven event input ignores host notes, so drop it from the broadcast (matches the scene view's
  // keybed-disable rule); an explicitly-selected single target is still sent (the user asked for it).
  const noteTargets = $derived(
    sendTo === "all"
      ? eventInputs
          .filter((d) => {
            const desc = descOf(d.id);
            return desc ? !eventsInputDriven(session.scene, desc, d.id) : false;
          })
          .map((d) => d.id)
      : [sendTo],
  );
  const playNote = wireKeyboardInput(session, () => noteTargets);

  // Web MIDI (hardware controllers): the scene view wires this on start; the bench must too, or a
  // plugged-in controller does nothing here. Request access once the engine is up (so notes have
  // somewhere to go) and route every note through the same `playNote` fan-out QWERTY/keybed use. A
  // cable-driven target is dropped by `noteTargets`, so MIDI follows the same "Send to" rules.
  let midiWired = false;
  $effect(() => {
    if (!session.ready || midiWired) return;
    midiWired = true;
    wireMidi(playNote, (m) => {
      session.midiStatus = m;
    });
  });

  let keybedOpen = $state(true);
  // The debug surface is a collapsible right-hand drawer (view-only state — pins persist in the scene,
  // but whether the drawer is open is ephemeral). Open by default so the instrumentation is visible.
  let debugOpen = $state(true);

  // --- Device focus mode (sit down at a device) -----------------------------------------------------
  // The same large interaction surface the scene view opens (Story 4.8) — for the 8i6 it's the Focusrite
  // Control routing matrix, which is how you actually route an input to an analog output on the bench.
  // Reuses the shared focusUi / isFocusable machinery; note input follows focus to the focused device.
  const FOCUS_FACE_WIDTH_PX = 720;
  let focusedDevice = $state<string | null>(null);
  let focusSurfaceEl = $state<HTMLElement | undefined>();
  const focused = $derived.by(() => {
    if (focusedDevice === null) return null;
    const device = session.scene.patch.devices.find((d) => d.id === focusedDevice);
    const desc = device ? descOf(device.id) : undefined;
    if (!device || !desc || !isFocusable(desc)) return null;
    return { device, desc };
  });
  // Move keyboard focus into the surface when it opens (a basic focus-trap), mirroring the scene view.
  $effect(() => {
    if (focusedDevice !== null) focusSurfaceEl?.focus();
  });
  // The focus keybed plays the *focused* device (not the bench's "Send to" fan-out) — you're sitting at it.
  // Velocity defaults in `session.playNote` (the Keybed's onNote passes only on/note, like the bench keybed).
  function focusPlayNote(on: boolean, note: number): void {
    if (focused) session.playNote(focused.device.id, on, note);
  }
</script>

<svelte:window
  onpointerdown={onPointerDown}
  onpointermove={(e) => patch.pointerMove(e, benchApi)}
  onpointerup={(e) => patch.pointerUp(e)}
  onkeydown={onKeyDown}
/>

<main class="workbench" class:drawer-open={!!requested && debugOpen}>
  {#if !session.ready}
    <p class="booting">Booting engine… <span class="muted">{session.status}</span></p>
  {:else if requested}
    <header class="head">
      <span class="name">{requested.name}</span>
      <span class="muted">{requested.typeId}</span>
      <!-- Open the DUT's focus surface (for the 8i6, the Focusrite Control routing matrix). Only shown for
           a device that has one — a converter/speaker has none. -->
      {#if isFocusable(requested)}
        <button type="button" class="open-focus" onclick={() => (focusedDevice = BENCH_DEVICE)}>
          ⛶ Open
        </button>
      {/if}
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
      <!-- Always-visible drawer toggle (the collapsed drawer has no on-stage handle to find). -->
      <button
        type="button"
        class="debug-toggle"
        aria-expanded={debugOpen}
        onclick={() => (debugOpen = !debugOpen)}
      >
        {debugOpen ? "▸" : "◂"} Debug
      </button>
      <button type="button" class="back" onclick={() => navigate("/")}>← scene view</button>
    </header>
    <BenchStage {session} desc={requested} {patch} bind:api={benchApi} />
    {#if eventInputs.length > 0}
      <!-- Play the rig via the shared keybed + QWERTY (same session.playNote / heldNotes as the scene
           view). "Send to" targets the MIDI inputs; the strip is sticky at the bottom and collapsible. -->
      <div class="keybed-row" class:collapsed={!keybedOpen}>
        <div class="keybed-head">
          <button
            type="button"
            class="collapse"
            aria-expanded={keybedOpen}
            onclick={() => (keybedOpen = !keybedOpen)}
          >
            {keybedOpen ? "▾" : "▸"} Keyboard
          </button>
          <label class="send-to">
            Send to
            <select value={sendTo} onchange={(e) => (sendTo = e.currentTarget.value)}>
              <option value="all">All MIDI inputs</option>
              {#each eventInputs as d (d.id)}
                <option value={d.id}>{d.name}</option>
              {/each}
            </select>
          </label>
        </div>
        {#if keybedOpen}
          <Keybed held={session.heldNotes} onNote={playNote} disabled={noteTargets.length === 0} />
        {/if}
      </div>
    {/if}
    <!-- The audio debug surface (Story 6.4) lives in a collapsible right-hand drawer so it doesn't add to
         the bench's vertical stack: always-on header (level/tap/latency/losses) + MIDI monitor + a
         filter+pin watch-list over the rig's params/configs/readouts. Reads the shared session. Toggled
         from the always-visible header "Debug" button; the × here just collapses it. -->
    {#if debugOpen}
      <aside class="debug-drawer">
        <div class="drawer-head">
          <span class="drawer-title">Debug</span>
          <button
            type="button"
            class="drawer-close"
            aria-label="collapse debug panel"
            onclick={() => (debugOpen = false)}>›</button
          >
        </div>
        <DebugPanel {session} />
      </aside>
    {/if}
    {#if focused}
      {@const f = focused}
      {@const Surface = focusUi(f.device.typeId)}
      <!-- Device focus overlay — the same large surface the scene view opens (Story 4.8). For the 8i6
           this is the Focusrite Control routing matrix; click the backdrop or press Esc to leave. -->
      <div
        class="focus-backdrop"
        role="button"
        tabindex="-1"
        aria-label="close focus"
        onclick={(e) => {
          if (e.target === e.currentTarget) focusedDevice = null;
        }}
        onkeydown={(e) => {
          if (e.key === "Enter") focusedDevice = null;
        }}
      >
        <div
          class="focus-surface"
          bind:this={focusSurfaceEl}
          role="dialog"
          aria-modal="true"
          aria-label={`${f.desc.name} — focus`}
          tabindex="-1"
        >
          <header class="focus-head">
            <span class="focus-name">{f.desc.name}</span>
            <button type="button" class="focus-close" onclick={() => (focusedDevice = null)}>
              Close
            </button>
          </header>
          {#snippet focusFace()}
            <Surface
              device={f.device.id}
              typeId={f.device.typeId}
              name={f.desc.name}
              params={f.desc.params}
              ports={f.desc.ports}
              readouts={f.desc.readouts}
              configs={f.desc.configs}
              valueFor={(id) => session.paramValue(f.device.id, f.desc, id)}
              readingFor={(id) => session.readingFor(f.device.id, id)}
              onParam={(p, v) => session.onParamInput(f.device.id, p, v)}
              configFor={(k) => session.configValue(f.device.id, f.desc, k)}
              onConfig={(k, v) => session.onConfigInput(f.device.id, k, v)}
            />
          {/snippet}
          <div class="focus-body">
            {#if hasFocusSurface(f.device.typeId)}
              {@render focusFace()}
            {:else}
              {@const fp = footprint(f.desc.formFactor)}
              {@const zoom = FOCUS_FACE_WIDTH_PX / fp.width}
              <div
                class="focus-zoom-sizer"
                style:width="{fp.width * zoom}px"
                style:height="{fp.height * zoom}px"
              >
                <div
                  class="focus-zoom"
                  style:width="{fp.width}px"
                  style:height="{fp.height}px"
                  style:transform="scale({zoom})"
                >
                  {@render focusFace()}
                </div>
              </div>
            {/if}
            {#if isPlayable(f.desc)}
              <Keybed
                held={session.heldNotes}
                onNote={focusPlayNote}
                disabled={eventsInputDriven(session.scene, f.desc, f.device.id)}
              />
            {/if}
          </div>
        </div>
      </div>
    {/if}
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
  /* The header pins to the top while the bench scrolls beneath it (bg hides content passing under). */
  .head {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    align-items: baseline;
    gap: 0.8rem;
    padding: 0.75rem 0;
    margin-bottom: 0.5rem;
    background: var(--ae-bg-room);
  }
  .name {
    font-size: 1.2rem;
    color: var(--ae-text-strong);
  }
  .muted {
    color: var(--ae-text-muted);
    font-size: 0.85rem;
  }
  /* "Open" opens the DUT's focus surface — a chip-styled header button, emphasised so it reads as the
     primary action (sit down at the device). */
  .open-focus {
    font: inherit;
    padding: 0.4em 1em;
    cursor: pointer;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  .open-focus:hover {
    background: var(--ae-bg-panel);
  }
  /* Device focus overlay — fixed full-viewport modal (the bench has no single relative stage container to
     anchor to like the scene view), above the debug drawer (z 20). */
  .focus-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
    background: rgb(0 0 0 / 0.55);
    cursor: default;
  }
  .focus-surface {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    max-width: min(90%, 900px);
    max-height: 90%;
    overflow: auto;
    padding: 1rem 1.2rem 1.4rem;
    background: var(--ae-bg-panel);
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-panel);
    box-shadow: var(--ae-shadow-card);
  }
  .focus-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .focus-name {
    font-size: 1rem;
    font-weight: 600;
    color: var(--ae-text-strong);
  }
  .focus-close {
    font: inherit;
    font-size: 0.72rem;
    padding: 0.2rem 0.7rem;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
    cursor: pointer;
  }
  .focus-body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.2rem;
  }
  .focus-body :global(.panel) {
    width: 100%;
    min-height: 220px;
  }
  .focus-zoom-sizer {
    position: relative;
    flex: none;
  }
  .focus-zoom {
    transform-origin: top left;
  }
  .focus-zoom :global(.panel) {
    width: 100%;
    height: 100%;
    min-height: 0;
  }

  /* The Debug toggle in the header is the always-visible open/close control (chip-styled like the other
     header buttons). */
  .debug-toggle {
    font: inherit;
    padding: 0.4em 1em;
    cursor: pointer;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  .debug-toggle[aria-expanded="true"] {
    background: var(--ae-bg-panel);
  }
  /* The debug surface is a collapsible right-hand drawer, pinned to the viewport so it never joins the
     bench's vertical stack. When open, `.workbench.drawer-open` reserves space (padding-right) so the
     header controls and stage aren't covered; the drawer scrolls independently. */
  .workbench.drawer-open {
    padding-right: 23rem;
  }
  .debug-drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 20; /* above the sticky header/keybed (z 5) and any dragged stage device (z 10) */
    width: 21rem;
    overflow-y: auto;
    padding: 1rem 1.1rem 2rem;
    background: var(--ae-bg-room);
    border-left: 1px solid var(--ae-line-panel);
    box-shadow: -10px 0 28px rgba(0, 0, 0, 0.22);
  }
  .drawer-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.7rem;
  }
  .drawer-title {
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: var(--ae-legend-spacing);
    color: var(--ae-text-muted);
  }
  .drawer-close {
    font: inherit;
    font-size: 1.1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0.1em 0.5em;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  /* The play strip pins to the bottom while the bench scrolls: a head bar (collapse + "Send to") plus the
     shared on-screen keybed (hidden when collapsed). */
  .keybed-row {
    position: sticky;
    bottom: 0;
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.6rem 0;
    margin-top: 1rem;
    background: var(--ae-bg-room);
  }
  .keybed-head {
    display: flex;
    align-items: center;
    gap: 1rem;
  }
  .collapse {
    font: inherit;
    cursor: pointer;
    padding: 0.3em 0.8em;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  .send-to {
    display: inline-flex;
    align-items: center;
    gap: 0.4em;
    font-size: 0.85rem;
    color: var(--ae-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--ae-legend-spacing);
  }
  .send-to select {
    font: inherit;
    text-transform: none;
    letter-spacing: normal;
    padding: 0.3em 0.5em;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
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
