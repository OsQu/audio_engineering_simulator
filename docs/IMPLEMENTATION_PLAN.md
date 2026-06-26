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

## Epic 3 — Real-Time Playback (the north star) — ✅ **Complete**

**Progress:** Stories 3.1–3.4 ✅ — **Epic 3 complete (north star reached).** 3.1 — the engine builds to WASM and the
in-browser feasibility benchmark clears the gate at **≈46× real-time** (in-worklet single-thread
confirmed; the heaviest unknown in PROJECT_PLAN §10 is retired). 3.2 — **first real-time sound**: the
canonical patch plays live in an `AudioWorkletProcessor`, drained zero-copy one quantum at a time, on
both a throwaway static page and the Vite/TS harness (~5.3 ms base latency, clean at idle). 3.3 —
**live control & playing**: sliders drive smoothed params and the computer keyboard / Web MIDI play
notes, both over `port.postMessage` onto `RtEngine`'s named setters; verified by ear (smooth
zipperless knobs, correct-pitch glitch-free notes from QWERTY and a MIDI source). 3.4 —
**glitch-free & low-latency hardening** (the epic exit): the live hot path audited panic-free (two
host-supplied index derefs in `process_io` hardened to total) with denormal coverage confirmed; a durable
real-time-health instrument (worklet compute-budget-overrun counter + engine queue-drop counts) surfaced to
the page; latency measured (engine signal-path **0.625 ms** + browser base/output, reported live).
**Verified in-browser** — glitch-free sustained playing, health clean. The **SAB event ring + COOP/COEP**
(deferred behind the `EventQueue::push` seam, cheap to retrofit) and the **schedule hot-swap** (→ Epic 4.3)
stay deferred, so the *"lock-free cross-thread validation"* item is intentionally open past Epic 3.

**Goal (delivered):** the engine live in the browser — turn knobs and play an instrument with low latency,
glitch-free, with the engine running **inside the AudioWorklet** (WASM) on the audio thread and control
crossing the main→audio boundary as sparse messages. This epic retired the heaviest technical unknown
(real-time fidelity of the oversampled voltage domain) flagged in PROJECT_PLAN §10.

> **Full design notes, rejected alternatives, per-task delivery records, and the settled deferrals live in
> [`EPIC_3_NOTES.md`](./EPIC_3_NOTES.md).** This section keeps only the decisions and the delivered surface
> that constrain later epics — enough to make good follow-up decisions without re-deriving Epic 3.

### What Epic 3 delivered (engine + web surface)

- **New `capture` crate** (workspace member, engine-only deps → wasm-reachable): the implicit capture
  (`Capture` — a stateful FIR `Decimator` + fixed monitor-reference volts→±1.0 + clamp) moved out of
  `harness`, now consumed by both `harness` and `wasm-bindings`. Adds `Capture::group_delay_samples`.
- **WASM build pipeline:** `wasm-bindgen` / `wasm-pack` — `--target web` for the bench page, **`--target
  no-modules`** for the worklet (a classic script: `AudioWorkletGlobalScope` lacks ES-module support); release
  `panic=abort`, `+simd128` via `RUSTFLAGS`; a build-only `wasm-pack build` CI step guards bindgen breakage.
  `web/build-wasm.sh` concatenates a `TextDecoder`/`TextEncoder` polyfill + glue + processor into one file.
- **`wasm-bindings` engine surface — two types.** `BenchEngine` (frozen 3.1 compute-only gate fixture:
  `render_blocks(n)` loops `process` + capture entirely in WASM; `scaled(N)` for the scaling probe).
  `RtEngine` (the real-time surface): owns the pinned canonical patch (`synth → AD → DA → speaker`) + `Capture`;
  `render_quantum()` drains `process_io` zero-alloc into an engine-owned host buffer; `out_ptr()` / `out_len()`
  expose it for a **zero-copy `Float32Array` view** over WASM memory (no `unsafe` — `as_ptr` is safe, the view
  is built JS-side); named control setters (`set_level` / `set_attack_ms` / `set_decay_ms` / `set_sustain` /
  `set_release_ms` / `note_on` / `note_off`) pushing latest-wins params / block-stamped events; real-time-health
  getters `event_drops` / `param_drops` / `signal_path_latency_ms`.
