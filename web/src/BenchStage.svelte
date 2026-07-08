<script lang="ts">
  // The workbench bench stage: one device shown at its **real dimensions** on a millimetre grid, both
  // faces at once. The device is rendered through the *same* faceplate widget (`deviceUi`) and the *same*
  // session props (valueFor/onParam/configFor/onConfig/readingFor) the scene view uses — no forked
  // rendering.
  //
  // Pan/zoom is the shared `Camera` (camera.svelte.ts — the same one WorldView drives): a
  // `transform: translate(pan)·scale(zoom)` on the whole surface, so the faceplate's fixed-px controls
  // scale uniformly (resizing the box instead would stretch the panel but leave the knobs their design
  // size). The content is laid out at natural scale (world mm ≈ px), so the mm grid and rack-U ruler read
  // true; the transform then blows it up. Wheel zooms (cursor-anchored); drag empty space to pan.
  //
  // Deliberately *not* a WorldView (the bench is one flat layout of both faces, not a spatial room), but
  // the three interaction concerns are shared, one place each: pan/zoom via `Camera`, moving a device via
  // the `draggable` action (device-drag.ts) — dragging a device body repositions it, its offset kept in
  // `scene.ui.bench` — and patching via `PatchController` + `cable-view`. It exposes a `WorldApi`-shaped
  // surface so that machinery can measure jack anchors + draw cables in surface-local space.

  import { Camera } from "./camera.svelte";
  import { type CableLayout, cableEndpoints, jackKey } from "./cable-view";
  import { draggable } from "./device-drag";
  import Cable from "./widgets/Cable.svelte";
  import CableInspector from "./widgets/CableInspector.svelte";
  import DeviceChrome from "./widgets/DeviceChrome.svelte";
  import type { DeviceDescriptor } from "./catalog";
  import { cablePathData } from "./connections";
  import { deviceUi } from "./device-ui";
  import type { PatchController } from "./patch-controller.svelte";
  import { effectiveFacing } from "./projection";
  import type { Connection } from "./scene";
  import { connectionKind, connKey, toggleBenchFacing } from "./scene-ops";
  import type { SceneSession } from "./session.svelte";
  import { footprint, RACK_UNIT_MM, type Rect2 } from "./spatial";
  import type { SurfacePoint, WorldApi } from "./world-api";
  import { BENCH_DEVICE } from "./workbench-scene";

  interface Props {
    session: SceneSession;
    desc: DeviceDescriptor;
    // The shared patching machinery (owned by the Workbench view root); the stage draws its cables +
    // measured anchors and its drag rubber-band.
    patch: PatchController;
    // The stage's coordinate seam, bound out to the Workbench for jack measurement + pointer routing.
    api?: WorldApi;
  }
  let { session, desc, patch, api = $bindable() }: Props = $props();

  // The bench's CableLayout for the shared cable-view geometry. Everything is in view. The **DUT** shows
  // both faces at once (two columns, both measured) so either face anchors precisely. A **supporting**
  // device shows one rotatable face (like the scene view): only its shown face anchors precisely, and a
  // cable to a socket on its hidden face falls back to an interior estimate near the chassis — so we
  // supply a measured chassis `rect` and clamp that estimate to the silhouette (the cables sit above the
  // flat panels, so an unclamped estimate would dangle mid-air). No front-panel tip-patch: nothing paints
  // over the cable layer on the bench.
  const benchLayout: CableLayout = {
    inView: () => true,
    faceAnchorable: (id, face) =>
      id === BENCH_DEVICE || face === effectiveFacing(session.scene, id),
    rect: (id) => deviceRects[id] ?? null,
    clampsEstimate: (id) => id !== BENCH_DEVICE,
    frontPatchOver: () => false,
  };

  // The monitored tap's measured anchor, for the "listening here" marker (the output PortRef → its jack).
  const tapAnchor = $derived.by(() => {
    const o = session.scene.patch.output;
    return patch.jackAnchors[jackKey(o.device, "output", o.port)] ?? null;
  });

  // A selected cable → the shared inspector (change type / disconnect). Cleared when it's disconnected.
  let selectedCableKey = $state<string | null>(null);
  const selectedConn = $derived(
    session.scene.patch.connections.find((c) => connKey(c) === selectedCableKey) ?? null,
  );

  // The shared pan/zoom camera (same one the scene view uses): the surface is laid out at 1 px/mm, then
  // `transform: translate(pan) scale(zoom)`d. A 1U 19" device is 482.6 × 44.45 mm — at the 3× default
  // that's ~1448 × 133 px, wide and legible. Wheel zooms (cursor-anchored); drag empty space to pan;
  // drag a device body to move it (device-drag). Wider zoom range than the room view (a lone device).
  const camera = new Camera({ zoom: 3, minZoom: 1, maxZoom: 12 });

  // The viewport (the pan grab target + wheel origin) and the transformed surface (the origin for
  // client↔surface conversion). `natW`/`natH` are the surface's natural (unscaled) size — the cables SVG
  // spans it; `clientWidth`/`Height` ignore the transform, so they track content, not the zoom.
  let viewport = $state<HTMLDivElement>();
  let surface = $state<HTMLDivElement>();
  let natW = $state(0);
  let natH = $state(0);

  function onWheel(e: WheelEvent): void {
    if (viewport) camera.wheelZoom(e, viewport);
  }

  // Drag empty space (not a device body/jack/control) to pan. A device-group press is left to its own
  // move gesture (and jacks/controls to theirs); anything else grabs the camera.
  function onViewportPointerDown(e: PointerEvent): void {
    if ((e.target as HTMLElement | null)?.closest(".device-group, .cables")) return;
    camera.startPan(e);
  }

  // The coordinate seam: client→surface is the camera's (transform-origin top-left ⇒ the surface's client
  // top-left is surface-local (0,0)). The bench's world ≡ surface mm (one flat layout), so `worldToSurface`
  // is identity.
  const worldToSurface = (worldX: number, worldY: number): SurfacePoint => ({ x: worldX, y: worldY });
  const clientToSurface = (clientX: number, clientY: number): SurfacePoint =>
    camera.clientToSurface(surface, clientX, clientY);
  const worldApi: WorldApi = { worldToSurface, clientToSurface, measureRoot: () => surface ?? null };
  $effect(() => {
    api = worldApi;
  });

  // --- Device move + rotate (shared with the scene view) --------------------------------------------
  // Each device is a group on the flat bench; dragging its body repositions it (the `draggable` action).
  // A supporting device also rotates front↔back (`toggleBenchFacing` → `effectiveFacing`, the same facing
  // model the scene view uses); the DUT shows both faces and never rotates. Committed offset + facing live
  // in `scene.ui.bench` (round-trips through the bench's URL persistence); `dragging` is the live move
  // preview. Bench placement is free — no walls/racks, overlaps allowed.
  let dragging = $state<{ id: string; x: number; y: number } | null>(null);
  function benchOffset(id: string): { x: number; y: number } {
    if (dragging?.id === id) return { x: dragging.x, y: dragging.y };
    const b = session.scene.ui.bench?.[id];
    return b ? { x: b.x, y: b.y } : { x: 0, y: 0 };
  }
  const flipDevice = (id: string): void => toggleBenchFacing(session.scene, id);

  // Measured chassis rects (surface coords) of the rotatable supporting devices — the hidden-face cable
  // estimate + its silhouette clamp read these (the bench's analog of the scene's projection-derived
  // rect). Keyed by device id off each `.device[data-device-id]` box; the DUT never needs one.
  let deviceRects = $state<Record<string, Rect2>>({});
  function measureDeviceRects(): void {
    const root = worldApi.measureRoot();
    if (!root) return;
    const next: Record<string, Rect2> = {};
    for (const el of root.querySelectorAll<HTMLElement>("[data-device-id]")) {
      const id = el.dataset.deviceId;
      if (!id) continue;
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      const tl = worldApi.clientToSurface(r.left, r.top);
      const br = worldApi.clientToSurface(r.right, r.bottom);
      next[id] = { x: tl.x, y: tl.y, width: br.x - tl.x, height: br.y - tl.y };
    }
    deviceRects = next;
  }

  // Re-measure jack anchors + device rects when the bench layout that determines them changes: a live
  // drag, a committed move/facing (`ui.bench`), the device set, or the catalog. Surface-local coords are
  // pan/zoom-invariant, so the camera needn't trigger it. Measure after paint (rAF) and again once the
  // 0.45s rotate transition settles (a just-flipped face reports its final jack positions then).
  $effect(() => {
    void dragging;
    JSON.stringify(session.scene.ui.bench ?? {});
    JSON.stringify(session.scene.patch.devices);
    void session.catalog.length;
    const remeasure = (): void => {
      patch.measure(worldApi);
      measureDeviceRects();
    };
    const raf = requestAnimationFrame(remeasure);
    const settle = setTimeout(remeasure, 480);
    return () => {
      cancelAnimationFrame(raf);
      clearTimeout(settle);
    };
  });

  // The devices to render: the scene's instances (the DUT + the fixed supporting cast) resolved against
  // the live catalog, in scene order (source → DUT → DA → speaker), each paired with its descriptor. The
  // stage renders whatever the scene holds — so a URL-restored scene (Story 6.3) needs no special-casing.
  type BenchDevice = { id: string; desc: DeviceDescriptor };
  const devices = $derived<BenchDevice[]>(
    session.scene.patch.devices
      .map((d) => {
        const found = session.catalog.find((c) => c.typeId === d.typeId);
        return found ? { id: d.id, desc: found } : null;
      })
      .filter((x): x is BenchDevice => x !== null),
  );

  // Per-device layout: real footprint (world mm) + rack-unit count (0 if not rackmount).
  function layoutOf(d: DeviceDescriptor) {
    const size = footprint(d.formFactor);
    const rackUnits = d.formFactor.kind === "rackmount" ? d.formFactor.rackUnits : 0;
    return { size, rackUnits, uTicks: Array.from({ length: rackUnits + 1 }, (_, i) => i) };
  }

  // A dimensions caption for the device-under-test: rack height in U for rackmount, W×H×D mm for desktop.
  const dims = $derived.by(() => {
    const size = footprint(desc.formFactor);
    return desc.formFactor.kind === "rackmount"
      ? `${RACK_UNIT_MM * (desc.formFactor.rackUnits ?? 0)} × ${size.width} mm · ${desc.formFactor.rackUnits}U`
      : `${size.width} × ${size.height} × ${size.depth} mm`;
  });

  // The faceplate props for one device/face — the identical descriptor-driven props App passes, bound to
  // this device instance + this session's lanes. `flipped` selects front vs back.
  function faceProps(deviceId: string, d: DeviceDescriptor, flipped: boolean) {
    return {
      device: deviceId,
      typeId: d.typeId,
      name: d.name,
      params: d.params,
      ports: d.ports,
      readouts: d.readouts,
      configs: d.configs,
      flipped,
      valueFor: (id: number) => session.paramValue(deviceId, d, id),
      readingFor: (id: number) => session.readingFor(deviceId, id),
      onParam: (p: DeviceDescriptor["params"][number], v: number) =>
        session.onParamInput(deviceId, p, v),
      configFor: (k: string) => session.configValue(deviceId, d, k),
      onConfig: (k: string, v: number) => session.onConfigInput(deviceId, k, v),
    };
  }
