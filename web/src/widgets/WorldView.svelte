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
  // **dragging** lives on the whole device body — a pointerdown that doesn't land on a control, jack,
  // or the corner chrome (see onDevicePointerDown) grabs the unit, so turning a knob never moves it.
  import type { Snippet } from "svelte";
  import { Camera } from "../camera.svelte";
  import { draggable } from "../device-drag";
  import { type Rect2, snapToGrid } from "../spatial";
  import type { SurfacePoint, WorldApi } from "../world-api";

  interface WorldItem {
    id: string;
    /** Front-elevation rect in world millimetres (`y` = bottom edge). */
    rect: Rect2;
    /** Background furniture (e.g. a rack frame). A styling hook; stacking is set via `z`. */
    background?: boolean;
    /** Stacking order (CSS z-index). The parent sets it per item to interleave items with the single
     *  cable layer (z 2): a **back-shown** device sits *below* the cables (z 1) so a lead reaches its
     *  rear sockets, a **front-shown** one *above* (z 3) so cables tuck behind its panel. A rack frame
     *  sits at the bottom. Defaults to 2. */
    z?: number;
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
    /** Free-placement grid step, world mm. A dragged/nudged item's position snaps to this grid for
     *  easier alignment (0 ⇒ no snapping). Rack mounting still snaps to U-slots in the parent's commit. */
    gridStep?: number;
    /** Optional content drawn in the surface's own coordinate space, on top of the gear (e.g. patch
     *  cables). Handed a {@link WorldApi} so the parent can place things without touching the transform.
     *  The world layer stays ignorant of what this is — the standing WebGL escape hatch. */
    overlay?: Snippet<[WorldApi]>;
    /** Like `overlay`, but drawn at the CABLE layer (z 2) — between back-facing and front-facing device
     *  panels — e.g. the patch cables. Each item's `z` decides whether it sits in front of or behind
     *  these, so a single continuous cable is occluded correctly per device. Same coordinate space. */
    cables?: Snippet<[WorldApi]>;
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
    gridStep = 0,
    overlay,
    cables,
    api = $bindable(),
  }: Props = $props();

  // The room the surface spans, world mm. Generous so there's room to pan around; refined per-space later.
  const ROOM_WIDTH = 4000;
  const ROOM_HEIGHT = 1400;
  const NUDGE_MM = 50; // keyboard arrow step
  const FIT_PADDING = 80; // viewport px of breathing room around the framed gear

  // The shared pan/zoom camera (device-drag's twin: it moves the whole view). Owns the surface transform,
  // cursor-anchored wheel zoom, and drag-to-pan; this component keeps only the world↔surface mapping + fit.
  const camera = new Camera({ zoom: 0.6, minZoom: 0.2, maxZoom: 3 });

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
  // Client→surface is the camera's (transform-origin top-left ⇒ the surface's client top-left is (0,0)).
  const clientToSurface = (clientX: number, clientY: number): SurfacePoint =>
    camera.clientToSurface(surface, clientX, clientY);
  const worldApi: WorldApi = { worldToSurface, clientToSurface, measureRoot: () => surface ?? null };
  // Expose the converters to the parent (for jack measurement outside the overlay snippet).
  $effect(() => {
    api = worldApi;
  });

  // Active device drag (world mm + whether the current spot is legal), or null. The drag *mechanics*
  // live in the shared `draggable` action (device-drag.ts); this state is just the live preview the
  // action feeds back, so `shown()` can render the item at the dragged position and flag illegal spots.
  let drag = $state<{ id: string; x: number; y: number; legal: boolean } | null>(null);

  const legalAt = (id: string, x: number, y: number): boolean => canPlace?.(id, x, y) ?? true;

  // Keyboard nudge fires only when the device box itself is focused — arrow keys bubbling up from a
  // focused control (which handles its own value change) must not also move the device.
  function onDeviceKey(e: KeyboardEvent, it: WorldItem): void {
    if (e.target !== e.currentTarget) return;
    nudge(e, it);
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
    const x = snapToGrid(it.rect.x + step[0], gridStep);
    const y = Math.max(0, snapToGrid(it.rect.y + step[1], gridStep));
    if (legalAt(it.id, x, y)) onMoveTo(it.id, x, y);
  }

  // A backdrop press starts a camera drag-pan (self-contained: it captures the pointer). A device press
  // never reaches the backdrop (the devices are siblings above it), so it drives the move action instead.
  function startPan(e: PointerEvent): void {
    userAdjusted = true;
    camera.startPan(e);
  }

  // Wheel zooms toward the cursor (the camera keeps the surface point under it fixed).
  function onWheel(e: WheelEvent): void {
    if (!viewport) return;
    userAdjusted = true;
    camera.wheelZoom(e, viewport);
  }

  // Where to render an item: its dragged preview if active, else its scene rect.
  const shown = (it: WorldItem): { x: number; y: number } =>
    drag?.id === it.id ? { x: drag.x, y: drag.y } : { x: it.rect.x, y: it.rect.y };

  // Frame all placed gear in the viewport: fit the zoom to the content bounding box (with padding) and
  // center it. Items are positioned by CSS `bottom` against the ROOM_HEIGHT-tall surface, so convert to
  // surface-local top-left coords (the space the `translate · scale` transform maps to the viewport).
  function fit(): void {
    if (!viewport || items.length === 0) return;
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
    camera.frame(
      viewport.clientWidth,
      viewport.clientHeight,
      { left, top, right, bottom },
      FIT_PADDING,
    );
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

<div class="viewport" bind:this={viewport} onwheel={onWheel}>
  <div
    class="surface"
    bind:this={surface}
    style="width: {ROOM_WIDTH}px; height: {ROOM_HEIGHT}px; transform: {camera.transform};"
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

    {#if cables}
      <!-- Cable layer (z 2): sits between back-facing device panels (below) and front-facing ones
           (above), so one continuous cable plugs into a visible back socket yet tucks behind a front. -->
      <svg
        class="cables"
        width={ROOM_WIDTH}
        height={ROOM_HEIGHT}
        viewBox="0 0 {ROOM_WIDTH} {ROOM_HEIGHT}"
        aria-hidden="true"
      >
        {@render cables(worldApi)}
      </svg>
    {/if}

    {#each items as it (it.id)}
      {@const p = shown(it)}
      <!-- The whole device body is the drag surface (grab anywhere to move); controls / jacks / the
           corner chrome opt out in onDevicePointerDown, so operating them never moves the unit. -->
      <div
        class="device"
        class:background={it.background}
        class:dragging={drag?.id === it.id}
        class:illegal={drag?.id === it.id && !drag.legal}
        style="left: {p.x}px; bottom: {p.y}px; width: {it.rect.width}px; height: {it.rect.height}px;
               z-index: {drag?.id === it.id ? 10 : (it.z ?? 2)};"
        role="button"
        tabindex="0"
        aria-label="{it.id} — drag to move"
        use:draggable={{
          origin: () => ({ x: it.rect.x, y: it.rect.y }),
          scale: () => camera.zoom,
          invertY: true,
          gridStep,
          clampFloor: true,
          canPlace: (x, y) => legalAt(it.id, x, y),
          onStart: () => {
            userAdjusted = true;
            drag = { id: it.id, x: it.rect.x, y: it.rect.y, legal: true };
          },
          onMove: (x, y, legal) => (drag = { id: it.id, x, y, legal }),
          onEnd: (x, y, legal) => {
            if (legal) onMoveTo(it.id, x, y);
            drag = null;
          },
        }}
        onkeydown={(e) => onDeviceKey(e, it)}
      >
        <div class="content">{@render item(it.id)}</div>
        {#if controls}
          <!-- Chrome (open / flip / space / remove) rides a slim bar across the device's top edge,
               revealed only on hover (or keyboard focus within the device) so it never clutters the
               faceplate. Its buttons/selects opt out of the body drag in onDevicePointerDown. -->
          <div class="chrome">{@render controls(it.id)}</div>
        {/if}
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
    height: 100%; /* fill the parent stage (App makes it a flex-fill full-height area) */
    overflow: hidden;
    background: var(--ae-bg-room);
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
  /* Cable layer: fixed at z 2. Items sort around it by facing (back-facing < 2 < front-facing), so a
     single continuous cable is drawn in front of the panels it plugs into and behind the ones it tucks
     under — see WorldItem.z. */
  .cables {
    position: absolute;
    top: 0;
    left: 0;
    overflow: visible;
    pointer-events: none;
    z-index: 2;
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
    /* No overflow clip here: the hover toolbar (`.chrome`) is a child positioned ABOVE the chassis and
       must not be clipped. The rounded-corner clip lives on `.content` instead. */
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
    border-radius: 6px;
    /* z-index is set inline per item (WorldItem.z) so panels interleave with the cable layer by facing;
       a dragged item is lifted above everything. */
    cursor: grab; /* the body is the drag surface; controls override with their own cursor */
  }
  .device.dragging {
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.55);
    cursor: grabbing;
  }
  .device.illegal {
    outline: 2px solid #d9534f;
    outline-offset: 1px;
  }
  .device:focus-visible {
    outline: 2px solid var(--ae-signal-mic-lit);
    outline-offset: 1px;
  }
  /* Chrome (open / flip / space / remove) — a slim floating toolbar that sits just ABOVE the chassis
     (not over the faceplate), outside the drag surface (its buttons/select opt out of the body drag in
     onDevicePointerDown). Hidden until the device is hovered or focused within, so it never clutters the
     view. Flush to the top edge (no gap) so moving the cursor from the panel onto the toolbar keeps the
     device hovered — no flicker. It escapes the chassis clip because overflow:hidden lives on `.content`,
     not `.device`. */
  .chrome {
    position: absolute;
    bottom: 100%;
    left: 0;
    right: 0;
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 4px;
    background: var(--ae-bg-panel);
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-control) var(--ae-radius-control) 0 0;
    box-shadow: 0 -4px 12px rgba(0, 0, 0, 0.4);
    opacity: 0;
    transform: translateY(6px);
    pointer-events: none;
    transition:
      opacity 0.12s ease,
      transform 0.12s ease;
  }
  .device:hover .chrome,
  .device:focus-within .chrome {
    opacity: 1;
    transform: none;
    pointer-events: auto;
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    border-radius: 6px; /* the chassis clip (moved off `.device` so the hover toolbar can sit above it) */
    /* A size container so a device panel can scale its internals to the chassis box (a 1U rack unit is
       a thin strip; fixed rem content would overflow it). Panel/Jack use `cqh`/`cqw` against this. */
    container-type: size;
  }
</style>
