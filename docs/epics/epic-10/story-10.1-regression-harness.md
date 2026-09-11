# Story 10.1 — Regression harness & resolver contract — program design

> Companion to the Story 10.1 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task
> program design — the shape of the code — drafted just-in-time as each Task is picked up.

## Task 10.1.1 — Supported browser harness and deterministic app seam  · done

This task adds the smallest browser boundary that can exercise the real application component tree without
booting audio. The ordinary root keeps constructing its production `SceneSession`; only the explicit test URL
constructs an adapter with deterministic scene, catalog, cable, and submission behavior. The adapter is test
infrastructure, not a second product session implementation, and it does not leak into pure view-resolution
modules planned for later Stories.

File-tree diff:

```text
web/
  package.json                              Playwright scripts and direct dev dependency
  playwright.config.ts                     Vite web server, isolated Chromium projects, artifacts on failure
  src/
    Root.svelte                             explicit test-only route beside studio/workbench routes
    App.svelte                              accept an optional preconstructed session composition input
    testing/
      BrowserFixtureRoot.svelte             select and mount deterministic browser fixtures
      browser-fixtures.ts                   named, unsaved scene/catalog/cable inputs
      test-session.svelte.ts                engine-independent SceneSession adapter
  test-browser/
    smoke.spec.ts                           required proof that the real studio tree is mounted
    support/
      fixture-page.ts                       typed navigation/query helpers
```

Key types and signatures (names are provisional; ownership is settled):

```ts
// Production App still owns the default when no session is supplied.
interface AppProps {
  session?: SceneSession;
}

type BrowserFixtureId = "rack-hover" | "cable-hover" | "patch-gestures" | "rack-drag";

interface BrowserFixture {
  id: BrowserFixtureId;
  scene: Scene;
  catalog: DeviceDescriptor[];
  cables: CableType[];
  initialView: StudioView;
}

class TestSceneSession extends SceneSession {
  readonly submissions: Scene[];
  constructor(fixture: BrowserFixture);
  submitPatchCandidate(candidate: Scene): boolean; // deterministic acceptance; no AudioContext/worklet
}

function browserFixture(id: BrowserFixtureId): BrowserFixture;
```

Composition call-stack (diff):

```text
main.ts
  Root.svelte
    route = studio
-     App()                                      constructs production session internally
+     App()                                      unchanged production default
    route = workbench
      Workbench(...)
+   route = explicit test fixture URL
+     BrowserFixtureRoot(fixtureId)
+       fixture = browserFixture(fixtureId)      plain deterministic data; no localStorage
+       session = new TestSceneSession(fixture)  no AudioContext or real worklet
+       App({ session })
+         WorldView / PatchController / faceplates / cable layers
```

The fixture route must be unavailable by accident rather than by ambient mode: it is selected only by its
reserved URL prefix and is never the default/fallback route. Fixtures expose stable semantic test locators at
the stage/object/toolbar/banner boundaries; tests must not couple to incidental generated Svelte classes.
Playwright starts Vite on its own test port, uses a fresh browser context per test, retains trace/screenshot
artifacts only on failure, and does not reuse a developer's running server. The first smoke asserts fixture
identity plus real rendered rack/device semantics, proving the route did not mount a substitute mini-stage.

Implementation prerequisite: Playwright's Chromium binary is a user-managed tool download. If absent, the
user runs the exact install command surfaced at task pickup; repository code and lockfile changes remain part
of the Task.

Validation passed: `pnpm --dir web test:browser` (one Chromium smoke), `pnpm --dir web check`,
`pnpm --dir web test` (291 tests), and `pnpm --dir web typecheck` (zero errors; the existing two
`FloatingWindow.svelte` accessibility warnings remain).

## Task 10.1.2 — Executing known-regression suite  · planned

_(program design drafted at pickup)_

## Task 10.1.3 — Real-worklet patch-acceptance smoke seam  · planned

_(program design drafted at pickup)_

## Task 10.1.4 — Pure resolver-contract fixture matrix  · planned

_(program design drafted at pickup)_
