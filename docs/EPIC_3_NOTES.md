# Epic 3 — Real-Time Playback — Design Notes & Delivery Record

Companion archive to `IMPLEMENTATION_PLAN.md`. Epic 3 is **complete** — Stories 3.1–3.4 done, the
engine live in the browser (turn knobs, play an instrument, glitch-free at low latency). The plan keeps
a tight summary of Epic 3; **this file is the full record** — the *settled design notes* (with the
rejected alternatives and the reasoning that justified each choice), the per-task breakdown, and the
per-task *Delivered* notes.

Read this when a later epic's design decision turns on **why** Epic 3 was built the way it was, or when
you need the exact API/behavior of something Epic 3 shipped. For *what exists and what binds you going
forward*, the plan's Epic 3 summary is enough; come here for the depth behind it.

The plan's section ordering (Goal → Watch out → Design notes → Tasks → Validate → Delivered) is preserved
per story.

---

## Epic 3 framing (as originally written)

**Goal:** the engine live in the browser — turn knobs and play an instrument with low latency, glitch-free.
The engine runs **inside the AudioWorklet** (WASM) on the audio thread; control crosses the main→audio
boundary as sparse messages. This epic retires the heaviest technical unknown (real-time fidelity of the
oversampled voltage domain) flagged in PROJECT_PLAN §10.

**Exit criteria:** a running patch is audible in real time, stable (no dropouts under normal use),
with knob changes and note playing responsive at low latency (~5–12 ms target).

**Watch-outs:** the hot-path contracts (zero-alloc, lock-free, panic-free, denormal flush) become
non-negotiable here — a panic or stall on the audio thread kills the stream. `cargo wasm` is only a
*portability check* today; this epic produces the first real WASM **build + instantiation**, a gap bigger
than the one-line task wording used to imply. Measure latency, don't assume it. **Mono only** (epic-wide,
inherited from Epic 2 — multichannel digital is Epic 5); the single output channel is duplicated to the
output device's channels.

**Settled this planning pass (the architecture decisions that shape the stories):**
- **Execution model — engine *inside* the AudioWorklet, single-threaded on the audio thread (not a
  Worker+ring).** `Schedule::process_io` runs synchronously in `process()`; lowest latency, simplest, no
  SharedArrayBuffer needed to make sound. The jitter cushion is the **browser's own output buffer**, sized
  via `AudioContext({ latencyHint })` to ~3–4 quanta (~8 ms) — a single thread *cannot grow its own
  render-ahead buffer* (it only computes during callbacks), so the browser buffer is the cushion, and its
  depth *is* added latency. **This is gated on the 3.1 perf spike:** comfortable throughput headroom
  (≳2–3× real-time) confirms it; marginal (~1.2–1.8×) forces a fallback to a **Worker + SAB ring** (engine
  renders ahead concurrently, worklet drains — robust to sustained load but adds latency + complexity);
  <1× means cut the oversampling factor or optimize the hot path before real-time is viable. A→Worker
  migration keeps the engine surface intact (only *where* `process_io` is called moves).
  **✅ Resolved (3.1 spike): ≈46× real-time in-browser (release, `+simd128`), far past the ≳2–3× bar —
  the in-worklet single-thread model is confirmed; the Worker+SAB fallback is not needed.**
- **Bindings — hybrid.** `wasm-bindgen` for the cold/setup surface (construct/configure the engine,
  generated TS types for Epic 4 to consume) — the well-beaten path — but the **per-quantum hot path
  reads/writes raw shared linear memory directly** (a `Float32Array` view over WASM memory), bypassing
  marshalling for zero-copy `process()`. Accepts the `wasm-bindgen-cli`/`wasm-pack` tooling + version
  pinning. The raw hot path needs a localized `#[allow(unsafe_code)]` (the per-module opt-in the workspace
  lint policy already anticipates). **Sequencing:** the **raw zero-copy hot path lands in 3.2**, when the
  worklet drains it every quantum; **3.1 ships only the minimal compute-only surface** the feasibility
  benchmark needs (loop `process` inside WASM, time from JS — no marshalling, no `unsafe`).
- **Web/dev harness — Vite + TypeScript, a *throwaway page* on *reusable infrastructure*.** A top-level
  `web/` dir (peer to `crates/`, own `package.json`) hosts the worklet + WASM and a dumb test rig (a few
  sliders + a keyboard). This respects **engine-before-UI** (PROJECT_PLAN §4): the *page* is disposable,
  but the *build/serve infrastructure* (Vite, wasm integration, worklet-loading pattern, COOP/COEP) carries
  into Epic 4's real UI. Not the Epic 4 product UI — that is built once against the proven API.
