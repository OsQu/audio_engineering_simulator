# Epic 6 — Device Workbench (developer tooling) — Design Notes & Delivery Record

Companion archive to `IMPLEMENTATION_PLAN.md`. Epic 6 built the developer tooling in service of Stage-5
breadth: a focused single-device workbench at `/devices/<typeId>` where devices are built, patched,
debugged, and hot-reloaded, driving the **same** extracted session/patching plumbing as the scene view.
Stories 6.1–6.3 are **done**; **6.4 is implemented and green (Rust + web gates), with in-browser
verification + commit pending** (and a known `wasm:watch` rebuild-loop to resolve). The plan keeps a tight
summary of Epic 6; **this file is the full record** — the *settled design notes* (with the rejected
alternatives and the reasoning that justified each choice), the per-task breakdown, and the per-task
*Delivered* / *Deviations* notes.

Read this when a later epic's design decision turns on **why** Epic 6 was built the way it was, or when you
need the exact API/behavior of something Epic 6 shipped. For *what exists and what binds you going forward*,
the plan's Epic 6 summary is enough; come here for the depth behind it.

The plan's section ordering (Goal → Watch out → Design notes → Tasks → Validate → Delivered) is preserved
per story.

---

## Epic 6 framing (as originally written)

**Goal:** a focused single-device development view at `/devices/<device_id>` — the place where devices
are built, tested, and debugged as Epic 5 grows the catalog. One device on a real-dimensions grid,
front **and** back faces visible, instantly patchable to a sound source and a monitor, every param and
meter exposed, and a temp scene that lives **in the URL** so the Rust-rebuild → page-reload loop
restores itself with zero scene management.

This is the first epic that is not a `PROJECT_PLAN.md` §9 roadmap stage — it is developer tooling in
service of Stage 5 breadth (every new 5.1 device gets built against this bench). It shares Epic 5's
ordering freedom: its stories can interleave with Epic 5 work.

**Exit criteria:** open `localhost:5173/devices/scarlett_8i6` → the device renders on a mm grid (both
faces), pre-patched to a synth source and a monitor chain; audio plays after one gesture; all params
are drivable and all readouts/health visible; repatching and param tweaks persist to the URL; editing
engine Rust + `pnpm run wasm` reloads the page and the bench restores itself from the URL, audible
again within seconds.

**Watch-outs:**

- **One plumbing path, two views.** The workbench must consume the *same* extracted session layer,
  widgets, and patching machinery as the scene view — never a forked copy of App's glue. If the
  workbench needs something App has, extract it; don't duplicate it.
- **Layer rule holds even for dev tooling.** The catalog gains no workbench vocabulary; the bench is a
  pure consumer of descriptors (`typeId`, ports, params, readouts, form factor).
- **The URL scene is disposable by design.** Versioned like the scene store; on mismatch, regenerate
  the default rig — never migrate. Write with debounced `replaceState` (no history spam), compressed.
