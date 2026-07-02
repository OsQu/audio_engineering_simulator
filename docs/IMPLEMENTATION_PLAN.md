# Audio Engineer Simulator — Implementation Plan

Companion to `PROJECT_PLAN.md`. The project plan is the _what and why_; this is the
_in what order, and at what granularity_. It is a living document — we elaborate the
near work in detail and keep the far work deliberately coarse, refining it as we arrive.

## How this plan is structured

Three levels, mirroring Epic → Story → Task:

- **Epic** — a roadmap stage from `PROJECT_PLAN.md` §9. The high-level arc:
  _engine → offline audio → real-time audio → UI → breadth._ Each delivers something
  usable and retires the riskiest remaining unknown.
- **Story** — a coherent slice within an Epic, with its own goal and watch-outs.
  Roughly a week-ish of focused work; the unit at which we think about design, **and the
  unit of branching**.
- **Task** — small, **1–10 commits**, the unit of execution. Tasks land as commits on the
  Story's branch; the Story merges to `main` when its _Validate_ gate is green.

**How we work this plan — overview first, flesh out on arrival.** The whole arc is mapped up
front (every Epic and Story is named, so the shape of the project is visible end to end), but a
Story is only _elaborated to Task level and design notes_ when we actually pick it up to build it.
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
fast-forward) to `main` and delete on merge once the Story's _Validate_ gate is green.

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
  `schedule.process(out, &control_queue, &event_queue)` — one code path for offline _and_ real-time.
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
back out of digital, with all physical behavior _emerging_ from the voltage math and asserted by tests.

> **Full design notes, rejected alternatives, hand-calc oracles, and per-task delivery records for
> every Story below live in [`EPIC_1_NOTES.md`](./EPIC_1_NOTES.md).** This section keeps only the
> decisions and the delivered API surface that constrain later epics — enough to make good follow-up
> decisions without re-deriving Epic 1. Go to the notes when a decision turns on _why_ something was
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
- **Input lanes (two, genuinely separate):** _Events_ are a **routed carrier** — `Lane::Events`
  (bounded, drop-on-overflow), `EventMessage` (note-on/off, gate), external `EventQueue` (SPSC seam,
  absolute-sample timestamps, block-bucketed). _Control params_ are a **host→node side-channel** —
  `ParamDecl` / `Node::params()`, latest-wins `ParamQueue`, framework-owned `Smoother` store with
  within-block linear-ramp de-zipper, exposed via `Params` (`Params::EMPTY` default). Driven through
  `Schedule::process_io` / `process_with_params` / `process_with_events`.

### Decisions that bind every later epic

- **Hot-path discipline (`process`): zero-alloc, lock-free-shaped, panic-free, denormal-flushed.** All
  validation, allocation, and error reporting live in graph construction and `compile`; `process` is
  total. A `no_alloc` counting-allocator test guards this and must stay green.
- **`f32` storage, `f64` accumulation** (summing, filter state, FIR/AA accumulators).
- **Two signal types never conflated; converters are the only domain bridge.** Every **edge connects
  same-domain ports** (`DomainMismatch` otherwise); a converter crosses domains _inside its own node_.
  A buffer storing dB/dBFS is a category error. Don't bake a _closed_ carrier set — `Lane` is open.
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
  is one-pole only today. → first reactive _device_, **Epic 5**.
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
  exercised single-threaded today. → **Epic 3**: the param/event _drain_ runs on the real audio thread from
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
- **1.3 — Minimal runnable engine** ✅ _(first end-to-end milestone)_ — `Node` trait, `Graph`, topo sort,
  `compile -> Schedule`, zero-alloc `process`, swap seam. Settled Node-vs-device naming, the stage model,
  and params-vs-recompile. **The engine became runnable here.**
- **1.4 — Analog-chain physics** ✅ — device noise as spectral density (V/√Hz), per-node seeding, SNR in
  quadrature, `DcBlocker`, rail clipping & headroom. "Tests are the oracle" cases proven on real chains.
- **1.5 — Balanced lines, pickup & common-mode** ✅ — two-conductor balanced lines, the per-conductor
  **lift**, edge-coupled pickup/hum, phantom. Ideal CMRR emerges from leg symmetry (finite CMRR deferred).
- **1.6 — AD/DA converters & the carrier seam** ✅ _(second carrier)_ — the `Lane` enum, `SampleBuffer`,
  domain-tagged ports, polyphase FIR converters, per-converter dBFS calibration, TPDF-dither quantization.
  Generalized one buffer type → an **open carrier set**; laid the MIDI / networked-audio seam.
- **1.7 — Input lanes & a playable voice** ✅ _(third carrier)_ — `Lane::Events` + `EventQueue`, the
  control-param system (`ParamDecl` / `Smoother` / `Params`), and `SynthVoice`. Kept events (routed
  carrier) and control params (side-channel) genuinely separate. **Epic 1 exit met.**

---

## Epic 2 — Offline Render ("hear it" cheaply) — ✅ **Substantially complete** (2.3 deferred)

Stories 2.1 ✅ and 2.2 ✅ done; **2.3 deferred**. The _same_ engine, driven block by block via
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
- **The host render is an _implicit capture_, outside the simulation** — harness plumbing that taps the
  speaker voltage and resamples to host rate. It carries **no `ClockDomainId`**, rides **no
  modeled-converter clock/rate**, and has **no dBFS role**. It **reuses the FIR `Decimator`** so it is
  transparent and adds no artifacts of its own — aliasing/quantization must come only from the _modeled_
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
  standing numeric oracles + render scenarios already pin behavior against hand calcs; a _regression_ layer
  only earns its keep once we're fighting drift/quality regressions. The payoff-demo knobs already exist
  (`AdConverter::with_aa_taps`, `BitDepth`) and the naive-sawtooth voice has the HF content aliasing needs,
  so resuming is cheap. The settled design (feature-vector JSON goldens, `--bless` over a shared
  `harness::golden` lib, six locked renders, a promoted spectral helper) is recorded in `EPIC_2_NOTES.md`.

### Story-by-story (status + the one thing each settled)

