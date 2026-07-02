# App.svelte split — plan (consolidation between Stories 4.6 and 4.7)

`web/src/App.svelte` is **1,863 lines** and has absorbed every feature from Stories 4.2–4.6: engine
lifecycle, device/rack/space CRUD, drag-placement validation, the whole cable-patching state machine
(drag + click-to-pick pending mode), cable/portal rendering, jack DOM measurement, param sync, the
cable inspector, and ~535 lines of CSS. The 2026-07-02 audit found the seams are clean — handlers
close over `scene`, `catalog`, `send`, which can be passed as parameters — so this is a mechanical
extraction, **not a redesign**. Goal: App.svelte becomes a ~400-line orchestrator; the moved logic
becomes plain, Vitest-testable TS modules; **zero behavior change**.

Why now: Story 4.7 (scope/spectrum) and the `docs/IMPROVEMENTS.md` items (catalog drawer, menus,
patching-cancel tweak, device focus mode) all land in exactly this code. Splitting first makes each
of those a contained change instead of another layer on the god component.

## Non-goals (do NOT do these here)

- **No behavior change.** Every interaction must work exactly as before. This includes the
  IMPROVEMENTS.md UX items — they come *after* this split, as their own tasks.
- **No worklet/protocol typing work** (that's a separate effort; see the audit).
- **No `scene-store.ts` / schema changes**, no persistence hardening, no new dependencies.
- **No Rust changes.** Engine and `patch` are untouched; this is all `web/src`.

## Settled constraints (violating one is a bug in the execution)

1. **Vitest stays Svelte-free.** `web/vitest.config.ts` documents this: tests run pure TS in a node
   environment, no Svelte compilation. Therefore **extracted modules are plain `.ts`** — they must
   not import from `svelte` or use runes. All `$state` / `$derived` / `$effect` declarations stay in
   App.svelte; modules receive state as explicit parameters.
   _Component/browser testing was considered and deferred (decision with Oskari):_ jsdom component
   tests can't exercise this app's interactions (`elementFromPoint`, real layout for jack
   measurement, pointer capture), and real-browser testing (Vitest browser mode +
   `vitest-browser-svelte`) is infra we only add once glue-layer regressions actually appear — the
   split intentionally moves the logic worth testing into pure TS, and the manual browser checklist
   covers the glue. **Do not add component-testing infrastructure in this work.**
2. **Modules may mutate the `scene` proxy passed to them** (property writes through Svelte 5's
   `$state` proxy are reactive regardless of which file the code lives in), but they must **never
   reassign** it — only App.svelte does `scene = loaded` (in `loadSaved`).
3. **Reactive contexts are built inline inside `$derived`/handlers in App.svelte**, so dependency
   tracking registers on the reads. Don't hoist a ctx object into a plain `const` at module-init
   time — its fields would be captured stale.
4. **`$state.snapshot(scene.patch)` before any `postMessage`** (the existing `plainPatch()`), never
   the raw proxy — DataCloneError otherwise. This stays in App.svelte next to the engine seam.
5. **The `WorldApi` contract and `WorldView` are untouched.** The world layer stays a dumb
   positioned-boxes container (the WebGL escape hatch).
6. Follow the repo module style: leaf modules are `src/<name>.ts`; components in `src/widgets/`
   (`src/panels/` etc. are not introduced — keep the flat shape).

## Target architecture

```
web/src/
  App.svelte            ~400 lines: $state/$derived/$effect, engine bring-up + callbacks,
                        save/load/reload, thin handler adapters, top-level composition
  projection.ts         NEW  — pure rect/item derivation (elevation + floor plan)
  placement.ts          NEW  — pure placement legality + drag-commit logic
  scene-ops.ts          NEW  — pure scene mutations (device/rack/space CRUD, connection edits)
  patching.ts           NEW  — the cable drag/pending state machine as pure transitions
  params.ts             NEW  — param key/value/seed/push helpers
  cable-view.ts         NEW  — cable anchor/visibility/portal-offset helpers (pure, DOM injected)
  jack-anchors.ts       NEW  — measureJacks DOM helper (thin, not unit-tested)
  widgets/
    CableLayer.svelte   NEW  — oneCable/onePortal snippets, rubber band, portal drag
    RoomOverlay.svelte  NEW  — decorative window + floor-plan outline/labels
    StageItem.svelte    NEW  — the per-item render (plan tile / rack frame / Panel)
    ItemControls.svelte NEW  — the per-item chip row (flip / space select / remove)
    CableInspector.svelte NEW
    LevelsPanel.svelte  NEW
    Toolbar.svelte      NEW  — header (space tabs, view switcher, palette, master, scene buttons)
test/
  projection.test.ts  placement.test.ts  scene-ops.test.ts  patching.test.ts  params.test.ts
```

Line references below are to App.svelte **at plan time** (commit `2006eb3`); they shift as tasks
land — locate by function name, not line number.

### Shared context types

Most modules take a small explicit context instead of closing over App state. Define once (in
`projection.ts`, exported):

```ts
/** What the current view shows. wall === null ⇔ view === "top". */
export type ViewCtx = { space: string; view: Wall | "top"; wall: Wall | null; room: Room };
/** The layout inputs every rect/placement computation needs. */
export type LayoutCtx = ViewCtx & { scene: Scene; catalog: DeviceDescriptor[] };
```

App builds these inline: `const layout = (): LayoutCtx => ({ scene, catalog, space: currentSpace, view: currentView, wall: currentWall, room });`
and calls `placedItemsFor(layout())` **inside** `$derived.by` so reads are tracked (constraint 3).

### Module specs

**`projection.ts`** — moves (from App.svelte ~136–235): `FRAME_MARGIN`, `GRID_MM`, `deviceUnits`,
`rackFrameSize`, `rackRect`, `deviceRect`, `rackFloorRect`, `deviceFloorRect`, `PlacedItem`,
`placedItemsFor(ctx: LayoutCtx): PlacedItem[]`. While moving, **collapse the triplicated rect logic**
(`deviceRect` / `deviceFloorRect`, `rackRect` / `rackFloorRect`) into wall-elevation vs top branches
of shared helpers — same outputs, one projection path per shape. Lookup helpers `deviceById` /
`rackById` / `isRack` become parameterized (`scene` arg) and live here too.

**`placement.ts`** — moves (~563–680, 757–771): `rackOccupants`, `rackSlotAt`, `canPlace`, `moveTo`,
`moveToTop`, `wallSpawn`. All take `LayoutCtx` (plus `placedItems` where overlap is checked — pass
it in; don't recompute, App already derives it). `moveTo`/`moveToTop` mutate `scene.ui` through the
proxy (constraint 2) — the wall re-tag + "mounted gear follows its rack's wall" logic moves verbatim.

**`scene-ops.ts`** — moves (~516–560, 682–821): `connKey`, `commitCable`, `disconnect`,
`setCableType`, `connectionDomain`, `connectionKind`, `addSpace`, `addDevice`, `removeDevice`,
`addRack`, `removeRack`, `moveDeviceToSpace`, `moveRackToSpace`, `toggleFlip`. These mutate the
scene but **know nothing about the engine**: no `hotSwap` calls inside the module. Each function's
doc comment states whether the caller must hot-swap. App wraps the ones that do:

| needs hotSwap after | no hotSwap (UI-only furniture) |
| --- | --- |
| `commitCable`, `disconnect`, `setCableType`, `addDevice`, `removeDevice` | `addSpace`, `addRack`, `removeRack`, `moveDeviceToSpace`, `moveRackToSpace`, `toggleFlip`, `moveTo`, `moveToTop` |

(This table is the current behavior — preserve it exactly; e.g. rack add/remove never rebuilt the
engine because racks are UI furniture.)

**`patching.ts`** — the `dragCable` state machine (~369–496) reworked into **pure transitions** so
the trickiest UI logic in the app becomes unit-testable:

```ts
export type PatchState = {
  source: Endpoint; srcPoint: Pt; free: Pt;
  over: boolean; legal: boolean; verdict: ConnectVerdict | null;
  mode: "drag" | "pending";
} | null;

// Pure transitions — no DOM, no Svelte. DOM facts (which jack is under the pointer, surface
// coords) are resolved by the App adapter and passed in.
export function pointerDown(state: PatchState, hit: JackHit | null, deps: PatchDeps): PatchResult;
export function pointerMove(state: PatchState, hit: JackHit | null, cursor: Pt, moved: boolean, deps: PatchDeps): PatchState;
export function pointerUp(state: PatchState, clickNotDrag: boolean): PatchResult;
export function cancel(state: PatchState): null;

export type JackHit = { key: string; endpoint: Endpoint; anchor: Pt | null };
export type PatchDeps = { connections: Connection[] };            // for evaluateConnection
export type PatchResult = { state: PatchState; commit?: ConnectVerdict }; // commit ⇒ caller commits + hot-swaps
```

`endpointFromJackKey` (~398–407) moves here (takes `scene` + `catalog`). App.svelte keeps: the
`dragCable = $state<PatchState>(null)` variable, the `cableDown` click-vs-drag threshold
bookkeeping, the `svelte:window` listeners, and thin adapters that read the DOM
(`closest("[data-jack]")`, `elementFromPoint`, `clientToSurface`) into a `JackHit`, call the
transition, assign the returned state, and on `commit` call `sceneOps.commitCable(...)` + hotSwap.
Preserve the exact semantics: pending survives empty-space clicks, re-clicking the source cancels,
illegal jack stays pending, Esc cancels, a sub-4px drag promotes to pending on release.

**`params.ts`** — moves (~106–116, 716–745): `key`, `paramValue`, `seedParamValues(scene, catalog):
Record<string, number>` (returns the map instead of assigning), `pushParams(sendFn, scene, catalog,
values)`. App keeps the `paramValues` `$state` and `onParamInput` (which touches all three lanes:
local map, `setSceneParam`, live `send` — keep that trio together in one visible place and say so in
a comment; the audit flagged it as a divergence risk).

**`cable-view.ts`** — moves (~239–346): `jackKey`, `cableAnchor`, `inView`, `bothInView`,
`oneInView`, `spaceName`, `otherEndLabel`, `PORTAL_LEN`, `portalKey`, `portalOffset`. Pure given
`(ctx: LayoutCtx, jackAnchors, api)` — the DOM-measured anchors and `WorldApi` are injected, so the
estimate math (the 0.62/0.38/0.45 chassis-edge fallback) is testable with a fake api.

**`jack-anchors.ts`** — moves `measureJacks` (~253–264) as `measureJacks(api: WorldApi):
Record<string, Pt>` (returns instead of assigning). The `$effect` that schedules it (RAF + 480 ms
flip-settle re-measure, ~269–283) **stays in App.svelte** (constraint 1: no effects in modules).

### Component extraction (template + its CSS move together)

Each new component takes props/snippets only — no reaching back into App state. The CSS blocks in
App.svelte's `<style>` (~1329–1863) move into the component that owns the class names; whatever
remains in App.svelte should only style App-level layout (`main`, `.toolbar` grid, `.stage`).

- **`CableLayer.svelte`** — the `oneCable` / `onePortal` snippets (~986–1064), the drag rubber
  band / pending floating end (~1113–1135), and the portal-drag pointer handlers (~350–367,
  including the `portalDrag` local — it's plain non-`$state` mutable state and moves cleanly).
  Props: `connections`, `api`, `ctx` fns (`cableAnchor`, `inView`…, prebound in App), `jackAnchors`,
  `dragCable` (read-only), `selectedCableKey` + `onSelect`, `portals` slice of scene.ui + write-back
  via `onPortalOffset(key, off)`. Rendered from inside WorldView's `cables`/`overlay` snippets in App.
- **`RoomOverlay.svelte`** — decorative window + top-view room outline (~1082–1108). Props: `api`,
  `view`, `room`.
- **`StageItem.svelte`** — the `item` snippet body (~1193–1243): plan tile / rack frame / `Panel`
  (+ the `synth_voice` `Screen` embellishment, kept as-is — descriptor-flagging it is a listed
  follow-up, not this plan). **`ItemControls.svelte`** — the `controls` snippet body (~1138–1191).
- **`CableInspector.svelte`** (~1255–1288) and **`LevelsPanel.svelte`** (~1293–1324) — props in,
  callbacks out (`onSetCableType`, `onDisconnect`, `onClose`).
- **`Toolbar.svelte`** (~905–978) — space tabs, view switcher, palette, volume+Vu, scene buttons,
  status line. Props + callbacks; no logic. (This is where IMPROVEMENTS.md's drawer/menu work will
  later land, contained.)

### What stays in App.svelte

All `$state`/`$derived`/`$effect`; `start()` + the engine callback wiring (~823–868); `hotSwap`;
`plainPatch`; volume (~73–87); `saveCurrent`/`loadSaved`/`reload`; keyboard/MIDI wiring; the
`svelte:window` pointer/key listeners with their thin patching adapters; composition of Toolbar,
WorldView (+ CableLayer/RoomOverlay/StageItem/ItemControls snippets), patch banner, CableInspector,
LevelsPanel.

## Task breakdown (one-by-one; each ends green + browser-verified; STOP after each for Oskari)

Order is bottom-up so every step compiles and behaves identically:

1. **`projection.ts` + tests.** Move rect/item derivation, dedupe the elevation/floor rect pairs,
   add `test/projection.test.ts` (hand-calc cases: a mounted device's rect inside a projected rack
   frame on each wall; a floor footprint of a side-wall unit; `placedItemsFor` filtering + z by
   facing for both view kinds). App.svelte imports and its `placedItems` `$derived` becomes a
   one-liner.
2. **`placement.ts` + tests.** Move legality/commit logic. Tests: rack-snap hit vs miss at slot
   boundaries, `canPlace` overlap rejection (elevation) vs always-true (top/racks), `moveToTop`
   wall re-tagging incl. mounted-gear-follows-rack, `wallSpawn` flush position per wall.
3. **`scene-ops.ts` + tests.** Move CRUD + connection edits. Tests: `removeDevice` cascades
   (connections + placement, output-tap refusal), `commitCable` fan-in replacement + default cable
   only for analog, `setCableType` clear-to-ideal, `removeRack` un-mounts but keeps positions.
   App handlers become `(...) => { sceneOps.x(...); hotSwap(); }` per the table above.
4. **`patching.ts` + tests.** The state-machine rework — the only extraction that changes code
   shape rather than location, so it gets the most careful browser pass. Tests: full drag-commit,
   click→pending→cross-view-commit, re-click-source cancel, illegal-stays-pending, Esc, the
   moved-threshold promotion rules.
5. **`params.ts` + `cable-view.ts` + `jack-anchors.ts` + tests** for the first two (seed respects
   saved values over defaults; push emits one message per param; anchor fallback math; portal
   default offset vs persisted). Small, mechanical.
6. **Component extraction** — `CableLayer` + `RoomOverlay` first (biggest chunk), then `StageItem` +
   `ItemControls`, then `Toolbar` + `CableInspector` + `LevelsPanel`. Move each CSS block with its
   component; delete what's then dead in App's `<style>`. Pure refactor, no logic edits.
7. **Sweep.** App.svelte down to target (~400 lines); magic numbers that survived (`FLUSH`,
   portal `dy: 36`, anchor ratios) get named constants next to their logic; final read-through for
   dead code. Confirm the file-level doc comment at the top of App.svelte is rewritten to describe
   the orchestrator role.

**Gate per task:** `cd web && pnpm run check && pnpm run typecheck && pnpm run test && pnpm run build`
— plus a manual browser pass (`pnpm run dev`, port 5173; the sim dev server convention is 5174 if
5173 is taken) covering at minimum: place/drag a device into a rack, flip it, drag-patch two jacks,
click-to-pick patch across a wall switch, drag a portal chip, change a cable type in the inspector,
disconnect, save → reload page → load, top view drag with wall re-tag, play the synth. The full
Rust gate is untouched by this work but run it once at the end (`cargo fmt --check && cargo lint &&
cargo test && cargo wasm && cargo docs`) to confirm nothing drifted.

## Workflow reminders (for the executing session)

- **Task-by-task.** After each task: green gate, then **STOP** — Oskari verifies and commits
  himself, then says continue. Review his commit message. Never `git commit`. (Memories:
  `dev-workflow`, `run-fmt-before-handoff` — for web work the fmt equivalent is `pnpm run check`.)
- **This is a refactor: resist improving behavior.** If a bug or UX itch surfaces (there are known
  ones in `docs/IMPROVEMENTS.md` and the audit), note it, don't fix it here — unless it blocks the
  extraction, in which case surface it to Oskari first.
- Biome config note: single config at repo root, no comments in `biome.json`, `.svelte` files are
  svelte-check's domain not Biome's (memory: `biome-config-monorepo-svelte`).
- New tests go in `web/test/*.test.ts`, import explicitly, no Svelte imports (the vitest config's
  documented convention).
- Branch: this is between-stories consolidation, not a Story — ask Oskari whether he wants a
  `chore/app-svelte-split` branch or to ride `main`, before the first edit.