- **Param/event transport — postMessage-first.** Both lanes cross via `port.postMessage`, drained at the
  top of `process()` into Epic 1's existing `ParamQueue`/`EventQueue`. Params push a **latest-wins target
  value** (the engine's own `Smoother` de-zippers — so **not** `AudioParam`, which would double-smooth and
  can't express a graph-dynamic param set). Events push `(when, message)`. The **lock-free SAB ring**
  (sample-accurate, zero-alloc, the thing that genuinely retires the deferred *"lock-free cross-thread
  validation"* item) is an isolated 3.4 upgrade *behind the same `EventQueue::push` interface* — and the
  one thing that forces COOP/COEP, so it stays off the critical path until then.
- **Testing posture — unit-test each side, verify the bridge by hand, automate later if needed.** Per the
  [[defer-speculative-test-infra]] approach: the engine is guarded by its existing Rust unit tests, JS glue
  by TS-side unit tests, and the Rust↔JS bridge is checked **manually**. A native↔WASM **parity** test
  (same patch/seed → output within tolerance) is the natural standing guard for numeric drift the
  build-only `wasm-pack build` can't catch — but it is **deferred** until a wasm-only divergence actually
  surfaces, rather than built speculatively in 3.1. Runtime health metrics (underrun counter, measured
  latency) arrive with the hardening work in 3.4. *(You cannot golden-file a live session regardless.)*
- **SIMD is measure-driven, not upfront.** Rely on LLVM autovectorization with `+simd128` first; reach for
  explicit intrinsics (more `unsafe`) only on hot loops the 3.1 spike proves are over budget.
  **✅ Resolved (3.1 spike): nothing is over budget (~46× headroom), and `+simd128` autovectorization buys
  only ~3% (the chain is largely serial/recursive) — so explicit SIMD intrinsics are *not* pursued *now*.
  Scoped to the mono gate: at scale, running one filter across many channels/conductors is a fresh,
  SIMD-friendly axis the gate didn't exercise — re-measure there before concluding.**

> *Tasks below were a coarse sketch, fleshed out to Task level when each Story was picked up — per the
> detail-gradient convention (Epics 2–3 carry Tasks but expect churn). Goals, watch-outs, and the settled
> decisions are recorded here.*

---

## Story 3.1 — WASM engine + real-time feasibility spike — ✅ **Done**
*Goal:* the first real **WASM artifact of the engine** plus the **in-browser faster-than-real-time
benchmark** that gates the whole epic — proof the oversampled voltage chain renders the canonical patch
with enough headroom for the in-worklet model. Stands up the WASM build pipeline + a **minimal
compute-only** `wasm-bindgen` surface, and relocates the **implicit capture** (`Decimator` +
monitor-reference scale/clamp) out of the native harness into a new shared **`capture` crate** reachable by
both the native harness and the WASM bindings (the capture is portable engine code today, but it lives in
`crates/harness` beside `hound`/`textplots`, which never reach wasm32).

*Scope decisions this planning pass (narrower than the original epic sketch — see the two amended
epic-level bullets above):*
- **The benchmark is the whole point, and it runs in a real browser, by hand.** The gate is "can the
  oversampled chain fly with this oversampling factor." We do **not** automate end-to-end testing here:
  the engine is guarded by its existing Rust unit tests, any JS glue by TS-side unit tests, and the
  Rust↔JS **bridge is verified manually**. A native↔WASM **parity test is deferred** (the
  [[defer-speculative-test-infra]] approach — same instinct as the 2.3 golden-harness deferral); it
  becomes the fast-follow *only if a wasm-only numeric divergence (SIMD reassociation, denormals, libm
  `exp`/`sin` drift) actually surfaces*. **Accepted risk:** until then, nothing automatically catches such
  a divergence.
- **Only the minimal compute-only bindgen surface ships here.** The hybrid bindings' **raw-memory
  zero-copy `process` hot path moves to 3.2**, where the worklet actually drains output every quantum. For
  a *feasibility gate* we loop `process` entirely inside WASM and time it from JS — this isolates raw
  compute headroom from marshalling cost (tiny and constant) and needs almost no surface and **no
  `unsafe`** yet.

*Watch out:* this is the first time we build and *instantiate* WASM, not just `cargo check` it — expect
toolchain friction; `wasm-bindgen-cli` must be pinned to **exactly** the `wasm-bindgen` crate version.
Measure, don't assume — benchmark at the **real RT block size** (128 host frames × M = **1024 analog
samples**), not the render harness's 384, and report **per-quantum worst case**, not just mean throughput
(real-time dies on the slow callback). The benchmark page is a **throwaway static page** — *not* the Vite
harness, which is 3.2; don't pull Vite forward. Enable `+simd128` for the gate (that's the real
deployment) and also record a scalar number to know the SIMD win. `panic=abort` in release/wasm means a
panic kills the run — fine, since `process` is already total.

*Design notes (settled at planning):*
- **New `capture` crate** (workspace member, peer to `engine`), depending **only on `engine`** — stays
  pure (no `hound`/`textplots`/native deps) so it compiles to wasm32. `Capture` and its tests move
  **verbatim** from `crates/harness/src/capture.rs`; `harness` and `wasm-bindings` both depend on it. A
  dedicated crate keeps the "**capture is outside the simulation**" boundary explicit (better than burying
  it in `engine`).