- **2.1 — Offline render to WAV + speaker terminus** ✅ _(first sound)_ — render driver loops `process_io`
  into a WAV writer; thin `Speaker` terminus; harness-side implicit capture. Settled: capture is a
  **stateful harness-held `Decimator`** (not a second engine, off-sim-clock, no `ClockDomainId`), canonical
  format is **float32 WAV** (PCM16 would contaminate 2.3's quantization measurement). **First sound.**
- **2.2 — First DSP: 3-band EQ + compressor (digital)** ✅ — `Biquad` + RBJ designers + `PeakEnvelope` in a
  new `dsp` module; `ThreeBandEq` and `Compressor` between AD and DA. Settled: pure-digital nodes need
  **no graph/schedule work** (1.6 ports carried them); **static** config (coeff smoothing → Epic 3).
- **2.3 — Golden-file harness + converter-payoff demos** ⏸️ **Deferred (2026-06-23)** — see _Deferred_
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
stay deferred, so the _"lock-free cross-thread validation"_ item is intentionally open past Epic 3.

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

- **Execution model: engine _inside_ the AudioWorklet, single-threaded on the audio thread.**
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
- **Precise `currentTime`→sample event mapping** (for _sequenced_ MIDI). Live playing uses next-quantum
  stamping (~2.7 ms); precise mapping lands with the sequencer — carry `when` + a shared clock over `postMessage`,
  no ring needed.

### Story-by-story (status + the one thing each settled)

- **3.1 — WASM engine + feasibility spike** ✅ — first WASM artifact + the in-browser faster-than-real-time gate.
  Settled: **≈46× real-time** ⇒ engine-in-worklet single-thread (no Worker+SAB); SIMD ~3% (intrinsics not
  justified); scaling **linear** (~64–68 ch/core). Stood up the `capture` crate + the build pipeline.
- **3.2 — First real-time sound** ✅ _(the live milestone)_ — the canonical patch audible live in an
  `AudioWorkletProcessor`, drained zero-copy. Settled: wasm crosses to the worklet as **raw bytes via
  `processorOptions`** (a `WebAssembly.Module` can't be cloned into the worklet realm — it was silently dropped),
  `--target no-modules` + a `TextDecoder` polyfill, pinned 48 kHz; the durable Vite `web/` infra stood up
  (~5.3 ms base latency).
- **3.3 — Live control & playing** ✅ — sliders + QWERTY / Web MIDI over `postMessage` onto named `RtEngine`
  setters; `render_quantum` switched to `process_io`. Settled: **named** setters (the generic UI-enumerable param
  API → Epic 4); notes stamped at the next quantum (precise host-time mapping → the sequencer).
- **3.4 — Glitch-free & low-latency hardening** ✅ _(the epic exit)_ — panic/denormal audit (two `process_io`
  index derefs hardened to total; denormals already covered), a durable real-time-health instrument (worklet
  budget-overrun counter + engine queue-drop counts), latency measured (engine signal-path **0.625 ms** + browser
  base/output). Settled: the SAB ring + COOP/COEP and the hot-swap **deferred**; verified in-browser.

---

## Epic 4 — UI: Skeuomorphic Panels + Patch Cables

**Progress:** **Stories 4.1 ✅, 4.2 ✅, 4.3 ✅, 4.4 ✅, and 4.5 ✅ done.** 4.1 — the engine→UI seam: a new `devices` crate,
scene IR + catalog + `build_patch`, and `SceneEngine` (scene-driven, generically controlled, hot-swappable).
4.2 — the skeuomorphic panel system on a **Svelte 5** harness: a descriptor → panel renderer + widget
vocabulary (knobs/faders/switches/jacks/screen/VU), front/back flip, a real `powered` control param, and a
host-side monitor volume; metering (a `VuMeter` node + node→host readout lane) stays deferred to 4.5. 4.3 —
the **spatial world**: a front-elevation pan/zoom studio where gear lives at real coordinates, mounts in
**rack U-slots** (drag-snap), moves between **rooms**, and is **added/removed from a catalog palette** (the
4.1 hot-swap recompile path); pure Vitest-tested spatial logic + a thin world layer (WebGL escape hatch),
engine untouched. Operator **reach** and **multi-view projections** were deferred to the new **Story 4.6**
(3-D coordinate truth is stored now so they stay cheap). 4.4 ✅ — **patch cables & snakes**:
drag-to-connect between back-panel jacks → `loadPatch` hot-swap, client-side legality (incl. feedback-cycle
rejection), a cable inspector with pickable cable types (R·C rides the edge, inaudible by design → Epic 5),
behind/front cable layering, and cross-space **portal** endpoints; engine untouched beyond the `devices`
cable catalog. 4.5 ✅ — **visualization**: the node→host scalar readout lane, a `VuMeter` (analog VU/dBu) + a
digital dBFS meter, and a static per-connection loading-loss annotation, surfaced as device meter screens, a
cable-inspector loss line, and a global levels panel; the raw-sample **scope + spectrum FFT** were split out
into a new **Story 4.7** at 4.5 pickup (waveform probes are a distinct mechanism from the scalar lane).
**Next: Story 4.6** (top-down view + operator reach). 4.6–4.7 stay at Story level until picked up. The original
4-story sketch was reshaped into the now-7-story arc below after the UI vision
grew from "device panels + cables" into a **game-like spatial studio/venue sim** (browsable gear catalog,
racks and containers, freely placed in a pan/zoom world, multiple _spaces_ with snakes between them,
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
  no panel concepts); UI-facing vocabulary that _is_ domain (param ranges, port domains) rides the API,
  not the renderer.
- **Graph edits run on the audio thread.** The engine lives _inside_ the AudioWorklet (Epic 3), and a
  `Schedule` is a Rust object in WASM linear memory that **cannot be compiled on the main thread and
  shipped in**. So a graph edit compiles _in the worklet_ (on a `port` message, between render quanta)
  and installs via `ScheduleSlot` at a block boundary, dropping the old schedule off-block. `compile`
  allocates — acceptable because edits are rare user gestures at small-studio scale, _not_ per-block —
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
  back" is a CSS 3-D transform. _Why this over the alternatives:_ a pure WebGL game engine (PixiJS/Phaser
  — "Stack 3") gives the best game feel but forces every widget, text, hit-test, and a11y to be
  hand-drawn and turns "add new gear" into bespoke draw code — directly against the easy-to-author-gear
  goal. A framework-over-PixiJS hybrid ("Stack 2") buys WebGL-grade world performance and Epic-5 scale
  headroom but at real complexity (DOM panels coordinate-synced over a WebGL world) that the
  small-studio target does not yet need. We take the simplest stack with the best gear-authoring DX
  (**"Stack 1"**) and **isolate the world layer** so it can be swapped to a WebGL canvas later _if_
  profiling at scale demands it — mirroring the engine's own "multi-core only if profiling demands it"
  philosophy. Svelte 5 (runes → fine-grained reactivity) over React because the UI has many live-bound
  controls and animating meters where a virtual DOM is the wrong tool; it builds on the existing Vite/TS
  `web/` harness with one dependency.
- **Device metadata lives in a `wasm-bindings` catalog**, not in the engine. The catalog maps a stable
  **type-id → (node/subgraph factory) + serializable descriptor** (display name; params with
  label/unit/control-kind/range/default; ports with label/kind/domain/direction; panel-layout hints).
  _Why not the engine:_ keeps the engine portable and UI-free. Lightweight name/unit _may_ be added to
  `ParamDecl` if duplication bites, but the catalog is the source of UI truth.
- **One serializable scene IR, shared by build and persistence.** A scene = nodes (type-id + **fixed
  construction config** + param values) + connections + output tap + UI placement (space, rack, position).
  The _same_ description the worklet builds an engine from is what we save/load. _Construction config is
  fixed per device type_ (realistic gear has fixed impedances/rails); only `params()` knobs are
  user-facing — this keeps the node factory a simple type-id → `Box<dyn Node>` (or subgraph) and avoids a
  generic constructor-argument marshalling problem.
- **A device is a group of 1..N nodes (the "one chassis → many nodes" seam, settled now).** The
  descriptor/catalog/scene IR and all addressing are built around `device → (node, param/port)` from the
  start, so a logical device can expand to several internally-wired nodes (preamps → internal AD → router
  → … ) with a grouped port face. _We ship single-node devices first_ and introduce the first concrete
  multi-node device only when a panel needs it — building the seam, not over-building the machinery. This
  retires the Epic-1 deferral ("multi-stage nodes & one-chassis-many-nodes grouping → Epic 4+").
- **Graph edits → recompile + `ScheduleSlot` hot-swap, in the worklet** (see Watch-outs). A _value_ param
  change still just reads in `process` (no recompile, per the Epic-1 params-vs-structure rule); only
  structural edits (add/remove device, connect/disconnect) recompile.
- **Power on/off is a control, not a structural edit.** A device exposes a "powered" param its node reads
  (powered-off ⇒ emits silence / passes nothing); toggling power never recompiles. _Why:_ power is
  flipped often and should be instant and glitch-free, like a real unit's standby — a structural rebuild
  per toggle would be wrong.
- **Spaces are a UI concept; snakes are visual bundles.** Live room / control room / stage / monitors /
  FOH are UI groupings over **one engine graph** (nodes carry a space tag); the engine never knows about
  rooms. A _snake_ between spaces is a UI bundle of individual mono analog cables drawn as one — **true
  multichannel digital bundling stays Epic 5** (5.1/5.3); nothing in the engine changes for snakes here.
- **The spatial sim is a data/constraint model with a 2-D presentation — it stays on the Svelte + DOM/SVG
  stack; it is _not_ a rendering problem.** Devices have real physical dimensions and live in containers
  (rack / desk / room); placement is constrained (rackmount → rack U-slots, desktop gear → a desk
  surface); the sim tracks the operator's position and what's within **reach** (zoom out for the overview,
  but then you can't touch); back-panel access is **gated behind a physical action** (flip a unit, pull it
  from the rack, roll the rack off the wall); bounds-checking is cheap **axis-aligned-rectangle (AABB)**
  overlap because audio gear is boxes; switching rooms switches the interactable set. _Why the stack is
  unchanged:_ the novel, hard parts — the spatial model, placement legality, reach, view projection — are
  **framework-agnostic data + math**, and the _presentation_ is only tens-to-low-hundreds of rectangles in
  a 2-D projection (≈260 nodes is the Epic-5 napkin ceiling), which DOM/SVG over the CSS-transform pan/zoom
  surface handles comfortably. A WebGL/game-engine stack only earns its complexity for thousands of
  animated sprites, a true 3-D perspective camera, or per-pixel shaders — **none of which this wants**
  (explicitly "no fancy 3-D"). The Stack-1 decision's _isolated world layer_ is the standing escape hatch:
  swap that layer to a WebGL canvas later **only if** profiling at venue scale demands it — same
  "multi-core only if profiling demands it" philosophy as the engine; don't pre-build it.
  - **Model in 3-D, render in 2-D (the one discipline that matters).** Store a _single_ coordinate truth
    per object — position `(x, y, z)` + a footprint box + a facing — and render top/side/front views as
    **projections** of it (each view just picks which two axes map to screen X/Y). Storing per-view 2-D
    positions is the trap that drifts the views out of sync. The "flip to back" CSS 3-D transform from 4.2
    is reused, but **gated** by a clearance state (the unit must be pulled out / the rack rolled off the
    wall before its back is reachable).
  - **Where the model lives — split by what it _is_.** _Placement, player position, reach, zoom/view
    state, room membership_ are UI state → the TS `ui` layer (the scene IR's reserved placement section;
    engine-stays-UI-free, "spaces are a UI concept"). _Physical dimensions_ (rack-U height, footprint) are
    **content, not UI** — about as intrinsic as a device's impedance — so they belong on the **`devices`
    catalog descriptor** (derived/authored alongside the rest of the device, native-testable, no drift),
    **not** invented in TS. _Rejected:_ dimensions as TS-only UI data (re-invents content the catalog owns,
    risks drift); a single "spatial = UI" lump (conflates intrinsic gear facts with view state). The engine
    gains **nothing** either way — no rooms, racks, or position.
  - **The spatial logic is pure and rendering-free → unit-testable** (AABB overlap, placement legality,
    reach queries, projection), fitting the project's "tests are the oracle" temperament; keep model and
    renderer separate and the WebGL escape hatch stays open for free.
- **Skeuomorphic = genuine interaction + recognizable layout, not photoreal textures.** Real
  knob/fader/meter/jack behavior and gear-like layout (the VST-mimics-analog feel); branding, photoreal
  skins, and onboarding polish are explicitly deferred (the project's deprioritize-polish non-goal). This
  reconciles the "feels like real audio gear" goal with "fidelity over polish."

### Stories

#### Story 4.1 — Engine/bindings API for the UI + scene IR + device catalog — ✅ **Done**

_Goal:_ the generic, UI-facing engine surface that retires the named-setter stopgap from Epic 3 and turns
the pinned-in-Rust canonical patch into a **scene the UI builds, plays, saves, and reloads** — the
foundation every later Epic-4 Story consumes. Delivered when the canonical patch is built from a
_serialized scene_ (not hardcoded), played and controlled **generically by device id** through the
worklet, a scene **save→load round-trips**, and a scene **reload hot-swaps glitch-free** under sound.
Anchors to PROJECT_PLAN §4 (Port/Device/Graph domain model), §7 (UI as a pure consumer), and the Epic-1
params-vs-structure + `ScheduleSlot` decisions.

_Watch out:_

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
- **Data-driven gear is UI-only** (epic rule): a catalog entry's _builder is real Rust_, not data. Scope
  guard — 4.1 ships single-node devices + **one** minimal multi-node entry to prove the seam; it is not a
  device-coverage story.

_Design notes (settled at planning):_

- **Persistence is two layers, decided separately.** (1) The **durable save file is TS-owned versioned
  JSON** — `{ schemaVersion, ui, patch }`, with load-time **migrations** in TS; human-readable, diffable,
  backward-compatible. It holds the whole scene including UI-only placement/spaces (the `ui` section,
  populated from 4.3; a stub in 4.1). (2) The **runtime ingress** hands the engine only the current
  **runnable `patch`** projection (devices + param values + connections + output — no UI data), which TS
  produces _after_ migrating. _Rejected:_ a Rust-owned canonical serde schema serialized to JSON — it
  pulls persistence, versioning, and UI-only fields into the engine-adjacent layer (against "UI owns UI
  data"), and TS still needs mirrored types anyway. The engine never sees the file, versioning, or UI data.
- **Runtime ingress = serde + `serde-wasm-bindgen`** (a structured JS object → Rust struct), not a JSON
  string. _Rejected:_ JSON strings (text + a redundant parse on each side) and `tsify` (an extra
  proc-macro to auto-generate TS types — not worth it for the small, central patch schema, whose TS
  interface we hand-write and keep in sync).
- **The catalog + scene IR live in a new `devices` crate, not `wasm-bindings`** (reshaped during 4.1.2).
  The catalog (builder + descriptor) and the scene/patch IR + build-from-scene are **core simulation
  content** (what gear exists, its fixed electrical config, how a serialized arrangement becomes an engine
  graph) — they belong _on_ the engine, not in the JS glue. `devices` depends on `engine` + serde and is
  consumed by **both** `wasm-bindings` (browser) and `harness` (native render scenarios); `wasm-bindings`
  keeps only the `JsValue` bridge (`catalog()` → JS value, `parse_patch` ← JS value). _Why:_ the engine
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
  `Graph::add_boxed`. _Refinement on the planned "zero engine change":_ a one-line `add_boxed` (which
  `add` now delegates to) gives **one construction site** that's both graph-insertable and introspectable
  for descriptors — killing builder/descriptor drift; worth the trivial engine addition.
- **Chassis-group seam (proven, not over-built).** `instantiate(type_id, &mut Graph)` expands a device
  into 1..N nodes + internal edges and returns a **`BuiltDevice`** map `{ nodes: [NodeId], inputs,
outputs, params }` from device-level ports/params to concrete `(NodeId, …)`. The **exposed face is
  derived by convention** — a port is exposed when no internal edge consumes it (open ports, node order);
  all node params are exposed, concatenated — so a device needn't hand-list its face. Patch connections
  are addressed by `(device, port)` and remapped through the map; generic control resolves `(device,
paramId) → (NodeId, ParamId)` (→ `ParamHandle`) and an `Events`-domain input port → `EventInputId`.
  Single-node devices are the trivial case (one node, whole face exposed). The minimal **multi-node**
  proof is a 2-stage analog `channel_strip` (`GainStage → GainStage`): input+output gain behind one
  device, exposing stage 0's input, stage 1's output, and _both_ gains' params (device param 1 → the
  second node — a non-trivial remap). _(The originally sketched `GainStage → ThreeBandEq` is
  electrically invalid — analog into a digital port — so a strip with digital EQ/dynamics needs an
  internal AD, which arrives with deeper devices; two analog stages is the smallest valid proof.)_
  Retires the Epic-1 "one-chassis-many-nodes → Epic 4+" deferral.
  - **Extension points (deferred, seam is stable).** Three kinds of internal routing, three homes:
    _(a) fixed topology_ — static `InternalEdge` data (now); _(b) build-time-parameterized topology_
    (an N-channel mixer, an interface with N preamps) — needs an **imperative builder** variant of a
    catalog entry (e.g. `Fixed { nodes, internal } | Built(fn(&Config, &mut Graph) -> BuiltDevice)`)
    plus an **optional structural-config field** on the scene IR's `DeviceInstance` (serde
    `#[serde(default)]`, backward-compatible); _(c) runtime-switchable routing_ (bypass, M/S, a routing
    matrix) — lives **inside a node** via a control param (never a topology change, per
    params-vs-structure), or is user-repatching → graph edit + recompile (4.3). Both (b)/(c) are
    **additive behind `instantiate -> BuiltDevice`** (callers unaffected); first needed in **Epic 5.1**
    (deeper mixer / patchbay), so built there, not now.
- **`RtEngine` becomes the scene-driven surface; `BenchEngine` stays frozen** (the 3.1 gate fixture).
  `RtEngine` owns a swap seam (`ScheduleSlot` or a pending-`Box<Schedule>`) and a stable output buffer;
  `new(patch)` / `load_patch(patch)` build → `compile` (fixed `SEED`, so same scene reproduces) → install
  at the next block boundary, dropping the old schedule off-block; control addressing is rebuilt after
  every swap. The named setters (`set_level`…) are removed in favor of generic
  `set_param(device, id, value)` / `note_on(device, …)` / `note_off(device, …)`.
- **Known simplification (not a bug):** the old schedule's `drop` (buffer dealloc) happens on the audio
  thread _between_ blocks — cheap at small-studio scale. A deferred-drop free-list is a later option if
  profiling at scale shows it costing a quantum. Recorded, not built.
- **Validation is behavioral/structural, not hand-calc volts.** 4.1 is plumbing; its oracle is that the
  _existing_ Epic 1–3 analog/DSP assertions still hold when the patch is built from a scene rather than
  hardcoded — i.e. **output parity** with the pinned patch, plus round-trip identity, descriptor↔node
  count parity, and swap continuity. All prior tests stay green.

- **Task 4.1.1 — Patch IR + serde ingress.** ✅ Define the runnable-patch structs (`DeviceInstance { id,
type_id, params }`, `Connection { from:(device,port), to:(device,port), cable? }`, output tap) with
  serde `#[derive]`; deserialize a JS object → patch (`parse_patch` in `wasm-bindings`, over
  `serde-wasm-bindgen`). _(Landed in the new `devices` crate — see the crate-layout design note.)_
  _Done:_ a patch object from JS deserializes into Rust and a malformed one yields a clean error (no
  panic); native tests round-trip the IR through JSON. TS `Patch` interface hand-written.
- **Task 4.1.2 — Device catalog: descriptor + builder (single-node entries).** ✅ The type-id registry:
  the serde **descriptor** (numeric/domain fields derived from the node, labels authored) exposed to JS
  via `wasm-bindings`' `catalog()` glue, and the **builder** `match` constructing nodes (`Box<dyn Node>`
  via `Graph::add_boxed`) with fixed config. Seeded with `SynthVoice`, `GainStage`, `ThreeBandEq`,
  `AdConverter`, `DaConverter`, `Speaker`. _Done:_ JS can fetch the catalog; tests assert UI-meta↔node
  count alignment and that descriptors carry bit-exact param ranges + correct port domains.