- **Autoplay policy is a hard constraint.** The catalog must be available *before* any user gesture
  (planned resolution: start the engine with the AudioContext suspended — the worklet instantiates and
  posts `ready`/catalog without audio; `resume()` on first interaction. Verify early; fall back to a
  main-thread catalog bridge if suspended instantiation doesn't hold).
- **Bootstrap sources with the synth.** A proper bench signal generator (sine/sweep/noise/DC, settable
  level + Zout) is deliberately deferred — it's an Epic-5-style device addition, not a bench
  prerequisite (candidate Story 6.4).

---

### Story 6.1 — Engine-session extraction — ✅ **Complete**

_Goal:_ Extract the engine/UI plumbing that today lives as `$state` + closures inside the ~1750-line
`App.svelte` into a **session layer** any view root can construct — so the Story-6.2 workbench consumes
the *same* interaction path as the scene view instead of a forked copy (the Epic's "one plumbing path,
two views" watch-out). Pure refactor: **zero behavior change** in the scene view, no Rust changes, no
new `postMessage` shapes. Anchors to PROJECT_PLAN §7 (UI a pure consumer of the engine API) — the
session layer is exactly that consumer surface, factored to be shared.

_Watch out:_

- **Behavior parity is the whole gate.** Every extraction step must leave the scene view pixel- and
  behavior-identical. Anything ambiguous → keep today's behavior and note it.
- **Rune modules aren't node-testable here.** Vitest runs `environment: "node"` with no Svelte
  compilation (`web/vitest.config.ts` — deliberate, per the defer-test-infra stance), so the new
  `.svelte.ts` classes must stay **thin reactive shells**: decision logic remains in the pure, already-
  tested modules (`params.ts`, `patching.ts`, `scene-ops.ts`, `cable-view.ts`, `jack-anchors.ts`). A
  session method that grows real logic should be pushing it into a pure module instead.
- **Don't entrench the `isPlayable` heuristic.** The keybed refactor (IMPROVEMENTS.md: note plumbing
  onto `DeviceHandle`, retire `isPlayable` from routing) is *not* this story — but the extraction must
  not dig it deeper: **target selection** (`keyboardTarget`, the `wireKeyboard` attach effect, `wireMidi`
  wiring) stays view-side; the session only exposes target-explicit `playNote(device, …)` + `heldNotes`.
- **`$state.snapshot` at the worklet boundary still holds** — `plainPatch()` moves into the session
  unchanged; every `postMessage` crossing keeps snapshotting the proxy.
- **The `$derived`/inline-ctx reactivity trap.** App deliberately rebuilds `ViewCtx`/`LayoutCtx` inline
  so field reads register as reactive dependencies (the comment at `App.svelte` ~145). Moving state into
  class fields must preserve that discipline — hoisting a session read into a module-init const captures
  it stale.
- **Scope guard: extraction only.** No suspended-context boot (6.2), no stop/teardown (deliberately
  absent today — "the page lives for the session"; routing in 6.2 is separate page loads, so teardown
  stays unneeded), no router, no workbench code.

_Design notes (settled at planning):_

- **The session owns the authoritative `Scene`.** `SceneSession` holds `scene` as `$state`; each view
  root constructs it with an initial scene (App seeds from `loadScene() ?? defaultScene()`; the 6.3
  workbench will seed from the URL). Param/config/hot-swap lanes live beside the scene they mutate.
  _Rejected: view-owned scene + accessor callbacks_ — every session method threads through getters and
  the two views can drift in how they wire it.
- **A class instance per view root, in the codebase's first `.svelte.ts` rune modules.** `SceneSession`
  (and the patch controller) are classes with `$state` fields; App constructs them in its script and
  passes closures down exactly as today (`DeviceUiProps` unchanged). No Svelte context until a child
  actually needs it. _Rejected: a module-level singleton_ — a hidden global that two view roots would
  share implicitly and tests couldn't construct fresh.
- **Full patching extraction, pointer adapters included.** `dragCable` + click-vs-drag bookkeeping,
  `pointerDown/Move/Up` adapters, `jackHitOf`, the measured jack-anchor store, and
  commit/disconnect/setCableType move into a `PatchController` bound to the session. The workbench
  needs the identical ~100 lines of glue in 6.3. _Rejected: ops-only extraction_ — the workbench would
  re-implement the pointer bookkeeping. The layout-dependent *measurement trigger* (the `$effect` with
  scene-view dep list) stays view-side; the controller exposes `measure(worldApi)` + the anchor store.
- **What stays in App (scene-view UI, not session):** spaces/walls/top-view state, placements + racks +
  portals, the focus overlay + `keyboardTarget` derivation, cable-inspector *selection* (ops are the
  controller's; App wraps disconnect to also clear its selection), toolbar chrome.
- **Known deferral (not a bug):** `startEngine` couples `audio.resume()` into start; the 6.2 suspended
  boot will need a variant. Left untouched here — parity first.

- **Task 6.1.1 — `SceneSession` core: engine lifecycle + readout state.** New `session.svelte.ts` with
  the class holding `started/ready/status/health/midiStatus/level/readings/losses/catalog/cables/send`,
  `volume` (+ its localStorage persistence), `start()` wrapping `startEngine`, and `readingFor`. App
  constructs one and consumes it throughout. _Done:_ scene view starts, meters/health/VU/volume behave
  identically; web gate green (`pnpm run check && pnpm run typecheck && pnpm run test && pnpm run build`);
  verified in-browser.
- **Task 6.1.2 — Scene + param/config lanes into the session.** Move `scene` ownership, `paramValues` +
  `paramValue`/`onParamInput`, `configValue`/`onConfigInput`, `plainPatch`, `hotSwap`, and
  save/load/reload into `SceneSession`. App reads `session.scene` everywhere. _Done:_ knob moves reach
  all three lanes (UI/scene/engine), config toggles recompile, save/load/reload work as before; gate
  green; in-browser.
- **Task 6.1.3 — Note routing into the session.** `playNote` becomes target-explicit
  (`session.playNote(device, on, note, velocity)`) with `heldNotes` on the session; `keyboardTarget`,
  the `wireKeyboard` attach/detach effect, and `wireMidi` wiring stay in App feeding it. _Done:_ focus
  keybed, QWERTY capture (attach on focus only, held notes released on detach), and Web MIDI all play
  exactly as before; gate green; in-browser.
- **Task 6.1.4 — `PatchController`.** Extract the patching glue per the design note (drag + click-to-
  pick + cross-view pending, `jackHitOf`, anchor store + `measure(worldApi)`, commitCable/disconnect/
  setCableType, each hot-swapping via the session). App's window handlers become one-line delegations;
  the measurement `$effect` stays in App. _Done:_ same-view drag, click-to-pick, cross-view pending
  patching, cable select/type-change/disconnect, and portal stubs all behave identically; gate green;
  in-browser.
- **Task 6.1.5 — Parity sweep + plumbing audit.** Full manual walkthrough of the scene view (start,
  play via focus + MIDI, patch all three ways, cable inspector, config recompile, save/load/reload,
  spaces/walls/top, add/remove device/rack, flip/eject, volume persistence) plus an audit that no
  engine/param/patching `$state` remains in `App.svelte` (only scene-view UI state). _Done:_ walkthrough
  clean; App is plumbing-free; full web gate green.

_Validate:_ the scene view is **behavior-identical** end to end (the 6.1.5 walkthrough), with the
engine session, scene/param/config lanes, note routing, and patching glue all living in view-agnostic
session modules that a second view root can construct; `App.svelte` retains only scene-view UI state;
no Rust or `postMessage` changes; `pnpm run check && pnpm run typecheck && pnpm run test &&
pnpm run build` green.

### Story 6.2 — Route + workbench shell — ✅ **Done**

_Goal:_ Stand up the **second view root** — a single-device workbench at `/devices/<typeId>` — on top of
the Story-6.1 session seam. This Story delivers the app's first URL routing, a suspended-context engine
boot so the catalog is available *before* any user gesture, and a **dedicated** bench stage that renders
one device (both faces, on a real-dimensions grid) with its params/config/meters driven live through the
*same* `SceneSession` the scene view uses. It is deliberately **device-only and silent** — no sound
source, monitor chain, or patching yet (that is the 6.3 rig). Anchors to PROJECT_PLAN §7 (UI a pure
consumer of the engine API): the workbench is a second consumer of the identical session surface, not a
forked copy of App's glue.

_Watch out:_

- **One plumbing path, two views (the Epic's load-bearing constraint).** The bench must consume the same
  `SceneSession` + faceplate widgets as the scene view. We chose a *dedicated* stage (not `WorldView`
  reuse), so the burden shifts onto 6.3: the stage must expose a **`WorldApi`-shaped surface**
  (`clientToSurface`/`worldToSurface`) and drive the **existing `PatchController` + `cable-view.ts`** when
  patching lands — never a second cable/anchor implementation. Design the stage in 6.2 with that seam in
  mind even though no cable is drawn yet.
- **Catalog only exists after the engine builds.** The worklet posts `ready` (with the catalog) from its
  **constructor**, but *only after* `new SceneEngine(patch)` succeeds (`processor.js` ~896) — a bad patch
  throws and no catalog arrives. So the workbench cannot validate an arbitrary `<typeId>` before booting:
  it must boot a **known-good bootstrap scene** first, then resolve the requested type against the catalog
  that comes back.
- **Autoplay policy is a hard constraint.** `new AudioContext()` before a gesture starts *suspended*; the
  worklet still instantiates and posts `ready`/catalog (verified — constructor-posted, not from
  `process()`), so the catalog arrives with no audio. `resume()` must wait for the first interaction. The
  current `startEngine` couples `await audio.resume()` into bring-up (`engine.ts` ~176) — that must be
  decoupled (the 6.1 "known deferral").
- **Layer rule holds.** The catalog gains no workbench vocabulary; the bench reads `typeId`, `formFactor`,
  `ports`, `params`, `readouts` as a pure consumer.
- **Scope guard.** No rig/source/monitor, no patching UI, no URL-persisted scene (all 6.3); no debug panel
  or `wasm:watch` hot loop (6.4). Just route + suspended boot + stage + params/meters.

_Design notes (settled at planning):_

- **Dedicated Workbench stage, not `WorldView` reuse.** A purpose-built flat bench (mm grid + rack-U
  ruler, both faces side by side) sized from the descriptor's `formFactor` (`rackmount rackUnits ×
  RACK_UNIT_MM` / `desktop widthMm×heightMm`). Renders `deviceUi(typeId)` **twice** — `flipped=false`
  (front) and `flipped=true` (back) — reusing the identical faceplate widget + session props
  (`valueFor`/`onParam`/`configFor`/`onConfig`/`readingFor`). _Rejected: reuse `WorldView`_ — its
  room/wall/spatial ctx is a poor fit for one bolted-down device, and forcing a "bench mode" into it
  muddies the scene view. The cost of the dedicated stage is the 6.3 patching seam (above), accepted.
- **Boot a known-good bootstrap scene to harvest the catalog, then resolve `typeId`.** Boot suspended on a
  minimal valid `synth_voice` scene (guaranteed to build) → catalog arrives → look up `<typeId>` in it:
  **valid** → `hotSwap` to the device's minimal scene (the device + an output tap on its first output
  port, or port 0 if none); **unknown** → render the catalog **index page** (the catalog's device names,
  each linking to `/devices/<typeId>`). One boot path, catalog always arrives, `hotSwap` already exists.
  _Rejected: a main-thread catalog bridge_ (a second wasm instance just for `catalog()`) — unneeded since
  the suspended constructor already delivers it (the Epic watch-out's fallback, not triggered). _Rejected:
  optimistic per-device boot_ — a bogus `typeId` throws with no catalog, so the index page would need a
  bootstrap fallback anyway; always-bootstrap is the single path.
- **`session.resume()` + a no-auto-resume `start()`.** `startEngine` stops calling `audio.resume()`
  itself; `EngineControl` gains `resume()`, surfaced as `session.resume()`. The scene view calls
  `session.resume()` from its start button (still one gesture — behavior identical); the workbench resumes
  on first interaction. One bring-up path both views share. _Rejected: a workbench-only suspended variant_
  — two bring-up paths to keep in sync.
- **Hand-rolled router (no dependency).** A tiny `Root` reads `location.pathname`: `/devices/<typeId>` →
  `Workbench`, else → `App` (scene view). `main.ts` mounts `Root`; navigation uses `pushState` +
  a `popstate` listener. _Rejected: a routing library_ — a new dependency for a two-route split; the
  repo's lean-deps posture and the fact that 6.3's URL scene already leans on `replaceState` favor
  hand-rolling.
- **Known simplification (not a bug):** the bench is **silent** in 6.2 — meters read the floor with no
  signal driving the device. That is expected; audibility is the 6.3 rig's job.

- **Task 6.2.1 — Hand-rolled router + `Root` split.** New `Root.svelte` (or a `router.ts` + `Root`) that
  routes `location.pathname` → `App` (scene view, default) vs a stub `Workbench`; `main.ts` mounts `Root`.
  A `navigate(path)` helper (`pushState`) + `popstate` handling for back/forward. Confirm Vite dev serves
  deep links (`/devices/x`). _Done:_ `/` shows the scene view unchanged; `/devices/<anything>` mounts the
  stub workbench; browser back/forward switches views; web gate green.
- **Task 6.2.2 — Decouple `resume()`; `session.resume()` + suspended boot.** Remove the unconditional
  `await audio.resume()` from `startEngine`; add `resume()` to `EngineControl` and `session.resume()`.
  Scene view's start button calls `session.start(...)` then `session.resume()` (behavior identical). Add a
  session bring-up that leaves the context suspended. _Done:_ scene view starts + plays exactly as before;
  a suspended boot receives `ready`/catalog with **no audio** until `resume()`; verified in-browser.
- **Task 6.2.3 — Workbench bring-up: bootstrap boot + `typeId` resolution + index.** The `Workbench`
  constructs its own `SceneSession`, boots suspended on the `synth_voice` bootstrap, resumes on first
  interaction. On catalog arrival, resolve the route's `<typeId>`: valid → `hotSwap` to the minimal
  single-device scene; unknown → a **catalog index** listing catalog devices (links to `/devices/<id>`).
  The output tap **must be an analog port** (it's rendered as a voltage) — tap the device's first analog
  output; a digital-only-output device (e.g. the computer) can't be tapped without a DA (the 6.3 monitor
  chain), so the bench refuses it with a message rather than building a digital-tap scene (which faults the
  engine — see 6.2.5). _Done:_ `/devices/scarlett_8i6` boots (silent) with the catalog present and the 8i6
  as the live scene; `/devices/computer` shows the no-analog-output message (engine stays alive);
  `/devices/bogus` shows the index; clicking an index entry routes + swaps to that device; in-browser.
- **Task 6.2.4 — The bench stage: grid + ruler + both faces + live params/meters.** The dedicated stage:
  mm grid + rack-U (44.45 mm) ruler sized from `formFactor`; render the faceplate twice (front + back) via
  `deviceUi`, wired to the session (`valueFor`/`onParam`/`configFor`/`onConfig`/`readingFor`). Structure
  the stage so a `WorldApi`-shaped surface can be exposed for 6.3 patching (no cables drawn yet). _Done:_
  the 8i6 renders both faces on a correctly-scaled grid; knobs/switches drive params (live) and config
  (recompiles); meters update from the session; in-browser.
- **Task 6.2.5 — Engine hardening: reject a non-analog output tap (a hot-path panic-freedom fix).**
  _Discovered during 6.2.3:_ tapping a **digital** output makes `render_quantum` hit `unreachable` — a
  **CLAUDE.md §6 non-negotiable violation** (the hot path must be panic-free; fallible validation belongs
  at `build`/`compile`), and it's **session-fatal** (a wasm trap poisons the worklet instance, so every
  later call cascades into "recursive use / unsafe aliasing"). The workbench works around it in JS
  (`analogOutputPort`), but the engine must not depend on callers being careful — especially a *dev bench*
  where odd patches are the point. Fix: `build_patch` (`crates/devices/src/build.rs`) **validates the
  output tap resolves to an analog port** and returns a `BuildError` otherwise; `load_patch` already keeps
  the running scene on a build error, so a bad tap becomes a rejected swap, not a trap. Rust change +
  `pnpm run wasm` rebuild. _Done:_ a unit test asserts `build_patch` rejects a digital output tap;
  `render_quantum` can no longer be reached with one; the JS `analogOutputPort` guard stays as the
  friendlier front-line message; full Rust gate + web gate green. (Once landed, the workbench could tap
  digital optimistically and fall back gracefully — but the analog-first choice stays for silent 6.2.)

_Validate:_ `localhost:5173/devices/scarlett_8i6` renders the device on a mm/rack-U grid (both faces),
with all params/config drivable and readouts live through the shared `SceneSession`; the suspended boot
delivers the catalog **before** any gesture and audio resumes on first interaction; an unknown `typeId`
lands on the catalog index; the **scene view still behaves identically** (no regression from the router /
`resume()` decoupling); no new dependency; `pnpm run check && pnpm run typecheck && pnpm run test &&
pnpm run build` green. (Rig, patching, and the URL-persisted scene are 6.3.)

_Delivered:_ ✅ all five tasks landed; `/devices/<typeId>` renders a single device (both faces, mm/rack-U
grid) with params/config/meters live through the shared `SceneSession`, booted suspended so the catalog
arrives pre-gesture. What shipped:

- **Router (6.2.1).** Hand-rolled, zero-dependency: `router.svelte.ts` parses `location.pathname`
  (`^\/devices(?:\/([^/]*))?\/?$` → `{ view: "workbench", typeId }`, else `{ view: "scene" }`), `Root.svelte`
  switches `Workbench` vs `App`, `main.ts` mounts `Root`; `navigate` uses `pushState` + a `popstate`
  listener. Bare `/devices` and unknown types both fall through to the catalog index.
- **Decoupled resume (6.2.2).** `startEngine` no longer calls `audio.resume()`; `EngineControl` gained
  `resume()`, surfaced as `session.resume()`. The scene view's start button calls `start()` then `resume()`
  (behavior identical); the workbench boots suspended and calls `resume()` once on the first
  pointer/keydown.
- **Workbench bring-up (6.2.3).** `Workbench.svelte` constructs its own `SceneSession` on a minimal
  `synth_voice` bootstrap scene (`workbench-scene.ts`), boots suspended, and on catalog arrival resolves the
  route's `<typeId>`: valid → `hotSwap` to a single-device scene tapped at its first analog output
  (`analogOutputPort`); digital-only-output devices (e.g. `computer`) → a "needs a DA / monitor chain (6.3)"
  message with the engine kept alive; unknown/bare → the catalog index (links to `/devices/<id>`). Covered
  by `workbench-scene.test.ts`.
- **Bench stage (6.2.4).** `BenchStage.svelte`: mm grid + rack-U (44.45 mm) ruler sized from `formFactor`
  (`footprint`), both faces rendered via `deviceUi` twice (front + back), all wired to the session through
  the identical `DeviceUiProps` (`valueFor`/`onParam`/`configFor`/`onConfig`/`readingFor`) — no forked
  rendering.
- **Engine hardening (6.2.5).** `build_patch` now validates the output tap resolves to an analog port and
  returns `BuildError::OutputTapNotAnalog` otherwise (unit test `non_analog_output_tap_is_rejected`), so a
  digital tap is a rejected swap instead of a `render_quantum` `unreachable` trap; `load_patch` keeps the
  running scene on the error. The JS `analogOutputPort` guard stays as the friendlier front-line message.

_Deviations from plan (not bugs):_

- **The `WorldApi`-shaped surface was not built.** The plan asked 6.2.4 to "structure the stage so a
  `WorldApi`-shaped surface can be exposed"; in practice the stage uses a plain `transform: scale` +
  native-scrollbar pan with **no** `WorldApi`/`clientToSurface`. Building that surface is folded wholesale
  into **Task 6.3.1** (where the patching machinery actually needs it), rather than half-built here.
- **Wheel-zoom + scrollbar pan added as a usability extra.** Beyond the planned "grid + ruler + faces,"
  6.2.4 also gave the bench `transform: scale` wheel-zoom (clamped, WorldView's sensitivity) + scrollbar
  pan. It is **not** cursor-anchored; scene-side-parity zoom is **Task 6.3.1**.

### Story 6.3 — Bench patching + URL-persisted temp scene — ✅ **Done**

_Goal:_ Turn the silent single-device bench of 6.2 into a **hand-patchable workbench**: the
device-under-test (DUT) plus a **fixed supporting cast** (synth source, DA converter, speaker) laid out
around it, cables the user draws to build source→DUT→monitor themselves, a clicked-jack tap that chooses
what's audible, a reused keybed to drive the source, and the whole temp scene living **in the URL** so the
Rust-rebuild → reload loop restores itself. This is the Epic's payoff (PROJECT_PLAN §7 — the UI a pure
consumer of the engine API): the bench becomes a full patching surface driving the **same**
`SceneSession` + `PatchController` + `cable-view` as the scene view, never a forked copy.

_Watch out:_

- **One cable implementation (the Epic's load-bearing watch-out).** The bench must drive the **existing**
  `PatchController` + `cable-view.ts` + `jack-anchors.ts` — never a second cable/anchor path. Those pure
  modules today take a scene-spatial `LayoutCtx` (`projection.deviceRect`/`effectiveFacing`, backed by
  placements/racks/room/wall); the bench has empty spatial `ui` and shows **both faces at once**. So the
  reuse is only real once `cable-view`/`jack-anchors` are **decoupled from `LayoutCtx`** behind a small
  injected layout interface (6.3.3) — otherwise the bench would fork the geometry, violating the rule.
- **Scene-view parity is a hard gate on the shared refactors.** Extracting the layout interface (6.3.3),
  hoisting `WorldApi` (6.3.1), and extracting the keyboard-input glue (6.3.5) must leave the scene view
  **pixel- and behavior-identical** — these touch code App depends on. Anything ambiguous → keep today's
  behavior and note it.
- **Hot-path / layer rules unchanged.** No Rust changes are expected (the catalog already carries
  `synth_voice`/`da_converter`/`speaker`, and `build_patch` already rejects a non-analog tap — 6.2.5). The
  catalog gains **no** bench vocabulary; the bench reads descriptors as a pure consumer.
- **`$state.snapshot` at the worklet boundary still holds** — every `postMessage` (hot-swap on each patch
  edit, param, tap change) snapshots the proxy, as the session already does.
- **URL scene is disposable by design.** Versioned via the existing `SCHEMA_VERSION`; on mismatch,
  **regenerate** the default bench for the route's `typeId` — never migrate. Debounced `replaceState` (no
  history spam).
- **Scope guard.** No debug panel, no `wasm:watch` hot-loop, no bench signal-generator device (all 6.4);
  no drag-to-pan / fit-reset (cut at planning — cursor-anchored zoom only); no auto-rig / listen-selector
  widget / port-domain monitor-chain builder (cut at planning — the user patches by hand).

_Design notes (settled at planning):_

- **No auto-rig — a fixed supporting cast, patched by hand.** The bench always shows the DUT (centerpiece:
  both faces, real dims, rack-U ruler) plus `synth_voice`, `da_converter`, `speaker` as adjacent
  both-faces devices, **auto-arranged in surface mm and left unwired**. The user draws source→DUT→monitor
  with cables. _Rejected: the sketch's auto-generated rig_ (port-domain branching, auto-inserted DA, a
  listen-selector widget) — the user wants the bench to "make everything available and patch as they see
  fit"; hand-patching is a first-class bench feature, and the auto-builder was speculative complexity for a
  dev tool. **Consequence (not a bug):** audibility is user-driven — you patch the chain, then play — which
  intentionally softens the Epic's "pre-patched … audio after one gesture" exit phrasing.
- **The tap is a clicked analog output jack.** Clicking an analog output jack sets `patch.output` and shows
  a "listening here" marker (default: the DUT's first analog out). Digital outs aren't directly tappable
  (`build_patch` rejects — 6.2.5), so to hear a digital device the user patches through DA+speaker and taps
  the speaker. _Rejected: a separate listen-selector widget_ — the jack UI already exists; clicking it is
  discoverable and needs no new synced control.
- **Decouple `cable-view`/`jack-anchors` behind an injected layout interface.** Replace their `LayoutCtx`
  dependency with a minimal injected surface (`deviceRect(id)`, facing/shown-faces, `deviceById`) — backed
  by `projection` for the scene view (identical behavior) and by the bench's auto-layout for the bench.
  Because the bench shows **both faces**, every jack is measured (the precise path always applies; the
  hidden-face estimate rarely fires). _Rejected: synthesizing a fake scene-spatial `LayoutCtx` for the
  bench_ (fake placements/room) — the bench has no walls/racks and both-faces breaks the single
  `effectiveFacing` assumption; a clean injected interface is the honest seam and the extract-don't-
  duplicate move.
- **`WorldApi`-shaped bench surface + cursor-anchored zoom.** Hoist `WorldApi`/`SurfacePoint` out of
  `WorldView.svelte`'s module script into a standalone module (the bench shouldn't import a type from the
  scene-view widget; 4 modules already import it). The bench implements `clientToSurface` (divide out the
  `scale`, subtract the live surface rect), `worldToSurface` (bench world ≡ surface mm), `measureRoot`
  (the scaled surface). Zoom becomes **cursor-anchored** by adjusting the scroll offset so the point under
  the cursor stays fixed — **keeping the existing scrollbar pan**. _Rejected: WorldView's translate-based
  pan + drag-to-pan backdrop + fit/reset_ — cut at planning ("only cursor-anchored zoom for usability");
  scroll-anchoring reuses the working scrollbar-pan and is the smaller change.
- **Reuse the keybed via extracted wiring glue.** `Keybed.svelte`, `wireKeyboard`/`wireMidi` (`engine.ts`),
  and `notes.ts` are already modular; the reuse target is App's inline glue — the `playNote` wrapper + the
  `wireKeyboard` attach `$effect` + the target accessor. Extract that into a shared keyboard-input helper
  that takes a `() => target | null` accessor; App passes its focus-derived target (behavior identical),
  the bench passes a fixed target. _Keeps 6.1's discipline_ (target selection stays view-side; nothing
  entrenches `isPlayable`). **Settled simplification (not a bug):** the bench keybed targets the synth
  source by default, or the DUT when the DUT itself has an event input — revisable in 6.4.
- **URL codec: `base64url(JSON)`, dep-free.** `JSON.stringify` the temp scene → base64url; decode + guard
  on `SCHEMA_VERSION` (mismatch → `null` → regenerate the default bench), mirroring `scene-store.parseScene`.
  Bench scenes are a handful of devices + a few param overrides (a few hundred bytes), so compression is
  unnecessary. _Rejected: `lz-string`_ (a new runtime dep against the zero-dep web posture) _and
  `CompressionStream`_ (async in the debounced save path) — both unjustified at this size.
- **Supporting-cast devices are auto-arranged, not user-draggable.** The bench isn't the scene view; device
  positions are computed from footprints. Only cables + params + the tap are user state (and thus the only
  things persisted).

- **Task 6.3.1 — Bench surface + cursor-anchored zoom.** Hoist `WorldApi`/`SurfacePoint` into a standalone
  module (both views import it; scene view unchanged). Give `BenchStage` a `WorldApi` implementation
  (`clientToSurface` dividing out `scale`, `worldToSurface`, `measureRoot`) and cursor-anchored wheel-zoom
  (adjust scroll offset to keep the point under the cursor fixed; keep scrollbars). _Done:_ zoom holds the
  cursor point fixed; the surface exposes a `WorldApi`; scene view behaves identically; web gate green;
  verified in-browser.
- **Task 6.3.2 — Fixed supporting cast on the bench.** Render the DUT (centerpiece, ruler, real dims) plus
  `synth_voice`/`da_converter`/`speaker` as adjacent both-faces devices, auto-laid-out in surface mm,
  unwired — each wired to the session's param/config/meter lanes via the shared `DeviceUiProps`. Extend the
  bench scene builder (`workbench-scene.ts`) to seed these device instances. _Done:_ the four devices
  render with all jacks visible/reachable and live params/meters; in-browser.
- **Task 6.3.3 — Decouple `cable-view`/`jack-anchors` from `LayoutCtx`.** Introduce a minimal injected
  layout interface (`deviceRect(id)`, facing/shown-faces, `deviceById`); back it with `projection` for the
  scene view and with the bench auto-layout for the bench. Unit-test the cable geometry against a fake
  bench context (both-faces → all-precise anchors; a clamped cable path computed by hand). _Done:_
  scene-view cables/anchors pixel-identical (parity check); the bench context resolves device rects/faces;
  new unit tests green; web gate green.
- **Task 6.3.4 — Bench patching + click-to-tap.** Construct `PatchController(session)` in the workbench,
  wire window `pointerDown/Move/Up` delegations + the `measure(worldApi)` `$effect` (layout-dep list), and
  render the cables/overlay SVG layers via `cable-view` + `cablePathData`. Clicking an analog output jack
  sets `patch.output` (default the DUT's first analog out) with a "listening here" marker; `hotSwap` on
  each edit. _Done:_ same-view drag, click-to-pick, disconnect, and cable-type change all work on the
  bench; a user-built synth→DUT→(DA→)speaker chain is audible via the clicked tap; in-browser.
- **Task 6.3.5 — Keyboard-input reuse.** Extract App's keybed glue (the `playNote` wrapper + `wireKeyboard`
  attach `$effect` + target accessor) into a shared keyboard-input helper taking a `() => target | null`;
  App consumes it (focus-derived target — behavior identical). Mount `Keybed` on the bench targeting the
  synth source (or the DUT when it has an event input), fed by `session.heldNotes`/`session.playNote`.
  _Done:_ bench notes play (QWERTY + on-screen keybed); scene-view keybed, QWERTY capture, and Web MIDI
  behave exactly as before; in-browser.
- **Task 6.3.6 — URL-persisted temp scene.** A `base64url(JSON)` codec with a `SCHEMA_VERSION` guard
  (mismatch → regenerate the default bench for the route's `typeId`); seed the workbench `SceneSession`
  from the URL on load; write the scene to the query with debounced `replaceState` on every patch/param/tap
  change (path stays `/devices/<typeId>`). Round-trip + version-mismatch unit tests. _Done:_ patch + param
  overrides + the tap survive a reload; a version mismatch regenerates cleanly; no history spam; the bench
  is audible again within seconds of a reload; gate green; in-browser.

_Validate:_ at `localhost:5173/devices/scarlett_8i6` the bench shows the DUT plus the synth/DA/speaker
supporting cast; the user can drag/click cables (via the shared `PatchController`/`cable-view`) to build
source→DUT→monitor, click an analog output jack to choose the audible tap, and play the reused keybed to
hear it; the temp scene (patch + param overrides + tap) round-trips through the URL and restores on reload
(regenerating on version mismatch); the **scene view still behaves identically** through the `WorldApi`
hoist, the `cable-view` `LayoutCtx` decouple, and the keyboard-glue extraction; no new dependency;
`pnpm run check && pnpm run typecheck && pnpm run test && pnpm run build` green. (Debug panel, `wasm:watch`
hot-loop, and a bench signal-generator device are 6.4.)

_Delivered:_ ✅ all six tasks landed; the workbench is now a full hand-patchable bench that drives the
**same** `SceneSession` / `PatchController` / `cable-view` / keybed as the scene view and restores itself
from the URL. What shipped:

- **Bench surface + cursor-anchored zoom (6.3.1).** Hoisted `WorldApi`/`SurfacePoint` into a standalone
  `world-api.ts` (both stages import it); `BenchStage` implements the `WorldApi` (`clientToSurface` divides
  out the scale) and got cursor-anchored wheel-zoom (scroll-anchored; scrollbar pan kept).
- **Fixed supporting cast (6.3.2).** `benchScene` seeds the DUT plus a synth source + DA + speaker
  (unwired); the digital-only refusal is gone (the speaker's analog tap makes any device benchable).
  Devices + both faces stack vertically.
- **`cable-view` decoupled from `LayoutCtx` (6.3.3).** The geometry takes an injected `CableLayout` (scene
  view backs it with the spatial projection; the bench with a flat both-faces layout), so both drive one
  cable/anchor implementation. Parity-guarded by the existing suite; bench cases added.
- **Bench patching + tap + inspector (6.3.4).** The bench drives the shared `PatchController` with the
  identical flow (drag **and** click-to-pick), draws leads via a new shared `Cable` component, and edits
  them via a new shared `CableInspector` (cable-type + disconnect). The monitored tap is a **"Listen"**
  header selector.
- **Keyboard-input reuse (6.3.5).** `wireKeyboardInput` (shared `keyboard-input.svelte.ts`) +
  `eventsInputDriven` (moved to `scene-ops`); the bench mounts the shared `Keybed` with a **"Send to"** selector
  (All / per-device), a sticky header + keybed, and a collapsible keybed.
- **URL-persisted temp scene (6.3.6).** `url-scene.ts` encodes the scene as URL-safe base64 (version-
  guarded, regenerate-on-mismatch); the bench seeds from `?s=` on load and writes it back via debounced
  `replaceState` (path kept at `/devices/<typeId>`) — the rebuild→reload→restore loop.

_Deviations from plan (not bugs):_

- **A large mm-sizing pass, mid-story (not a planned task).** The 6.2 faceplates were tuned for the old
  oversized `formFactor`; once the 8i6 dimensions were corrected the controls dwarfed the panel, so
  faceplate controls were re-sized in **real mm** — knobs/jacks/legends via `size` props + inherited face
  vars, from measured Focusrite dimensions (XLR 23 mm, gain 14 mm, monitor 28 mm, ¼" 8 mm, DIN 18 mm,
  digital 9 mm) — the cables scaled to real gauge, and the **focus overlay became a zoomed physical view**
  (a magnified faceplate, gated by `hasFocusSurface`). Net-new shared widgets: `Cable.svelte`,
  `CableInspector.svelte`.
- **Tap = "Listen" header selector, not a clicked output jack** (the plan's design note). A plain jack
  click collided with click-to-pick patching, so the tap moved to a header dropdown — keeping patching feel
  identical to the scene view.
- **Patching kept full parity** (drag + click-to-pick) rather than the drag-only sketch — one flow, both
  views.
- **Keybed gained a "Send to" multi-target selector (All / any MIDI input) + sticky/collapse** beyond the
  plan's "reuse the keybed"; `wireKeyboardInput` therefore fans a note to a *list* of targets.
- **Fixed a latent 6.2 bug:** benching `synth_voice` (== the bootstrap type) skipped the cast swap — the
  guard now tracks a `benchedFor` type. Also corrected the 8i6 `formFactor` (was 1216 mm wide).

### Story 6.4 — Debug surface + the hot loop — 🚧 **In progress**

_Goal:_ Give the bench the developer instrumentation it exists for (PROJECT_PLAN §7 — the UI a pure
consumer of the engine API): an **audio-parameter debug surface** — a scalable, searchable/pinnable
inspector over the rig's params, configs, and readouts, plus an always-on header (master level + tap,
signal-path latency, connection losses) — and the **hot loop** that closes the Rust-edit → audible-again
cycle (`wasm:watch` → rebuild → Vite reload → URL restore → resume). The panel reads the **same**
`SceneSession` the scene view does; nothing forks the engine/patch plumbing.

_Watch out:_

- **Scale from the get-go — the DUT has hundreds of params.** The 8i6 exposes 206 (mostly routing
  crosspoints), so a flat "every param" list is unusable. The inspector is a **filter + pin watch-list**,
  not a dump; you monitor only what the task at hand needs.
- **Pins must survive the hot loop.** The whole point is editing Rust and reloading; a watched param
  re-renders after the reload only if the pin set persists — so pins live in `scene.ui` and round-trip
  through the URL (like the rest of the bench state), never in ephemeral component state.
- **"Auto-resume" is one-click, not zero-click.** Browser autoplay suspends audio on every reload; the
  bench already resumes on first interaction. The loop restores the bench (pins included) and needs one
  click to sound — a browser constraint, recorded as **not a bug**.
- **Audio surface only (scope narrowed at planning).** The panel shows audio parameters —
  params/configs/readouts/losses/latency/level/tap — **not** engine-internal health (overruns, render-ms,
  drops) nor a seed control. Those stay on `session.health` / the pinned `SEED` as today.
- **Layer + hot-path rules hold.** The catalog gains no debug vocabulary; the panel is a pure
  descriptor/reading consumer. `$state.snapshot` still guards every worklet post; no Rust changes for the
  debug surface (the seed control that *would* need Rust is deferred).

_Design notes (settled at planning):_

- **The inspector is a filter + pin watch-list over params + configs + readouts.** A plain text filter
  matches across all devices by name/label/id (a results list you pin from — **not** an ARIA combobox:
  same value, far less complexity, chosen explicitly over the fancier control). Pinned items form a live
  watch-list with unpin; the searchable set unifies params, **configs** (tagged *recompile-on-change*,
  which is how the "config-vs-param" distinction reads), and readouts, so you can watch any of them.
  _Rejected: a flat grouped-by-device list_ — usable at 5 params, useless at 206.
- **Pins persist in `scene.ui` (URL round-trip).** A `benchWatch` list of `{device, kind, id}` on the
  UI-only scene, optional like `bench`, so pins survive `wasm:watch` reloads. Panel values are
  **read-only** — the faceplate knobs remain the editor; the panel's job is the exact numbers + ranges +
  ids the knobs don't surface. _Rejected: localStorage_ — the bench's state already lives in the URL; keep
  one persistence home.
- **Always-on header for the few-enough things.** Master output peak (`session.level`) + monitored tap
  (`patch.output`), signal-path latency (new: stored on the session from the `ready` message), and the
  connection-loss list — no filtering needed at these counts.
- **App's existing losses/readouts/level panels stay (dedup deferred).** The bench inspector (filter +
  pins) is a genuinely different presentation, and the load-bearing plumbing (the `SceneSession`) is
  already shared — so a bench-specific debug UI doesn't fork the layer the Epic's "one plumbing path"
  watch-out protects. Unifying the small losses/level renderers is a later cleanup if it earns its keep.
  _Deferred with it (recorded, not dropped):_ the **seed control**, the **engine-health surface**, and the
  bench **signal-generator** device.
- **The hot loop needs a watcher the user installs.** `cargo watch` over `crates/` runs `build-wasm.sh`;
  Vite full-reloads on the rebuilt artifact. `cargo install cargo-watch` is a system change — surfaced for
  the user to run, not performed by tooling.

- **Task 6.4.1 — `DebugPanel` shell + always-on audio header.** A session-driven `DebugPanel.svelte`:
  master output peak + monitored tap (`patch.output`), signal-path latency (store it on the session from
  `ready`), and the connection-loss list (from→to · dB). Embed as a collapsible panel in the workbench.
  _Delivered:_ `web/src/DebugPanel.svelte` (reads the shared `SceneSession`); a new `SceneSession.latencyMs`
  field seeded from the `ready` message; peak shown as dBFS, tap + losses resolved via the catalog. Web gate
  green (typecheck · Biome · Vitest).
- **Task 6.4.2 — Searchable, pinnable watch-list.** A text filter across every device's
  params/configs/readouts; pin/unpin to a live watch-list; persist pins in `scene.ui.benchWatch` (URL
  round-trip). Configs tagged recompile; values read-only. _Delivered:_ pure enumeration + matching in
  `web/src/bench-watch.ts` (`watchables` / `matchesQuery`, empty query matches nothing — filter-to-pin, not
  a dump), pin helpers (`watchKey` / `isWatched` / `toggleWatch`) in `scene-ops.ts`, `BenchWatch` added to
  `SceneUi` (optional, so it rides the existing URL round-trip). Stale pins degrade to an "unavailable" row.
  Unit-tested (`test/bench-watch.test.ts`); results capped at 40 with an overflow note (no silent cap).
- **Task 6.4.3 — The hot loop (`wasm:watch`).** A watch script (`cargo watch` over `crates/` →
  `build-wasm.sh`) + Vite full-reload on the rebuilt wasm; document the loop and the one-click-resume.
  _Delivered:_ `wasm:watch` script (runs `cargo watch` from the repo root so `--watch crates` resolves) + a
  `reload-on-wasm-artifact` Vite dev plugin (full-reloads when `public/*.wasm`/`processor.js` land, since
  they're outside the module graph); documented in `CLAUDE.md`; `cargo install cargo-watch` surfaced.
  _Known issue (parked):_ the watcher currently **rebuild-loops** — cargo-watch reacts to the build's own
  `web/public/*` output; the fix is a source-only watch / ignore rule.

_Also delivered this Story (emergent, beyond the planned tasks):_

- **Web MIDI wired into the workbench.** It was missing entirely (only the scene view called `wireMidi`), so
  a hardware controller did nothing on the bench — now wired (once the engine is ready) through the same
  `playNote` fan-out QWERTY/keybed use.
- **MIDI monitor in the debug panel.** MIDI-access status, currently-held notes, and a live routed-event log
  (note name / velocity / target device, with an "engine not ready" flag). Backed by a rolling
  `SceneSession.midiLog` appended in `playNote` (the one funnel every note source passes through) + a
  `noteName` helper in `notes.ts` (unit-tested). The first thing to check when a triggered note is silent.
- **Scarlett 8i6 input readouts.** Two transparent inline `VuMeter` nodes appended (indices 22–23, after the
  matrix so no existing node index / param id shifts) post-preamp on combo 1/2, exposing 4 readouts (In 1/In
  2 VU + peak-dBu). Engine change in `crates/devices/src/catalog.rs`; full Rust gate green (render tests
  incl.); wasm rebuilt. The knob-**ring** rendering that motivated them is deferred to `docs/IMPROVEMENTS.md`.
- **Debug panel → collapsible right-hand drawer.** Moved out of the bench's vertical stack into a fixed
  right-side drawer with an always-visible header toggle; scrolls independently.
- **Device focus mode on the bench.** An "⛶ Open" header button opens the DUT's focus surface via the shared
  `focusUi` / `isFocusable` machinery — for the 8i6 the Focusrite Control **routing matrix**, which is how
  you route an input to an analog output on the bench (Esc / backdrop closes; a focus keybed plays the
  focused device). Previously the bench had no way in.

_Clarified (by design, not a bug):_ the bench has **no auto-rig** (Story 6.3) and the 8i6 **boots
powered-off** (the main scene turns it on explicitly), so there is no sound until you patch the chain by
hand, power the DUT on, and route an input to the tapped analog output.

_Validate:_ at `localhost:5173/devices/scarlett_8i6` the debug panel shows the rig's master level/tap,
signal-path latency, and connection losses live; you can filter to any of the 8i6's 206 params/configs (or
a readout), pin a handful, and monitor only those; pins survive a reload; and `pnpm wasm:watch` (with
`cargo-watch` installed) turns a Rust save into a restored, one-click-audible bench within seconds.
**Status:** implemented and green (Rust + web gates); in-browser verification + commit by Oskari pending
(and the `wasm:watch` rebuild-loop above to resolve).
