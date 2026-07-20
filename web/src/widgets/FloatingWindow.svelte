<script lang="ts">
  // A generic floating window: draggable by its title bar, resizable from the bottom-right grip, and
  // brought to the front on any pointer-down (the parent reorders its window stack, which we reflect via
  // the `z` prop). Deliberately *non-modal* — there's no backdrop, so the scene underneath stays live and
  // several windows can be open at once. It owns no content; the parent renders the focus surface into the
  // body via `children`. Geometry (x/y/w/h) is `$bindable` so the parent's window-stack state is the single
  // source of truth (it survives a bring-to-front reorder of the keyed `{#each}`).
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    /** Top-left position + size in px, relative to the positioned stage. Two-way bound to the parent. */
    x: number;
    y: number;
    w: number;
    h: number;
    /** Stack order → CSS z-index (parent assigns from the window array's order). */
    z: number;
    /** Bring this window to the front (parent moves it to the top of the stack). */
    onActivate: () => void;
    onClose: () => void;
    children: Snippet;
  }
  let {
    title,
    x = $bindable(),
    y = $bindable(),
    w = $bindable(),
    h = $bindable(),
    z,
    onActivate,
    onClose,
    children,
  }: Props = $props();

  // Small floors so a window can't be shrunk into an unusable sliver.
  const MIN_W = 260;
  const MIN_H = 160;

  let rootEl = $state<HTMLElement>();

  // One gesture at a time — drag (from the title bar) XOR resize (from the grip). We capture the pointer on
  // the element the gesture started on, so tracking continues even when the pointer leaves it, and mutate
  // the bound geometry directly from the delta against the gesture's start.
  let drag: { px: number; py: number; ox: number; oy: number } | null = null;
  let resize: { px: number; py: number; ow: number; oh: number } | null = null;

  function startDrag(e: PointerEvent): void {
    if (e.button !== 0) return;
    onActivate();
    drag = { px: e.clientX, py: e.clientY, ox: x, oy: y };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  }
  function startResize(e: PointerEvent): void {
    if (e.button !== 0) return;
    onActivate();
    resize = { px: e.clientX, py: e.clientY, ow: w, oh: h };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    e.stopPropagation(); // don't also start a drag via the root's pointer-down
  }
  function onMove(e: PointerEvent): void {
    if (drag) {
      x = drag.ox + (e.clientX - drag.px);
      y = drag.oy + (e.clientY - drag.py);
    } else if (resize) {
      w = Math.max(MIN_W, resize.ow + (e.clientX - resize.px));
      h = Math.max(MIN_H, resize.oh + (e.clientY - resize.py));
    }
  }
  function endGesture(e: PointerEvent): void {
    drag = null;
    resize = null;
    (e.currentTarget as Element).releasePointerCapture?.(e.pointerId);
  }

  // Move keyboard focus into the window when it first mounts, so Esc / on-screen play read from it.
  $effect(() => {
    rootEl?.focus();
  });
</script>

<!-- The window shell. A pointer-down anywhere brings it to the front; the title bar and grip own the
     move/resize gestures (they stop the event from re-triggering below). -->
<section
  bind:this={rootEl}
  class="window"
  role="dialog"
  aria-label={title}
  tabindex="-1"
  style:left="{x}px"
  style:top="{y}px"
  style:width="{w}px"
  style:height="{h}px"
  style:z-index={z}
  onpointerdown={onActivate}
>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <header
    class="titlebar"
    onpointerdown={startDrag}
    onpointermove={onMove}
    onpointerup={endGesture}
    onpointercancel={endGesture}
  >
    <span class="title">{title}</span>
    <button
      type="button"
      class="close"
      aria-label="close {title}"
      onpointerdown={(e) => e.stopPropagation()}
      onclick={onClose}>✕</button
    >
  </header>
  <div class="body">
    {@render children()}
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="grip"
    role="separator"
    aria-label="resize {title}"
    onpointerdown={startResize}
    onpointermove={onMove}
    onpointerup={endGesture}
    onpointercancel={endGesture}
  ></div>
</section>

<style>
  .window {
    position: absolute;
    display: flex;
    flex-direction: column;
    min-width: 260px;
    min-height: 160px;
    background: var(--ae-bg-panel);
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-panel);
    box-shadow: var(--ae-shadow-card);
    overflow: hidden; /* the body scrolls, not the shell */
  }
  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex: none;
    padding: 0.35rem 0.4rem 0.35rem 0.7rem;
    background: var(--ae-bg-chip);
    border-bottom: 1px solid var(--ae-line-panel);
    cursor: grab;
    touch-action: none;
    user-select: none;
  }
  .title {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--ae-text-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .close {
    flex: none;
    font: inherit;
    font-size: 0.72rem;
    line-height: 1;
    padding: 0.25rem 0.5rem;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
    cursor: pointer;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 1rem 1.2rem 1.4rem;
  }
  /* The panel fills the window body, as the old focus surface did. */
  .body :global(.panel) {
    width: 100%;
    min-height: 220px;
  }
  .grip {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    touch-action: none;
    /* A little corner wedge so the resize affordance is visible. */
    background: linear-gradient(
      135deg,
      transparent 0 50%,
      var(--ae-line-panel) 50% 60%,
      transparent 60% 75%,
      var(--ae-line-panel) 75% 85%,
      transparent 85%
    );
  }
</style>