- **Task 4.1.3 — Chassis-group seam: expansion, addressing, connection remap.** Generalize the builder to
  emit 1..N nodes + internal edges + the exposed face; `instantiate` builds the `BuiltDevice` map.
  Add one minimal multi-node entry (the 2-stage analog `channel_strip`).
  _Done:_ a unit test builds the multi-node device, asserts its internal wiring, and resolves its exposed
  ports/params to the correct `(NodeId, …)`; single-node remains the trivial path.
- **Task 4.1.4 — Build-engine-from-patch: assemble, compile, resolve handles, surface errors.** Assemble a
  `Graph` from a patch via the catalog, `compile` (fixed seed), and resolve generic addressing through the
  instance map; surface `CompileError` as a structured `Result` to JS. _Done:_ a native test builds the
  **canonical patch from a patch struct** and renders the _same_ non-silent output as the pinned patch
  (output parity); a bad patch (dangling/cycle/domain-mismatch) returns a legible error, never a panic.
- **Task 4.1.5 — Scene-driven `RtEngine` + recompile/hot-swap + generic control.** Refactor `RtEngine` to
  own the swap seam and a stable output buffer; `new(patch)` / `load_patch(patch)` (compile off-block,
  install at the next `render_quantum`, drop old off-block) with addressing rebuilt post-swap; generic
  `set_param` / `note_on` / `note_off` by device id; remove the named setters. _Done:_ native tests —
  silent-until-note still holds; loading patch A then B makes output reflect B after the swap; a no-op
  reload preserves output continuity (the swap is glitch-free); `BenchEngine` untouched and still green.
- **Task 4.1.6 — Worklet + TS: scene-driven bring-up, generic control, save/load, in-browser reload.**
  Refactor `processor-impl.js` (construct from a patch via `processorOptions`; a `loadPatch` message →
  `engine.load_patch`; generic param/note messages by device id; `CompileError` → the status line) and
  `main.ts` (hold the authoritative scene as versioned JSON `{ schemaVersion, ui, patch }`; build the
  default canonical scene; generic controls; save/load via a JSON string + `localStorage`; a **reload**
  action proving the glitch-free swap with the health line clean). _Done:_ the canonical patch runs _from
  a scene_ in-browser, controls work generically by device, save→load round-trips, and reload is audibly
  glitch-free with health clean.

_Validate:_ ✅ **met.** The canonical patch is built from a serialized scene and played/controlled
**generically by device id** through the worklet; `catalog()` exposes every device's descriptor; the
chassis seam is proven by the multi-node entry's test; a scene **save→load round-trips** and a **reload
hot-swaps glitch-free** under sound; a malformed patch surfaces a legible error, never an audio-thread
panic; the engine touches only its public API and remains serde-free; all prior Epic 1–3 tests stay green
and the full gate passes (`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`, plus
the `wasm-pack build` and `web` Biome/typecheck/build). Verified in-browser by ear (notes, knobs, save/load,
reload).

_Delivered:_ a clean engine→UI seam, with the catalog + scene assembly factored into a new crate and the
real-time host generalized from the pinned patch to a scene it builds, plays, saves, and hot-swaps.

- **New `devices` crate** (the product/content layer, `engine` + serde) — extracted mid-story when the
  catalog/scene logic was recognized as core simulation, not JS glue. Holds the **`Patch` IR** (`scene.rs`),
  the **catalog** (`catalog.rs`), and **`build_patch`** (`build.rs`); consumed by `wasm-bindings` _and_
  available to `harness`. `wasm-bindings` kept to the thin `JsValue` bridge (`catalog()`, `parse_patch`).
  _Engine stays serde-free_ (serde lives in `devices`).
- **Catalog = one `CATALOG` table** of self-contained entries (builder + UI descriptor together — "add
  gear in one place"). Descriptor numeric/domain fields are **derived from a freshly built node** (no
  drift); only labels/units/kinds are authored. Seeded: synth, gain, 3-band EQ, AD, DA, speaker, + the
  multi-node `channel_strip`. Exposed via `catalog()` to JS; hand-written TS mirrors in `web/src/`.
- **Chassis-group seam** — `instantiate(type_id, &mut Graph) -> BuiltDevice` expands a device into 1..N
  nodes + internal edges; **exposed face derived by convention** (open ports + concatenated params).
  Proven by `channel_strip` (two analog gains; the planned gain→EQ was electrically invalid). Retires the
  Epic-1 "one-chassis-many-nodes" deferral. _Minimal engine addition:_ `Graph::add_boxed` (one
  construction site, both insertable + introspectable).
- **`build_patch -> BuiltScene`** assembles a scene (instantiate → remap connections/output → compile →
  resolve control handles by device id), with `BuildError` for every failure (unknown type/device, port
  out of range, `CompileError`) — never a panic. Oracle: **byte-exact output parity** with a hand-built
  engine graph.
- **`RtEngine` → `SceneEngine`** (renamed; retrofit, not rewrite — the proven Epic-3 real-time machinery
  kept). Scene-driven (`new(patch)` / `load_patch(patch)`); **hot-swap** at a block boundary (compile
  off-block in the message handler, install + drop-old + clear stale queues in `render_quantum`); generic
  `set_param`/`note_on`/`note_off` by device id; named setters removed; `BenchEngine` left frozen. Engine
  gained `ParamQueue::clear()` (drop stale handles on swap).
- **Worklet + TS go-live** — `SceneProcessor` builds `SceneEngine(patch)` from `processorOptions {bytes,
patch}`, forwards generic messages, and hot-swaps on a `loadPatch` message. The page owns the
  **versioned JSON save** (`{ schemaVersion, ui, patch }`, TS-side `migrate` scaffold; `ui` a stub until
  4.3) with save/load (localStorage) + a live reload button.
- **Bug found & fixed in-browser:** a hot-swap deep in a session left notes lagging multiple seconds —
  the fresh schedule's event clock restarts at 0 but `SceneEngine.blocks` (the note-stamping clock) wasn't
  reset. One-line fix + a regression test (`note_fires_promptly_after_deep_swap`); the prior swap test had
  masked it (its patch silenced the synth, so _delayed_ looked like _silent_).
- **Known simplifications (not bugs):** scene **param _values_** are applied by the host via the queue
  (glide from default), not baked at build; the old `BuiltScene` drops on the audio thread _between_
  blocks (cheap at small scale); `ParamQueue` cap is a fixed 256; the `ui` save section is reserved;
  build-time-parameterized + runtime-switchable device routing are deferred to Epic 5.1 (recorded). A
  pre-existing Epic-3 limitation surfaced: the worklet's overrun _timing_ is inactive when
  `performance.now()` isn't exposed in `AudioWorkletGlobalScope` (queue-drop counters still live).
- **Concepts captured** in `osku_rust_concepts.md`: serde / serde-wasm-bindgen; move-vs-heap & `Box` for
  unsized; references as borrowing pointers; non-capturing closures → `fn` pointers; block-vs-closure +
  `if let`/`Option::take`.

#### Story 4.2 — Skeuomorphic device panels: controls → params, front/back, power — ✅ **Done**

_Goal:_ the **descriptor → panel renderer** — the data-driven panel system every later device reuses —
plus the skeuomorphic **widget vocabulary** (knobs, faders, switches, jacks, a screen, a VU), introducing
**Svelte 5** to the harness (the Epic-4 stack decision) and standing it up against the _static_ canonical
engine. Two devices (`synth_voice` showcased; one `gain_stage` for renderer-generality + a back-panel jack
story) get real panels: drag-real knobs/faders driving params live, a front/back **CSS flip** to
descriptor-driven jacks, a synth ADSR **screen**, a master-output **VU**, and a real **power** switch
(a control param, never a recompile). Anchors to PROJECT_PLAN §4 (Device/Port domain model surfaced as a
panel) and §7 (UI as a pure consumer of the published engine API), and to the Epic-4 settled decisions
(Svelte 5 + DOM/SVG; descriptor-as-UI-truth; power-as-control; skeuomorphic = genuine interaction +
recognizable layout, not photoreal).

_Watch out:_

- **UI touches only the published API** — `catalog()` descriptors + `set_param`/`note_on`/`note_off`/
  `load_patch`. The engine and the `devices` descriptor gain **no** panel/layout vocabulary; visual layout
  lives entirely in TS. (Engine-stays-UI-free, epic rule.)
- **Power is a _value_ param, so no recompile** (params-vs-structure, Epic 1). Toggling is instant and
  **de-clicked** by the existing `Smoother` ramp — never a graph edit. Adding `powered` must stay an
  **identity at the default (`1.0`)** so every existing Epic 1–3 analog/DSP test still holds.
- **Hot-path contract unchanged.** The `powered` gate runs _in_ `process` (a smoothed multiply) — must
  stay zero-alloc, panic-free, denormal-flushed; all new fallibility (panel build, catalog fetch) is cold.
- **Do not pull Story 4.5 forward.** No node→host readout lane, no per-device probe, no scope/spectrum.
  The only live signal a meter may read in 4.2 is the **already-exposed master-output buffer**
  (`out_ptr`/`out_len`) — see the 4.5 "meter is a node" note. The synth screen draws the ADSR curve from
  param _values_ (pure TS), not from a tap.
- **Static engine only** — no graph mutation (→ 4.4) and no spatial world / app shell (→ 4.3). Jacks
  render but are **display-only**; panels just stack.
- **Svelte is additive** on the existing Vite/TS harness — repackage `main.ts`'s transport/keyboard/MIDI
  logic, **don't rebuild** the worklet, the scene store, or the engine bring-up.

_Design notes (settled at planning):_

- **Metering is deferred (the headline decision).** A VU meter is a **node** (voltage-native: bridging
  `InputZ`, ~300 ms ballistics, `0 VU ≙ +4 dBu ≙ 1.228 V RMS`) computing a scalar reading in-engine — _not_
  a getter retrofitted onto every node — and surfacing it needs a **new node→host scalar readout
  side-channel** the engine doesn't have today. Both land in **Story 4.5** (recorded in its sketch). 4.2
  therefore ships **no engine metering surface**: its panel VU reads the **master-output buffer** (the host
  monitor level — honest, but not a simulated meter device) and repoints onto a `VuMeter` node's readout in
  4.5. _Rejected:_ building the readout lane now (overlaps 4.5, adds engine surface to a UI story);
  retrofitting a VU getter onto every node (wrong model — measurement belongs in a meter node).
