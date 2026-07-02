// The DOM jack-measurement helper: read every rendered jack's connector centre into surface-local space
// (pan/zoom-invariant, because clientToSurface divides out the transform). Thin and DOM-bound — App owns
// the $effect that schedules it (after paint + once the flip transition settles); this just does the
// measuring and returns the map.

import type { WorldApi } from "./widgets/WorldView.svelte";

export type Pt = { x: number; y: number };

// Measure every rendered `[data-jack]` element's centre into surface coords. Cheap — a handful of
// getBoundingClientRect. Skips elements with no layout box (hidden / not yet laid out).
export function measureJacks(api: WorldApi): Record<string, Pt> {
  const next: Record<string, Pt> = {};
  for (const el of document.querySelectorAll<HTMLElement>("[data-jack]")) {
    const key = el.dataset.jack;
    if (!key) continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue; // not laid out / hidden
    next[key] = api.clientToSurface(r.left + r.width / 2, r.top + r.height / 2);
  }
  return next;
}