- **Engine additions:** `Node::group_delay_samples` (defaulted 0, overridden by AD/DA),
  `Decimator`/`Interpolator::group_delay` (`(taps−1)/2`), `Schedule::group_delay_samples` (chain sum);
  `Schedule::process_io` **hardened to be total** over host-supplied param/event handles (`.get`/`.get_mut`,
  variant-checked) so a stale/foreign handle skips rather than panicking on the audio thread.
- **`web/` harness** — the durable Vite + TypeScript build/serve infrastructure Epic 4 inherits (the
  engine-before-UI "throwaway page on reusable infrastructure"): `main.ts` (worklet bring-up via
  `processorOptions` bytes, sliders, QWERTY + Web MIDI, live latency + health readout), the AudioWorklet
  processor (`worklet/processor-impl.js` + UTF-8 polyfill), Biome lint/format, Node 24.

### Decisions that bind every later epic

- **Execution model: engine *inside* the AudioWorklet, single-threaded on the audio thread.**
  `Schedule::process_io` runs synchronously in `process()`. A single thread can't grow its own render-ahead
  buffer, so the **browser output buffer (sized by `latencyHint`) is the only jitter cushion**, and its depth
  is added latency. Confirmed by the 3.1 spike (≈46× real-time) — the Worker+SAB-ring fallback is **not** needed.
- **Hybrid bindings:** `wasm-bindgen` for cold/setup; the per-quantum hot path reads/writes **raw WASM linear
  memory directly** (a `Float32Array` view) for zero-copy. The output-pointer accessor needed **no `unsafe`**
  (returning `as_ptr()` is safe; the view is built JS-side).
- **Control transport is `postMessage`** for both lanes (params latest-wins; events block-stamped), drained at
  the top of `process_io`. The lock-free SAB ring is deferred (below).
- **Clocks/rates pinned:** `AudioContext({ sampleRate: 48_000 })` (the engine's hardcoded host rate; M = 8 from
  384 kHz). The AudioWorklet quantum (128 host frames) **is exactly one engine block** (1024 analog samples).
- **Mono only** (epic-wide; multichannel digital is Epic 5). The single output channel is duplicated to the
  device's channels.
- **Real-time at scale** is bounded by the 3.1 scaling probe: throughput is **linear in node count**, one core
  crosses real-time at **~64–68 heavy channels / ~260 nodes**. The levers past that — **multi-core DAG
  partition** and a **lower oversample factor** (8×→4×) — are Epic-5 concerns, flagged not built.
- **SIMD is measure-driven:** rely on `+simd128` autovectorization (the spike showed only ~3% on the
  serial/recursive chain); explicit intrinsics are not pursued — re-measure on the across-channels axis at scale.
- **Determinism preserved:** wall-clock health timing lives **JS-side** (the engine stays clock-free — no
  ambient `Instant`/`SystemTime`).

### Deferred — decided, not gaps

- **Lock-free SAB event ring + COOP/COEP.** `postMessage` is clean at human rates; the ring's payoffs (no
  audio-thread alloc; sample-accurate timing) aren't demanded by the Epic-3 exit and are **decoupled from the
  sequencer goal** — sample-accuracy rides the message's `when`, not the transport, and a sequencer schedules
  ahead of time where latency is irrelevant. Cheap to retrofit behind the single `EventQueue::push` seam (a plain
  `SharedArrayBuffer` ring → the same setters; engine untouched, no `unsafe`). Build it when live performance
  misbehaves or scale's event rate demands it (Epic 5); COOP/COEP defers with it. **The "lock-free cross-thread
  validation" item is intentionally still open.**
