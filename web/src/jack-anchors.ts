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
export function measureJacks(api: WorldApi): Record<string, JackAnchor> {
  const next: Record<string, JackAnchor> = {};
  for (const el of document.querySelectorAll<HTMLElement>("[data-jack]")) {
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
