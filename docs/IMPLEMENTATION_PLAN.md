# Audio Engineer Simulator — Implementation Plan

Companion to `PROJECT_PLAN.md`. The project plan is the *what and why*; this is the
*in what order, and at what granularity*. It is a living document — we elaborate the
near work in detail and keep the far work deliberately coarse, refining it as we arrive.

## How this plan is structured

Three levels, mirroring Epic → Story → Task:

- **Epic** — a roadmap stage from `PROJECT_PLAN.md` §9. The high-level arc:
  *engine → offline audio → real-time audio → UI → breadth.* Each delivers something
  usable and retires the riskiest remaining unknown.
- **Story** — a coherent slice within an Epic, with its own goal and watch-outs.
  Roughly a week-ish of focused work; the unit at which we think about design, **and the
  unit of branching**.
- **Task** — small, **1–10 commits**, the unit of execution. Tasks land as commits on the
  Story's branch; the Story merges to `main` when its *Validate* gate is green.

**How we work this plan — overview first, flesh out on arrival.** The whole arc is mapped up
front (every Epic and Story is named, so the shape of the project is visible end to end), but a
Story is only *elaborated to Task level and design notes* when we actually pick it up to build it.
Working a Story is what fleshes it out: its tasks, hand-calc oracles, "Watch out" traps, and
settled design decisions are written as we discover them in the doing. **This is why already-worked
items carry far more detail than future ones** — the density of an entry tracks how close it is to
(or how far past) the moment we built it, not its importance. A sparse future Story isn't
under-specified by neglect; it's deliberately left coarse until its turn, because the earlier work
routinely changes its shape.

**Detail gradient (concretely):** Epic 1 is broken to Task level, and its completed Stories
(1.1–1.6) carry full design notes because they've been built. Epics 2–3 have Tasks but expect
churn. Epics 4–5 stay at Story level — their Tasks get written when we reach them. Don't over-plan
work whose shape the earlier work will change.

**Branch convention:** one branch per **Story**, `e<epic>-s<story>/<short-story-slug>`,
e.g. `e1-s2/electrical-primitives`. Its Tasks are commits on that branch; PR (or
fast-forward) to `main` and delete on merge once the Story's *Validate* gate is green.

### Architecture decisions baked into this plan

These were settled in design discussion and constrain every Epic:

- **Engine in Rust**, native for dev/test, `wasm32` + SIMD for the browser. **UI in TypeScript.**
- **An open set of signal carriers**, never conflated — analog voltage (`VoltageBuffer`), digital
  audio (`SampleBuffer`, **linear** normalized samples + sample rate / bit depth / clock domain; dBFS
  is a measurement, not storage), MIDI/control events, and later networked audio. Nodes present
  **domain-tagged ports**; the only cross-domain elements are converters/bridges (AD/DA, protocol
  receivers); a physical multi-I/O device is a **group of nodes**, not one. Carriers ride one `Lane`
  enum so adding a carrier is additive, and domain-compatibility is validated at `compile`.
- **Clocks are real rates against the analog continuum, not labels.** The analog oversample rate is
  the universal time reference; each digital clock is a frequency (phase accumulator) against it.
  Clock distribution is resolved as a compile-time side-graph (recovered-in-data vs dedicated word
  clock vs PTP; clock source is per-device config); the failure of an unlocked link **emerges** as a
  FIFO slip at runtime, not a flag, and SRC is the honest fix. Physical-layer decode (line coding,
  PLL) is out of scope.
- **Block-based, pull-based core**: `compile(graph) -> Schedule`, then
  `schedule.process(out, &control_queue, &event_queue)` — one code path for offline *and* real-time.
- **Zero-alloc, lock-free, panic-free hot path.** Flush denormals. A panic in a WASM
  AudioWorklet kills the stream — the `process` path must never panic.
- **Two input lanes:** smoothed continuous **control params** (knobs) and sample-accurate
  timestamped **events** (note-on/off, gate).
- **Local solve only** (Thévenin source + voltage divider + cable R·C); no global nodal solve.
  The schedule is a DAG, kept partitionable for possible multi-core later (not needed at stadium scale).
- **Deterministic given a seed** (noise, hum phase) so tests and replays are stable.
- **Real-time interactive is the north star.** Offline render is a test harness, not a destination.

---

## Epic 1 — Headless Voltage Engine — ✅ **Complete**

Stories 1.1–1.7 done; **229 engine tests green**; hot path zero-alloc throughout. A defined patch
runs end-to-end `analog → AD → digital → DA → analog`, with all voltage / conversion / event / param
behavior asserted against hand calcs. The carrier set grew from one buffer type to three — analog
voltage (`VoltageBuffer`), digital audio (`SampleBuffer`), and sparse MIDI/control events
(`Lane::Events`) — plus the smoothed control-param side-channel. **Next: Epic 2 — offline render to
WAV (the audio oracle).**