- **Schedule hot-swap under load → Epic 4.3.** `ScheduleSlot` exists with a native smoke test; the
  single-threaded in-worklet model has no cross-thread swap path, and graph edits get their first real trigger
  with patch cables in 4.3.
- **Automated native↔WASM parity test.** Deferred until a wasm-only numeric divergence (SIMD reassociation,
  denormals, libm drift) actually surfaces; Rust unit tests + a manual bridge check guard it until then.
- **Precise `currentTime`→sample event mapping** (for *sequenced* MIDI). Live playing uses next-quantum
  stamping (~2.7 ms); precise mapping lands with the sequencer — carry `when` + a shared clock over `postMessage`,
  no ring needed.

### Story-by-story (status + the one thing each settled)

- **3.1 — WASM engine + feasibility spike** ✅ — first WASM artifact + the in-browser faster-than-real-time gate.
  Settled: **≈46× real-time** ⇒ engine-in-worklet single-thread (no Worker+SAB); SIMD ~3% (intrinsics not
  justified); scaling **linear** (~64–68 ch/core). Stood up the `capture` crate + the build pipeline.
- **3.2 — First real-time sound** ✅ *(the live milestone)* — the canonical patch audible live in an
  `AudioWorkletProcessor`, drained zero-copy. Settled: wasm crosses to the worklet as **raw bytes via
  `processorOptions`** (a `WebAssembly.Module` can't be cloned into the worklet realm — it was silently dropped),
  `--target no-modules` + a `TextDecoder` polyfill, pinned 48 kHz; the durable Vite `web/` infra stood up
  (~5.3 ms base latency).
- **3.3 — Live control & playing** ✅ — sliders + QWERTY / Web MIDI over `postMessage` onto named `RtEngine`
  setters; `render_quantum` switched to `process_io`. Settled: **named** setters (the generic UI-enumerable param
  API → Epic 4); notes stamped at the next quantum (precise host-time mapping → the sequencer).
- **3.4 — Glitch-free & low-latency hardening** ✅ *(the epic exit)* — panic/denormal audit (two `process_io`
  index derefs hardened to total; denormals already covered), a durable real-time-health instrument (worklet
  budget-overrun counter + engine queue-drop counts), latency measured (engine signal-path **0.625 ms** + browser
  base/output). Settled: the SAB ring + COOP/COEP and the hot-swap **deferred**; verified in-browser.

---

## Epic 4 — UI: Skeuomorphic Panels + Patch Cables

**Progress:** **Story 4.1 in progress** (planned to Task level 2026-06-25); 4.2–4.5 stay at Story level
until picked up. The original 4-story sketch was reshaped into the 5-story arc below after the UI vision
grew from "device panels + cables" into a **game-like spatial studio/venue sim** (browsable gear catalog,
racks and containers, freely placed in a pan/zoom world, multiple *spaces* with snakes between them,
VST-grade skeuomorphic panels with front controls and back I/O). Per the detail-gradient convention
(§"How this plan is structured"), each Story's Tasks + behavioral/hand-calc gates are written when it is
picked up via the story-planning skill. Settled architecture decisions are below.

**Goal:** the product interface on the proven engine — a game-like studio you build by browsing a gear
catalog, placing devices in racks and spaces, wiring them with patch cables and snakes, operating
realistic skeuomorphic panels, and seeing/hearing the result. A **pure consumer of the published engine
API** (params, events, scene build/load, probes) — never reaching into engine internals.

**Exit criteria:** build and operate a small studio entirely through the UI — add gear from the catalog,
place and patch it across spaces, turn its knobs and play it, and see/hear the results — glitch-free,
with graph edits hot-swapping live under sound.

**Watch-outs:**
- The UI never reaches into engine internals — only the published API. Engine stays UI-free (no layout,
  no panel concepts); UI-facing vocabulary that *is* domain (param ranges, port domains) rides the API,
  not the renderer.
- **Graph edits run on the audio thread.** The engine lives *inside* the AudioWorklet (Epic 3), and a
  `Schedule` is a Rust object in WASM linear memory that **cannot be compiled on the main thread and
  shipped in**. So a graph edit compiles *in the worklet* (on a `port` message, between render quanta)
  and installs via `ScheduleSlot` at a block boundary, dropping the old schedule off-block. `compile`
  allocates — acceptable because edits are rare user gestures at small-studio scale, *not* per-block —
  but it must be measured (a long compile delays the next `process()` ⇒ a glitch). This is the riskiest
  interaction in the epic; prove it in 4.1 and re-measure as graphs grow.
- **Data-driven gear is UI-only.** A catalog entry is a **pair**: an engine node-or-subgraph **factory
  (real Rust code — the black-box transform and internal routing, arbitrarily complex)** + a UI
  **descriptor** (panel layout, control→param bindings, ports). Never model a device as "just data" —
  that paints us into a corner the moment a device needs real behavior. Start with simple single-node
  devices; keep the seam able to express complex / multi-node ones.
- Keep the realism budget on the volts-and-converters layer (epic-wide rule); device panels and the
  world renderer stay understandable.

### Settled architecture decisions (epic planning)

Reached in the planning dialogue; they constrain every Story. Where it matters, the rejected alternative
and the why are recorded.

- **Frontend stack: Svelte 5 + DOM/SVG, with the world-rendering layer isolated behind a thin
  interface.** The spatial world is a CSS-transform pan/zoom surface; devices/racks are components driven
  by descriptors; knobs/faders/jacks are SVG; meters/scopes/screens are small `<canvas>`es; "flip to
  back" is a CSS 3-D transform. *Why this over the alternatives:* a pure WebGL game engine (PixiJS/Phaser
  — "Stack 3") gives the best game feel but forces every widget, text, hit-test, and a11y to be
  hand-drawn and turns "add new gear" into bespoke draw code — directly against the easy-to-author-gear
  goal. A framework-over-PixiJS hybrid ("Stack 2") buys WebGL-grade world performance and Epic-5 scale
  headroom but at real complexity (DOM panels coordinate-synced over a WebGL world) that the
  small-studio target does not yet need. We take the simplest stack with the best gear-authoring DX
  (**"Stack 1"**) and **isolate the world layer** so it can be swapped to a WebGL canvas later *if*
  profiling at scale demands it — mirroring the engine's own "multi-core only if profiling demands it"
  philosophy. Svelte 5 (runes → fine-grained reactivity) over React because the UI has many live-bound
  controls and animating meters where a virtual DOM is the wrong tool; it builds on the existing Vite/TS
  `web/` harness with one dependency.
