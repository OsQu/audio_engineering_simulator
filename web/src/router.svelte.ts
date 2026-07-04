// The app's first URL routing (Epic 6) — hand-rolled, no dependency. Holds the current pathname as
// reactive state, parses it into a `Route`, and drives navigation via the History API. Two routes today:
// the scene view (default) and the single-device workbench at `/devices/<typeId>`. A class instance (the
// rune-module pattern from Story 6.1), constructed once at the app root; `popstate` wiring lives in the
// root component's `$effect` (so its listener lifecycle is tied to the mount), calling `sync()`.

// The resolved route. `workbench` with an empty `typeId` (bare `/devices`) is the catalog index; a
// non-empty `typeId` is a specific device (which the workbench resolves against the catalog — an unknown
// id also falls back to the index, but that decision needs the catalog, so it lives in the workbench).
export type Route = { view: "scene" } | { view: "workbench"; typeId: string };

export function parseRoute(pathname: string): Route {
  const m = pathname.match(/^\/devices(?:\/([^/]*))?\/?$/);
  if (m) return { view: "workbench", typeId: m[1] ? decodeURIComponent(m[1]) : "" };
  return { view: "scene" };
}

export class Router {
  // The live pathname; `route` derives from it. Navigation and browser back/forward both flow through
  // updates to this field, so every consumer re-renders reactively.
  pathname = $state(location.pathname);
  route = $derived(parseRoute(this.pathname));

  // Navigate to a path (a link click / programmatic move): push a history entry and update the state.
  // A no-op if we're already there, so it never spams duplicate history entries.
  navigate = (path: string): void => {
    if (path === this.pathname) return;
    history.pushState({}, "", path);
    this.pathname = path;
  };

  // Re-read the URL after a browser back/forward. The root wires this to the window `popstate` event.
  sync = (): void => {
    this.pathname = location.pathname;
  };
}