- **Power = real per-node `powered` control param**, added to `SynthVoice` and `GainStage`: a Switch-kind
  param, range `[0, 1]`, default `1.0`, whose **smoothed** value gates the node's output (powered-off ⇒
  output × 0 ⇒ silence, with the smoother's ramp de-clicking the transition — the "instant, glitch-free
  standby" the Epic decision asks for). _Rejected for now:_ a **generic framework-level** power gate (like
  smoothing-written-once) — cleaner long-term and the natural future refactor, but it touches the node/param
  framework broadly, beyond a UI story; doing it per-node keeps 4.2 contained (known simplification, not a
  bug). _Rejected:_ a UI-only cosmetic switch (contradicts the settled power-as-control decision). Ripple:
  `catalog_aligns_with_exposed_face` forces the catalog UI metadata for `synth_voice`, `gain_stage`, **and**
  `channel_strip` (two `GainStage`s) to list the new switch param(s) — bookkeeping, expected.
- **Panel layout is TS-side auto-layout, no descriptor fields.** The generic renderer lays out a panel from
  the descriptor: param `kind` (`knob`/`fader`/`switch`) picks the widget; port `direction`+`kind` style and
  place the back-panel jacks. Per-type **embellishments** (the synth's ADSR screen) are opt-in TS components,
  not descriptor data. _Rejected:_ layout-hint fields (positions/groupings) on the Rust `DeviceDescriptor` —
  couples the engine/content layer to visual layout, against keeping `devices` lean and the renderer the home
  of UI truth.
- **Second device = `gain_stage`, not `channel_strip`.** A multi-node device's chassis-ness is **invisible**
  to the descriptor-driven renderer (4.1 flattens the exposed face), so `channel_strip` adds no rendering
  proof — while its two internal gains would force the panel's single power switch to coalesce two `powered`
  params. `gain_stage` is a clean single-node panel (one gain knob + one power switch + in/out jacks) and
  still proves the renderer is generic across device types. The **default scene** gains a unity gain stage:
  `synth → gain_stage → ad → da → spk` (gain `1.0` = passthrough, so audio is unchanged).
- **Interaction model:** pointer-drag widgets (vertical drag for knobs, along-axis for faders), **Shift =
  fine** (reduced sensitivity), **double-click = reset to the descriptor default**, with a live value readout
  in the param's unit. Functional skeuomorphism (SVG + CSS), not photoreal — branding/skins/onboarding stay
  deferred (project non-goal).
- **`catalog()` reaches the main thread via the worklet's `ready` message.** The wasm instance lives in the
  worklet (`--target no-modules`); rather than instantiate a second copy on the main thread, the processor
  calls `catalog()` in its constructor and includes the descriptors in `ready`. The page hands them to the
  Svelte app. (Hand-written TS mirrors in `web/src/catalog.ts` already type them.)

- **Task 4.2.1 — `powered` control param on `SynthVoice` + `GainStage` (engine + catalog).** Add a
  Switch-kind `powered` `ParamDecl` (`[0,1]`, default `1.0`) to both nodes; gate each node's output by the
  smoothed `powered` value in `process` (zero-alloc, denormal-flushed). Update the `synth_voice`,
  `gain_stage`, and `channel_strip` catalog entries' UI metadata to expose the new switch param(s).
  _Done:_ engine tests assert powered→0 settles to silence and powered→1 is normal on both nodes; the
  default `1.0` leaves every prior engine test green; `catalog_aligns_with_exposed_face` +
  `descriptors_carry_engine_truth` pass with the added param. (Oracle: behavioral — peak(powered 0) ≈ 0 vs
  peak(powered 1) > 0 for the same input/note.)
- **Task 4.2.2 — Svelte 5 in the harness + transport repackage + catalog ingress.** Add Svelte 5 +
  `@sveltejs/vite-plugin-svelte` (one dependency) to `web/`; wire `vite.config.ts`, `tsconfig`, and Biome
  for `.svelte`. Mount a Svelte root replacing the hardcoded `#controls` block; move `main.ts`'s
  transport/keyboard/MIDI/scene-button logic into a Svelte-consumable module/store (engine bring-up, worklet,
  and `scene-store` untouched). Have the worklet post `catalog()` descriptors in `ready`; expose them to the
  app. _Done:_ the existing synth controls work, now rendered by Svelte and **driven by the fetched
  descriptor** (not hardcoded ids); `npm run check`, `npm run typecheck`, `npm run build` green; in-browser
  parity with current behavior (notes, knobs, save/load/reload, health/latency).
- **Task 4.2.3 — Descriptor-driven panel renderer + control widgets.** The generic `Panel` (front face)
  auto-laid-out from a descriptor, with `Knob` / `Fader` / `Switch` widgets chosen by param `kind` —
  pointer-drag + Shift-fine + double-click-reset + live unit readout — each bound to `set_param` _and_ the
  scene (persists on save). Render a panel per scene device (synth + gain_stage operable; zero-param devices
  show only power + jacks); add `gain_stage` to `defaultScene`. Power switch drives the `powered` param.
  _Done:_ in-browser, the synth and gain_stage panels operate the live engine (knobs/faders/power change the
  sound), values persist across save/load, and a low `powered` audibly silences the device.
- **Task 4.2.4 — Back panel (jacks) + front/back flip.** The back face rendered from the descriptor's
  ports: `Jack` widgets styled by port `kind`/`domain`, inputs and outputs laid out and labeled; a per-panel
  CSS 3-D **flip** affordance. Jacks are **display-only** (patching → 4.4). _Done:_ each panel flips
  front↔back; the back shows correctly-styled, labeled jacks for every descriptor port; verified in-browser.
- **Task 4.2.5 — Synth ADSR screen + master-output VU.** A synth-specific `Screen` embellishment (a small
  `<canvas>` drawing the envelope from the live `level`/A/D/S/R param values, updating as knobs turn); a
  `Vu` widget driven by a **throttled level message** the worklet computes from the already-exposed output
  buffer (peak/RMS over recent quanta — **no engine change**). _Done:_ the ADSR screen tracks the synth
  knobs; the master VU moves with output level and rests at idle; verified in-browser by eye.

_Validate:_ ✅ **met.** descriptor-driven panels for `synth_voice` + `gain_stage` operate the live static
engine (knobs/faders change the sound and persist to the scene); each panel **flips** front↔back to
descriptor-driven, correctly-styled (display-only) jacks; the synth **ADSR screen** tracks its knobs and the
**master-output VU** moves with output; **power** is a real `powered` param (off ⇒ silence, de-clicked, no
recompile); **Svelte 5** stands up the renderer on the untouched worklet/transport; the engine gains only
the `powered` params (no probe/readout lane — deferred to 4.5) and stays UI-free; the full Rust gate
(`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`) plus `wasm-pack build` and the
`web` `check`/`typecheck`/`build` pass; verified in-browser by ear and eye.

_Delivered:_ the data-driven skeuomorphic panel system + the widget vocabulary every later device reuses,
on a Svelte 5 harness, with two device panels operating the live engine and the rest rendered generically.

- **Svelte 5 introduced** (the Epic-4 stack decision realized): `@sveltejs/vite-plugin-svelte` + runes, a
  slim `index.html`/`main.ts` mount. Transport (engine/worklet bring-up, `send`, keyboard, Web MIDI,
  latency/health formatting) extracted to `web/src/engine.ts`; `App.svelte` owns the reactive scene/UI
  state. The worklet, scene-store, and engine bring-up were **repackaged, not rebuilt**.
- **Descriptor → panel renderer** (`widgets/Panel.svelte`): laid out generically from a device's descriptor
  — a control widget per param chosen by `kind`, plus a back face of I/O jacks; zero-param devices show
  "no front-panel controls". **Widget vocabulary** (SVG + CSS, functional-not-photoreal): `Knob` (270°
  rotary), `Fader`, `Switch` (LED power), `Jack` (color by connector `kind`, shape by carrier `domain`),
  `Screen`, `Vu`, with a shared pointer-drag (`drag.ts`: vertical drag, Shift = fine, double-click = reset,
  arrow-key nudge). Front/back is a **CSS 3-D flip** (grid-stack trick → no manual height sync), kept
  self-contained so 4.3 can gate it behind a physical-clearance action.
- **`powered` control param** on `SynthVoice` + `GainStage` (engine): a Switch-kind param `[0,1]` default
  `1.0`, whose smoothed value gates the node's output (off ⇒ silence, de-clicked; never a recompile —
  params-vs-structure). Default `1.0` is identity, so all prior analog tests held; catalog entries
  (`synth_voice`/`gain_stage`/`channel_strip`) gained the switch. _Generic framework-level power deferred_
  (per-node for now — known simplification, not a bug).
- **Catalog ingress**: the worklet calls `wasm_bindgen.catalog()` (where the wasm instance lives) and ships
  descriptors in its `ready` message; the page renders panels from them. `defaultScene` gained a unity
  `gain_stage` (`synth → gain → ad → da → spk`) for a second controllable device.
- **Metering deferred to 4.5 (as planned):** the master VU reads the **already-exposed output buffer** (a
  throttled peak the worklet posts ~47×/s) — the host monitor level, _not_ a simulated meter device; no
  engine probe/readout lane added. The synth screen draws the ADSR contour from param _values_, not a tap.