- **Device metadata lives in a `wasm-bindings` catalog**, not in the engine. The catalog maps a stable
  **type-id → (node/subgraph factory) + serializable descriptor** (display name; params with
  label/unit/control-kind/range/default; ports with label/kind/domain/direction; panel-layout hints).
  *Why not the engine:* keeps the engine portable and UI-free. Lightweight name/unit *may* be added to
  `ParamDecl` if duplication bites, but the catalog is the source of UI truth.
- **One serializable scene IR, shared by build and persistence.** A scene = nodes (type-id + **fixed
  construction config** + param values) + connections + output tap + UI placement (space, rack, position).
  The *same* description the worklet builds an engine from is what we save/load. *Construction config is
  fixed per device type* (realistic gear has fixed impedances/rails); only `params()` knobs are
  user-facing — this keeps the node factory a simple type-id → `Box<dyn Node>` (or subgraph) and avoids a
  generic constructor-argument marshalling problem.
- **A device is a group of 1..N nodes (the "one chassis → many nodes" seam, settled now).** The
  descriptor/catalog/scene IR and all addressing are built around `device → (node, param/port)` from the
  start, so a logical device can expand to several internally-wired nodes (preamps → internal AD → router
  → … ) with a grouped port face. *We ship single-node devices first* and introduce the first concrete
  multi-node device only when a panel needs it — building the seam, not over-building the machinery. This
  retires the Epic-1 deferral ("multi-stage nodes & one-chassis-many-nodes grouping → Epic 4+").
