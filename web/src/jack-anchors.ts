// The DOM jack-measurement helper: read every rendered jack's connector centre into surface-local space
// (pan/zoom-invariant, because clientToSurface divides out the transform). Thin and DOM-bound — App owns
// the $effect that schedules it (after paint + once the flip transition settles); this just does the
// measuring and returns the map.

import type { JackAnchor } from "./cable-view";
import type { WorldApi } from "./widgets/WorldView.svelte";

// Measure every rendered `[data-jack]` element's centre into surface coords, tagged with the chassis face
// it sits on (read off the nearest `[data-face]` ancestor; defaults to "back" if none is found, matching
// the pre-per-face behaviour). Cheap — a handful of getBoundingClientRect. Skips elements with no layout
// box (hidden / not yet laid out).
//
// Scoped to the world surface (`api.measureRoot()`), not the whole document: a duplicate faceplate
// rendered elsewhere (the device-focus overlay draws the same panel with the same `data-jack` keys) would
// otherwise clobber a real anchor with its off-world position. Falls back to the document before the
// surface mounts (nothing off-world yet, so the sweep is safe).
export function measureJacks(api: WorldApi): Record<string, JackAnchor> {
  const next: Record<string, JackAnchor> = {};
  const root: HTMLElement | Document = api.measureRoot() ?? document;
  for (const el of root.querySelectorAll<HTMLElement>("[data-jack]")) {
    const key = el.dataset.jack;
    if (!key) continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue; // not laid out / hidden
    const pt = api.clientToSurface(r.left + r.width / 2, r.top + r.height / 2);
    const face = el.closest<HTMLElement>("[data-face]")?.dataset.face;
    next[key] = { x: pt.x, y: pt.y, face: face === "front" ? "front" : "back" };
  }
  return next;
}