- **Canonical patch, pinned (the gate fixture):** `synth → modeled AD → modeled DA → speaker` (the
  `first_sound_graph` shape), `AnalogRate` 384 kHz, host 48 kHz (M = 8), `seed = 0`, **`block_len = 1024`**
  (the RT quantum), 1 V full-scale; a sustained note gated on so the voice is actually generating. Built
  **inline in the bindgen type** (a ~10-line duplication of `first_sound_graph` — acceptable; keeps the
  harness's `main` scenarios independent). Record this config so the headroom figure is reproducible.
- **Minimal compute-only surface:** a `#[wasm_bindgen]` engine type that, on construction, builds the
  pinned patch via `compile(graph, block_len, rate, seed)` + a `Capture`, and exposes a `render_blocks(n)`
  that loops `process_with_events` + `Capture::process` **inside WASM** (no per-block marshalling). JS
  times it with `performance.now()`. No raw pointer / `Float32Array` view yet (→ 3.2).
- **Build pipeline:** `wasm-pack build --target web` (release; `panic=abort` already set), `+simd128` via
  a wasm32 `target-feature` (RUSTFLAGS or `.cargo/config.toml`). Add a **build-only `wasm-pack build` step
  to CI** to catch bindgen breakage (`cargo wasm` only `check`s — it won't catch a broken artifact). The
  benchmark itself is **run manually in a browser**, not in CI.
- **Reporting:** the page prints the realtime ratio (wall-clock to render T s of audio ÷ T) and the
  per-quantum max, with and without `+simd128`. The number **decides the 3.2 execution model**: ≳2–3× →
  engine-in-worklet single-thread; marginal (~1.2–1.8×) → Worker + SAB ring fallback; <1× → cut the
  oversample factor / optimize the hot path before real-time is viable.

- **Task 3.1.1** — New shared **`capture` crate**: move `Capture` (+ its tests) out of `harness` into a
  workspace member depending only on `engine`; repoint `harness` to it; confirm it `cargo wasm`-checks.
  Pure mechanical move, no behavior change — the full Rust gate stays green.
- **Task 3.1.2** — **WASM build pipeline**: add `wasm-bindgen` to `wasm-bindings`, the `+simd128` config,
  and `wasm-pack build --target web` producing an artifact; add a build-only `wasm-pack build` step to CI.
  Document the exact install + build commands (the `wasm-pack`/`wasm-bindgen-cli` installs are the user's
  to run; pin the CLI to the crate version).
- **Task 3.1.3** — **Minimal compute-only bindgen surface**: a `#[wasm_bindgen]` engine type that builds
  the pinned canonical patch + `Capture` and exposes `render_blocks(n)` looping `process` + capture inside
  WASM. A native rlib unit test asserts `render_blocks` runs and produces non-silence (guards the surface
  natively, no browser needed).
- **Task 3.1.4** — **Throwaway browser benchmark page**: static HTML + JS loading the `--target web`
  artifact, running `render_blocks` in a `performance.now()` loop at the pinned config, reporting realtime
  ratio + per-quantum worst case, with and without `+simd128`. Record the measured headroom here and the
  resulting 3.2 execution-model decision.

*Validate (✅ met):* WASM builds and instantiates in a browser; the canonical patch renders far
faster-than-real-time (headroom recorded below; **in-worklet single-thread confirmed** — the Worker+SAB
fallback is not needed for 3.2); the full Rust gate (`fmt`/`lint`/`test`/`wasm`/`docs`) stays green and a
build-only `wasm-pack build` step guards bindgen breakage in CI. *(No automated parity — Rust unit tests
+ a manual bridge check, per the deferral above.)*

*Delivered:* the first real WASM build of the engine + the gate that clears Epic 3's heaviest unknown.
**Gate result (in-browser, release):** the canonical patch (`synth → AD → DA → speaker`, 384 kHz / 48 kHz
M8, block_len 1024) renders **≈46× real-time** with `+simd128` (≈45× scalar), at **≈0.058 ms mean per
quantum against the 2.667 ms budget** — a ~46× throughput / ~13× worst-case-quantum cushion (the
per-quantum *max* of 0.200 ms is `performance.now()` resolution-clamped, so that 13× is a conservative
floor). **Decision: engine-in-worklet single-thread for 3.2** (comfortably past the ≳2–3× bar; no
Worker+SAB ring needed). The **SIMD win is ~3%** — the oversampled chain is largely serial/recursive
(one-pole filters, FIR, synth) and doesn't autovectorize much, so **explicit SIMD intrinsics are not
justified** (the measure-driven SIMD decision, settled: rely on `+simd128` autovectorization only).
**Shipped:** a shared **`capture`** crate (engine-only deps, wasm-reachable; the implicit capture moved
out of the native harness) consumed by both `harness` and `wasm-bindings`; a `wasm-bindgen`/`wasm-pack`
build pipeline (`--target web`, release `panic=abort`, `+simd128` via `RUSTFLAGS`) with a CI bindgen-build
step; a minimal compute-only `BenchEngine` surface (`render_blocks` loops `process` + capture entirely in
WASM, zero per-block marshalling, no `unsafe`) with native unit tests; and a throwaway `bench/` page
(scalar vs SIMD side by side) + `build.sh`. The raw zero-copy `process` hot path and the Vite/TS harness
remain 3.2.

*Scaling probe (in-browser, SIMD; `BenchEngine::scaled(N)` — N parallel `synth → gain → AD → DA` chains
summed to one speaker, `4N+2` nodes):* **throughput is linear in node count** — `realtime× · N` converges
to ≈68 and stays flat (8 ch → 7.97×, 32 ch/130 nodes → 2.08×, 64 ch/258 nodes → 1.06×, 128 ch/514 nodes →
0.53×), so there is **no superlinear scheduling/edge overhead** — pure per-node compute, as the
partitionable-DAG design intends. **One core crosses real-time at ~64–68 of these channels (~260 nodes).**
That's ≈192 heavy units (3 per channel: synth + AD + DA) — matching the ~190 the 1-channel gate predicted,
an independent cross-check. This patch is **conservative** (every channel gets its own AD *and* DA + a
synth); a realistic stadium has ~1 converter per I/O and a cheap digital routing core at 1×, so its node
ceiling on one core is **higher**. Beyond it, the levers are **multi-core DAG partition** and a **lower
oversample factor** (8×→4× ≈ halves analog-domain cost) — Epic-5 concerns, flagged now, not built.

---

## Story 3.2 — First real-time sound *(the live milestone)* — ✅ **Done**
*Goal:* the Epic-3 analogue of Story 1.3's "first runnable" and 2.1's "first sound" — a fixed patch
(`synth note → AD → (DSP) → DA → speaker → capture`) **audible in the browser in real time**, clean at
idle. Hosts the engine in an `AudioWorkletProcessor`, lands the **raw zero-copy output hot path** (the
per-quantum `Float32Array` view over WASM memory the epic deferred from 3.1), and — once that works on the
proven static page — stands up the minimal Vite + TS harness the rest of the epic and Epic 4 build on.

*Watch out:*
- The **wasm-in-worklet** seam — no reliable `fetch` in `AudioWorkletGlobalScope`, so fetch the wasm bytes
  on the main thread and hand **the `ArrayBuffer` bytes** to the processor via `processorOptions` on the
  `AudioWorkletNode` constructor; the processor compiles them **synchronously** in its *constructor* via
  `initSync` (allowed off the main thread, any size) — no init message, no ready/error handshake to race.
  **Do *not* post a compiled `WebAssembly.Module`** — a Module is only structured-cloneable within one
  *agent cluster*, and an AudioWorklet is a separate realm; the clone can fail as a `messageerror` (not
  `message`) and be *silently dropped* (3.2 hit exactly this with `port.postMessage`). Bytes always clone,
  and recompiling in the worklet is the WebKit/Emscripten-recommended approach regardless.
  `AudioWorkletGlobalScope` also has inconsistent **ES-module** support, so the glue the worklet runs must
  be a **classic script** — use `wasm-bindgen --target no-modules` for the worklet artifact, *not* the
  `--target web` ES-module glue the 3.1 bench page uses on the main thread. And it lacks
  **`TextDecoder`/`TextEncoder`** (a Chrome gap) which the no-modules glue constructs *eagerly at load*, so
  a tiny UTF-8 polyfill must be concatenated **ahead** of the glue or the whole module fails to evaluate
  (and `registerProcessor` never runs ⇒ "node name not defined").
- **Pin `AudioContext({ sampleRate: 48_000 })`.** The engine's host rate is hardcoded 48 kHz (M = 8 from
  384 kHz); if the context runs at the device default (often 44.1 kHz) every quantum is the wrong rate ⇒
  wrong pitch + drift. One line, silent bug if missed. The browser resamples 48 k → device for us.
- The **`Float32Array`-view detach** footgun — a view over WASM linear memory detaches if memory ever
  `grow`s. Acquire the output view **after** construction; the zero-alloc hot path keeps it valid for the
  session (it never grows mid-render).
- The **bundler↔worklet** seam (Vite phase only) — the processor must load as a real URL via `addModule`
  (`new URL('./processor.ts', import.meta.url)` or a separate entry), not get inlined.
- Set the browser cushion via `latencyHint` (~8 ms). The AudioWorklet quantum is 128 host frames =
  **exactly one engine block** (1024 analog ÷ M = 128) — honor the multiple-of-M constraint. Duplicate
  mono → the output device's channels. **No COOP/COEP** needed here (postMessage transport only; the SAB
  ring that forces cross-origin isolation is 3.4).

