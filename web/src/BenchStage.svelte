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

  const Faceplate = $derived(deviceUi(desc.typeId));
  const size = $derived(footprint(desc.formFactor)); // world mm
  const rackUnits = $derived(desc.formFactor.kind === "rackmount" ? desc.formFactor.rackUnits : 0);

  // A dimensions caption: rack height in U for rackmount, W×H×D mm for desktop.
  const dims = $derived(
    desc.formFactor.kind === "rackmount"
      ? `${RACK_UNIT_MM * rackUnits} × ${size.width} mm · ${rackUnits}U`
      : `${size.width} × ${size.height} × ${size.depth} mm`,
  );

  // The faceplate props for one face — the identical descriptor-driven props App passes, bound to the
  // bench's single device instance + this session's lanes. `flipped` selects front vs back.
  function faceProps(flipped: boolean) {
    return {
      device: BENCH_DEVICE,
      typeId: desc.typeId,
      name: desc.name,
      params: desc.params,
      ports: desc.ports,
      readouts: desc.readouts,
      configs: desc.configs,
      flipped,
      valueFor: (id: number) => session.paramValue(BENCH_DEVICE, desc, id),
      readingFor: (id: number) => session.readingFor(BENCH_DEVICE, id),
      onParam: (p: DeviceDescriptor["params"][number], v: number) =>
        session.onParamInput(BENCH_DEVICE, p, v),
      configFor: (k: string) => session.configValue(BENCH_DEVICE, desc, k),
      onConfig: (k: string, v: number) => session.onConfigInput(BENCH_DEVICE, k, v),
    };
  }

  // U-slot tick offsets (mm from the top) for the rack ruler — one line per U boundary.
  const uTicks = $derived(Array.from({ length: rackUnits + 1 }, (_, i) => i));
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
        <div class="faces">
          {#if rackUnits > 0}
            <!-- Rack-unit ruler: a tick per U boundary at 44.45 mm, matching the (natural) panel height. -->
            <div class="ruler" style:height="{size.height}px">
              {#each uTicks as u (u)}
                <div class="tick" style:top="{u * RACK_UNIT_MM}px">
                  {#if u < rackUnits}<span class="u-label">{u + 1}U</span>{/if}
                </div>
              {/each}
            </div>
          {/if}

          {#each [{ flipped: false, label: "Front" }, { flipped: true, label: "Back" }] as face (face.label)}
            <div class="face-col">
              <span class="face-label muted">{face.label}</span>
              <div class="device" style:width="{size.width}px" style:height="{size.height}px">
                <Faceplate {...faceProps(face.flipped)} />
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
  .faces {
    display: flex;
    align-items: flex-start;
    gap: 40px;
    width: max-content;
  }
  .face-col {
    display: flex;
    flex-direction: column;
    gap: 6px;
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
  /* Rack-U ruler down the left of the panel. */
  .ruler {
    position: relative;
    width: 28px;
    margin-top: 22px; /* clear the face label row so ticks line up with the panel top */
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
