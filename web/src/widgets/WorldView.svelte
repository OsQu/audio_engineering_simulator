<script module lang="ts">
  /** A point in the overlay's surface-local pixel space (y increases downward — SVG convention). */
  export interface SurfacePoint {
    x: number;
    y: number;
  }

  /** Coordinate converters the world layer exposes (to its `overlay` snippet, and to the parent via
   *  `bind:api`) so cables can be placed/measured in the surface's own space without touching the
   *  pan/zoom transform or the room height. Surface-local coords are pan/zoom-invariant.
   *  - `worldToSurface`: world mm (x right, y **up** from the floor) → surface-local (y **down** from top).
   *  - `clientToSurface`: a viewport client point (e.g. a measured DOM rect) → surface-local. */
  export interface WorldApi {
    worldToSurface: (worldX: number, worldY: number) => SurfacePoint;
    clientToSurface: (clientX: number, clientY: number) => SurfacePoint;
  }
</script>

<script lang="ts">
  // The isolated **world layer**: a pan/zoom surface that lays out devices in one space's front
  // elevation. This is the *thin interface* the rest of the UI talks to (the standing WebGL escape
  // hatch — a future canvas renderer reimplements this same prop contract). It knows only about
  // positioned boxes (`items`, in world millimetres) + pointer mechanics; it has no idea what a
  // "device" or "patch" is. Placement legality and scene mutation live in the parent.
  //
  // Coordinates: world is +x right, +y up, millimetres ≈ pixels at zoom 1. An item's `rect.y` is its
  // **bottom** edge, so it maps straight to CSS `bottom` (floor = 0); pan/zoom is one CSS transform on
  // the surface, so items stay in raw world coordinates.
  //
  // Interaction is split so it never conflicts: **panning** lives on a `.backdrop` *sibling* of the
  // devices (a device press can't bubble to a sibling, so operating a control never pans), and
  // **dragging** lives on each device's `.grip` handle (so turning a knob never moves the device).
  import type { Snippet } from "svelte";
  import type { Rect2 } from "../spatial";

  interface WorldItem {
    id: string;
    /** Front-elevation rect in world millimetres (`y` = bottom edge). */
    rect: Rect2;
  }

  interface Props {
    items: WorldItem[];
    /** Renders one item's content (a device panel or a rack frame) by id. */
    item: Snippet<[string]>;
    /** Optional per-item chrome in the top bar beside the drag grip (e.g. a device's flip / space /
     *  remove controls). The world layer stays ignorant of what these mean. */
    controls?: Snippet<[string]>;
    /** Commit a move — the item's new lower-left-front position, world mm. Only called for a legal
     *  spot; an illegal drop is ignored here and the item snaps back to its scene position. */
    onMoveTo: (id: string, x: number, y: number) => void;
    /** Legality predicate for live drag feedback + the commit gate: is `(x,y)` a legal spot for `id`? */
    canPlace?: (id: string, x: number, y: number) => boolean;
    /** When this changes (e.g. the shown space switches), the camera re-frames the new content. */
    fitKey?: string;
    /** Optional content drawn in the surface's own coordinate space, on top of the gear (e.g. patch
     *  cables). Handed a {@link WorldApi} so the parent can place things without touching the transform.
     *  The world layer stays ignorant of what this is — the standing WebGL escape hatch. */
    overlay?: Snippet<[WorldApi]>;
    /** Like `overlay`, but drawn **behind** the gear (below the device panels) — e.g. cables that should
     *  tuck behind a front-facing unit. Same {@link WorldApi}, same coordinate space. */
    underlay?: Snippet<[WorldApi]>;
    /** Bound out to the parent so it can convert coordinates outside the overlay snippet (e.g. to
     *  DOM-measure jack positions into surface space). `undefined` until the surface mounts. */
    api?: WorldApi;
  }
  let {
    items,
    item,
    controls,
    onMoveTo,
    canPlace,
    fitKey,
    overlay,
    underlay,
    api = $bindable(),
  }: Props = $props();

  // The room the surface spans, world mm. Generous so there's room to pan around; refined per-space later.
  const ROOM_WIDTH = 4000;
  const ROOM_HEIGHT = 1400;
  const MIN_ZOOM = 0.2;
  const MAX_ZOOM = 3;
  const NUDGE_MM = 50; // keyboard arrow step
  const ZOOM_SENSITIVITY = 0.0015; // zoom change per pixel of scroll (gentle; trackpad-friendly)
  const FIT_PADDING = 80; // viewport px of breathing room around the framed gear

  let panX = $state(0);
  let panY = $state(0);
  let zoom = $state(0.6);

  // The viewport element (measured for fit-to-content) and whether the user has taken over the camera.
  let viewport = $state<HTMLDivElement>();
  // The transformed surface element — its client rect is the origin for client↔surface conversion.
  let surface = $state<HTMLDivElement>();
  let userAdjusted = $state(false);

  // Surface-local coords are what the overlay draws in: the surface is `ROOM_WIDTH × ROOM_HEIGHT` and
  // carries the pan/zoom transform, so points here are invariant to pan/zoom. World mm is y-up from the
  // floor; the surface (like SVG) is y-down from the top, so `worldToSurface` flips y by ROOM_HEIGHT.
  const worldToSurface = (worldX: number, worldY: number): SurfacePoint => ({
    x: worldX,
    y: ROOM_HEIGHT - worldY,
  });
  // A viewport client point → surface-local: subtract the (post-transform) surface origin and divide out
  // the zoom. transform-origin is top-left, so the surface's client top-left is surface-local (0,0).
  const clientToSurface = (clientX: number, clientY: number): SurfacePoint => {
    const r = surface?.getBoundingClientRect();
    if (!r) return { x: 0, y: 0 };
    return { x: (clientX - r.left) / zoom, y: (clientY - r.top) / zoom };
  };
  const worldApi: WorldApi = { worldToSurface, clientToSurface };
  // Expose the converters to the parent (for jack measurement outside the overlay snippet).
  $effect(() => {
    api = worldApi;
  });

  // Active device drag (world mm + whether the current spot is legal), or null.
  let drag = $state<{ id: string; x: number; y: number; legal: boolean } | null>(null);
  // Active background pan, or null.
  let pan = $state<{ px: number; py: number; panX0: number; panY0: number } | null>(null);

  // Drag bookkeeping in screen px + the device's world origin at grab time.
  let grab = { px: 0, py: 0, worldX: 0, worldY: 0 };

  const legalAt = (id: string, x: number, y: number): boolean => canPlace?.(id, x, y) ?? true;

  function startDeviceDrag(e: PointerEvent, it: WorldItem): void {
    e.preventDefault();
    userAdjusted = true;
    grab = { px: e.clientX, py: e.clientY, worldX: it.rect.x, worldY: it.rect.y };
    drag = { id: it.id, x: it.rect.x, y: it.rect.y, legal: true };
  }

  function nudge(e: KeyboardEvent, it: WorldItem): void {
    const step =
      e.key === "ArrowLeft"
        ? [-NUDGE_MM, 0]
        : e.key === "ArrowRight"
          ? [NUDGE_MM, 0]
          : e.key === "ArrowUp"
            ? [0, NUDGE_MM]
            : e.key === "ArrowDown"
              ? [0, -NUDGE_MM]
              : null;
    if (!step) return;
    e.preventDefault();
    userAdjusted = true;
    const x = it.rect.x + step[0];
    const y = Math.max(0, it.rect.y + step[1]);
    if (legalAt(it.id, x, y)) onMoveTo(it.id, x, y);
  }

  function startPan(e: PointerEvent): void {
    userAdjusted = true;
    pan = { px: e.clientX, py: e.clientY, panX0: panX, panY0: panY };
  }

  function onPointerMove(e: PointerEvent): void {
    if (drag) {
      // Screen delta → world delta (÷ zoom); screen-y grows down, world-y grows up, so negate dy.
      const x = grab.worldX + (e.clientX - grab.px) / zoom;
      const y = Math.max(0, grab.worldY - (e.clientY - grab.py) / zoom);
      drag = { id: drag.id, x, y, legal: legalAt(drag.id, x, y) };
    } else if (pan) {
      panX = pan.panX0 + (e.clientX - pan.px);
      panY = pan.panY0 + (e.clientY - pan.py);
    }
  }

  function onPointerUp(): void {
    if (drag) {
      if (drag.legal) onMoveTo(drag.id, drag.x, drag.y);
      drag = null;
    }
    pan = null;
  }

  function onWheel(e: WheelEvent): void {
    e.preventDefault();
    userAdjusted = true;
    // Zoom by an amount proportional to the scroll distance (not a fixed step per event, which makes a
    // trackpad's many small events explode). Normalize line-mode deltas (deltaMode 1) to pixels.
    const px = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    const factor = Math.exp(-px * ZOOM_SENSITIVITY);
    const next = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom * factor));
    if (next === zoom) return;
    // Zoom toward the cursor: keep the world point under it fixed by adjusting pan. Surface maps a
    // local point s to viewport coords as v = pan + zoom·s, so v stays put when pan' = v − (next/zoom)(v − pan).
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    panX = cx - (next / zoom) * (cx - panX);
    panY = cy - (next / zoom) * (cy - panY);
    zoom = next;
  }

  // Where to render an item: its dragged preview if active, else its scene rect.
  const shown = (it: WorldItem): { x: number; y: number } =>
    drag?.id === it.id ? { x: drag.x, y: drag.y } : { x: it.rect.x, y: it.rect.y };

  // Frame all placed gear in the viewport: fit the zoom to the content bounding box (with padding) and
  // center it. Items are positioned by CSS `bottom` against the ROOM_HEIGHT-tall surface, so convert to
  // surface-local top-left coords (the space the `translate · scale` transform maps to the viewport).
  function fit(): void {
    if (!viewport || items.length === 0) return;
    const vw = viewport.clientWidth;
    const vh = viewport.clientHeight;
    let left = Infinity;
    let right = -Infinity;
    let top = Infinity;
    let bottom = -Infinity;
    for (const { rect } of items) {
      left = Math.min(left, rect.x);
      right = Math.max(right, rect.x + rect.width);
      top = Math.min(top, ROOM_HEIGHT - rect.y - rect.height);
      bottom = Math.max(bottom, ROOM_HEIGHT - rect.y);
    }
    const z = Math.min(
      MAX_ZOOM,
      Math.max(
        MIN_ZOOM,
        Math.min((vw - 2 * FIT_PADDING) / (right - left), (vh - 2 * FIT_PADDING) / (bottom - top)),
      ),
    );
    zoom = z;
    panX = vw / 2 - z * ((left + right) / 2);
    panY = vh / 2 - z * ((top + bottom) / 2);
  }

  let lastFitKey = $state<string | undefined>(undefined);

  // Frame the gear on first appearance and whenever `fitKey` changes (switching spaces re-centers,
  // re-enabling auto-fit); otherwise stop once the user takes over the camera.
  $effect(() => {
    if (!viewport || items.length === 0) return;
    if (fitKey !== lastFitKey) {
      lastFitKey = fitKey;
      userAdjusted = false;
      fit();
    } else if (!userAdjusted) {
      fit();
    }
  });