**Goal (delivered):** the novel, risky core, built and validated headless — a graph of devices and
cables propagating oversampled voltage in the analog domain, crossing the AD/DA boundary into and
back out of digital, with all physical behavior *emerging* from the voltage math and asserted by tests.

> **Full design notes, rejected alternatives, hand-calc oracles, and per-task delivery records for
> every Story below live in [`EPIC_1_NOTES.md`](./EPIC_1_NOTES.md).** This section keeps only the
> decisions and the delivered API surface that constrain later epics — enough to make good follow-up
> decisions without re-deriving Epic 1. Go to the notes when a decision turns on *why* something was
> built the way it was, or you need the exact behavior of a shipped piece.

### What Epic 1 delivered (engine public surface)

The vocabulary later epics build on. Names are the actual public API unless marked `#[cfg(test)]`.

- **Unit newtypes** (`repr(transparent)`, construct-time validation, no implicit numeric conversion):
  `Volts`, `Ohms` (series `Add` + `parallel`), `Farads`, `NoiseDensity` (V/√Hz), `AnalogRate`,
  `SampleRate` (distinct from `AnalogRate` by design), `BitDepth`, `ClockDomainId`.
- **Buffers (linear storage):** `VoltageBuffer` (linear volts @ `AnalogRate`), `SampleBuffer` (linear
  normalized ±1.0 = full scale; carries `SampleRate` / `BitDepth` / `ClockDomainId`). dB/dBu/dBV/dBFS
  are **measurement helpers**, never storage.
- **Conversions / level helpers:** dBu↔V, dBV↔V, volts/samples↔dBFS, `headroom_db`.
- **Determinism:** seeded splittable `Rng` (uniform + Gaussian). `compile(graph, seed)` splits an
  independent child stream into **every node** (`Node::seed`) and **every edge**, in index order, so a
  stream is stable regardless of topology. No `thread_rng` / `Instant` anywhere.
- **Electrical (local solve only):** `Thevenin`/`OutputZ` (source face), `InputZ` (load face),
  `divider_gain` (`Zin/(Zout+Zcable+Zin)`, a compile-time scalar), `fan_out_gains` (parallel branch
  loading), `Cable { r, c }` → `OnePole` (matched-coefficient one-pole LPF, with a per-sample `step`),
  `DcBlocker` (one-pole HPF = dual of `OnePole`).
- **FIR (converter infra):** `Decimator` / `Interpolator` — Kaiser-windowed-sinc, linear-phase,
  **polyphase**, taps designed at `compile`, zero-alloc ring-buffer convolution, `f64` accumulator.
- **The `Node` trait & node set.** Trait surface: `process(&mut self, params: &Params, inputs: &[Lane],
  outputs: &mut [Lane])` (total, zero-alloc, panic-free), `prepare(rate)`, `seed(rng)`, `params()`,
  `per_conductor()` / `replicate()`, and per-port `domain()` / `lane_count()`. Nodes shipped:
  `TestSource`, `GainStage` (rail clip + input-referred noise floor + smoothed gain param), `PassiveSum`,
  `BalancedDriver`, `BalancedReceiver`, `CondenserMic` (+48 V phantom), AD, DA, `SynthVoice` (mono
  sawtooth + ADSR), plus the internal `Lifted` per-conductor lane-wrapper.
