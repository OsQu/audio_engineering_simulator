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
  // **dragging** lives on the whole device body — a pointerdown that doesn't land on a control or jack
  // (see DRAG_EXCLUDE) grabs the unit, so turning a knob never moves it.
  //
  // Physical order (Story 10.5) is resolver-owned: `items` arrive in their total paint order and each
  // carries a semantic layer mapped to one fixed z band. Hover, focus, toolbar grace and dragging never
  // touch a chassis' z. Toolbars and the drag/legality outline live in a separate **interaction layer**
  // keyed by object ID, so revealing them can't raise (or trap) a chassis.
  import type { Snippet } from "svelte";
  import { Camera } from "../camera.svelte";
  import { draggable } from "../device-drag";
  import { type Rect2, snapToGrid } from "../spatial";
  import { LAYER_Z, type ObjectLayer } from "../view-resolution";
  import type { SurfacePoint, WorldApi } from "../world-api";
  import DeviceChrome from "./DeviceChrome.svelte";

  interface WorldItem {
    id: string;
    /** Front-elevation rect in world millimetres (`y` = bottom edge). */
    rect: Rect2;
    /** The resolved semantic paint layer. Items arrive already sorted in total paint order. */
    layer: ObjectLayer;
  }

  interface Props {
    items: WorldItem[];
    /** Renders one item's content (a device panel or a rack frame) by id. */
    item: Snippet<[string]>;
    /** Optional per-item chrome in the top bar beside the drag grip (e.g. a device's flip / space /
     *  remove controls). The world layer stays ignorant of what these mean. */
    controls?: Snippet<[string]>;
    /** Commit a keyboard nudge — the item's new lower-left-front position, world mm. */
    onMoveTo: (id: string, x: number, y: number) => void;
    /** Parent-owned drag state. Item geometry itself already arrives through `items`. */
    drag?: { id: string; legal: boolean } | null;
    /** Reports each pointer candidate; the parent derives the transient placement overlay. */
    onPreviewCandidate?: (id: string, x: number, y: number, legal: boolean) => void;
    /** A legal pointer release, for the parent to commit. */
    onPreviewCommit?: (id: string, x: number, y: number) => void;
    /** Pointer cancellation, for the parent to clear its overlay without committing. */
    onPreviewCancel?: () => void;
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
    /** Like `overlay`, but drawn at the CABLE layer — between rear-object and front-object panels —
     *  e.g. the patch cables. Each item's layer decides whether it sits in front of or behind these, so
     *  a single continuous cable is occluded correctly per device. Same coordinate space. */
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
    drag = null,
    onPreviewCandidate,
    onPreviewCommit,
    onPreviewCancel,
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
  const worldApi: WorldApi = {
    worldToSurface,
    clientToSurface,
    stageRoot: () => surface ?? null,
  };
  // Expose the converters to the parent (for jack measurement outside the overlay snippet).
  $effect(() => {
    api = worldApi;
  });

  // A toolbar can be wider than the device that reveals it. Keep it open briefly after the pointer
  // leaves the chassis so a diagonal move toward an overhanging control (usually remove, at the far
  // right) can reach the toolbar. The toolbar lives in the interaction layer, not inside the chassis, so
  // its own enter/focus cancels the close. Nothing here changes physical stacking.
  let openChromeId = $state<string | null>(null);
  let chromeCloseTimer: ReturnType<typeof setTimeout> | undefined;
  function showChrome(id: string): void {
    if (chromeCloseTimer !== undefined) clearTimeout(chromeCloseTimer);
    chromeCloseTimer = undefined;
    openChromeId = id;
  }
  function hideChromeSoon(id: string): void {
    if (chromeCloseTimer !== undefined) clearTimeout(chromeCloseTimer);
    chromeCloseTimer = setTimeout(() => {
      if (openChromeId === id) openChromeId = null;
      chromeCloseTimer = undefined;
    }, 300);
  }

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
      <!-- The whole device body is the drag surface (grab anywhere to move); controls / jacks opt out via
           DRAG_EXCLUDE, so operating them never moves the unit. Its z is its layer's band — always. -->
      <div
        class="device"
        data-stage-object={it.id}
        data-paint-layer={it.layer}
        class:background={it.layer === "furniture"}
        class:dragging={drag?.id === it.id}
        style="left: {it.rect.x}px; bottom: {it.rect.y}px; width: {it.rect.width}px; height: {it
          .rect.height}px; z-index: {LAYER_Z[it.layer]};"
        role="button"
        tabindex="0"
        aria-label="{it.id} — drag to move"
        onpointerenter={() => showChrome(it.id)}
        onpointerleave={() => hideChromeSoon(it.id)}
        onfocusin={() => showChrome(it.id)}
        onfocusout={() => hideChromeSoon(it.id)}
        use:draggable={{
          origin: () => ({ x: it.rect.x, y: it.rect.y }),
          scale: () => camera.zoom,
          invertY: true,
          gridStep,
          clampFloor: true,
          canPlace: (x, y) => legalAt(it.id, x, y),
          onStart: () => {
            userAdjusted = true;
          },
          onMove: (x, y, legal) => onPreviewCandidate?.(it.id, x, y, legal),
          onEnd: (x, y, legal) => {
            if (legal) onPreviewCommit?.(it.id, x, y);
            else onPreviewCancel?.();
          },
          onCancel: () => onPreviewCancel?.(),
        }}
        onkeydown={(e) => onDeviceKey(e, it)}
      >
        <div class="content">{@render item(it.id)}</div>
      </div>
    {/each}

    <!-- Interaction layer: never physical. Per-object toolbars (keyed by object ID, positioned from the
         resolved rect) and the drag/legality outline. The layer itself is transparent to pointers; only
         the toolbar controls take hits. A rack's outline is its frame, which bounds its occupants, so the
         feedback follows the whole assembly without covering it. -->
    <div class="interaction" style="z-index: {LAYER_Z.interaction};">
      {#each items as it (it.id)}
        {#if drag?.id === it.id}
          <div
            class="drag-outline"
            data-drag-outline={it.id}
            class:illegal={!drag.legal}
            style="left: {it.rect.x}px; bottom: {it.rect.y}px; width: {it.rect.width}px; height: {it
              .rect.height}px;"
          ></div>
        {/if}
      {/each}
      {#if controls}
        {#each items as it (it.id)}
          <!-- The anchor box spans the chassis' top edge; DeviceChrome sits flush above it. -->
          <div
            class="object-chrome"
            data-chrome-for={it.id}
            role="toolbar"
            tabindex="-1"
            aria-label="{it.id} controls"
            class:open={openChromeId === it.id}
            style="left: {it.rect.x}px; bottom: {it.rect.y + it.rect.height}px; width: {it.rect
              .width}px;"
            onpointerenter={() => showChrome(it.id)}
            onpointerleave={() => hideChromeSoon(it.id)}
            onfocusin={() => showChrome(it.id)}
            onfocusout={() => hideChromeSoon(it.id)}
          >
            <DeviceChrome>{@render controls(it.id)}</DeviceChrome>
          </div>
        {/each}
      {/if}
    </div>

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
    /* No overflow clip here: the hover header (DeviceChrome) is a child positioned ABOVE the chassis and
       must not be clipped. The rounded-corner clip lives on `.content` instead. */
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
    border-radius: 6px;
    /* z-index is set inline from the item's resolved layer band (LAYER_Z) — never from interaction. */
    cursor: grab; /* the body is the drag surface; controls override with their own cursor */
  }
  .device.dragging {
    cursor: grabbing;
  }
  /* The interaction layer spans the surface but never takes pointer hits itself. */
  .interaction {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .drag-outline {
    position: absolute;
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.55);
    outline: 2px dashed rgba(255, 255, 255, 0.35);
    outline-offset: 1px;
  }
  .drag-outline.illegal {
    outline: 2px solid #d9534f;
  }
  /* A zero-height anchor on the chassis' top edge; DeviceChrome positions itself above it. */
  .object-chrome {
    position: absolute;
    height: 0;
  }
  .device:focus-visible {
    outline: 2px solid var(--ae-signal-mic-lit);
    outline-offset: 1px;
  }
  /* Reveal the shared header (DeviceChrome) while its object is open (hovered/focused, plus the grace
     period). The bar stays transparent to pointers so its hit box is only its controls. */
  .object-chrome.open :global(.device-chrome),
  .object-chrome:focus-within :global(.device-chrome) {
    opacity: 1;
    transform: none;
  }
  .object-chrome.open :global(.device-chrome > *),
  .object-chrome:focus-within :global(.device-chrome > *) {
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