*Design notes (settled at planning):*
- **Decision — static page first, then Vite.** 3.2 introduces two independent new seams; bring them up in
  isolation. **Phase A:** get first-sound on the existing no-bundler static page (reuse the 3.1 `bench/`
  setup) so a failure is unambiguously the worklet/wasm seam, not the bundler. **Phase B:** stand up
  Vite + TS under `web/` and port the working worklet over, isolating the bundler↔worklet seam. The Vite
  *infrastructure* (not the throwaway page) carries into Epic 4 — engine-before-UI holds.
- **Decision — new `RtEngine` bindgen type; freeze `BenchEngine`.** 3.1's `BenchEngine` (peak-float return,
  internal host `Vec`) stays as the frozen feasibility-gate fixture. The real-time path is a new
  `#[wasm_bindgen]` `RtEngine` that owns the same pinned patch + `Capture` but exposes the host block as a
  **raw pointer + len** (`out_ptr()` / `out_len()`), so JS can build one `Float32Array` view over WASM
  memory once and read it every quantum with **zero copy / zero marshalling**. `render_quantum()` runs one
  `process_with_events` + `Capture::process` into the engine-owned host buffer (no return value on the hot
  path). This is the localized `#[allow(unsafe_code)]` the workspace lint policy anticipates — scoped to
  the pointer accessor; the compute stays safe.