</script>

<svelte:window onpointermove={onPointerMove} onpointerup={onPointerUp} />

<div class="viewport" bind:this={viewport} onwheel={onWheel}>
  <div
    class="surface"
    bind:this={surface}
    style="width: {ROOM_WIDTH}px; height: {ROOM_HEIGHT}px;
           transform: translate({panX}px, {panY}px) scale({zoom});"
  >
    <!-- Pan backdrop: a sibling of the devices, so a device press never bubbles here. -->
    <div
      class="backdrop"
      role="application"
      aria-label="studio floor — drag to pan, scroll to zoom"
      onpointerdown={startPan}
    >
      <div class="floor"></div>
    </div>

    {#if underlay}
      <!-- Behind-the-gear layer (below the device panels): cables that should tuck behind a
           front-facing unit. Same surface-local space as the overlay. -->
      <svg
        class="underlay"
        width={ROOM_WIDTH}
        height={ROOM_HEIGHT}
        viewBox="0 0 {ROOM_WIDTH} {ROOM_HEIGHT}"
        aria-hidden="true"
      >
        {@render underlay(worldApi)}
      </svg>
    {/if}

    {#each items as it (it.id)}
      {@const p = shown(it)}
      <div
        class="device"
        class:dragging={drag?.id === it.id}
        class:illegal={drag?.id === it.id && !drag.legal}
        style="left: {p.x}px; bottom: {p.y}px; width: {it.rect.width}px; height: {it.rect.height}px;"
      >
        <div class="bar">
          <div
            class="grip"
            role="button"
            tabindex="0"
            aria-label="move {it.id}"
            onpointerdown={(e) => startDeviceDrag(e, it)}
            onkeydown={(e) => nudge(e, it)}
          >
            <span class="dots">⠿</span>
          </div>
          {#if controls}
            <div class="bar-controls">{@render controls(it.id)}</div>
          {/if}
        </div>
        <div class="content">{@render item(it.id)}</div>
      </div>
    {/each}

    {#if overlay}
      <!-- Overlay in surface-local space (SVG y-down), on top of the gear. pointer-events:none so it
           never blocks panning/dragging; individual elements can opt back in (e.g. a clickable cable). -->
      <svg
        class="overlay"
        width={ROOM_WIDTH}
        height={ROOM_HEIGHT}
        viewBox="0 0 {ROOM_WIDTH} {ROOM_HEIGHT}"
        aria-hidden="true"
      >
        {@render overlay(worldApi)}
      </svg>
    {/if}
  </div>
</div>

<style>
  .viewport {
    position: relative;
    width: 100%;
    height: 62vh;
    overflow: hidden;
    background: #2a2d31;
    border: 1px solid #1c1e21;
    border-radius: 8px;
    touch-action: none;
  }
  .surface {
    position: absolute;
    top: 0;
    left: 0;
    transform-origin: top left;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    cursor: grab;
  }
  /* Cable overlay: covers the surface, above the gear; transparent to pointers by default. */
  .overlay {
    position: absolute;
    top: 0;
    left: 0;
    overflow: visible;
    pointer-events: none;
    z-index: 5;
  }
  /* Behind-the-gear cable layer: below the device panels (z-index 1), above the backdrop. */
  .underlay {
    position: absolute;
    top: 0;
    left: 0;
    overflow: visible;
    pointer-events: none;
    z-index: 0;
  }
  .floor {
    position: absolute;
    left: 0;
    bottom: 0;
    width: 100%;
    height: 2px;
    background: repeating-linear-gradient(90deg, #555 0 20px, #444 20px 40px);
  }
  .device {
    position: absolute;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
    border-radius: 6px;
    z-index: 1;
  }
  .device.dragging {
    z-index: 10;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.55);
  }
  .device.illegal {
    outline: 2px solid #d9534f;
    outline-offset: 1px;
  }
  .bar {
    flex: none;
    height: 16px;
    display: flex;
    align-items: stretch;
    background: #3a3d42;
    color: #9aa0a6;
    font-size: 10px;
    line-height: 1;
  }
  .grip {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: grab;
    user-select: none;
  }
  .grip:focus-visible {
    outline: 2px solid #6ab0f3;
    outline-offset: -2px;
  }
  .device.dragging .grip {
    cursor: grabbing;
  }
  .bar-controls {
    flex: none;
    display: flex;
    align-items: center;
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    /* A size container so a device panel can scale its internals to the chassis box (a 1U rack unit is
       a thin strip; fixed rem content would overflow it). Panel/Jack use `cqh`/`cqw` against this. */
    container-type: size;
  }
</style>