- **Two detours folded in:** (1) a **monitor volume** — a Web Audio `GainNode` between the worklet and
  `destination`, **outside the simulation** (doesn't touch the modeled signal or the meter), defaulting to
  25% and persisted under its own `localStorage` key (not the scene). (2) **`SynthVoice::LEVEL` range fixed**
  to `0–1.5 V` (default `1.0`; was `0–100 V`, which left the usable range in the fader's bottom 1.5%); floor
  kept at 0 so it still fades to silence. Both surfaced from the engine "runs hot" symptom.
- **Bugs found & fixed:** Svelte 5 `$state` wraps the scene in a Proxy that `postMessage` can't
  structured-clone (`DataCloneError`) — fixed with `$state.snapshot(patch)` at every worklet boundary
  (`plainPatch()`). And a long Biome/Svelte tooling untangling: **`biome.json` is strict JSON (no comments)**
  — comments silently broke config parsing → default rules linting `.svelte` and _corrupting_ files on save;
  resolved by a single comment-free **root** `biome.json` (the editor LSP loads the workspace-root config),
  with `.svelte` excluded and owned by `svelte-check` + the Svelte extension (prettier via `.prettierrc`).
- **Known simplifications (not bugs):** jacks are **display-only** (drag-to-connect → 4.4); the meter is the
  host monitor level, not a voltage-native `VuMeter` node + node→host readout lane (→ 4.5); panel layout is
  TS auto-layout from param/port `kind` (no descriptor layout fields); **physical dimensions are not yet on
  the descriptor** (the spatial-sim content → 4.3, per the spatial-sim settled decision in this Epic).

#### Story 4.3 — The spatial world: spaces, racks, placement, catalog browsing — ✅ **Done**

_Goal:_ turn the flat panel rack into a **game-like spatial studio** — the Svelte app shell + an isolated
world layer where you pan/zoom across a **space** rendered as a **front rack-elevation**, place and move
gear in **real rack-U slots** and on a desk, switch between multiple spaces, and **browse the catalog to
add/remove gear** — the gesture that exercises the 4.1 recompile/hot-swap path live. Anchors to
PROJECT_PLAN §7 (skeuomorphic panels as the primary paradigm) and §9 Stage 4 (build and operate a small
studio through the UI). The novel parts — the spatial model, placement legality, projection — are
**framework-agnostic data + math** (the epic's "spatial sim is a data/constraint model, not a rendering
problem" decision); the engine learns nothing about rooms, racks, or position.

_Watch out:_

- **Engine + `patch` stay free of any spatial concept.** No rooms/racks/positions in the engine or the
  runnable `Patch`. Placement, spaces, container membership, and clearance are **UI scene state** (the TS
  `ui` section) only — "spaces are a UI concept."
- **Model in 3-D, render in 2-D — never store per-view 2-D positions** (the drift trap). Store a single
  coordinate truth per device (position `(x,y,z)` + facing; footprint comes from the descriptor) and
  derive the front-elevation screen rect by **projection**. One view ships now; the projection stays pure
  so top/side views are cheap later.
- **Dimensions are content, not UI-invented.** A device's rack-U height / footprint lives on the
  **`devices` catalog descriptor** (engine-adjacent, native-testable), mirrored into TS — _not_ re-typed
  in the UI layer where it would drift.
- **Only structural edits recompile.** Add/remove device (and the connections it drags along) mutates the
  `patch` → `loadPatch` hot-swap (the proven 4.1 path). Placement, move, flip, space-switch, and clearance
  are **pure UI — no recompile.** Add/remove are discrete gestures, so swap on commit; no continuous
  debounce is needed (resolves the sketch's "add/remove debouncing" open question).
- **Keep the world renderer behind a thin interface** so a future swap to a WebGL canvas touches only that
  layer (the standing escape hatch) — but **do not build WebGL**; DOM/SVG over a CSS-transform pan/zoom
  surface is right for tens-to-low-hundreds of rectangles at studio scale.
- **Skeuomorphic = genuine interaction + recognizable layout, not photoreal** (epic rule). Reuse the 4.2
  `Panel`/`Knob`/`Fader`/`Switch`/`Jack` widgets; don't paint textures.
- _Scope guard:_ this is the spatial-sim home — resist pulling cables/snakes (4.4) or probes/meters (4.5)
  forward, and resist the deferred reach/multi-view work below.

_Design notes (settled at planning):_

- **View model — store 3-D truth, render one view (front rack-elevation) now.** The full 3-D coordinate
  truth is stored, but only the front-elevation projection is rendered this Story; the projection is a
  **pure, unit-tested function** so adding top/side/front views later is cheap. _Rejected: multiple synced
  projections now_ — a second renderer + view-switching on top of placement + spaces + catalog overruns
  one Story. Front elevation (over a top-down floor plan) because it reuses the 4.2 panel + flip directly
  and is the most "studio rack" feel; a floor plan would turn panels into rectangles and need a separate
  "operate" view.
- **Reach deferred; clearance is a simple per-device boolean.** 4.3 ships placement + spaces + add/remove
  - a **"pulled-out" clearance state** that gates back-panel access (the back-flip from 4.2 is lifted out
    of `Panel` and gated on clearance — "pull the unit / roll the rack off the wall"). _Rejected: full
    operator-position + reach-radius model now_ — a whole interaction subsystem; it lands in a later Story
    and the stored 3-D truth keeps it cheap. Clearance is a boolean, not a position/reach computation.
- **Rack model — real rack-U slots.** A rackmount device carries a **U-height** (standardized 19" width);
  a rack has **N U-slots**; placement legality is **slot occupancy** (a device's U-run must be free).
  Desktop gear carries a **footprint box** and places freely on the desk surface with **AABB no-overlap**.
  This is the unit-testable spatial core the epic decision calls for. _Rejected: free 2-D placement only_
  — defers the most distinctive constraint.
- **Where the model lives.** _Dimensions_ → the Rust `DeviceDescriptor` (a `formFactor` + size:
  rack-U height for rackmount, a footprint box for desktop), authored per `CatalogEntry`, mirrored in
  `catalog.ts`. _Placement / spaces / clearance / view (pan-zoom) state_ → the TS scene `ui` section. The
  engine `patch` projection is unchanged.
- **`SceneUi` is reshaped freely — no migration / back-compat.** localStorage is disposable (no real
  scenes are stored anywhere), so the old `placements?: {x,y,space?}` stub is **replaced** by the 3-D
  placement model (position + facing + container membership `{rack, uSlot}` | `{desk, pos}` + space id +
  clearance/flip flags) and the `SCHEMA_VERSION`/`migrate` scaffold is dropped or reset — no vN→vN+1 step.
- **World-layer interface.** A thin `WorldView` boundary fed by the **pure layout/projection model** and
  emitting **placement intents** (move-to-slot, place-on-desk, switch-space), so the DOM/SVG renderer is
  the only thing a future WebGL swap replaces. The spatial logic (projection, AABB, U-slot legality) is a
  rendering-free module with its own Vitest tests — the "tests are the oracle" temperament applied to the UI.

- **Task 4.3.1 — Device dimensions on the catalog.** Add `formFactor` + size fields to the Rust
  `DeviceDescriptor` (rackmount → U-height; desktop → footprint box), authored per `CatalogEntry`, derived
  where engine truth allows; mirror in `catalog.ts`. _Done/validate:_ native test that every entry carries
  a sane form factor + size and serializes camelCase (extends `catalog_serializes_with_expected_types`);
  TS mirror compiles.
- **Task 4.3.2 — The pure spatial model + logic (TS, unit-tested).** A rendering-free module: 3-D
  coordinate + footprint types, the **front-elevation projection** (3-D → screen rect), **AABB overlap**,
  and **rack U-slot occupancy + placement legality** (can a device of U-height H occupy rack R from slot
  S?). The `web` project has **no test runner yet** — stand up **Vitest** first (a dev-dep install, _the
  user runs_ `npm install -D vitest`, plus a `test` script + a CI step mirroring `typecheck`/`check`).
  _Done/validate:_ Vitest unit tests on projection, AABB, and slot-legality (including illegal /
  overlapping cases); the module imports no DOM/Svelte.
- **Task 4.3.3 — Scene `ui` placement state + store.** Replace `SceneUi` with the 3-D placement model
  (position + facing + container membership + space id + clearance/flip flags) and update `scene-store`
  (default scene seeds placements; save/load persists; **no migration**). The engine `patch` projection
  stays untouched. _Done/validate:_ a scene round-trips placement through save/load; the worklet still
  receives only `patch`; existing scene tests stay green.
- **Task 4.3.4 — World layer + app shell (pan/zoom, one space, front elevation).** Replace the flat
  `.rack` with a `WorldView` behind the thin interface: a CSS-transform **pan/zoom** surface rendering the
  current space's gear from placement via the 4.3.2 projection, showing front panels (reuse `Panel`);
  **drag a device** to a new placement, legality-checked. _Done/validate:_ you can pan/zoom and move gear;
  placement persists; illegal moves are rejected. Verified in-browser.
- **Task 4.3.5 — Racks & containers + clearance-gated back access.** Render racks as **U-slot columns**;
  place/move devices into rack slots and onto the desk; **open/close** (expand/collapse) a container; lift
  the back-flip out of `Panel` and **gate it on a per-device clearance** ("pull out" / "roll off wall").
  _Done/validate:_ gear occupies real U-slots (overlaps rejected); a unit's back is reachable only after
  the clearance action. Verified in-browser.
- **Task 4.3.6 — Multiple spaces + switching.** Several spaces (e.g. live room / control room); each
  device belongs to one; switching a space switches the rendered/interactable set. _Done/validate:_
  create/switch spaces; gear appears only in its space; membership persists.
- **Task 4.3.7 — Catalog browser + add/remove gear (the recompile exercise).** Browse the fetched catalog
  descriptors; **add** a device (new id + default placement → mutate `patch.devices` → `loadPatch`) and
  **remove** one (drop from `patch.devices`/`connections`/placement → `loadPatch`). _Done/validate:_
  add/remove through the UI hot-swaps the engine **glitch-free under sound** with the health line clean —
  the 4.1 recompile path proven on user-driven add/remove. Verified in-browser by ear.

_Validate:_ ✅ **met.** Through the UI, in a pan/zoom **front-elevation** world: gear is placed and moved
in **real rack-U slots** (drag-snap to the nearest free slot) and free-standing on the floor, with illegal
(overlapping / no-free-slot) drops rejected; rooms are **created and switched** (the default ships one room

- an "add space" control, and gear/racks move between rooms); a unit's back is reachable **only after** the
  pull-out clearance action; the catalog palette **adds and removes gear**, hot-swapping the engine via the
  4.1 `loadPatch` recompile path. The spatial logic (projection, AABB, U-slot legality, nearest-free-slot) is
  **Vitest-unit-tested**; device dimensions are **catalog content** with native tests; the engine and `patch`
  stay free of any rooms/racks/positions. Full gate green (`cargo fmt --check && cargo lint && cargo test &&
cargo wasm && cargo docs`, plus `web` Vitest/Biome/typecheck/build). Verified in-browser.

_Delivered:_ a game-like spatial studio on the Svelte harness — a pan/zoom front-elevation world where gear
lives at real coordinates, mounts in rack U-slots, and moves between rooms, with add/remove driving the
engine's hot-swap. The engine and runnable `patch` gained **nothing** (no rooms/racks/positions) — all
spatial state is UI-only, and add/remove rides the existing 4.1 `loadPatch`/`catalog()` surface, so **no
Rust changed** beyond the catalog dimensions.

- **Device dimensions are catalog content.** `DeviceDescriptor` gained a `FormFactor` (`Rackmount { rack_units }`
  | `Desktop { width/height/depth_mm }`), authored per `CATALOG` entry, mirrored in `catalog.ts`; native
  tests pin sane values + the tagged camelCase wire shape. The UI derives a device's box from it.
- **Pure spatial module (`web/src/spatial.ts`), Vitest-tested.** 3-D coordinate/footprint types, the
  `project(pos, size, view)` **seam** (front renders now; top/side exist so Story 4.6 is a few lines),
  `footprint`, `rectsOverlap` (AABB), and the rack model (`fitsInRack` / `canPlaceInRack` /
  `nearestFreeSlot`). Rendering-free — the "tests are the oracle" temperament applied to the UI.
- **Scene `ui` reshaped (`scene-store.ts`, schema v4, no migration).** `SceneUi = { spaces, racks, placements }`;
  a `Placement` carries `position` (3-D truth) + optional rack mount + `facing` + `pulledOut`. localStorage
  is disposable, so the shape was replaced outright (parse discards any other version). Pure
  `serializeScene`/`parseScene` are unit-tested for round-trip + version-discard.
- **Isolated world layer (`WorldView.svelte`)** behind a thin prop contract (`items` in world-mm + an `item`
  snippet + a generic `controls` snippet + `onMoveTo`/`canPlace`/`fitKey`) — the standing WebGL escape hatch.
  CSS-transform **pan/zoom** (cursor-anchored, scroll-distance-proportional), **fit-to-content** framing that
  re-frames on room switch (`fitKey`) and backs off once the user takes over, per-device **drag grip**
  (so operating a control never drags or pans), and a red-outline illegal-drop preview.
- **App wiring (`App.svelte`):** front-elevation projection of placements; **drag-snap** rackmount gear into
  the nearest free U-slot (or out to the floor); **movable racks** rendered as U-slot frames; **clearance-gated
  back access** (`Panel`'s flip is now a controlled prop, gated behind pull-out); **multiple rooms** with tab
  switching + add + per-item room selectors; a **catalog palette** whose add/remove mutate the `patch` and
  hot-swap the engine (re-pushing params after each swap).
- **Deviations from the plan (not bugs):** rack **collapse/expand was built then removed** — real racks don't
  collapse (user call); **"desk" is the free floor**, not a distinct desk container (deferred); the default
  scene ships **one room** (add more via the control) rather than two; **reach + multi-view projections stay
  deferred to Story 4.6** as planned (the 3-D coordinate truth is stored now so they're cheap).
- **Known limitations (recorded):** the computer keyboard is wired once to the **initial** synth — removing it
  or adding a second doesn't re-route input; dragging a **rack** moves its frame live and its mounted gear
  repositions on drop; racks reposition **freely** (no rack-vs-rack overlap rejection); "pulled out" has **no
  z-offset** in the front elevation (z isn't visible head-on — it only unlocks the flip; the visible
  pull-forward is a Story 4.6 top-view concern).
- **Tooling:** stood up **Vitest** in `web/` (the project is pnpm-managed; `CLAUDE.md` corrected from npm).
  No web CI job exists yet, so `web` typecheck/Biome/test/build aren't gated on PRs — a candidate follow-up.

#### Story 4.4 — Patch cables & snakes → live graph mutation — ✅ **Done**

_Goal:_ make the studio **patchable** — drag a cable between two devices' back-panel jacks and the
engine rewires live: connect/disconnect mutates `patch.connections` → the proven 4.1 `loadPatch`
recompile/hot-swap, glitch-free under sound. A chosen **cable type** carries real R·C so the modeled
loading loss + treble rolloff are physically correct — **verified numerically** (per §9, cable loss is a
hand-calc oracle, not an ear test); with realistic cables into realistic impedances the degradation is
**inaudible by design**, which is the point (a good signal chain doesn't degrade audibly even though the
system models it). And **cross-space connections** render as portal endpoints (the snakes MVP). Anchors
to PROJECT_PLAN §4 (the Port/Device/Graph domain model surfaced as
draggable jacks + cables), §7 (UI as a pure consumer — the engine learns nothing new), and §9 Stage 4
(build and operate a small studio through the UI). This is the "patching feels natural" payoff and the
**swap-under-load proof** — re-measure the audio-thread compile cost at realistic graph size.

_Watch out:_

- **The recompile/swap runs on the audio thread** (engine-in-worklet; a `Schedule` can't cross realms) —
  connect/disconnect is the _same_ 4.1 `loadPatch` path `addDevice`/`removeDevice` already use. Edits are
  rare gestures, so the off-block compile is acceptable, but **re-measure** it at a realistic graph size;
  a long compile delays the next `process()` ⇒ a glitch. Keep `compile` off the per-block path.
- **Fan-in is illegal in the engine** — an input port accepts exactly **one** incoming edge (the engine
  rejects "two edges into one input" at compile; fan-_out_ from an output is fine and solves as parallel
  loading). The UI must enforce this **before** compile (dropping onto an occupied input _replaces_ its
  connection), not let a mid-patch `compile` fail.
- **Cables only affect analog edges.** The engine's cable one-pole + loading divider ride **analog**
  edges only; a digital/event route ignores any `CableSpec`. So offer cable physics on **analog↔analog**
  connections only — a "cable" on a digital link would be a lie (no rolloff there).
- **Don't re-derive the cable physics in TS** (epic rule: engine stays the home of volts-and-converters
  realism). The rolloff/loss is the engine's _already-tested_ concern (Epic 1.2 `Cable`/`OnePole`/
  `divider_gain`); 4.4 only authors realistic R·C **content** and wires it onto the edge.
- **Engine + `patch` gain nothing structural.** Connections already live in the `Patch` IR and
  `build_patch` already remaps them, bakes cables, validates domains, and rejects cycles. No `engine`
  crate change; the only Rust touch is the **cable catalog content** in `devices`.
- **Keep the world layer thin.** Cables are parent-owned and drawn through a surface-space overlay; the
  `WorldView` still knows only about positioned boxes + pointer mechanics (no "cable"/"patch" concept) —
  the WebGL escape hatch stays intact.
- _Scope guard:_ this is the cabling story — resist pulling probes/meters (4.5) or the top-view / reach
  work (4.6) forward; snakes stay at the **portal-endpoint MVP**, not a full bundle-routing subsystem.

_Design notes (settled at planning):_

- **Patching UX — per-device flip, no new view.** Jacks live on the **back** panel (4.2), reachable only
  when a device is **pulled-out + flipped** (the 4.3 clearance gate). Since `facing` is per-device, two
  backs can face the operator at once, so you patch by pulling out + flipping both endpoints and dragging
  jack→jack. _Rejected: a room-wide "rear view" toggle_ (flip every unit to its back at once) — more
  realistic ("walk behind the rack") and easier to patch, but it's effectively a second projection that
  overlaps Story 4.6's view-switching; defer it there if the per-device flow proves fiddly. _Rejected:
  front-panel patch points_ — abandons the back-panel realism settled in 4.2. **Known simplification (not
  a bug):** a cable to a device whose back isn't currently shown (front-facing / pushed-in) anchors to its
  chassis edge rather than a precise jack, so the connection is never visually lost.
- **Cross-space connections = portal endpoints (snakes MVP).** Only one space renders at a time, so a
  connection whose endpoints sit in different rooms **cannot** draw as a continuous bezier; it renders as a
  labeled stub (`→ Live Room`) at each end. A **"snake"** is a UI label bundling several such cross-space
  mono cables — the engine sees **plain mono connections**; portals + bundles are UI-only. _Rejected:
  full snake create/break/expand routing UX_ (largest scope for one story); _rejected: same-space cables
  only_ (the epic exit needs patching across ≥2 spaces). Satisfies the exit without a second simultaneous
  view.
- **Pickable cable types now; cable catalog is Rust `devices` content.** A connection carries a chosen
  cable → `CableSpec { resistance_ohms, capacitance_farads }` (the field already on `Connection`), so the
  engine's loading divider + treble rolloff become audible. The **cable catalog** (named presets:
  connector kind + R·C, optionally length-scaled) lives in the **`devices` crate** with a native hand-calc
  oracle and is exposed to the UI alongside the device catalog. _Why Rust, not TS presets:_ R·C is
  physical **content** as intrinsic as a device's impedance — authoring it in TS re-invents content the
  content layer owns and risks drift (the exact rationale 4.3 used for device dimensions). _Rejected:
  ideal wires only_ — leaves the cable-physics payoff on the table, which the engine already supports for
  free.
- **The cable effect is modeled-but-inaudible here, and that is correct — not a shortfall.** Cable rolloff
  needs a **high-impedance source** to be audible (`f_c = 1/(2π·R_thev·C)`, `R_thev = (Zout+R_cable) ∥ Zin`
  — dominated by the smaller side). Every source in the current catalog is low-Z (synth 1 Ω, gain/DA
  150 Ω), so with realistic R·C the corner sits far above 20 kHz and the series-R level drop is negligible:
  a clean chain **does not degrade audibly even though the system models it faithfully**. This _is_ the
  design intent, and it matches §9 ("cable loss… can't be heard reliably, so [it's] asserted numerically").
  So 4.4 validates the physics by **hand-calc oracle** (numeric), the chosen cable **rides the edge
  correctly**, and the effect becomes **visible** when 4.5's analog-domain readouts land and **audible**
  when **Epic 5** adds high-Z instrument sources (a passive DI / guitar-level device). No by-ear gate in
  4.4. _Rejected: exaggerated (unrealistic) cable C to force audibility_ — dishonest, against the
  realism ethos; _rejected: adding a high-Z source now_ — a device-catalog change beyond this cabling
  story, and Epic 5's natural home.
- **Endpoints are DOM-measured; legality + geometry are a pure module.** Jack screen positions come from
  the panel's **flexbox** layout, so cable endpoints are discovered by DOM measurement
  (`getBoundingClientRect` → world-mm via the `WorldView` transform), **not** computed analytically. The
  new pure `connections.ts` (peer to `spatial.ts`, rendering-free, **Vitest-tested**) owns the parts that
  _can_ be pure: the **legality predicate** (output→input, same carrier domain, fan-in rejected, no
  self-loop, cable only on analog) and the **bezier geometry given two endpoints** + point-near-curve
  **hit-testing** (for click-to-delete). Endpoint discovery is the DOM-coupled part, isolated in Svelte —
  the "tests are the oracle for the UI" temperament applied where it fits.
- **Legality feedback is pre-compile; cycles fall back to `BuildError`.** Direction, domain, and fan-in
  are all in the descriptor / scene, so the UI shows live green/red feedback **before** `loadPatch`. A
  cycle (the one illegality the descriptor can't see locally) is caught by `compile` → surfaces as the
  legible `BuildError` on the status line and the cable **snaps back** — no broken patch, no audio-thread
  panic.

*Known limitation — connector *kind* is not enforced (TODO, follow-up):* connection legality currently
checks only the **carrier domain** (analog / digital / events), so **any analog jack accepts any other**
— a TRS output patches into an XLR input, a speaker binding-post into a line jack, etc. In the real world
connectors are physically specific. The `kind` (mic / line / instrument / speaker / digital / midi)
already rides every `PortDescriptor` and `CableType`, so the fix is additive: extend `evaluateConnection`
(and the cable picker) with a **connector-compatibility rule** — either exact-kind match or a small
compatibility table (e.g. TRS↔TRS, XLR↔XLR, with sanctioned adapters), and pick the cable's connectors
from the endpoints. This is a UI/legality refinement (no engine change — the engine validates by domain);
schedule it as a 4.4 follow-up or a small Epic-5 wiring-realism item.

- **Task 4.4.1 — Cable catalog (content) + UI exposure + hand-calc oracle.** A `CABLES` table in `devices`
  of named cable presets (`type_id`, label, connector `kind`, series R + shunt C; the seam for
  length-scaling noted but a fixed nominal length is fine), exposed to the UI alongside the device catalog
  (an extra field on the `ready` handshake / a small bridge), mirrored in a TS `CableType`. _Done/validate:_
  a **hand-calc oracle** (a `devices`/`harness` test, at the `Cable`/electrical level like the engine's own
  cable tests): a specific preset's R·C into a **representative high-Z source** yields the hand-computed
  corner `f_c` + divider loss (calc in a comment), **and** the same preset into the catalog's low-Z synth
  source puts `f_c` far above 20 kHz (the modeled-but-inaudible intent, also hand-checked); plus a native
  test that every preset has sane R·C and serializes camelCase; TS mirror compiles.
- **Task 4.4.2 — Pure `connections.ts` module (TS, Vitest).** A rendering-free module: the
  **legality predicate** (output→input; same carrier `domain`; reject fan-in into an already-driven input;
  reject self-loop; cable allowed only on analog↔analog), the **bezier path** given two endpoint points
  (a natural hanging-cable curve), and **hit-testing** (point-near-bezier, for click-to-delete).
  _Done/validate:_ Vitest unit tests on legality (incl. wrong-direction, domain-mismatch, fan-in, self-loop
  cases), bezier control-point math, and hit-test hits/misses; the module imports no DOM/Svelte.
- **Task 4.4.3 — Cable overlay + jack world-positions + render existing connections.** Extend `WorldView`
  with a thin surface-space **`overlay` snippet** so the parent draws cables in world coordinates; make
  `Jack` report its world position (DOM-measured through the pan/zoom transform); render the current
  scene's `patch.connections` as beziers between the back-panel jacks of pulled-out/flipped devices (a
  front/pushed-in endpoint anchors to the chassis edge). _Done/validate:_ the default scene's connections
  draw as cables that stay aligned through pan/zoom and device moves; verified in-browser.
- **Task 4.4.4 — Drag-to-connect + disconnect → hot-swap.** Pointer-down on a jack starts a rubber-band
  cable; live **green/red legality feedback** via 4.4.2; a legal drop commits `patch.connections` →
  `hotSwap()` (the 4.1 path); **click a cable to delete** → hot-swap; dropping on an occupied input
  **replaces** its connection; a cycle/`BuildError` surfaces on the status line and the cable snaps back.
  **Re-measure the audio-thread compile cost** at a realistic graph size (the swap-under-load proof).
  _Done/validate:_ connect/disconnect through the UI hot-swaps the engine **glitch-free under sound** with
  the health line clean; illegal drops are rejected with feedback. Verified in-browser by ear.
- **Task 4.4.5 — Cable-type picker + edge wiring.** On an analog connect, attach a cable from the
  4.4.1 catalog (sensible default), changeable by clicking the cable; digital/event connections stay ideal
  (no picker). The cable's R·C rides the edge through `build_patch`. _Done/validate:_ the chosen cable
  **rides the edge** (its R·C reaches the compiled schedule — asserted via the 4.4.1 oracle direction, not
  by ear: realistic cables into the catalog's low-Z sources are inaudible **by design**, §9), the choice
  **persists** in the scene across save/load, and digital links show **no cable affordance**. The audible
  payoff waits on Epic 5's high-Z sources / 4.5's readouts. Verified in-browser (picker + persistence,
  glitch-free swap).
- **Task 4.4.6 — Cross-space connections via portal endpoints (snakes MVP).** A connection whose endpoints
  are in different spaces renders as a labeled **portal stub** (`→ Live Room`) at each end instead of a
  continuous cable; a basic **snake** label bundles several such cross-space cables. The engine sees plain
  mono connections throughout. _Done/validate:_ a device in room A patched to one in room B hot-swaps and
  sounds; the connection shows as portals in each room and survives save/load; verified in-browser.

_Validate:_ ✅ **met.** Through the UI, in the pan/zoom front-elevation world: **drag-to-connect** between
two flipped-to-back devices' jacks wires the engine live via `loadPatch`, and **clicking a cable** selects
it into an inspector (change cable type / disconnect) — both **glitch-free under sound** with the health
line clean; **illegal drops** (wrong direction, domain mismatch, fan-in into an occupied input, self-loop,
feedback cycle) are rejected with live green/red feedback (cycle detection is client-side, so no bad patch
ever compiles); a **chosen cable type** rides the analog edge with correct R·C (**hand-calc-tested** in
`devices`; inaudible by design into the current low-Z sources per §9 — the audible payoff is Epic 5) while
digital links stay ideal; a **cross-space** connection renders as **portal endpoints** in each room and
hot-swaps; the pure `connections.ts` (legality, cycle, bezier, hit-test, cable-spec mapping) is
**Vitest-tested**; the `engine` crate and the runnable `patch` gain nothing (cables ride the existing
`Connection.cable` + `loadPatch`). Full gate green (`cargo fmt --check && cargo lint && cargo test &&
cargo wasm && cargo docs`, plus the `web` Vitest/Biome/typecheck/build). Verified in-browser.

_Delivered:_ live patching on the spatial studio — drag a cable between two devices' back-panel jacks and
the engine rewires via the proven 4.1 `loadPatch` hot-swap; the only new Rust is **cable content** in
`devices` (the engine and runnable `patch` are otherwise untouched — cables ride the existing
`Connection.cable`).

- **Cable catalog is `devices` content** (`cables.rs`) — a `CABLES` table of realistic presets (patch /
  instrument 3 m & 6 m / mic / speaker; connector `kind` + series R + shunt C authored from a per-metre
  basis), `cable_types()` mirroring `descriptors()`, exposed via a `cable_catalog()` `wasm-bindings` bridge
  and the worklet `ready` handshake, mirrored in a TS `CableType`. **Hand-calc oracle:** a preset's R·C into
  a representative high-Z source hits the computed −3 dB corner; the same preset into the catalog's real
  1 Ω synth source sits far above 20 kHz — modelled-but-inaudible **by design**, matching §9. The audible
  payoff waits on Epic-5 high-Z instrument sources.
- **Pure `connections.ts` (Vitest-tested)** — the legality predicate (output→input, same domain, self-loop
  + **feedback-cycle rejection** via a DFS `wouldCreateCycle`, fan-in→replace, duplicate), cubic-bezier
  cable geometry + point-near-curve hit-testing, and the cable-spec↔type-id round-trip. Rendering-free
  (type-only imports), the "tests are the oracle" temperament applied to the UI. Client-side cycle
  rejection means a bad patch never reaches `compile`, so there's no async-`BuildError` revert to handle.
- **Cable rendering — two layers behind a thin `WorldView` seam.** `WorldView` gained `overlay` +
  `underlay` snippets (both handed a `WorldApi` coordinate converter, `bind:api` for measurement) and a
  per-item `background` flag; the world layer still knows only positioned boxes (WebGL escape hatch intact).
  Cables draw **in front** of a device when it shows its back (you see the plug) and **behind** when it
  faces front (tucked away); stacking is rack frame (0) → behind-cables (1) → panels (2) → front-cables.
  A shown-back end anchors to the **DOM-measured socket** (`getBoundingClientRect` → surface space,
  correctly reflecting the 3-D flip); a front-facing end **estimates** the socket near the chassis centre.
- **Drag-to-connect + inspector.** Jacks carry a `data-jack` tag; a window-level pointer drag draws a
  rubber-band with live green/red feedback, snapping to a candidate jack; a legal drop commits + hot-swaps
  (fan-in replaces the occupied input's cable). Clicking a cable/portal opens a **cable inspector** (type
  dropdown for analog — *Ideal wire* + presets; "ideal" note for digital; disconnect). A fresh analog
  connection defaults to a transparent patch cable.
- **Cross-space portals (snakes MVP).** A connection with one end in the shown room renders as a labelled
  portal stub (`→ Room`); created by moving a patched device to another room (the engine sees a plain mono
  connection throughout). Full bundle-into-one-line UX stays deferred.
- **Detours folded in (not bugs):** (1) **panel layout** reworked so thin 1U rack units stop clipping —
  chassis is a CSS **size container**, header/jacks/padding scale with **container units capped at the old
  rem**, the **back panel is a horizontal jack row**, and the device **name floats in the corner** (the
  header no longer steals height). (2) The **pull-out clearance step was removed** (flip is now direct;
  scene `SCHEMA_VERSION` 4→5, no migration). (3) **Rack frames restacked** below the cable underlay so a
  cable between two rack units stays visible.
- **Known limitations (recorded):** connector **`kind` is not enforced** — any analog jack accepts any
  other (TRS↔XLR) — a UI-legality follow-up noted in the design notes above; a **fan-out drag from an
  already-connected output** is blocked by its own cable's hit-path (delete the cable to re-patch); the
  **snake bundle** is minimal (per-cable stubs sharing a room label); the cable effect is **inaudible by
  design** with today's low-Z sources.

#### Story 4.5 — Visualization: meters + analog-domain readouts (the node→host lane) — ✅ **Done**

_Goal:_ the distinctive **visualization payoff** — *gain-staging across the AD/DA boundary made visible* —
on the proven engine. It delivers the genuinely-new **node→host scalar readout lane** (the engine's third
control lane: it has host→node params and routed events, but **nothing node→host** today), a voltage-native
**`VuMeter`** node (analog VU/dBu) and a **digital dBFS meter** node sharing that lane, and the **static
analog-domain readout** of **per-connection loading loss** read off the compiled edge gains. Rendered as
device **meter screens**, in the 4.4 **cable inspector** (per-cable dB loss), and as a **global levels
panel**. Anchors to PROJECT_PLAN §4 (Port/Device model surfaced as readings) and §7 (UI as a pure consumer),
and to the Epic-4 "metering = a node + a readout lane" decision settled at 4.2 planning. **Scope + spectrum
FFT are Story 4.7**, not this Story (waveform probes are a different mechanism — see below).

_Watch out:_

- **The readout snapshot runs on the audio thread.** The schedule snapshots each node's readings **once per
  block** (not per sample) after the step loop — so `Node::read_readouts` must be **zero-alloc, panic-free,
  total** (it writes into a pre-sized slice). The meters' ballistics run *inside* `process` per sample —
  same hot-path discipline (denormal-flush the one-pole state).
- **Single-threaded in-worklet, so no lock-free ring.** The readout store is engine-owned and read after the
  block completes, exactly like params/events are SPSC-shaped but exercised single-threaded (Epic-3 model).
  **Do not** build a cross-thread SAB ring for readouts — it's the same deferred retrofit as the event ring,
  justified only if a Worker execution model ever lands.
- **Measurement is a node; it must emerge from the volts.** Never bolt a reading getter onto `GainStage` /
  `AdConverter` / the speaker — the meter is its **own inserted node** computing a scalar from the signal it
  taps. The **one** honest exception is **loading loss**, which is an *edge* property, not a node: source it
  from the **baked `EdgeTransform.gain`** the schedule already computed (never recompute it in `devices`).
- **Meters must be signal-transparent.** `VuMeter` / the digital meter are **inline passthrough** (high-Z
  bridge, near-unity), so inserting one anywhere in a chain doesn't change the sound (assert it). They add no
  randomness (determinism preserved).
- **Loading loss reads the *baked* gain**, which already accounts for fan-out parallel loading — don't
  reconstruct it from a single branch's divider.
- _Scope guard:_ **no raw per-sample ring taps, scope, or spectrum FFT** (→ Story 4.7); **no phantom-presence
  readout** until a condenser-mic *device* is cataloged (Epic 5 — nothing in the default catalog supplies
  phantom to read); **no clip readout bolted onto `GainStage`** (headroom is UI math from the meter's peak;
  the honest hard-clip indicator is the **digital meter at 0 dBFS**). Master-output VU stays UI chrome.

_Design notes (settled at planning):_

- **The readout lane is getter-based; `Node::process` is unchanged.** A node declares `readouts() ->
  &[ReadoutDecl]` (mirroring `params()`), computes its reading into its own state during `process`, and the
  schedule pulls it via a new defaulted `read_readouts(&self, out: &mut [f32])` in a one-pass snapshot after
  the step loop. The schedule owns a flat `readout_store: Vec<f32>` contiguous by node
  (`readout_base`/`readout_count`), resolved by `Schedule::readout(node, id) -> ReadoutHandle` — the exact
  mirror of the param store. _Rejected:_ adding a `readouts: &mut [f32]` 4th argument to `process` — the
  clean symmetry with `Params`, but it ripples through **every** `Node` impl and test helper for a feature
  only meter/probe nodes use; the getter keeps the change to the two meter nodes.
- **A meter is a node (settled 4.2), split into measurement + exposure.** _(1)_ `VuMeter` — voltage-native,
  bridging `InputZ`, ~300 ms quasi-RMS ballistics, calibrated `0 VU ≙ +4 dBu ≙ 1.228 V RMS`; a **digital
  meter** — peak/RMS **dBFS** on a `SampleBuffer` (via the existing `level.rs` helpers). _(2)_ both surface
  their scalar(s) through the new lane. Two nodes (not one) so the **across-converter** story is complete —
  read dBu on the analog side of the AD and dBFS on the digital side; the second node is cheap since the
  lane, handle resolution, catalog `readouts` metadata, and meter screen are shared.
- **Meters are inline passthrough**, insertable at any point in a chain. _Rejected:_ a sink (input-only)
  meter — simpler node, but it can't sit mid-chain (only hang off a fan-out), which is the common "meter this
  point" gesture.
- **The master-output VU stays UI chrome.** It keeps reading the already-exposed `out_ptr` buffer (the host
  monitor level — an honest signal, throttled ~47×/s). A placeable `VuMeter` **device** is the real
  node-readout meter. _Rejected:_ forcing a `VuMeter` into the default scene to back the master VU — it
  conflates "the monitor level" (host chrome, outside the sim) with "a meter device in the signal path."
- **Static loading loss comes from the schedule's baked edge gains.** The schedule exposes its per-analog-edge
  gain; `build_patch` correlates each scene `Connection` to its graph edge and `BuiltScene` answers
  `connection_loading_loss(i) -> Option<f32>` in dB (`20·log₁₀(gain)`). _Rejected:_ recomputing loss in
  `devices` from the endpoints' impedances — duplicates the compile-time local solve and gets **fan-out
  parallel loading wrong** (a branch's loss depends on its siblings).
- **Readings reach the page as a throttled `readouts` postMessage snapshot** (like the existing `level`
  message), keyed by device id through the live `BuiltScene` maps, so it survives a hot-swap; static
  connection losses ride the `ready`/post-swap handshake (like the catalog). _Rejected for now:_ a zero-copy
  `Float32Array` view over the readout store — readouts are tiny and low-rate, and a zero-copy view needs its
  offset map rebuilt on every swap; adopt it only if the snapshot cost ever bites (measure-driven, like SIMD).
- **Scope + spectrum are Story 4.7 (reshaped at pickup).** The sketch bundled them into 4.5; a **scalar
  readout** (a few numbers per block) and a **raw-sample waveform probe** (a high-rate zero-copy ring, plus
  an FFT) are genuinely different mechanisms, and the 4.2 note already said "design the scalar lane first;
  rings are for waveform probes." Splitting keeps 4.5 to one coherent week and lets the ring/FFT be designed
  on its own terms.

- **Task 4.5.1 — The node→host readout lane (engine core).** New `readout.rs` (`ReadoutId` / `ReadoutDecl` /
  `ReadoutHandle`, mirroring `param.rs`); `Node::readouts()` + `read_readouts()` defaulted no-ops; the
  schedule builds the readout store at compile and snapshots it each block (one pass, zero-alloc, panic-free);
  `Schedule::readout(node, id)`. Exercised with an in-tree test node emitting a known scalar. _Done:_ the test
  node's reading resolves and appears in the store after a block; the `no_alloc` counting-allocator test stays
  green; `read_readouts` is total over out-of-range handles.
- **Task 4.5.2 — `VuMeter` node (analog, inline passthrough).** `node/vu_meter.rs`: analog in→out high-Z
  bridge, near-unity passthrough; VU (300 ms quasi-RMS one-pole, coeff baked in `prepare`) + peak-dBu
  readouts. _Done (hand-calc oracle):_ a **1.228 V RMS** sine settles to **0 VU** (calc in a comment:
  +4 dBu = 0.775·10^(4/20) V RMS, with the sine average↔RMS form-factor folded into the calibration);
  passthrough is signal-transparent into a high-Z load (asserted); the reading reaches the store.
- **Task 4.5.3 — Digital dBFS meter node.** `node/*` digital meter: `SampleBuffer` in→out passthrough; peak +
  RMS **dBFS** via `level.rs`. _Done (hand-calc oracle):_ a **0.5-full-scale** sine reads **−6.02 dBFS** peak
  (`20·log₁₀(0.5)`, calc in a comment); passthrough copies samples exactly.
- **Task 4.5.4 — Static loading-loss surface (engine + build).** Expose the baked per-analog-edge gains from
  `Schedule`; `build_patch` correlates scene connections → graph edges; `BuiltScene::connection_loading_loss`
  in dB. _Done (hand-calc oracle):_ `z_out = 150 Ω` into `z_in = 10 kΩ`, no cable → **−0.129 dB**
  (`20·log₁₀(10000/10150)`, calc in a comment); adding a cable's series R increases the loss as computed.
- **Task 4.5.5 — Catalog: meter devices + readout descriptors.** Add `vu_meter` + digital-meter `CATALOG`
  entries; extend `DeviceDescriptor` with a `readouts` list (engine-truth ids/count derived from the node,
  labels/units hand-authored), and extend `catalog_aligns_with_exposed_face` + `descriptors_carry_engine_truth`
  to readouts; mirror in TS `catalog.ts`. _Done:_ descriptors carry the meters' readout ids + labels; the
  alignment tests cover readouts; native + wasm serialization pass.
- **Task 4.5.6 — `SceneEngine` + worklet: readout snapshot + losses.** `BuiltScene` resolves `(device,
  readout id) → ReadoutHandle`; `SceneEngine` exposes a readout snapshot keyed by device id and a
  connection-loss accessor; the worklet posts a throttled `readouts` message and ships losses in
  `ready`/post-swap; `engine.ts` gains the message types + handlers. _Done:_ a native `SceneEngine` test — a
  scene with a `VuMeter`, after a note, reports a non-idle reading addressed by `(device, id)`, and losses
  resolve; in-browser the readings update live and survive a hot-swap.
- **Task 4.5.7 — UI: meter screens, cable-inspector loss, global levels panel.** Drive a device meter screen
  (reuse the `Vu` widget / a meter `Screen`) from the live readouts; add per-cable loading-loss dB to the 4.4
  cable inspector; a global "signal path / levels" panel reading across the AD/DA boundary; add the meters to
  `defaultScene`. Pure display/formatting logic is Vitest-tested. _Done:_ in-browser the meters move with the
  signal, the cable inspector shows each cable's dB loss, and the global panel shows dBu→dBFS across the
  converter; the master-output VU is unchanged (chrome); the `web` `check`/`typecheck`/`build` + Vitest pass.

_Validate:_ ✅ **met.** A `VuMeter` and a digital dBFS meter, placed in a scene through the UI, show **live**
readings via the new node→host lane (VU/dBu on the analog side of the AD, dBFS on the digital side — gain-staging
across the boundary made visible); the cable inspector shows each analog connection's **loading loss** in dB
and the global panel lists levels/losses along the chain; meters are **signal-transparent** (inserting one
doesn't change the sound) and the readout snapshot survives a hot-swap; the engine gains **only** the readout
lane + two meter nodes and stays UI-free; the full Rust gate (`cargo fmt --check && cargo lint && cargo test
&& cargo wasm && cargo docs`) plus `wasm-pack build` and the `web` `check`/`typecheck`/`build` pass; verified
in-browser by eye.

_Delivered:_ the node→host **readout lane** (the engine's third control lane) with two voltage-native meter
nodes, a static **loading-loss** annotation off the compiled edges, surfaced as device meter screens, a
cable-inspector loss line, and a global levels panel. The engine gained only the readout lane, the two meter
nodes, and an `edge_gain` readback; it stays serde-free and UI-free. Scope + spectrum (waveform probes) were
split out to **Story 4.7** at pickup.

- **Readout lane (engine core).** New `readout.rs` — `ReadoutId` / `ReadoutDecl` / `ReadoutHandle`, mirroring
  `param.rs`. `Node` gained defaulted `readouts()` + `read_readouts(&self, &mut [f32])`; **`process()` is
  unchanged** (getter-based, chosen over a 4th `process` arg to avoid rippling every node). `Schedule` owns a
  flat `readout_store` contiguous by node, snapshotted **once per block after the step loop** (zero-alloc,
  panic-free — the `no_alloc` guard stays green), resolved by `readout(node, id)` / read by
  `readout_value(handle)`, both total over stale handles.
- **`VuMeter` node** (analog, inline passthrough: 1 MΩ bridge, 150 Ω out, unity). VU (quasi-RMS one-pole,
  τ≈65 ms ⇒ ~300 ms to 99 %, baked in `prepare`) + peak-dBu readouts. Calibrated `0 VU ≙ +4 dBu ≙ 1.228 V
  RMS` via the sine form factor `2√2/π`. **Oracles:** a 1.228 V RMS sine ⇒ 0 VU; its peak ⇒ +7.01 dBu.
- **`DigitalMeter` node** (digital, inline passthrough). Per-block peak + RMS **dBFS** (the block is the
  ~21 ms integration window — no ballistic state), full scale = 0 dBFS. **Oracle:** a 0.5-FS sine ⇒ −6.02 dBFS
  peak / −9.03 dBFS RMS.
- **Static loading loss** = the §5.3 impedance divider, read back — *not* a live meter (settled with Oskari
  during the task). `Schedule` records the baked per-edge divider gain (`edge_gain`, fan-out-aware; `None` for
  digital/event edges); `build_patch` correlates each scene connection to its graph edge; `BuiltScene::
  connection_loading_loss(i)` returns `20·log10(gain)` dB. Kept **out** of the readout lane, so the measured
  path stays pure. **Oracle:** `da(150 Ω)→spk(10 kΩ)` ⇒ −0.129 dB; a 1 kΩ cable deepens it to −0.946 dB.
- **Catalog + resolution.** `vu_meter` + `digital_meter` entries; `DeviceDescriptor` gained a `readouts` list
  (engine-truth id + authored label/unit) and `BuiltDevice` a readout map; `BuiltScene::readout(device, id)` +
  `readout_snapshot()`; `SceneEngine::readouts()` / `connection_losses()` (JS values). TS `catalog.ts` mirrored.
- **Transport.** The worklet posts a throttled `readouts` message (~47×/s, keyed by device id so it survives a
  hot-swap) and ships the static `losses` in `ready` **and once after each swap** (a `lossesDirty` flag) — not
  per frame, as designed. `engine.ts` gained the message types + **optional** `onReadouts`/`onLosses` handlers.
  _Scalar snapshot over `postMessage`_ (a zero-copy readout view stays deferred — measure-driven).
- **UI.** New `Meter.svelte` (unit-aware bar: VU / dBu / dBFS scales); `Panel` renders a meter screen for a
  device with readouts; the **cable inspector** shows the analog connection's loading loss (labelled as the
  impedance divider, not a meter); a global **“Signal path — levels & losses”** panel lists every meter's live
  readings and each analog connection's loss. The **default scene** became `synth → gain → VU → AD → digital
  meter → DA → speaker` (so dBu↔dBFS across the converter shows out of the box); `SCHEMA_VERSION` 5→6.
- **Known simplifications (not bugs):** loading loss is the **resistive divider only** — cable rolloff and
  coupled interference emerge via the meters, not this number; the master-output VU stays **host-monitor
  chrome** (`out_ptr`), distinct from the placeable `VuMeter` device; the readouts snapshot is re-serialized per
  throttle tick (tiny — a handful of scalars); a **phantom-presence** readout is deferred to Epic 5 (no
  condenser-mic device is cataloged yet to attach it to).
- **Story 4.6 — The spatial world, part 2: top-down view + operator reach.** The deferred half of the
  spatial sim — 4.3 stored the full 3-D coordinate truth precisely so this stays cheap. Add a **top-down
  floor-plan projection** of a space as a _second view over the same model_ (the real test of "model in
  3-D, render in 2-D as projections": a new projection, no new coordinate state), with **view switching**
  (front elevation ↔ top); and an **operator position + reach** model — move around a space, zoom out for
  the overview but interaction **disables beyond reach** (you can only touch what's within arm's length),
  with back-panel access still gated by the 4.3 clearance action. The new pure spatial logic (the second
  projection, reach queries) extends the 4.3 rendering-free module and stays Vitest-unit-tested. _Open at
  pickup:_ the top-view projection + its placement affordances (drag on the floor plan vs. in the rack);
  the reach metric and the zoom→view-only gating rule; how operator position lives in the scene `ui`;
  whether containers need a plan-view footprint distinct from their elevation. _Scope guard:_ views +
  reach only — no new devices, cables, or probes. Also make the devices snap to grid for easier placement
- **Story 4.7 — Visualization, part 2: scope + spectrum (waveform probes).** Split out of Story 4.5 at its
  pickup: the **raw per-sample tap** surface a scope and spectrum need — a distinct mechanism from 4.5's
  scalar readout lane. A **zero-copy sample ring** (à la `out_ptr`, tapping a node/port's block), a **scope**
  rendering the waveform on a device screen / global tool, and an **FFT spectrum**. Builds on the 4.5 probe
  addressing (`(device, probe id)` through `BuiltScene`) and the meter-screen UI. _Open at pickup:_ the ring
  shape + who owns it (engine-owned buffer vs. exposed pool lane); **FFT in the engine vs. JS** (a JS FFT
  keeps the engine lean; an engine FFT keeps the DSP in one place — measure); tap cost on the hot path;
  which probes are device-embedded vs. global. _Scope guard:_ waveform probes only — the scalar meters +
  analog-domain readouts are 4.5.

_Validate (epic exit):_ a small studio built, placed, patched across at least two spaces, played, and
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

_Tasks to be elaborated when we reach this Epic._

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

_Decision — ground-loop hum should become emergent from grounding topology (deferred to this Epic)._
Today (Story 1.5) `Cable::with_hum` is a **manual** injection — the user asserts "a ground loop exists
on this cable." That's a phenomenological stand-in, not the final design. A ground loop is a **loop in
the ground network**: two mains-earthed devices _also_ tied together by a cable shield form two ground
paths between them ⇒ circulating 50/60 Hz current ⇒ hum. Break any leg (a floating/battery device, a
**ground lift**, transformer/DI isolation) and the loop — and the hum — is gone, _regardless_ of
balanced vs. unbalanced (balanced merely rejects the hum when a loop does exist; it doesn't prevent the
loop). So whether hum _appears_ is a property of the patch's grounding, and should **emerge**, not be a
flag:

- Model a small **ground-connectivity** side-graph — devices declare mains-earthing; cables declare
  whether the shield bonds the two grounds and whether it's lifted at an end.
- At **compile**, **detect cycles** in that graph; a cable on a cycle between earthed devices is in a
  ground loop ⇒ inject hum there. A lift / floating device / isolator removes an edge ⇒ no cycle ⇒ no hum.
- This is compile-time **connectivity analysis, not a per-sample electrical loop solve**, so it honors
  the "local solve only / no global nodal solve / signal graph is a DAG" decision (§5.3) — same kind of
  cheap graph pass we already run for signal-DAG cycle detection, just on a separate graph.
- The hum **amplitude stays phenomenological** (the induced voltage from loop area / earth-potential is
  the "EM source" we hold out of scope). Only the _appearance and location_ become emergent.
  _Prerequisites (none exist yet):_ a ground/earth concept on devices, shield modeling on cables, and
  ground-lift controls — naturally introduced alongside Story 5.1 (patchbay/wiring) and consumed by the
  "fix the hum" diagnostic here. ROI is high then (the heart of the troubleshooting lesson), low now.

_Decision — clock domains and their failures emerge from a clock-distribution side-graph + real
per-domain rates (deferred to this Epic)._ Through Story 1.6 there is a single internal clock domain
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
  de-framing (inside-the-box circuitry, §2). We model whether a link _locks_ and _slips_, not its
  bitstream. True jitter _spectra_ are a further optional depth we do not expect to need.
  _Prerequisites:_ the carrier/clock seam and `ClockDomainId` stamp (Story 1.6); multiple digital
  devices and the fractional resampler (this Epic). ROI is high here (multi-device digital sync is the
  heart of the lesson), nil before.