- **Parity stays a manual by-ear check (automated parity remains deferred, per 3.1).** Confirm the live
  output sounds like the offline render of the same patch; do **not** build an `OfflineAudioContext` parity
  harness now. The automated native↔WASM parity test is still the fast-follow *only if* a wasm-only numeric
  divergence surfaces ([[defer-speculative-test-infra]]).
- **First-sound demo content:** a short repeating note (re-queue a note-on/off cycle every N blocks), not a
  held drone — recognizably musical and proves the event lane advances correctly over a long session,
  without the grating sustained sawtooth. The event re-queue happens engine-side in `render_quantum` for
  now (live keyboard/MIDI is 3.3).
- **Testing posture (unchanged):** the engine is guarded by its Rust unit tests; `RtEngine`'s surface gets
  a native rlib test (construct, render quanta, assert the exposed buffer is non-silent — no browser); the
  worklet glue is verified **manually** in the browser. Little non-trivial TS logic lands until 3.3's
  time-mapping.

- **Task 3.2.1** — `RtEngine` bindgen surface (raw zero-copy output): new `#[wasm_bindgen]` type owning the
  pinned patch + `Capture` + engine-owned host buffer; `render_quantum()` (one block, no marshalling),
  `out_ptr()` / `out_len()` for a `Float32Array` view, `host_rate_hz` getter; scoped `#[allow(unsafe_code)]`
  on the pointer accessor. Native rlib test: render quanta, assert non-silence and correct sample count.
- **Task 3.2.2** — Worklet artifact + main-thread compile: build the `--target no-modules` wasm for the
  worklet; main thread `WebAssembly.compile`s the bytes and `postMessage`s the `Module` to the worklet,
  which `initSync`s it and constructs `RtEngine`. (Static-page scaffold — Phase A.)
- **Task 3.2.3** — `AudioWorkletProcessor` hot path: per-`process()` call `render_quantum()`, read the
  host buffer via the cached `Float32Array` view, write it to `outputs[0]` and duplicate mono → all output
  channels. Pin `AudioContext({ sampleRate: 48_000, latencyHint })`. First sound on the static page.
- **Task 3.2.4** — Idle-stability check: run the fixed patch continuously; confirm clean playback, no
  dropouts/glitches at no load over a sustained session (manual, plus an optional `process()`-duration log).
- **Task 3.2.5** — Vite + TS harness (Phase B): scaffold `web/` (own `package.json`, dev server, worklet
  loaded via real URL through `addModule`); port the working worklet + main-thread bring-up; confirm
  identical first-sound. This is the reusable build/serve infra Epic 4 inherits.

*Validate (✅ met):* the canonical patch plays continuously and recognizably in the browser, **clean at
idle** (~5.3 ms base latency), on both the static page and the Vite harness; the live output sounds like
the offline render (by-ear — automated parity stays deferred). The hot path is zero-copy (one
`Float32Array` view, no per-quantum marshalling) and the Rust gate stays green.