- **Graph edits → recompile + `ScheduleSlot` hot-swap, in the worklet** (see Watch-outs). A *value* param
  change still just reads in `process` (no recompile, per the Epic-1 params-vs-structure rule); only
  structural edits (add/remove device, connect/disconnect) recompile.
- **Power on/off is a control, not a structural edit.** A device exposes a "powered" param its node reads
  (powered-off ⇒ emits silence / passes nothing); toggling power never recompiles. *Why:* power is
  flipped often and should be instant and glitch-free, like a real unit's standby — a structural rebuild
  per toggle would be wrong.
- **Spaces are a UI concept; snakes are visual bundles.** Live room / control room / stage / monitors /
  FOH are UI groupings over **one engine graph** (nodes carry a space tag); the engine never knows about
  rooms. A *snake* between spaces is a UI bundle of individual mono analog cables drawn as one — **true
  multichannel digital bundling stays Epic 5** (5.1/5.3); nothing in the engine changes for snakes here.
- **Skeuomorphic = genuine interaction + recognizable layout, not photoreal textures.** Real
  knob/fader/meter/jack behavior and gear-like layout (the VST-mimics-analog feel); branding, photoreal
  skins, and onboarding polish are explicitly deferred (the project's deprioritize-polish non-goal). This
  reconciles the "feels like real audio gear" goal with "fidelity over polish."

### Stories

#### Story 4.1 — Engine/bindings API for the UI + scene IR + device catalog — 🚧 **In progress**

*Goal:* the generic, UI-facing engine surface that retires the named-setter stopgap from Epic 3 and turns
the pinned-in-Rust canonical patch into a **scene the UI builds, plays, saves, and reloads** — the
foundation every later Epic-4 Story consumes. Delivered when the canonical patch is built from a
*serialized scene* (not hardcoded), played and controlled **generically by device id** through the
worklet, a scene **save→load round-trips**, and a scene **reload hot-swaps glitch-free** under sound.
Anchors to PROJECT_PLAN §4 (Port/Device/Graph domain model), §7 (UI as a pure consumer), and the Epic-1
params-vs-structure + `ScheduleSlot` decisions.

*Watch out:*
- **The recompile/swap runs on the audio thread** (engine-in-worklet; a `Schedule` can't cross realms).
  `load_patch` compiles in the `port` message handler between quanta and installs at a `render_quantum`
  boundary; `compile` allocates, so a long one delays the next `process()` ⇒ a glitch. Acceptable because
  edits are rare gestures at small-studio scale, **not** per-block — but measure it, and keep `compile`
  off the per-block path. This is the riskiest interaction in the epic; prove it here.
- **Engine stays serde-free and persistence-free.** serde lives in the new **`devices`** crate (the
  catalog + scene/build layer), not the engine; the engine gains no UI/scene/versioning vocabulary. The
  runtime ingress is **deserialize-only** on the Rust side (TS → a runnable patch → build); Rust never
  writes the save file.
- **Hot-path contract unchanged.** `render_quantum` stays zero-alloc / panic-free / denormal-flushed; all
  the new fallibility (parse a patch, build a graph, `compile`) lives off the hot path, and a malformed
  patch must surface as a legible error, never a panic on the audio thread.
- **Data-driven gear is UI-only** (epic rule): a catalog entry's *builder is real Rust*, not data. Scope
  guard — 4.1 ships single-node devices + **one** minimal multi-node entry to prove the seam; it is not a
  device-coverage story.

*Design notes (settled at planning):*
- **Persistence is two layers, decided separately.** (1) The **durable save file is TS-owned versioned
  JSON** — `{ schemaVersion, ui, patch }`, with load-time **migrations** in TS; human-readable, diffable,
  backward-compatible. It holds the whole scene including UI-only placement/spaces (the `ui` section,
  populated from 4.3; a stub in 4.1). (2) The **runtime ingress** hands the engine only the current
  **runnable `patch`** projection (devices + param values + connections + output — no UI data), which TS
  produces *after* migrating. *Rejected:* a Rust-owned canonical serde schema serialized to JSON — it
  pulls persistence, versioning, and UI-only fields into the engine-adjacent layer (against "UI owns UI
  data"), and TS still needs mirrored types anyway. The engine never sees the file, versioning, or UI data.
- **Runtime ingress = serde + `serde-wasm-bindgen`** (a structured JS object → Rust struct), not a JSON
  string. *Rejected:* JSON strings (text + a redundant parse on each side) and `tsify` (an extra
  proc-macro to auto-generate TS types — not worth it for the small, central patch schema, whose TS
  interface we hand-write and keep in sync).
- **The catalog + scene IR live in a new `devices` crate, not `wasm-bindings`** (reshaped during 4.1.2).
  The catalog (builder + descriptor) and the scene/patch IR + build-from-scene are **core simulation
  content** (what gear exists, its fixed electrical config, how a serialized arrangement becomes an engine
  graph) — they belong *on* the engine, not in the JS glue. `devices` depends on `engine` + serde and is
  consumed by **both** `wasm-bindings` (browser) and `harness` (native render scenarios); `wasm-bindings`
  keeps only the `JsValue` bridge (`catalog()` → JS value, `parse_patch` ← JS value). *Why:* the engine
  has no opinion on what gear ships (a product decision), and the catalog should be native-testable +
  harness-usable, not trapped behind wasm. Honors "engine stays serde-free" (serde is in `devices`).
- **A catalog entry = descriptor + builder.** The **descriptor** is serde data the UI fetches (display
  name; params with `id/label/unit/control-kind/min/max/default`; ports with
  `id/label/kind/domain/direction`) — it drives the catalog browser, panel rendering (4.2), and
  connection-legality hints (4.4). Its numeric/domain fields are **derived from a freshly built node**
  (engine truth, no drift); only labels/units/kinds are hand-authored. Builder + descriptor live
  **together in one `CATALOG` table** — each entry bundles its `type_id`, name, a `build: fn() -> Box<dyn
  Node>` (fixed construction config), and the UI metadata, so adding gear is one self-contained entry
  (`build_node` is a lookup; `descriptors()` iterates the same table). Nodes go in via a minimal new
  `Graph::add_boxed`. *Refinement on the planned "zero engine change":* a one-line `add_boxed` (which
  `add` now delegates to) gives **one construction site** that's both graph-insertable and introspectable
  for descriptors — killing builder/descriptor drift; worth the trivial engine addition.
- **Chassis-group seam (proven, not over-built).** The builder returns a **device-instance map**
  `device → { nodes: [NodeId], param_map, port_map, event_map }`; patch connections are addressed by
  `(device, port)` and remapped to node-port edges at build; generic control resolves `(device, paramId)
  → ParamHandle` and `device → EventInputId` through the map. Single-node devices are the trivial case
  (one node, identity maps). One minimal **multi-node** entry (e.g. a 2-node channel strip:
  `GainStage → ThreeBandEq`) exercises expansion + internal wiring + exposed-port/param remapping in a
  unit test — no panel needed. Retires the Epic-1 "one-chassis-many-nodes → Epic 4+" deferral.
- **`RtEngine` becomes the scene-driven surface; `BenchEngine` stays frozen** (the 3.1 gate fixture).
  `RtEngine` owns a swap seam (`ScheduleSlot` or a pending-`Box<Schedule>`) and a stable output buffer;
  `new(patch)` / `load_patch(patch)` build → `compile` (fixed `SEED`, so same scene reproduces) → install
  at the next block boundary, dropping the old schedule off-block; control addressing is rebuilt after
  every swap. The named setters (`set_level`…) are removed in favor of generic
  `set_param(device, id, value)` / `note_on(device, …)` / `note_off(device, …)`.
- **Known simplification (not a bug):** the old schedule's `drop` (buffer dealloc) happens on the audio
  thread *between* blocks — cheap at small-studio scale. A deferred-drop free-list is a later option if
  profiling at scale shows it costing a quantum. Recorded, not built.
- **Validation is behavioral/structural, not hand-calc volts.** 4.1 is plumbing; its oracle is that the
  *existing* Epic 1–3 analog/DSP assertions still hold when the patch is built from a scene rather than
  hardcoded — i.e. **output parity** with the pinned patch, plus round-trip identity, descriptor↔node
  count parity, and swap continuity. All prior tests stay green.

- **Task 4.1.1 — Patch IR + serde ingress.** ✅ Define the runnable-patch structs (`DeviceInstance { id,
  type_id, params }`, `Connection { from:(device,port), to:(device,port), cable? }`, output tap) with
  serde `#[derive]`; deserialize a JS object → patch (`parse_patch` in `wasm-bindings`, over
  `serde-wasm-bindgen`). *(Landed in the new `devices` crate — see the crate-layout design note.)*
  *Done:* a patch object from JS deserializes into Rust and a malformed one yields a clean error (no
  panic); native tests round-trip the IR through JSON. TS `Patch` interface hand-written.
- **Task 4.1.2 — Device catalog: descriptor + builder (single-node entries).** ✅ The type-id registry:
  the serde **descriptor** (numeric/domain fields derived from the node, labels authored) exposed to JS
  via `wasm-bindings`' `catalog()` glue, and the **builder** `match` constructing nodes (`Box<dyn Node>`
  via `Graph::add_boxed`) with fixed config. Seeded with `SynthVoice`, `GainStage`, `ThreeBandEq`,
  `AdConverter`, `DaConverter`, `Speaker`. *Done:* JS can fetch the catalog; tests assert UI-meta↔node
  count alignment and that descriptors carry bit-exact param ranges + correct port domains.
- **Task 4.1.3 — Chassis-group seam: expansion, addressing, connection remap.** Generalize the builder to
  emit 1..N nodes + internal edges + the exposed `port/param/event` maps; build the device-instance map;
  remap `(device, port)` connections to node-port edges. Add one minimal multi-node entry (channel strip).
  *Done:* a unit test builds the multi-node device, asserts its internal wiring, and resolves its exposed
  ports/params to the correct `(NodeId, …)`; single-node remains the trivial path.
- **Task 4.1.4 — Build-engine-from-patch: assemble, compile, resolve handles, surface errors.** Assemble a
  `Graph` from a patch via the catalog, `compile` (fixed seed), and resolve generic addressing through the
  instance map; surface `CompileError` as a structured `Result` to JS. *Done:* a native test builds the
  **canonical patch from a patch struct** and renders the *same* non-silent output as the pinned patch
  (output parity); a bad patch (dangling/cycle/domain-mismatch) returns a legible error, never a panic.
- **Task 4.1.5 — Scene-driven `RtEngine` + recompile/hot-swap + generic control.** Refactor `RtEngine` to
  own the swap seam and a stable output buffer; `new(patch)` / `load_patch(patch)` (compile off-block,
  install at the next `render_quantum`, drop old off-block) with addressing rebuilt post-swap; generic
  `set_param` / `note_on` / `note_off` by device id; remove the named setters. *Done:* native tests —
  silent-until-note still holds; loading patch A then B makes output reflect B after the swap; a no-op
  reload preserves output continuity (the swap is glitch-free); `BenchEngine` untouched and still green.
- **Task 4.1.6 — Worklet + TS: scene-driven bring-up, generic control, save/load, in-browser reload.**
  Refactor `processor-impl.js` (construct from a patch via `processorOptions`; a `loadPatch` message →
  `engine.load_patch`; generic param/note messages by device id; `CompileError` → the status line) and
  `main.ts` (hold the authoritative scene as versioned JSON `{ schemaVersion, ui, patch }`; build the
  default canonical scene; generic controls; save/load via a JSON string + `localStorage`; a **reload**
  action proving the glitch-free swap with the health line clean). *Done:* the canonical patch runs *from
  a scene* in-browser, controls work generically by device, save→load round-trips, and reload is audibly
  glitch-free with health clean.

*Validate:* the canonical patch is built from a serialized scene and played/controlled **generically by
device id** through the worklet; `catalog()` exposes every device's descriptor; the chassis seam is proven
by the multi-node entry's test; a scene **save→load round-trips** and a **reload hot-swaps glitch-free**
under sound (health clean); a malformed patch surfaces a legible error, never an audio-thread panic; the
engine touches only its public API and remains serde-free; **all prior Epic 1–3 tests stay green** and the
full gate passes (`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`, plus the
`wasm-pack build` step and `web` Biome/build).
- **Story 4.2 — Skeuomorphic device panels: controls → params, front/back, power.** The data-driven
  panel system, rendered from the descriptor, for one or two devices bound to a running *static* engine:
  real knobs/faders, a VU/meter, a screen, jacks on the **back** (CSS flip), and a **power** switch
  (control, not recompile). Establishes the widget vocabulary and the descriptor → panel renderer that
  every later device reuses. *Open at pickup:* the widget set + interaction model (drag-to-turn,
  fine/coarse, value readout); the descriptor's panel-layout schema; which two devices to build first.
- **Story 4.3 — The spatial world: spaces, racks, placement, catalog browsing.** The Svelte app shell +
  the isolated world layer: pan/zoom; place and move devices and racks; open/close containers; multiple
  **spaces** with switching; **browse the catalog and add/remove gear** — exercising the 4.1 recompile
  path on add/remove. UI scene state stays in sync with the engine scene IR. *Open at pickup:* the world
  layer's interface (the swap-to-WebGL-later boundary); rack/unit sizing model; how a space maps onto the
  scene IR's placement fields; add/remove debouncing vs recompile cost.
- **Story 4.4 — Patch cables & snakes → live graph mutation.** Drag-to-connect between jacks with bezier
  cables; **snakes** as visual bundles of mono cables crossing spaces; connect/disconnect mutates the
  graph → recompile/**hot-swap live under sound**. The "patching feels natural" payoff and the
  swap-under-load proof (re-measure the audio-thread compile cost at realistic graph size). *Open at
  pickup:* cable hit-testing/routing visuals; legal-connection feedback (domain compatibility from the
  descriptor, before compile rejects it); snake bundle/break UX.
- **Story 4.5 — Visualization: meters, scope, spectrum, analog-domain readouts.** A new engine/bindings
  **probe** surface — per-node/port sample taps (meters, scope), an FFT path (spectrum), and the
  distinctive **analog-domain readouts** (per-edge loading loss, clipping/headroom, noise floor, dBu/dBFS
  levels, phantom presence) read from compiled edge gains + runtime peak/clip detection. Rendered as
  device **screens** and as **global tools** — the pedagogical payoff (gain-staging across the AD/DA
  boundary made visible). *Open at pickup:* probe API shape (zero-copy ring taps like `out_ptr`?); FFT in
  engine vs JS; which readouts are device-embedded vs global; tap cost on the hot path.

*Validate (epic exit):* a small studio built, placed, patched across at least two spaces, played, and
metered entirely through the UI; structural edits hot-swap glitch-free under sound; the UI touches only
the published engine API.

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
