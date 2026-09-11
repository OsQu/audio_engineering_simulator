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

## Task 10.1.2 — Executing known-regression suite  · done

The suite executes the existing broken behavior in a named, isolated Playwright file. Each test declares
an owning later Story before it makes its semantic assertion with `test.fail(true, ...)`: while the bug is
present, it reports as an expected failure; when a later migration fixes it without moving the case, the
unexpected pass fails closed. Moving the case into the ordinary required suite is therefore an explicit
part of the implementing Story, not a silent disappearance. This is deliberately different from a skip,
retry, or screenshot baseline.

File-tree diff:

```text
 web/
   package.json                                add `test:browser:known` focused command
   src/testing/browser-fixtures.ts             four named deterministic fixture records
   test-browser/
     support/fixture-page.ts                   stage navigation, geometry, and semantic DOM helpers
     known-regressions.spec.ts                 expected-failing cases, each with an owner Story
```

Key types and signatures:

```ts
type BrowserFixtureId =
  | "rack-hover"
  | "cable-hover"
  | "patch-gestures"
  | "rack-drag";

interface KnownRegression {
  id: string;
  owner: "10.5" | "10.7";
  title: string;
}

async function openFixture(page: Page, fixture: BrowserFixtureId): Promise<void>;
async function elementCentre(locator: Locator): Promise<{ x: number; y: number }>;
async function topmostDeviceAt(page: Page, point: { x: number; y: number }): Promise<string | null>;
```

Test flow:

```text
known-regressions.spec.ts
  openFixture(page, fixtureId)                 fresh Playwright context + unsaved TestSceneSession
  test.fail(true, "Story 10.5: …")            expected failure while the known bug remains
  drive real pointer/hover sequence
  assert semantic browser fact
    stack: elementFromPoint / computed physical layer, never a screenshot-only claim
    gesture: resulting connection/banner state after actual press/move/release ordering
    drag: mounted child's client rect follows the rack before pointer-up

later Story lands a fix
  known test unexpectedly passes              suite fails closed
  implementer moves it to required browser spec
```

The six required cases are: rack hover leaves its mounted device topmost (Story 10.5); rear- and
front-facing connected-device hover leave their cable layer visible (Story 10.5); click-click and
drag-drop produce the same accepted connection (Story 10.7); release over a destination without a final
move event still connects (Story 10.7); a near-socket press at representative minimum and maximum zoom
does not become a device drag (Story 10.7); and a populated rack drag moves the occupant's displayed
geometry before release (Story 10.5). The fixture adapter accepts a valid candidate synchronously only
to isolate these view/gesture facts from the real-worklet protocol, which remains Task 10.1.3.

Expected verification: `pnpm --dir web test:browser:known` reports every known case as an expected failure;
`pnpm --dir web test:browser` reports the required smoke plus the expected-failure suite; `pnpm --dir web
check`, `test`, and `typecheck` remain green. A temporary local change that makes one assertion pass must
make the known suite fail, proving graduation is fail-closed; that demonstration is not committed.

Validation passed: `pnpm --dir web test:browser:known` reports all six cases as expected failures;
`pnpm --dir web test:browser` reports the required smoke plus those six expected failures; `pnpm --dir web
test` passes 291 tests; and `pnpm --dir web check` / `typecheck` pass with the two pre-existing
`FloatingWindow.svelte` accessibility warnings. Temporarily expecting the current rear-device hover layer
(`6`) instead of its required layer (`1`) caused Playwright's `Expected to fail, but passed` error, proving
the graduation mechanism fails closed; the assertion was restored before this record was written.

## Task 10.1.3 — Real-worklet patch-acceptance smoke seam  · planned

_(program design drafted at pickup)_

## Task 10.1.4 — Pure resolver-contract fixture matrix  · planned

_(program design drafted at pickup)_