- **Carrier seam:** `Lane { Voltage(VoltageBuffer), Sample(SampleBuffer), Events(...) }`, an **open**
  enum; ports are per-direction enums `InputPort` / `OutputPort` over `{ Analog(InputZ/OutputZ),
  Digital(DigitalFace), Events(EventFace) }`. Hot-path accessors `lane.voltage()` / `.sample()` whose
  wrong arm is `unreachable!` (safe because `compile` validated every edge's domain).
- **Graph / compile / schedule:** `Graph` (`NodeId`, typed edges, construct-in-code), Kahn topo sort
  (cycle-rejecting), `compile(graph, seed) -> Schedule` with `CompileError` (dangling / duplicate /
  cycle / `ConductorMismatch` / `DomainMismatch`); `EdgeKind { Analog(EdgeTransform), DigitalRoute,
  EventRoute }` (analog edge = baked `divider_gain × optional cable one-pole`); two-pool zero-alloc
  `Schedule::process*`; `ScheduleSlot` ownership-handoff swap seam.
- **Balanced lines** as **"buffer = conductor"** (`InputZ`/`OutputZ::balanced`, one flat `f32` buffer
  per conductor); ordinary single-conductor nodes opt into `per_conductor()` and `compile` infers
  conductor multiplicity and replicates them per leg via `Lifted` — so "balanced" is never a flag and
  ideal CMRR emerges from leg symmetry. Interference (`Cable::with_pickup` Gaussian, `Cable::with_hum`
  50/60 Hz) couples on the **edge** as common-mode.
- **Input lanes (two, genuinely separate):** *Events* are a **routed carrier** — `Lane::Events`
  (bounded, drop-on-overflow), `EventMessage` (note-on/off, gate), external `EventQueue` (SPSC seam,
  absolute-sample timestamps, block-bucketed). *Control params* are a **host→node side-channel** —
  `ParamDecl` / `Node::params()`, latest-wins `ParamQueue`, framework-owned `Smoother` store with
  within-block linear-ramp de-zipper, exposed via `Params` (`Params::EMPTY` default). Driven through
  `Schedule::process_io` / `process_with_params` / `process_with_events`.

### Decisions that bind every later epic

- **Hot-path discipline (`process`): zero-alloc, lock-free-shaped, panic-free, denormal-flushed.** All
  validation, allocation, and error reporting live in graph construction and `compile`; `process` is
  total. A `no_alloc` counting-allocator test guards this and must stay green.
- **`f32` storage, `f64` accumulation** (summing, filter state, FIR/AA accumulators).
- **Two signal types never conflated; converters are the only domain bridge.** Every **edge connects
  same-domain ports** (`DomainMismatch` otherwise); a converter crosses domains *inside its own node*.
  A buffer storing dB/dBFS is a category error. Don't bake a *closed* carrier set — `Lane` is open.
- **Determinism:** same seed ⇒ identical run; recompile/swap with the same seed reproduces.
- **One analog rate** (continuous proxy, a parameter not a constant); **digital rates are per-converter
  and must integer-divide it** (`compile` rejects non-integer `M`). No global oversample factor.
- **Local solve only** (Thévenin + divider + cable R·C); the schedule is a partitionable DAG.
- **Params vs. structure:** a value-only param change is read in `process` (no recompile); only
  structural change (add/remove node, repatch, reroute topology) triggers recompile + atomic swap.
- **Smoothing / de-zipper is written once in the framework**, never per node (the "balanced as a label"
  anti-pattern). Same principle: balanced is composition, not a node variant.

### Deferred — decided, not gaps (earliest epic that needs each)

- **Reactive source impedance / inductive-pickup resonance peak** (2nd-order resonant LPF): the cable
  is one-pole only today. → first reactive *device*, **Epic 5**.
- **Finite CMRR** (leg-impedance imbalance → CM-to-differential conversion): ideal/infinite rejection
  only today. → deferred, **possibly never** (only if a scenario needs a finite figure).
- **Multi-stage nodes & "one chassis → many nodes" grouping machinery** (inserts, routable interface):
  single-stage nodes today; retrofit is additive. → first insert / routable interface, **Epic 4+**.
- **Multichannel digital ports** (ADAT 8-lane etc.): every 1.6 digital port is `lane_count() == 1`. → **Epic 5**.
- **Clock domains, async-boundary FIFO slip, word-clock master/slave, fractional SRC:** one clock domain
  today; `ClockDomainId` is stamped and ready to grow. → **Epic 5 (5.3)**.
- **Ground-topology-emergent hum:** `Cable::with_hum` is a manual phenomenological stand-in; appearance
  should emerge from a grounding side-graph cycle pass. → **Epic 5 (5.4)**.
- **Polyphony / voice allocation:** the voice is monophonic, last-note priority. → past Epic 1.
- **MIDI CC (events→param):** would blur the two-lane separation; note events only today. → deferred.
- **Lock-free cross-thread validation** of the param/event queues and schedule swap: SPSC-shaped but
  exercised single-threaded today. → **Epic 3**: the param/event *drain* runs on the real audio thread from
  3.2 (over `postMessage`), and the genuinely lock-free SAB transport lands in **3.4**.
- **Phantom supply path / current-draw sag:** the condenser source just emits +48 V common-mode when
  powered. → informed approximation, deferred.

### Story-by-story (status + the one thing each settled)

- **1.1 — Scaffold & core numeric types** ✅ — workspace, CI (incl. wasm32 check), the analog type
  vocabulary (`Volts`, `VoltageBuffer`, `AnalogRate`), dBu/dBV↔V, seeded splittable `Rng`. Settled the
  one-analog-rate + `f32`/`f64` + linear-storage spine.
- **1.2 — Electrical primitives & local solve** ✅ — `Ohms`/`Farads`, Thévenin/`InputZ`, `divider_gain`,
  `Cable`/`OnePole`. Settled: divider (resistive gain) and cable LPF compose exactly; edge-shaping seam
  kept open for a future reactive source.
- **1.3 — Minimal runnable engine** ✅ *(first end-to-end milestone)* — `Node` trait, `Graph`, topo sort,
  `compile -> Schedule`, zero-alloc `process`, swap seam. Settled Node-vs-device naming, the stage model,
  and params-vs-recompile. **The engine became runnable here.**
- **1.4 — Analog-chain physics** ✅ — device noise as spectral density (V/√Hz), per-node seeding, SNR in
  quadrature, `DcBlocker`, rail clipping & headroom. "Tests are the oracle" cases proven on real chains.
- **1.5 — Balanced lines, pickup & common-mode** ✅ — two-conductor balanced lines, the per-conductor
  **lift**, edge-coupled pickup/hum, phantom. Ideal CMRR emerges from leg symmetry (finite CMRR deferred).
- **1.6 — AD/DA converters & the carrier seam** ✅ *(second carrier)* — the `Lane` enum, `SampleBuffer`,
  domain-tagged ports, polyphase FIR converters, per-converter dBFS calibration, TPDF-dither quantization.
  Generalized one buffer type → an **open carrier set**; laid the MIDI / networked-audio seam.
- **1.7 — Input lanes & a playable voice** ✅ *(third carrier)* — `Lane::Events` + `EventQueue`, the
  control-param system (`ParamDecl` / `Smoother` / `Params`), and `SynthVoice`. Kept events (routed
  carrier) and control params (side-channel) genuinely separate. **Epic 1 exit met.**

---

## Epic 2 — Offline Render ("hear it" cheaply) — ✅ **Substantially complete** (2.3 deferred)

Stories 2.1 ✅ and 2.2 ✅ done; **2.3 deferred**. The *same* engine, driven block by block via
`Schedule::process_io`, renders to a float32 WAV you can listen to: a played note runs
`synth → AD → (DSP) → DA → speaker`, the speaker voltage is captured **off-sim-clock** to 48 kHz and
written to disk. First real DSP landed — a `Biquad` primitive + RBJ designers, a `ThreeBandEq` and a
feed-forward `Compressor`, all pure-digital, sitting between the modeled AD and DA. Behavior is pinned by
**numeric oracles** (engine unit tests + harness integration tests in `tests/render.rs`) and validated
**by ear** via the render scenarios. **254 engine tests + 5 render integration tests green. Next: Epic 3
— real-time playback in the browser.**

**Goal (delivered):** reach the audio oracle without real-time infrastructure — the same engine rendered
flat-out into a WAV, with the first real DSP and a trivial speaker terminus so there's something
meaningful to hear. The render driver is a thin loop over `process_io` plus a file writer — a **test
harness, not a second engine**.

> **Full design notes, rejected alternatives, per-task delivery records, and the settled design for the
> deferred Story 2.3 live in [`EPIC_2_NOTES.md`](./EPIC_2_NOTES.md).** This section keeps only the
> decisions and the delivered surface that constrain later epics — enough to make good follow-up
> decisions without re-deriving Epic 2.

### What Epic 2 delivered (engine + harness surface)

- **Engine — `Speaker` terminus:** a flat voltage→voltage node (sensitivity gain, bridging `InputZ`,
  nominal terminus `OutputZ`, no rail). The graph's analog terminus; the output tap stays a **voltage** tap.
- **Engine — new `dsp` module** (peer to `electrical` / `fir`): `Biquad` (Transposed Direct Form II,
  `f64` coeffs + state, zero-alloc denormal-flushed) with RBJ designers `peaking` / `low_shelf` /
  `high_shelf`; `PeakEnvelope` (rectify → switched attack/release one-pole, `a = 1 − e^(−1/(τ·fs))`);
  `flush_denormal` promoted here and shared by analog + digital filters. Layout `dsp.rs + dsp/biquad.rs +
  dsp/envelope.rs`.
- **Engine — DSP nodes** (pure-digital, one channel in/out, on `SampleBuffer`): `ThreeBandEq` (LF shelf +
  mid peak + HF shelf, three biquads in series; **static** config baked at `prepare` from its own
  `SampleRate`) and `Compressor` (feed-forward, no lookahead; `PeakEnvelope` → dB gain computer with
  threshold / ratio / soft knee → manual makeup; builders `with_knee` / `with_makeup`). **No graph/schedule
  changes** — the Story 1.6 digital ports/edges carried them.
- **Harness** (native-only, restructured **lib + bin**): `capture::Capture` (stateful harness-held
  `Decimator` at `M = analog/host` + fixed monitor-reference volts→±1.0 + clamp); `wav` (mono **float32**
  via `hound`, file + in-memory round-trip); `render::render_to_samples` (loops `process_with_events`).
  `hound` + `approx` are harness-only deps — they never reach the engine or its wasm32 build.

### Decisions that bind every later epic

- **The simulation ends in the analog domain at the speaker feed (volts); we do not simulate acoustics**
  (no air→ear). The graph terminates at the thin `Speaker` node; the engine **output tap is a voltage
  tap** — there is no Sample-lane output tap.
- **The host render is an *implicit capture*, outside the simulation** — harness plumbing that taps the
  speaker voltage and resamples to host rate. It carries **no `ClockDomainId`**, rides **no
  modeled-converter clock/rate**, and has **no dBFS role**. It **reuses the FIR `Decimator`** so it is
  transparent and adds no artifacts of its own — aliasing/quantization must come only from the *modeled*
  AD/DA under test. Volts→full-scale via a **fixed monitor reference** (no per-render auto-normalize —
  that would break determinism and cross-render comparison). Epic 2 host rate **integer-divides** the
  analog rate; arbitrary/fractional host rates are deferred.
- **First DSP lives in the digital domain** — biquad EQ and compressor operate on `SampleBuffer`, between
  the modeled AD and DA (the "plugins/DAW" position). Avoids the ~8× oversample cost and exercises the
  digital lane; analog-domain (voltage) outboard DSP is a later option.
- **DSP config is static this epic — no smoothed control params on it.** Safely smoothing biquad
  coefficients is a real problem and live knob-turning belongs to Epic 3 (real-time). A pure-digital filter
  designs coeffs from its **own `SampleRate`** (stored at construction), not `prepare`'s `AnalogRate` arg.
- **Mono only** (epic-wide; converters/lanes are mono, multichannel digital is Epic 5).
- **Golden-file comparison, when built, is feature-vector / tolerance based, not bit-exact** — coeff-design
  `sin`/`exp` + FMA contraction aren't bit-portable native↔wasm↔arch, so physically meaningful measured
  metrics are the portable, refactor-robust guard.

### Deferred — decided, not gaps

- **Story 2.3 — golden-file regression harness + converter-payoff demos (aliasing, quantization).** The
  standing numeric oracles + render scenarios already pin behavior against hand calcs; a *regression* layer
  only earns its keep once we're fighting drift/quality regressions. The payoff-demo knobs already exist
  (`AdConverter::with_aa_taps`, `BitDepth`) and the naive-sawtooth voice has the HF content aliasing needs,
  so resuming is cheap. The settled design (feature-vector JSON goldens, `--bless` over a shared
  `harness::golden` lib, six locked renders, a promoted spectral helper) is recorded in `EPIC_2_NOTES.md`.

### Story-by-story (status + the one thing each settled)

- **2.1 — Offline render to WAV + speaker terminus** ✅ *(first sound)* — render driver loops `process_io`
  into a WAV writer; thin `Speaker` terminus; harness-side implicit capture. Settled: capture is a
  **stateful harness-held `Decimator`** (not a second engine, off-sim-clock, no `ClockDomainId`), canonical
  format is **float32 WAV** (PCM16 would contaminate 2.3's quantization measurement). **First sound.**
- **2.2 — First DSP: 3-band EQ + compressor (digital)** ✅ — `Biquad` + RBJ designers + `PeakEnvelope` in a
  new `dsp` module; `ThreeBandEq` and `Compressor` between AD and DA. Settled: pure-digital nodes need
  **no graph/schedule work** (1.6 ports carried them); **static** config (coeff smoothing → Epic 3).
- **2.3 — Golden-file harness + converter-payoff demos** ⏸️ **Deferred (2026-06-23)** — see *Deferred*
  above; design settled in `EPIC_2_NOTES.md` should it resume.

---

## Epic 3 — Real-Time Playback (the north star)

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
- **Bindings — hybrid.** `wasm-bindgen` for the cold/setup surface (construct/configure the engine,
  generated TS types for Epic 4 to consume) — the well-beaten path — but the **per-quantum hot path
  reads/writes raw shared linear memory directly** (a `Float32Array` view over WASM memory), bypassing
  marshalling for zero-copy `process()`. Accepts the `wasm-bindgen-cli`/`wasm-pack` tooling + version
  pinning. The raw hot path needs a localized `#[allow(unsafe_code)]` (the per-module opt-in the workspace
  lint policy already anticipates).
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
- **The real-time correctness oracle is a native↔WASM *parity* test** (same patch, same seed → engine
  output matches within tolerance), plus runtime health metrics (underrun counter, measured latency). You
  cannot golden-file a live session, and `cargo wasm` won't catch numeric drift or a broken artifact, so
  parity is the standing guard for the WASM build.
- **SIMD is measure-driven, not upfront.** Rely on LLVM autovectorization with `+simd128` first; reach for
  explicit intrinsics (more `unsafe`) only on hot loops the 3.1 spike proves are over budget.

> *Tasks below are a coarse sketch, fleshed out to Task level when each Story is picked up — per the
> detail-gradient convention (Epics 2–3 carry Tasks but expect churn). Goals, watch-outs, and the settled
> decisions are recorded now.*

### Story 3.1 — WASM engine + real-time feasibility spike
*Goal:* the first real **WASM artifact of the engine** plus the **faster-than-real-time benchmark** that
gates the whole epic — proof the oversampled chain renders the canonical patch with enough headroom for the
in-worklet model. Stands up the hybrid-bindings scaffolding (`wasm-bindgen` cold surface + a raw-memory
`process`) and relocates the **implicit capture** (`Decimator` + monitor-reference scale/clamp) out of the
native harness into a WASM-reachable layer (the capture is portable engine code today, but it lives in
`crates/harness` beside `hound`/`textplots`, which never reach wasm32).
*Watch out:* this is the first time we build and *instantiate* WASM, not just `cargo check` it — expect
toolchain friction. Don't optimize before measuring; SIMD/hot-loop work is justified only if the spike is
marginal. Keep the `unsafe` of the raw hot path localized and commented.
- WASM build pipeline (`wasm-pack`/`wasm-bindgen`, `+simd128`, release `panic=abort`); real `wasm-bindings` surface.
- Hybrid surface: `wasm-bindgen` construct/configure; raw `process(out_ptr, n)` over linear memory.
- Relocate the capture to a portable (wasm-reachable) spot; the WAV/textplots bits stay harness-native.
- Faster-than-real-time benchmark of the canonical patch in WASM; record the measured headroom.
- Native↔WASM **parity test** (same patch/seed, output within tolerance) — wired into CI alongside a real wasm build.

*Validate:* WASM builds and instantiates; the canonical patch renders comfortably faster than real-time in
WASM (headroom recorded, execution model confirmed or the fallback triggered); the parity test is green;
the full Rust gate stays green.

### Story 3.2 — First real-time sound *(the live milestone)*
*Goal:* the Epic-3 analogue of Story 1.3's "first runnable" and 2.1's "first sound" — a fixed patch
(`synth note → AD → (DSP) → DA → speaker → capture`) **audible in the browser in real time**, clean at
idle. Stands up the minimal Vite + TS harness, hosts the engine in an `AudioWorkletProcessor`, and wires the
host-rate output into the worklet's output buffer.
*Watch out:* the **bundler↔worklet** seam — the processor must load as a real URL via `addModule`
(`new URL('./processor.ts', import.meta.url)` or a separate entry), not get inlined. The
**wasm-bindgen-in-worklet** seam — no reliable `fetch` in the worklet scope, so compile the module bytes on
the main thread and `postMessage` the `WebAssembly.Module` into the worklet to instantiate there. Set the
browser cushion via `latencyHint` (~8 ms). `block_len` is the 128-frame quantum × M analog samples — honor
the multiple-of-M constraint. Duplicate mono → output channels.
- Vite + TS scaffold under `web/`; dev server; load the WASM + worklet.
- `AudioWorkletProcessor` hosting the engine instance (main-thread compile → postMessage module → instantiate).
- Per-quantum `process()`: run the engine, capture to host rate, write the output buffer.
- Fixed first-sound demo patch + an idle-stability check (no dropouts at no load).

*Validate:* a fixed patch plays continuously and recognizably in the browser, clean at idle; the live output
matches the offline render of the same patch (parity holds across the worklet boundary).

### Story 3.3 — Live control & playing
*Goal:* turn knobs and play the instrument live — **control params** (sliders → latest-wins target →
`ParamQueue`) and **events** (computer keyboard + Web MIDI → `EventQueue`), both over `postMessage`, applied
at the top of `process()`.
*Watch out:* the **event-clock trap** — Web MIDI and DOM events fire on the main thread in host-frame /
`AudioContext.currentTime` units, but the engine timestamps events in its own **analog-rate** sample clock
(`sample_pos` advances by `block_len`); map host time → analog samples (× M) and schedule slightly ahead to
absorb postMessage delivery jitter, or notes land at the wrong instant. Push **raw target values**, not
`AudioParam` automation (the engine's `Smoother` owns de-zippering). postMessage deserialization allocates
on the audio thread — fine at human rates, but watch it under a flood (the 3.4 SAB ring is the fix).
- Param transport: main→worklet slider values → `ParamQueue` (a couple of mapped knobs on the demo patch).
- Event transport: computer-keyboard keys → `EventQueue`, with the host-time→analog-sample mapping.
- Web MIDI input → the same event path (playing from a controller).

*Validate:* a slider audibly and smoothly changes a param (no zipper); playing keys / MIDI sounds notes at
the right pitch and timing, glitch-free and responsive at low latency.

### Story 3.4 — Glitch-free & low-latency hardening *(the epic exit)*
*Goal:* make it robust and *measured* — a panic/denormal audit of the hot path under real-time, the
**lock-free SAB event ring** (which truly retires the deferred *"lock-free cross-thread validation"* item),
underrun-free operation under load, and **latency measurement + tuning** (cushion depth vs. responsiveness
against the ~5–12 ms target).
*Watch out:* the SAB ring is what **forces COOP/COEP** (cross-origin isolation) — add the headers in Vite's
`server.headers` here, not earlier. Measure real latency (`baseLatency`/`outputLatency` + engine FIR group
delay + cushion), don't assume it. The ring upgrades the event lane *behind* `EventQueue::push`, so nothing
above it moves.
- Panic-free / denormal audit of the live hot path; stress under sustained playing + load with an underrun counter.
- SAB event ring (JS writer + Rust atomics reader) behind `EventQueue::push`; COOP/COEP via Vite.
- Latency measurement + cushion tuning; document the achieved figure and the latency/robustness tradeoff.

*Open question (resolve at story pickup):* **schedule hot-swap under load** has no real trigger in Epic 3 —
there is no UI to edit the graph until Epic 4 (graph mutation → recompile + atomic swap is a 4.3 concern).
Either keep a *minimal* swap smoke-test here (exercise the `ScheduleSlot` handoff on the real audio thread)
or move that deferred item to Epic 4 with the patch-cable work. Leaning: a minimal test here (cheap, proves
the seam cross-thread), the real graph-edit flow in 4.3.

*Validate:* no underruns under sustained playing + load; measured round-trip latency within the ~5–12 ms
target (or the gap documented with the cushion tradeoff); the hot path is audited panic-free; the SAB ring
carries events sample-accurately. **Epic 3 exit met.**

---

## Epic 4 — UI: Skeuomorphic Panels + Patch Cables

**Goal:** the product interface on the proven engine — realistic device panels, drag-to-patch cables,
and product visualization. Pure consumer of the engine API.

**Exit criteria:** build and operate a small studio entirely through the UI and hear/see the results.

**Watch-outs:** the UI must never reach into engine internals — only the published API (params, events,
scene load/save). Graph edits flow through the off-thread schedule recompile + atomic swap.

*Tasks to be elaborated when we reach this Epic.*

- **Story 4.1** — Engine API surface for UI: TS types, scene serialize/load, the param/event bridge.
- **Story 4.2** — Device panel rendering + controls bound to params (realistic layout).
- **Story 4.3** — Patch-cable drag-to-connect; graph mutation → schedule recompile.
- **Story 4.4** — Visualization: meters, scope, spectrum, analog-domain readouts.

---

## Epic 5 — Breadth & Challenges

**Goal:** grow device coverage and the medium (routing, studio wiring, live sound scaling toward large
venues), deepen DSP and AD/DA, and add the game layer.

**Exit criteria:** the same engine credibly supports studio, routing, and live-sound scenarios; structured
challenges layer on top of the sandbox.

**Watch-outs:** multi-core only if profiling at scale demands it (single core covers stadium on the napkin).
Keep device transforms understandable — spend the realism budget on the volts-and-converters layer.

*Tasks to be elaborated when we reach this Epic.*

- **Story 5.1** — More devices: deeper mixer, more processors, patchbay, more converters.
- **Story 5.2** — Routing & live-sound scenarios at scale (multi-core partition of the schedule if
  needed). Includes **networked audio** (Dante/AES67) as a carrier: digital-audio sample streams over
  an IP transport with its own routing, subscriptions, latency, and a network clock (PTP) — modeled
  "TCP/IP layer upwards" (network behavior + encoding), reusing the `Sample` lane and the clock-domain
  machinery, with the transport/subscription model as the net-new piece.
- **Story 5.3** — Deeper DSP and deeper AD/DA modeling as needed. Includes **clock domains, sync, and
  sample-rate conversion** — the payoff of the "crossing any clock = resample" rate model (Story 1.1)
  and the carrier/clock seam (Story 1.6). This is where the **emergent clock model** (the clock-domains
  decision below) is built: per-domain oscillators as real rates against the analog continuum, elastic
  FIFOs at async boundaries that genuinely slip, word-clock master/slave, recovered-vs-dedicated
  clocking, and a **fractional sample-rate converter** as the honest fix (also what lets a 44.1 k
  device meet a 48 k one). "Fix the clocking" (set a master, slave the rest) becomes a diagnostic
  challenge alongside "fix the hum".
- **Story 5.4** — Challenge / diagnostic-scenario framework on the sandbox. Includes the
  **ground-topology-derived hum** decision below ("fix the hum" is a named challenge scenario).
- **Story 5.5** — Optional schematic / node-graph view over the same model.

*Decision — ground-loop hum should become emergent from grounding topology (deferred to this Epic).*
Today (Story 1.5) `Cable::with_hum` is a **manual** injection — the user asserts "a ground loop exists
on this cable." That's a phenomenological stand-in, not the final design. A ground loop is a **loop in
the ground network**: two mains-earthed devices *also* tied together by a cable shield form two ground
paths between them ⇒ circulating 50/60 Hz current ⇒ hum. Break any leg (a floating/battery device, a
**ground lift**, transformer/DI isolation) and the loop — and the hum — is gone, *regardless* of
balanced vs. unbalanced (balanced merely rejects the hum when a loop does exist; it doesn't prevent the
loop). So whether hum *appears* is a property of the patch's grounding, and should **emerge**, not be a
flag:
- Model a small **ground-connectivity** side-graph — devices declare mains-earthing; cables declare
  whether the shield bonds the two grounds and whether it's lifted at an end.
- At **compile**, **detect cycles** in that graph; a cable on a cycle between earthed devices is in a
  ground loop ⇒ inject hum there. A lift / floating device / isolator removes an edge ⇒ no cycle ⇒ no hum.
- This is compile-time **connectivity analysis, not a per-sample electrical loop solve**, so it honors
  the "local solve only / no global nodal solve / signal graph is a DAG" decision (§5.3) — same kind of
  cheap graph pass we already run for signal-DAG cycle detection, just on a separate graph.
- The hum **amplitude stays phenomenological** (the induced voltage from loop area / earth-potential is
  the "EM source" we hold out of scope). Only the *appearance and location* become emergent.
*Prerequisites (none exist yet):* a ground/earth concept on devices, shield modeling on cables, and
ground-lift controls — naturally introduced alongside Story 5.1 (patchbay/wiring) and consumed by the
"fix the hum" diagnostic here. ROI is high then (the heart of the troubleshooting lesson), low now.

*Decision — clock domains and their failures emerge from a clock-distribution side-graph + real
per-domain rates (deferred to this Epic).* Through Story 1.6 there is a single internal clock domain
and no async boundary, so a `SampleBuffer` merely carries its producing oscillator's identity and
rate. The full model lands here, mirroring the ground-loop-hum approach (a cheap compile-time
connectivity pass over a side-graph, plus an emergent runtime consequence — never a flag):
- Devices declare a **clock source** — `Internal(rate)`, `RecoverFrom(digital input)`, or
  `WordClock(input)` — and word-clock links form a **clock-distribution side-graph**, independent of
  the audio DAG (a dedicated master is a star over BNC, decoupling clock topology from audio routing:
  re-patch audio without breaking sync, one place to change rate, no clock loops).
- At **compile**, resolve the side-graph: assign each device to a **clock domain** (follow sources to
  a root master), detect no-clock / clock-loops / rate conflicts, and mark the **async boundaries**
  where two domains meet.
- At **runtime**, the consequence is **emergent**: each domain advances a phase accumulator at its
  real rate (with crystal-ppm differences) against the analog continuum, and a finite **elastic FIFO**
  at each async boundary genuinely over/underflows → the clicks/slips of an unlocked link. Sharing a
  master collapses the domains (no boundary, no slip); a **sample-rate converter** at the boundary
  re-grids one domain onto the other (the honest fix).
- **Out of scope:** the physical layer — line coding (biphase-mark), PLL clock recovery, bit
  de-framing (inside-the-box circuitry, §2). We model whether a link *locks* and *slips*, not its
  bitstream. True jitter *spectra* are a further optional depth we do not expect to need.
*Prerequisites:* the carrier/clock seam and `ClockDomainId` stamp (Story 1.6); multiple digital
devices and the fractional resampler (this Epic). ROI is high here (multi-device digital sync is the
heart of the lesson), nil before.
