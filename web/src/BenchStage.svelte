<script lang="ts">
  // The workbench bench stage: one device shown at its **real dimensions** on a millimetre grid, both
  // faces at once. The device is rendered through the *same* faceplate widget (`deviceUi`) and the *same*
  // session props (valueFor/onParam/configFor/onConfig/readingFor) the scene view uses — no forked
  // rendering.
  //
  // Zoom is a `transform: scale` on the whole surface — exactly how WorldView zooms, so the faceplate's
  // fixed-px controls scale uniformly (resizing the box instead would stretch the panel but leave the
  // knobs their design size). The content is laid out at natural scale (world mm ≈ px), so the mm grid and
  // rack-U ruler read true; the transform then blows it up. A `transform` doesn't create scroll extent, so
  // a `sizer` sits under it at the *scaled* size to give the scroll container its scrollbars (pan).
  //
  // Deliberately *not* a WorldView: the bench is one bolted-down device, not a spatial room. So it keeps
  // scrollbar pan (rather than WorldView's translate-pan + drag-backdrop) but brings the zoom to parity —
  // **cursor-anchored** — and exposes a `WorldApi`-shaped surface so the shared patching machinery
  // (`PatchController` + `cable-view`) can measure jack anchors + draw cables in surface-local space
  // (Story 6.3). Cables themselves land in a later 6.3 task; this task is the surface + zoom.

  import { tick } from "svelte";
  import type { DeviceDescriptor } from "./catalog";
  import { deviceUi } from "./device-ui";
  import type { SceneSession } from "./session.svelte";
  import { footprint, RACK_UNIT_MM } from "./spatial";
  import type { SurfacePoint, WorldApi } from "./world-api";
  import { BENCH_DEVICE } from "./workbench-scene";

  interface Props {
    session: SceneSession;
    desc: DeviceDescriptor;
    // The stage's coordinate seam, exposed for the shared patching machinery (wired in a later 6.3 task).
    api?: WorldApi;
  }
  let { session, desc, api = $bindable() }: Props = $props();

  // The zoom, in px per mm (the surface is laid out at 1 px/mm, then `transform: scale(scale)`d). A 1U 19"
  // device is 482.6 × 44.45 mm — at the 3× default that's ~1448 × 133 px, wide and legible. Wheel zooms
  // (like the scene view); the surface scrolls (scrollbars) for anything larger than the viewport.
  let scale = $state(3);
  const MIN_SCALE = 1;
  const MAX_SCALE = 12;
  const ZOOM_SENSITIVITY = 0.0015; // zoom change per px of scroll (gentle; trackpad-friendly), as WorldView

  // The scroll container (pan) and the transformed surface (the origin for client↔surface conversion).
  let viewport = $state<HTMLDivElement>();
  let surface = $state<HTMLDivElement>();

  // The surface's natural (unscaled) layout size, measured so the `sizer` can advertise the scaled extent
  // to the scroll container. `clientWidth`/`Height` ignore the transform, so these track content (the
  // device), not the zoom — no feedback loop with `scale`.
  let natW = $state(0);
  let natH = $state(0);

  // Cursor-anchored zoom: keep the surface point under the cursor fixed by adjusting the scroll offset.
  // The surface maps a local point `s` to a viewport-local x of `s·scale − scrollLeft`, so the point under
  // the cursor is `s = (vx + scrollLeft) / scale`; after re-scaling we solve `scrollLeft' = s·scale' − vx`.
  // The sizer's scaled extent only updates after the reactive flush, so wait a `tick()` before scrolling.
  async function onWheel(e: WheelEvent): Promise<void> {
    e.preventDefault();
    if (!viewport) return;
    // Proportional to scroll distance (not a fixed step, which makes a trackpad explode); normalize
    // line-mode deltas to px.
    const px = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    const factor = Math.exp(-px * ZOOM_SENSITIVITY);
    const next = Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale * factor));
    if (next === scale) return;

    const vpRect = viewport.getBoundingClientRect();
    const vx = e.clientX - vpRect.left;
    const vy = e.clientY - vpRect.top;
    const sx = (vx + viewport.scrollLeft) / scale;
    const sy = (vy + viewport.scrollTop) / scale;

    scale = next;
    await tick();
    if (!viewport) return;
    viewport.scrollLeft = sx * next - vx;
    viewport.scrollTop = sy * next - vy;
  }

  // The coordinate seam (built once; its methods read live `scale`/`surface`, as WorldView does).
  // `clientToSurface` subtracts the (post-transform, post-scroll) surface origin and divides out the zoom —
  // transform-origin is top-left, so the surface's client top-left is surface-local (0,0). The bench's
  // world ≡ surface mm (one bolted-down layout, no room flip), so `worldToSurface` is identity.
  const worldToSurface = (worldX: number, worldY: number): SurfacePoint => ({ x: worldX, y: worldY });
  const clientToSurface = (clientX: number, clientY: number): SurfacePoint => {
    const r = surface?.getBoundingClientRect();
    if (!r) return { x: 0, y: 0 };
    return { x: (clientX - r.left) / scale, y: (clientY - r.top) / scale };
  };
  const worldApi: WorldApi = { worldToSurface, clientToSurface, measureRoot: () => surface ?? null };
  $effect(() => {
    api = worldApi;
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
  <p class="dims muted">{desc.name} · {dims} · {scale.toFixed(1)} px/mm</p>

  <!-- Wheel zooms (cursor-anchored); the surface scrolls (scrollbars) to pan. -->
  <div
    class="viewport"
    role="application"
    aria-label="device bench — scroll to zoom, scrollbars to pan"
    bind:this={viewport}
    onwheel={onWheel}
  >
    <!-- Advertises the scaled extent so the viewport shows scrollbars (transform alone wouldn't). -->
    <div class="sizer" style:width="{natW * scale}px" style:height="{natH * scale}px">
      <!-- Laid out at 1 px/mm, then scaled — so grid/ruler read true and the faceplate controls scale. -->
      <div
        class="surface"
        bind:this={surface}
        bind:clientWidth={natW}
        bind:clientHeight={natH}
        style:transform="scale({scale})"
      >
        <!-- The device-under-test plus the fixed supporting cast, stacked top→bottom by signal flow. Each
             device shows both faces (front above back); the rack-U ruler marks the DUT (the centerpiece). -->
        <div class="bench-stack">
          {#each devices as bd (bd.id)}
            {@const lay = layoutOf(bd.desc)}
            {@const Faceplate = deviceUi(bd.desc.typeId)}
            {@const ruled = bd.id === BENCH_DEVICE && lay.rackUnits > 0}
            <div class="device-group" class:dut={bd.id === BENCH_DEVICE}>
              <span class="dev-name muted">{bd.desc.name}</span>
              <div class="dev-faces">
                {#each [{ flipped: false, label: "Front" }, { flipped: true, label: "Back" }] as face, i (face.label)}
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
                      <div class="device" style:width="{lay.size.width}px" style:height="{lay.size.height}px">
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
  </div>
</div>

<style>
  .stage {
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
  /* Scroll viewport for the zoomed surface (pan via scrollbars). */
  .viewport {
    overflow: auto;
    max-height: 80vh;
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-control);
    background-color: var(--ae-bg-panel-2, var(--ae-bg-panel));
  }
  /* Holds the transformed surface at its scaled size, so the viewport can scroll around it. */
  .sizer {
    position: relative;
  }
  /* The zoomable surface: laid out at 1 px/mm and scaled by the transform. The mm grid lives here so it
     scales with the content (fine 10 mm lines + a stronger 50 mm line, both true-to-scale). */
  .surface {
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
  /* The stack of devices (source → DUT → monitor), top→bottom; a wide gap separates them for cable room. */
  .bench-stack {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 90px;
    width: max-content;
  }
  /* One device: a name caption above its two faces (front + back), stacked vertically. */
  .device-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
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
  /* The device box at natural (1 px/mm) size; the faceplate fills it, the surface transform scales it. */
  .device {
    position: relative;
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