</script>

<div class="stage">
  <p class="dims muted">{desc.name} · {dims} · {camera.zoom.toFixed(1)} px/mm</p>

  <!-- Wheel zooms (cursor-anchored); drag empty space to pan; drag a device body to move it. -->
  <div
    class="viewport"
    role="application"
    aria-label="device bench — scroll to zoom, drag to pan, drag a device to move it"
    bind:this={viewport}
    onwheel={onWheel}
    onpointerdown={onViewportPointerDown}
  >
    <!-- Laid out at 1 px/mm, then translate·scale'd by the shared camera — grid/ruler read true and the
         faceplate controls scale. -->
    <div
      class="surface"
      bind:this={surface}
      bind:clientWidth={natW}
      bind:clientHeight={natH}
      style:transform={camera.transform}
    >
        <!-- Patch cables, drawn in surface-local coords (the same space the measured jack anchors live in)
             via the shared cable-view geometry. `pointer-events:none` so jack presses pass through to the
             faceplates; only each cable's hit-path takes clicks (disabled mid-drag so a release lands on a
             jack). Cables sit above the flat panels (no z-interleave on the bench). -->
        <svg class="cables" width={natW} height={natH} viewBox="0 0 {natW} {natH}">
          {#each session.scene.patch.connections as c (connKey(c))}
            {@const ends = cableEndpoints(benchLayout, patch.jackAnchors, c, worldApi)}
            {#if ends}
              {@const d = cablePathData(ends.a, ends.b)}
              {@const kind = connectionKind(session.scene, session.catalog, c)}
              <Cable {d} {kind} selected={connKey(c) === selectedCableKey} />
              <path
                class="cable-hit"
                {d}
                role="button"
                tabindex="-1"
                aria-label={`select cable ${connKey(c)}`}
                style:pointer-events={patch.dragCable ? "none" : "stroke"}
                onclick={() => (selectedCableKey = connKey(c))}
                onkeydown={(e: KeyboardEvent) => {
                  if (e.key === "Enter" || e.key === " ") selectedCableKey = connKey(c);
                }}
              ></path>
            {/if}
          {/each}

          {#if patch.dragCable}
            <!-- The drag rubber-band from the source jack to the cursor, coloured legal/illegal on hover. -->
            <Cable
              drag
              d={cablePathData(patch.dragCable.srcPoint, patch.dragCable.free)}
              legal={patch.dragCable.over && patch.dragCable.legal}
              illegal={patch.dragCable.over && !patch.dragCable.legal}
            />
          {/if}

          {#if tapAnchor}
            <!-- "Listening here": the monitored output tap (what the capture hears). -->
            <circle class="tap-marker" cx={tapAnchor.x} cy={tapAnchor.y} r="4.5" />
          {/if}
        </svg>

        <!-- The device-under-test plus the fixed supporting cast, stacked top→bottom by signal flow. The
             **DUT** shows both faces at once (front + back columns), with the rack-U ruler marking it as
             the centerpiece; a **supporting** device shows one rotatable face (flip button), exactly as
             the scene view renders a device. -->
        <div class="bench-stack">
          {#each devices as bd (bd.id)}
            {@const isDut = bd.id === BENCH_DEVICE}
            {@const lay = layoutOf(bd.desc)}
            {@const Faceplate = deviceUi(bd.desc.typeId)}
            {@const ruled = isDut && lay.rackUnits > 0}
            {@const facing = effectiveFacing(session.scene, bd.id)}
            <!-- The DUT lists both faces; a supporting device lists only its shown face (rotate to swap). -->
            {@const faces = isDut
              ? [
                  { flipped: false, label: "Front" },
                  { flipped: true, label: "Back" },
                ]
              : [{ flipped: facing === "back", label: facing === "back" ? "Back" : "Front" }]}
            <!-- One device: drag its body to move it on the bench (the shared `draggable` action, same as
                 the scene view). Its jacks/controls/flip opt out (DRAG_EXCLUDE); its committed offset +
                 facing live in `scene.ui.bench`. The transform is surface mm, applied inside the surface. -->
            <div
              class="device-group"
              class:dut={isDut}
              class:dragging={dragging?.id === bd.id}
              style:transform="translate({benchOffset(bd.id).x}px, {benchOffset(bd.id).y}px)"
              use:draggable={{
                origin: () => benchOffset(bd.id),
                scale: () => camera.zoom,
                onStart: () => (dragging = { id: bd.id, ...benchOffset(bd.id) }),
                onMove: (x, y) => (dragging = { id: bd.id, x, y }),
                onEnd: (x, y) => {
                  const bench = (session.scene.ui.bench ??= {});
                  bench[bd.id] = { ...bench[bd.id], x, y }; // keep any facing
                  dragging = null;
                },
              }}
            >
              {#if !isDut}
                <!-- Rotate control in the shared hover header (same pattern as the scene view): appears
                     above the device on hover. The DUT shows both faces at once, so it has no header. -->
                <DeviceChrome>
                  <button
                    type="button"
                    class="flip-btn"
                    aria-label="rotate {bd.desc.name}"
                    onclick={() => flipDevice(bd.id)}
                  >
                    ⟲ {facing === "back" ? "front" : "back"}
                  </button>
                </DeviceChrome>
              {/if}
              <span class="dev-name muted">{bd.desc.name}</span>
              <!-- Keyed by index (stable per device) so a supporting device's single face-card persists
                   across a flip — the `flipped` prop changes and the 0.45s rotate transition plays,
                   rather than the element remounting already-flipped. -->
              <div class="dev-faces">
                {#each faces as face, i (i)}
                  <div class="face-col">
                    <span class="face-label muted">{face.label}</span>
                    <div class="face-body">
                      {#if ruled && i === 0}
                        <!-- Rack-U ruler beside the front face: a tick per U boundary at 44.45 mm. -->
                        <div class="ruler" style:height="{lay.size.height}px">
                          {#each lay.uTicks as u (u)}
                            <div class="tick" style:top="{u * RACK_UNIT_MM}px">
                              {#if u < lay.rackUnits}<span class="u-label">{u + 1}U</span>{/if}
                            </div>
                          {/each}
                        </div>
                      {:else if ruled}
                        <!-- Spacer so the back face lines up under the front (which carries the ruler). -->
                        <div class="ruler-spacer"></div>
                      {/if}
                      <!-- The DUT's two columns each hide the away face (so each face measures once); a
                           supporting device is a single flip-card (like the scene view) and carries a
                           `data-device-id` so its chassis rect is measured for hidden-face cable anchoring. -->
                      <div
                        class="device"
                        class:show-front={isDut && !face.flipped}
                        class:show-back={isDut && face.flipped}
                        data-device-id={isDut ? undefined : bd.id}
                        style:width="{lay.size.width}px"
                        style:height="{lay.size.height}px"
                      >
                        <Faceplate {...faceProps(bd.id, bd.desc, face.flipped)} />
                      </div>
                    </div>
                  </div>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      </div>
  </div>

  {#if selectedConn}
    <!-- The shared cable inspector (same as the scene view): change the selected lead's type / disconnect. -->
    <CableInspector {session} {patch} conn={selectedConn} onClose={() => (selectedCableKey = null)} />
  {/if}
</div>

<style>
  .stage {
    position: relative; /* positioning context for the floating cable inspector */
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    min-height: 0;
  }
  .dims {
    margin: 0;
  }
  .muted {
    color: var(--ae-text-muted);
    font-size: 0.8rem;
  }
  /* Camera viewport: a fixed window the surface pans/zooms inside (drag empty space to pan). Clips the
     transformed surface (which is absolutely positioned, so it doesn't size the viewport — hence the
     explicit height). `touch-action: none` so a touch-drag pans rather than scrolls the page. */
  .viewport {
    position: relative;
    height: 80vh;
    overflow: hidden;
    touch-action: none;
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-control);
    background-color: var(--ae-bg-panel-2, var(--ae-bg-panel));
    cursor: grab;
  }
  /* The zoomable surface: laid out at 1 px/mm and translate·scale'd by the camera (absolute, top-left
     origin — as WorldView's). The mm grid lives here so it scales with the content (fine 10 mm lines + a
     stronger 50 mm line, both true-to-scale). */
  .surface {
    position: absolute;
    top: 0;
    left: 0;
    width: max-content;
    transform-origin: top left;
    padding: 2rem;
    background-image:
      linear-gradient(var(--ae-line-panel) 1px, transparent 1px),
      linear-gradient(90deg, var(--ae-line-panel) 1px, transparent 1px),
      linear-gradient(var(--ae-line-chip) 1px, transparent 1px),
      linear-gradient(90deg, var(--ae-line-chip) 1px, transparent 1px);
    background-size:
      50px 50px,
      50px 50px,
      10px 10px,
      10px 10px;
    background-position: -1px -1px;
  }
  /* The cables overlay covers the surface in its own (unscaled, surface-local) coordinate space — the
     same space the measured jack anchors live in. Transparent to pointers except each cable's hit-path. */
  .cables {
    position: absolute;
    top: 0;
    left: 0;
    overflow: visible;
    pointer-events: none;
    z-index: 2; /* above the flat panels */
  }
  /* Wide invisible click target over the thin cable (the visual lead is the shared <Cable>). `tabindex=-1`
     keeps it out of the tab order, so suppressing the click-focus outline (a huge rectangle around the
     cable's bounding box) costs no keyboard accessibility. */
  .cable-hit {
    fill: none;
    stroke: transparent;
    stroke-width: 8px;
    stroke-linecap: round;
    cursor: pointer;
    outline: none;
  }
  /* "Listening here" marker at the monitored output tap. */
  .tap-marker {
    fill: none;
    stroke: var(--ae-signal-mic-lit, #6cf);
    stroke-width: 1.5px;
  }
  /* The bench shows both faces at once, so each device faceplate is rendered twice (front column +
     back column). Hide the away face in each so its (mirrored, backface) jacks aren't measured — the
     shared jack keys stay unique. */
  .device.show-front :global(.face.back) {
    display: none;
  }
  .device.show-back :global(.face.front) {
    display: none;
  }
  /* The stack of devices (source → DUT → monitor), top→bottom; a wide gap separates them for cable room. */
  .bench-stack {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 90px;
    width: max-content;
  }
  /* One device: a name caption above its two faces (front + back), stacked vertically. The whole group is
     a drag handle (grab to move it on the bench); its jacks/controls opt out via the shared action. */
  .device-group {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 8px;
    cursor: grab;
  }
  .device-group.dragging {
    cursor: grabbing;
    z-index: 10; /* lift the moving device above its peers (and the cable layer) while dragging */
  }
  .dev-name {
    letter-spacing: var(--ae-legend-spacing, 0.05em);
    text-transform: uppercase;
    font-weight: 600;
  }
  /* The DUT is the centerpiece — nudge its name so the eye lands on it. */
  .device-group.dut .dev-name {
    color: var(--ae-text-strong);
  }
  .dev-faces {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 24px;
    width: max-content;
  }
  .face-col {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  /* The face's device box, with the rack-U ruler (or its alignment spacer) to its left. */
  .face-body {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }
  .ruler-spacer {
    width: 28px; /* matches .ruler width, so front + back faces line up */
    flex: none;
  }
  .face-label {
    letter-spacing: var(--ae-legend-spacing, 0.05em);
    text-transform: uppercase;
  }
  /* Reveal the shared hover header (DeviceChrome) when the device group is hovered / focused within — the
     bar's appearance + hidden default live in that component; this is just the per-view reveal trigger. */
  .device-group:hover :global(.device-chrome),
  .device-group:focus-within :global(.device-chrome) {
    opacity: 1;
    transform: none;
    pointer-events: auto;
  }
  /* Rotate a supporting device front↔back — a chip in the hover header. */
  .flip-btn {
    font: inherit;
    font-size: 10px;
    line-height: 1;
    padding: 3px 8px;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
    cursor: pointer;
  }
  .flip-btn:hover {
    background: var(--ae-bg-panel-2);
  }
  /* The device box at natural (1 px/mm) size; the faceplate fills it, the surface transform scales it.
     A size container (as WorldView's `.content` is) so the faceplate scales its internals — knobs, jacks,
     legends — to the *real* chassis box via `cqh`/`cqw`. `container-type` reads the pre-transform layout
     size (the true mm footprint), so the controls size to the device, not the viewport (without this they
     fall back to their fixed-rem caps and dwarf a small faceplate). */
  .device {
    position: relative;
    container-type: size;
  }
  .device :global(> *) {
    width: 100%;
    height: 100%;
  }
  /* Rack-U ruler down the left of the panel (sits beside the device box, below the face label). */
  .ruler {
    position: relative;
    width: 28px;
    flex: none;
    border-right: 1px solid var(--ae-line-hard, var(--ae-line-panel));
  }
  .tick {
    position: absolute;
    right: 0;
    width: 7px;
    border-top: 1px solid var(--ae-line-hard, var(--ae-line-panel));
  }
  .u-label {
    position: absolute;
    right: 10px;
    top: 2px;
    font-size: 9px;
    color: var(--ae-text-muted);
    white-space: nowrap;
  }
</style>