*Delivered:* first real-time sound. **Engine (`wasm-bindings`):** a new `#[wasm_bindgen]` `RtEngine`
alongside the frozen 3.1 `BenchEngine` — owns the pinned canonical patch + `Capture`; `render_quantum()`
renders one block (1024 analog → 128 host) zero-alloc; `out_ptr()`/`out_len()` expose the host block for a
single `Float32Array` view over wasm memory (zero-copy — **no `unsafe` needed**: `as_ptr()` is safe, the
view is built JS-side); a repeating-note demo (`render_quantum`-driven) proves the event lane advances.
Native rlib tests guard the surface (non-silence, geometry, note cadence). **Two deviations from plan:** no
`#[allow(unsafe_code)]` (returning a pointer is safe Rust); and the wasm crosses to the worklet as **raw
bytes via `processorOptions`**, not a `postMessage`'d `WebAssembly.Module` — a Module can't be
structured-cloned into the AudioWorklet realm (it was silently dropped); bytes always clone and the worklet
compiles them synchronously in its constructor (the WebKit/Emscripten-recommended approach). **Phase A
(`crates/wasm-bindings/rt/`):** a throwaway no-bundler static page — `--target no-modules` glue +
`TextDecoder`/`TextEncoder` polyfill (the worklet scope lacks them and the glue builds one eagerly) +
processor, concatenated by `build.sh` into one classic script (`addModule` can't import). **Phase B
(`web/`):** the durable Vite + TS harness Epic 4 inherits — `main.ts`, the worklet served as a static
asset, `build-wasm.sh`, `.node-version` (Node 24 LTS), and **Biome** (lint+format, `biome.json` + committed
`.vscode/` config). Both pages play identically.

---

## Story 3.3 — Live control & playing — ✅ **Done**
*Goal:* turn knobs and play the instrument live — **control params** (sliders → latest-wins target →
`ParamQueue`) and **events** (computer keyboard + Web MIDI → `EventQueue`), both over `port.postMessage`,
drained at the top of `process()`. The patch already has what's needed: `SynthVoice` declares 5 smoothed
params (`LEVEL`, `ATTACK_MS`, `DECAY_MS`, `SUSTAIN`, `RELEASE_MS`) and the note event input RtEngine
already uses — so 3.3 is wiring, not new engine nodes.

*Watch out:* the **event-clock trap** — Web MIDI / DOM events fire on the main thread in
`AudioContext.currentTime` (seconds) units, but the engine timestamps events in its own **analog-rate**
sample clock (`sample_pos`, advances by `block_len`). Push **raw target values**, not `AudioParam`
automation (the engine's `Smoother` owns de-zippering). `postMessage` deserialization allocates on the
audio thread — fine at human rates, watch it under a flood (the 3.4 SAB ring is the fix).

*Design notes (settled at planning):*
- **Decision — named demo setters, not a generic param registry.** `RtEngine` resolves the handles it
  needs at construction (`schedule.param(voice, SynthVoice::LEVEL)` etc., and the existing
  `event_input`) and exposes **specific** methods: `set_level(v)` / `set_attack_ms(v)` / … and
  `note_on(note, vel)` / `note_off(note)`. JS calls them by name. The generic, UI-enumerable param API
  (expose `ParamDecl`s + `set_param(id, value)`) is deferred to **Epic 4 / Story 4.1** — this page is
  still throwaway.
- **`render_quantum` switches to `process_io`.** `RtEngine` owns a `ParamQueue` + `EventQueue`; the
  setters push into them (latest-wins / timestamped); `render_quantum` calls
  `process_io(out, &mut params, &mut events)` so both lanes drain each block. The internal repeating-note
  demo is **removed** — notes now come from input.
- **Runtime control re-uses `port.postMessage`.** `processorOptions` delivered the wasm at construction
  (3.2); live control is a *runtime* channel, so the worklet regains a `port.onmessage` that maps
  `{type:"param"|"noteOn"|"noteOff", …}` messages onto the `RtEngine` setters. (Still postMessage; the
  lock-free SAB ring is 3.4.)
- **Event-clock — stamp at the current quantum, don't map host time.** For *live playing*, when a
  note message reaches the worklet, stamp it at the block about to render (`blocks · BLOCK_LEN`, the
  same timeline 3.2's demo used) — "play at the next quantum," ~2.7 ms granularity, imperceptible for
  human input and zero host-time math. Precise `currentTime`→sample mapping only matters for *sequenced*
  MIDI and is deferred (note it as a known simplification, not a bug).
- **Keyboard mapping:** a small QWERTY→MIDI-note map (one octave + shift) in `main.ts` → `note_on`/
  `note_off` with key-repeat suppressed. **Web MIDI** reuses the identical message path (a `MIDIMessage`
  → the same `noteOn`/`noteOff`), so it's a thin add, not a separate lane.
- **Testing posture (unchanged):** `RtEngine`'s new setters get native rlib tests (a param target moves
  the smoother / a `note_on` produces output); keyboard/MIDI glue is verified **manually** in the browser.

- **Task 3.3.1** — `RtEngine` control surface: hold a `ParamQueue` + `EventQueue`; resolve param/event
  handles at construction; add named setters (`set_level` / `set_attack_ms` / … / `note_on` / `note_off`)
  that push into the queues; switch `render_quantum` to `process_io`; remove the internal repeating note.
  Native tests: a setter moves the param; `note_on` then render → non-silence.
- **Task 3.3.2** — Worklet runtime channel: `port.onmessage` maps param/note messages onto the setters,
  stamping notes at the current block. (Both `rt/` and `web/` — or `web/` only if `rt/` is being retired.)
- **Task 3.3.3** — `web/` controls UI: a couple of sliders (e.g. level + attack) wired to `param`
  messages, and a QWERTY keyboard handler → `noteOn`/`noteOff` (key-repeat suppressed).
- **Task 3.3.4** — Web MIDI: request access, route note-on/off (and note-off-as-velocity-0) through the
  same message path; pick a device. Thin add over 3.3.3.

*Validate (✅ met):* a slider audibly and smoothly changes a param (no zipper); playing keys and a MIDI
source sound notes at the right pitch, glitch-free and responsive at low latency. Hot path stays
zero-alloc on the Rust side; the Rust gate + `web/` `biome check`/`typecheck` stayed green.

*Delivered:* live control & playing — the patch is now **played and tweaked from the page**.
**Engine (`wasm-bindings`, Task 3.3.1):** `RtEngine` resolves its five smoothed-knob handles + the
voice event input at construction and exposes named setters — `set_level` / `set_attack_ms` /
`set_decay_ms` / `set_sustain` / `set_release_ms` push **latest-wins targets** into an owned
`ParamQueue` (the engine's `Smoother` de-zippers — *not* `AudioParam`), and `note_on(note, vel)` /
`note_off(note)` push events into the `EventQueue` **stamped at the block about to render**
(`blocks · BLOCK_LEN` — "play at the next quantum," ~2.7 ms granularity, zero host-time math).
`render_quantum` switched to `process_io` so both lanes drain each block; the internal repeating-note
demo was **removed** (the engine starts silent). Native rlib tests cover silence-until-`note_on` and
`set_level` scaling output. **Web (`web/`, Tasks 3.3.2–3.3.4):** the worklet regained a
`port.onmessage` mapping `{type:"param"|"noteOn"|"noteOff", …}` onto the setters (off the hot path —
enqueue only, applied by the next `process_io` drain); `main.ts` wires two sliders (Level, Attack)
with live readouts, a QWERTY→note keyboard (one octave from C4 + Z/X octave shift, key-repeat
suppressed via a held-note `Set`), and **Web MIDI** routing every input's note-on/off (velocity-0 =
note-off) through the *same* `send` path, re-attaching on device hot-plug. **Deviations from plan:**
3.3.3 + 3.3.4 landed together in one `main.ts` (they share the send path); and the throwaway `rt/`
static page was **left frozen as the 3.2 Phase-A artifact** — all live-control work went into the
durable `web/` harness (the plan's "`web/` only if `rt/` is being retired" branch). **Note — known
simplification (not a bug):** releasing a key *after* an octave shift can leave a note hanging (release
keys before shifting); a held-key→note map is Epic 4 polish.

---

## Story 3.4 — Glitch-free & low-latency hardening *(the epic exit)* — ✅ **Done**
*Goal:* make it robust and *measured* — a panic/denormal audit of the live hot path, a durable
**real-time-health instrument** (compute-budget-overrun + queue-overflow counters), and **latency
measurement + cushion tuning** against the ~5–12 ms target. The headline item of the old sketch — the
lock-free SAB event ring — is **deferred** (see design notes); 3.4 is the hardening + instrumentation that
maps to the actual exit criteria (audible, stable under normal use, low latency).

*Watch out:* the hot-path contracts (zero-alloc, lock-free, panic-free, denormal flush) are non-negotiable
under real-time — a panic or stall on the audio thread kills the stream. Wall-clock timing must stay
**worklet-side**: the engine is deterministic and clock-free (no ambient `Instant`/`SystemTime`, CLAUDE.md
§6), so budget timing lives in JS, not Rust. Latency must be **measured**, not assumed (`baseLatency` /
`outputLatency` + engine FIR group delay + cushion). **Mono only** (epic-wide).

*Design notes (settled at planning):*
- **The SAB event ring is deferred, not built here — and decoupled from the sequencer goal.** 3.3 delivered
  live playing over `postMessage`, verified clean at human input rates. The ring's payoffs are (a) no
  audio-thread allocation and (b) *sample-accurate* timing — but **both are independent of Epic 3's exit**.
  Sample-accuracy is a property of the *message*, not the transport: the engine's
  `EventQueue::push(when, …)` + `drain_due` are already sample-accurate; only `RtEngine::note_on`'s
  "next-quantum" stamp rounds. And a **sequencer schedules ahead of time**, where latency is irrelevant — the
  standard Web-Audio look-ahead pattern (push future events with a precise `when`) works fine over
  `postMessage`. So the eventual sequencer is unlocked by carrying `when` + a shared clock, *not* by the ring.
  **Decision:** defer the ring until live performance actually misbehaves (or scale's higher event rate
  demands it — Epic 5). **Cheap to retrofit:** events funnel through the single `EventQueue::push` seam (built
  to have its backing swapped); the low-cost path is a **plain `SharedArrayBuffer` ring drained in the worklet
  into the same setters — engine untouched, no `unsafe`, no shared-wasm-memory build** (the heavier "shared
  wasm memory + Rust-atomics reader" variant is only if the plain ring proves insufficient). **Consequence:**
  COOP/COEP defers *with* the ring — 3.4 does **not** touch Vite headers — and Epic 3 exits with the
  *"lock-free cross-thread validation"* item still open, intentionally.
- **"Underrun" reframed for the in-worklet model.** 3.1 resolved to a single-threaded engine *inside* the
  worklet — there is no render-ahead ring to under/overflow. The honest failure mode is a *quantum whose
  compute exceeds its ~2.67 ms budget* (128 frames @ 48 kHz). So the instrument is a **compute-budget-overrun
  counter**, timed with `performance.now()` **in the worklet** (wall-clock health must live JS-side; the
  engine stays clock-free), paired with engine-side **queue-overflow counts** (`EventQueue`/`ParamQueue`
  `push` already return `false` on overflow) surfaced via `RtEngine`.
- **The instruments are durable and scale-facing, not throwaway.** The 3.1 scaling probe found one core
  crosses real-time at **~64–68 heavy channels / ~260 nodes** — the stadium scale this project aims for
  (dozens of analog + a couple hundred digital channels, PROJECT_PLAN §9) is exactly where overruns become a
  *live* risk, mitigated by multi-core DAG partition + a lower oversample factor (**Epic 5**, flagged not
  built). So the budget-overrun counter + latency methodology are built as the **permanent real-time-health
  instrument Epic 5 leans on**, even though 3.4 can only exercise them on the **mono** path. **Boundary:**
  3.4 hardens + instruments mono; real-time *at scale* is bounded by the 3.1 probe and re-confirmed in Epic 5
  with the real multichannel engine.
- **Hot-swap deferred to Epic 4.3.** `ScheduleSlot` already exists with a native smoke test
  (`install_swaps_the_active_schedule`); the single-threaded in-worklet model has **no cross-thread swap path**
  to exercise, and graph edits get their first real trigger with patch cables in 4.3. The deferred swap item
  moves there; the native smoke test stays the standing guard. *(Resolves the old open question.)*
- **The audit is verify-plus-targeted-tests, not a rewrite.** `flush_denormal` is already applied in every
  filter (`OnePole`, `Biquad`, envelope, `DcBlocker`) and there is no `unsafe` in the `process` path
  (structural disjoint borrows). The audit confirms coverage and adds standing tests, changing code only
  where a real gap surfaces.

- **Task 3.4.1** — Panic/denormal audit of the live hot path. Review `Schedule::process_io` + every
  `Node::process` for panic-capable ops (indexing, `unwrap`/`expect`, slice bounds) and denormal-accumulation
  points; confirm `flush_denormal` coverage. Add native tests: a silent decay tail flushes to zero (no
  denormal creep); sustained note rendering stays finite (no NaN/inf) over many blocks. Code changes only
  where a gap is found. *Done:* audit notes recorded, new tests green, full gate green.
- **Task 3.4.2** — Real-time-health instrument. Worklet times `render_quantum()` with `performance.now()`
  against the quantum budget and counts overruns; `RtEngine` exposes queue-overflow counts (from `push`
  returning `false`). Both surfaced to the page status via `port` messages, off the hot path. *Done:* the
  counter fires when forced (an artificial spin proves it), reads zero under normal mono playing; native test
  for the queue-overflow counters.
- **Task 3.4.3** — Latency measurement + cushion tuning. Add a group-delay/latency accessor to the
  converter/`capture` FIRs; set `AudioContext({ latencyHint })`; compute + surface round-trip latency =
  `baseLatency` + `outputLatency` + engine FIR group delay + cushion; tune `latencyHint` against ~5–12 ms;
  document the achieved figure and the latency/robustness tradeoff. *Done:* the page shows a measured
  round-trip figure within target (or the gap documented with the tradeoff); gate green.

*Validate (✅ met):* the live hot path is audited panic-free with denormals flushed (targeted tests green);
the real-time-health instrument (compute-budget-overrun counter + queue-drop counts) runs in the live
harness, fires when forced, and stays clean under sustained mono playing; round-trip latency is reported
(`baseLatency` + `outputLatency` + 0.625 ms FIR group delay) within the ~5–12 ms target. Rust pre-push gate +
`web/` `biome check`/`typecheck` green; **verified in-browser** (glitch-free playing, clean health line).
**Epic 3 exit met (mono);** real-time *at scale* is bounded by the 3.1 probe (~64–68 ch/core) and
re-confirmed in Epic 5.

*Delivered (✅ verified in-browser):* the hardening + instrumentation landed across the engine and the
`web/` harness. **3.4.1 — panic/denormal audit:** the denormal side was already complete (every IIR —
`OnePole`, `Biquad`, `PeakEnvelope` — flushes; the linear ADSR + exact-snap `Smoother` + FIRs reach exact
zero), so no change there. The audit's one real finding was panic-shaped: `Schedule::process_io` indexed the
param store and input pool by **host-supplied** handles (`handle.0`, `e.target.0`) — a stale/foreign handle
would panic on the audio thread. Hardened both to `.get`/`.get_mut` (the event path matches
`Some(Lane::Events(_))`, so an in-range wrong-variant id also skips the `events_mut` `unreachable!`) —
totality over the cross-thread seam. Pinned by 6 standing guards in `schedule::hot_path_robustness` (idle
voice exactly silent; released note → exact zero; sustained chain stays finite; foreign handle/id skipped,
not panicked). **3.4.2 — real-time-health instrument:** `RtEngine` counts `event_drops` / `param_drops`
(queue `push` returning `false`; setters route through a private `set_param`), exposed as getters; the
worklet times `render_quantum()` with `performance.now()` against the ~2.67 ms quantum budget (the
"underrun" of the single-threaded model), counting **overruns** + worst render time, and posts a throttled
(~4 Hz) health snapshot incl. the engine drops; the page renders a `#health` line. Native flood test confirms
the drop counter. **3.4.3 — latency measurement:** added linear-phase **group-delay** accessors
(`Decimator`/`Interpolator::group_delay` = `(taps−1)/2`), a defaulted `Node::group_delay_samples` (0)
overridden by AD/DA, `Schedule::group_delay_samples` (chain sum), and `Capture::group_delay_samples`;
`RtEngine::signal_path_latency_ms` composes them = **0.625 ms** (three matched 161-tap FIRs @ 384 kHz → 240
samples) — hand-calc tested. The worklet sends it in `ready`; the page shows `base + output + engine ≈ T ms
(+ up to ~2.7 ms note quantum)`. `latencyHint` kept **"interactive"** (smallest cushion = lowest latency;
the 3.1 ~46× headroom makes the bigger-buffer robustness unnecessary) with the tradeoff documented in
`main.ts`. **Deviations from plan:** the counter is named "drops" (clearer than "overflow"); the hot-swap
and SAB ring / COOP/COEP stayed deferred (no code, as decided); group delay got a new concept-doc entry
(`osku_physics_concepts.md` §16). **Gates:** Rust `fmt`/`lint`/`test` (engine 261, wasm-bindings 8)/`wasm`/
`docs` green; `web/` `biome` (0 warnings) + `typecheck` green; wasm rebuilt and the new getters confirmed in
the glue. **Verified in-browser (the Validate gate, by hand):** glitch-free sustained playing, the health
line holding clean, and the live latency figure (`base + output + 0.625 ms engine`) within the ~5–12 ms
target. **Epic 3 — the north star — is reached.**
