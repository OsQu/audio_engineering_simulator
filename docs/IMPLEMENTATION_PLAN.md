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

**Detail gradient (concretely):** Epics 1–4 are built — their completed Stories carry full design
notes and per-task delivery records, now **archived to `EPIC_<N>_NOTES.md`** with only a summary kept
here (Epic 4's one deferred Story, 4.7, keeps its coarse sketch). **Epics 5 and 6 stay at Story level** —
their Tasks get written when we reach them. Don't over-plan work whose shape the earlier work will change.

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
confirmed; the heaviest unknown in PROJECT*PLAN §10 is retired). 3.2 — **first real-time sound**: the
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
stay deferred, so the *"lock-free cross-thread validation"\_ item is intentionally open past Epic 3.

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

## Epic 4 — UI: Skeuomorphic Panels + Patch Cables — ✅ **Substantially complete** (4.7 deferred)

**Progress:** Stories 4.1 ✅, 4.2 ✅, 4.3 ✅, 4.4 ✅, 4.5 ✅, 4.6 ✅, 4.8 ✅ done; **4.7 deferred**. The proven
engine now has a **game-like studio UI** built entirely on its published API. 4.1 — the engine→UI seam: a
new `devices` crate (device **catalog** + serializable **scene/`Patch` IR** + `build_patch`) and
`SceneEngine` (scene-driven, generically controlled, hot-swappable) with a `catalog`/`parse_patch` JS
bridge. 4.2 — the **skeuomorphic panel system** on a **Svelte 5** harness: a descriptor→panel renderer +
widget vocabulary (knobs/faders/switches/jacks/screen/VU), front/back flip, a real `powered` param, and a
host-side monitor volume. 4.3 — the **spatial world**: a pan/zoom front-elevation studio where gear lives
at real coordinates, mounts in **rack U-slots**, moves between rooms, and is added/removed from a **catalog
palette** (the hot-swap recompile path); pure Vitest-tested spatial logic behind a thin world layer (the
WebGL escape hatch), engine untouched, the full 3-D coordinate truth stored. 4.4 — **patch cables &
snakes**: drag-to-connect between back-panel jacks → `loadPatch` hot-swap, client-side legality (incl.
feedback-cycle rejection), a cable inspector with pickable cable types (R·C rides the edge), front/behind
layering, and cross-space **portal** endpoints. 4.5 — **visualization**: the node→host scalar readout lane,
a `VuMeter` (analog VU/dBu) + a digital dBFS meter, and static per-connection loading-loss annotations,
surfaced as meter screens, a cable-inspector loss line, and a global levels panel. 4.6 — **room walls +
multi-view**: a space becomes a rectangular room whose four wall-elevations you turn between, plus a
top-down floor plan, with cross-wall/room **click-to-pick** patching and draggable portal chips (operator
reach was dropped as not worth the interaction cost). 4.8 — **device focus mode**: click a synth/console to
open a large interaction surface (an on-screen keybed, a channel-strip console), with note/param input
scoped to the focused device — retiring the global virtual keyboard — plus a standalone MIDI controller
driving a synth over the first UI-managed **events cable** (one new engine node, `EventThru`).

**Goal (delivered):** the product interface on the proven engine — a game-like studio you build by browsing
a gear catalog, placing devices in racks and spaces, wiring them with patch cables and snakes, operating
realistic skeuomorphic panels, playing and metering the result — glitch-free, with graph edits hot-swapping
live under sound, the UI a **pure consumer of the published engine API** (never reaching into internals).

> **Full design notes, rejected alternatives, per-task delivery records, and the settled sketch for the
> deferred Story 4.7 live in [`EPIC_4_NOTES.md`](./EPIC_4_NOTES.md).** This section keeps only the decisions
> and the delivered surface that constrain later epics — enough to make good follow-up decisions without
> re-deriving Epic 4.

### What Epic 4 delivered (engine + web surface)

- **New `devices` crate** (engine + serde): the device **catalog** (real Rust node builders + internal
  edges + a hand-authored UI descriptor, with numeric/domain fields _derived_ from the nodes so they can't
  drift), the serializable **scene/`Patch` IR**, `build_patch` (device→node expansion, connection remap,
  handle resolution), the **cable catalog** (R·C presets), and connector-type + domain **legality checks**.
- **`wasm-bindings` — `SceneEngine`**: the real-time, scene-driven surface the AudioWorklet drains, built
  from a serialized `Patch`, **generically controlled by device id** (params/notes/`loadPatch`), with a
  **`loadPatch` hot-swap** (compile + `ScheduleSlot` swap) for every structural edit; plus the
  `catalog()` / `parse_patch()` JS bridge and per-connection loading-loss reporting.
- **The node→host readout lane** (engine): scalar probes addressed by `(device, readout id)` through the
  built scene, refreshed each block off the hot path — the `VuMeter` (analog VU + peak dBu) and the digital
  dBFS meter ride it. One new event node, **`EventThru`** (events-in→events-out copy), + a `midi_controller`
  device; `synth_voice` unchanged.
- **The `web/` app** (Svelte 5 + Vite + TS): a descriptor-driven skeuomorphic **panel** system (widget
  vocabulary + front/back flip); a pan/zoom **spatial world** (`WorldView` — positioned boxes + pointer
  mechanics; the WebGL escape hatch) with rooms as **rectangular four-wall rooms + a top-down plan**; racks
  with U-slot mounting; a **catalog palette**; **patch cabling** (drag + click-to-pick, cross-view portal
  stubs, cable inspector); **meters/levels** panels; and **device focus mode** (overlay surfaces: `Keybed`,
  `Console`). Pure logic (spatial projection, patching state machine, note mapping, focus/params) lives in
  Vitest-tested rendering-free `.ts` modules; the aesthetic layer (`skin.ts`) is UI-only.

### Decisions that bind every later epic

- **Spaces, racks, walls, placement, and focus are a UI concept — the engine/`patch` gain nothing.** The
  worklet only ever receives the `patch` projection (devices + connections + output); rooms/positions/walls
  /portals/focus are all TS + scene `ui`. A task that wants a Rust change to model any of them is modelling
  in the wrong layer.
- **The catalog holds the _specs_; the web layer holds the _rendering_.** The `devices` catalog declares a
  device's capabilities/electrical truth (which ports exist, params, ranges, connector shape, dimensions);
  how it _looks and is operated_ — faceplate/knob skins, which chassis face a jack is drawn on, focus
  surfaces, keybed/console layout — lives in `web/`, keyed by `typeId`. Kept UI-presentation vocabulary out
  of engine/`devices`.
- **The UI is a pure consumer of the published engine API, generic by device id.** Panels/controls are
  rendered _from the fetched catalog descriptor_ (never hardcoded), params/notes/edits addressed by id; the
  UI never reaches into engine internals.
- **Single 3-D coordinate truth per placement, projected per view.** A placement keeps one `(x,y,z)` + a
  `wall` tag; every wall elevation and the top plan are _projections_ — never per-view 2-D positions.
- **Every structural edit hot-swaps via `loadPatch`** (compile off-block + `ScheduleSlot`); connection
  **legality is checked client-side** (domain + connector shape + feedback-cycle) mirroring the engine's
  authoritative `build_patch`, so illegal patches never reach compile.
- **MIDI is a signal on a cable, inter-chassis only.** Event routing rides the engine's `EventRoute` edges;
  the keybed is a device's **open events input** (host-fed via focus, edge-fed when patched), _not_ a node.
  A device that emits events is the only new piece (`EventThru`); no internal keys→voice edge.
- **Two visualization mechanisms, kept distinct:** the **scalar node→host readout lane** (meters, this
  epic) vs. **raw per-sample waveform probes** (scope/spectrum, Story 4.7). Don't conflate them.
- **`SCHEMA_VERSION` stamps the localStorage save; it is disposable — no migration** (a mismatched save is
  discarded and the default scene rebuilt). Ended the epic at **v10**.

### Deferred — decided, not gaps

- **Story 4.7 — Visualization, part 2: scope + spectrum (waveform probes).** The scalar readout lane (4.5)
  already makes gain-staging visible; the **raw per-sample tap** a scope/FFT needs is a _different_
  mechanism and wasn't required by the epic's exit criteria. It's independent of every shipped story (no
  shared surface — which is why 4.8 was taken first). The settled sketch (zero-copy sample ring, scope +
  FFT, engine-vs-JS FFT open question) is recorded in `EPIC_4_NOTES.md`; resume it when a scope/analyzer is
  actually wanted (it sits comfortably alongside Epic 5's deeper-DSP work).
- **Operator reach / zoom-to-operate gate — cut (in 4.6), not merely deferred.** An avatar-with-reach and
  its zoom-threshold fallback both added a locked/operable split across every control for too little payoff
  in a single-operator sandbox; all gear in the current view is fully operable. The 3-D truth needed to
  revisit it (e.g. a challenge layer) is stored, so it's cheap to reintroduce if Epic 5 wants it.

### Story-by-story (status + the one thing each settled)

- **4.1 — Engine/bindings API + scene IR + device catalog** ✅ — the `devices` crate + `SceneEngine`.
  Settled: the UI drives the engine **generically by device id** over a serialized `Patch`; the catalog's
  numeric/domain descriptor fields are **derived from the nodes** (can't drift), only labels/kinds authored.
- **4.2 — Skeuomorphic panels: controls→params, front/back, power** ✅ — the Svelte 5 panel renderer +
  widget vocabulary. Settled: panels render **from the descriptor** (front = controls, back = I/O jacks);
  `powered` is a real smoothed control param (de-clicked), not a structural edit; metering deferred to 4.5.
- **4.3 — The spatial world: spaces, racks, placement, catalog browsing** ✅ — pan/zoom front-elevation
  world, U-slot racks, catalog palette. Settled: **spaces/racks/placement are UI-only** (engine untouched);
  a placement stores the **full 3-D coordinate truth** (so 4.6's multi-view stayed cheap); a thin world
  layer keeps the WebGL escape hatch.
- **4.4 — Patch cables & snakes → live graph mutation** ✅ — drag-to-connect → `loadPatch` hot-swap.
  Settled: **client-side legality** (domain/connector/feedback-cycle) mirrors `build_patch`; the cable's
  **R·C rides the edge** (inaudible into today's low-Z sources by design — audible payoff waits for Epic 5);
  cross-space links draw as **portal stubs**.
- **4.5 — Visualization: meters + analog-domain readouts (node→host lane)** ✅ — the scalar readout lane +
  VU/dBFS meters + loading-loss annotations. Settled: probes addressed by **`(device, readout id)`** through
  the built scene, refreshed **off the hot path**; scope + spectrum are a **separate mechanism → 4.7**.
- **4.6 — The spatial world, part 2: room walls + multi-view** ✅ — rectangular rooms, four wall-elevations
  - top plan, cross-view click-to-pick patching. Settled: **one coordinate truth projected per view** (a
    `wall` tag, never per-view 2-D); **operator reach dropped**; the window stays decorative.
- **4.7 — Visualization, part 2: scope + spectrum (waveform probes)** ⏸️ **Deferred (2026-07-03)** — see
  _Deferred_ above; settled sketch in `EPIC_4_NOTES.md` should it resume.
- **4.8 — Device focus mode + the interaction seam** ✅ _(the epic's UX capstone)_ — focus overlays +
  focus-scoped input, retiring the global keyboard. Settled: **MIDI is a cable signal** (one new `EventThru`
  node; the keybed is a device's open events input, not a node); focus/keybed/console are **web-layer
  presentation** keyed by `typeId`; `synth_voice` unchanged.

---

## Epic 5 — Breadth & Challenges

**Goal:** grow device coverage and the medium (routing, studio wiring, live sound scaling toward large
venues), deepen DSP and AD/DA, and add the game layer.

**Exit criteria:** the same engine credibly supports studio, routing, and live-sound scenarios; structured
challenges layer on top of the sandbox.

**Watch-outs:** multi-core only if profiling at scale demands it (single core covers stadium on the napkin).
Keep device transforms understandable — spend the realism budget on the volts-and-converters layer.

**Notes:** The stories in this epics are not related to each other unless \*otherwise stated. We can do them in any order or only do part of them

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
- **Story 5.6** — Device fidelity & the corner cases it forces. As devices get more faithful (a real
  audio interface, a proper mixer with sends/inserts, a patchbay), they break simplifying assumptions the
  earlier UI baked in; this is where those get collected and resolved (distinct from 5.1's breadth-of-
  coverage — this is depth-of-realism). The layer rule holds throughout: the **catalog owns which ports
  exist** (specs), the **web owns how they're drawn** (look & feel). _First known case — mixed-face I/O:_
  Story 4.8's `skin.ioFace` is **per-device** (a whole device's jacks on one face — `back` default,
  `front` for the MIDI controller). Real gear splits them — an audio interface puts **hi-Z instrument
  inputs on the front** and line/digital/word-clock on the back. **Resolution: Story 5.7** — once a
  device authors its own faceplate as a Svelte component, face assignment is simply _which face's markup
  you write a jack into_, so the mixed-face problem dissolves (no `ioFace` per-port resolver needed; an
  earlier sketch of that resolver is superseded). The audio interface that forces this is 5.7's proving
  device. Further fidelity corner cases accrue here as they surface.
- **Story 5.7** — Per-device faceplate UIs: each device authors its own look & feel. ✅ **Complete**
  (parts 1 + 2; merged to `main`).

### Story 5.7 — Per-device faceplate UIs — ✅ **Complete**

_Goal:_ Give the web layer a way for **each device to author its own faceplate** — real look-and-feel per
device — while the engine/`devices` catalog stays specs-only. Today every device is drawn by **one
generic, flow-based renderer** (`web/src/widgets/Panel.svelte`: flexbox controls in exposed-param order,
flexbox In/Out jack groups) dressed by a thin `skin.ts` (three faceplate finishes + knob caps + a
**per-device** `ioFace`). That cannot express real gear: controls positioned relative to their jacks, a
big centre monitor knob, section legends ("MONITOR", "LINE OUTPUTS"), brand identity (a red Focusrite
chassis, a "Teletronix" wordmark), or **mixed-face I/O** (front instrument inputs, rear line/MIDI/USB).
Anchors to PROJECT_PLAN §4 (device/port domain model) and §7 (UI a pure consumer of the published engine
API), and to the Epic-4 settled layer rule (the **catalog owns which ports/params exist**; the **web owns
how they're drawn**). Proven end-to-end on a **simplified Focusrite Scarlett 8i6** — the first mixed-face,
branded device.

_Done (both parts; merged to `main`):_

- **Part 1 (5.7.1–5.7.4) — the faceplate system.** A device-UI registry (`typeId → component`, else the
  generic `Panel`), a `DeviceHandle` context + `Chassis` bezel publishing it, **bound widgets**
  (`Control`/`Socket`/`Reading`/`ConfigSwitch`) that bind by id, focus surfaces generalized per device, and
  a static **coverage guardrail** (`web/test/faceplate.test.ts`) proving every exposed param/port is placed
  and only valid ids referenced. Proven on the (then reduced) Focusrite Scarlett 8i6 with its red-chassis
  brand accent.
- **Part 2 (5.7.5–5.7.10) — the fidelity needed to make the 8i6 real**, each an engine/devices concept the
  UI then consumes:
  - **5.7.5 device-level power** — a catalog **param group** binds one exposed control to N node params;
    `AdConverter`/`DaConverter` gained a smoothed `powered` gate. One Power switch silences the whole unit.
  - **5.7.6 preamp physics** — a `MicPreamp` node (PAD −10 dB, AIR high-shelf, INST/hi-Z as a **structural
    config** that recompiles), plus the scene `config` seam and generic `configs` descriptor; the Focusrite
    Control focus surface drives them (48V still deferred to the phantom side-graph).
  - **5.7.7 multichannel digital** — `DigitalMux`/`DigitalDemux`, per-port lane counts on the descriptor, a
    `Combo` connector, and the first end-to-end N-lane digital coverage.
  - **5.7.9 runtime routing matrix** — a params-driven `Matrix` node (route/mix/mute per crosspoint, no
    recompile), surfaced as the data-driven `RoutingGrid` in the focus view.
  - **5.7.8 full 8i6 + `computer` peer** — the 8i6 grown to the real unit (9 in / 9 out: 2 combo + 4 line +
    S/PDIF + USB + MIDI; 14×14 matrix; 206-param face), a generated crosspoint-label mechanism (`GridSpec`),
    and a minimal `computer` USB peer (per-lane send meters + loopback). Closing the monitoring loop through
    the computer was a graph cycle, so we added a **delayed-edge** primitive (`Graph::connect_delayed`, cut
    from the topo sort, served from the persistent pool → one block of round-trip latency; the schedule
    stays a DAG). The default scene is now this playable loop.
  - **5.7.10 device dimensions** — form factors corrected against real gear (8i6 → 216×47×173 mm; a laptop
    `computer`; the rest sanity-checked).
- **Layer rule held throughout:** the Rust catalog gained no layout vocabulary; the web stayed a pure
  consumer binding by id.

_(The detailed part-2 execution plan and the round-trip-latency design write-up lived in
`one_off_plans/story_5_7_part2_plan.md` and `one_off_plans/roundtrip_latency_plan.md`; both are retired now
that the work is captured here.)_

_Watch out:_

- **Layer rule, unchanged.** The Rust catalog gains **no layout vocabulary** (positions/faces/colours) —
  the Story 4.2 decision that rejected descriptor layout fields still holds. A faceplate references
  params/ports/readouts **by id**; all appearance lives in the web component. A task that wants a Rust
  layout field is a bug, not a shortcut.
- **No cosmetic controls.** A faceplate can only draw controls that map to **real params** (the
  power-as-control ethos: "don't flag what should emerge"). The 8i6 therefore exposes only **gain + power**
  on its preamps; its INST/AIR/PAD/48V switches are **omitted, not faked**, because none can be honestly
  modeled in today's engine (see the deferred-preamp-physics design note). **No new engine node this
  Story** — the preamps reuse the existing `GainStage`.
- **Fallback parity.** After the `Chassis` refactor, the generic `Panel` (every un-authored device) must
  render and flip **identically** to today — verified in-browser. The registry defaults to `Panel`, so
  nothing existing changes silently.
- **Signal-type split intact.** The 8i6's internal AD/DA are the only analog↔digital bridges (§5); its
  "USB" is modeled as digital ports, not a magic passthrough.
- **`$state.snapshot` at the worklet boundary still holds** — the `DeviceHandle` only repackages App's
  existing `set_param` path; no new `postMessage` shape.

_Design notes (settled at planning):_

- **Component over coordinate-map (the headline decision).** A device optionally registers **its own
  Svelte component** as its faceplate, composing the shared skeuomorphic widgets
  (`Knob`/`Fader`/`Switch`/`Jack`/`Meter`) + design-system `--ae-*` tokens but arranging them with **its
  own scoped CSS** (grid/flex/absolute; absolute px is safe — the world's single zoom transform scales
  it). _Rejected: a normalized-coordinate / layout-data model_ (`paramId→{x,y}` on the skin) — a component
  gives full CSS expressiveness, free-form text/legends/logos as plain HTML, Svelte's automatic
  per-component style scoping, and it makes the **front/back-face problem dissolve** (a jack's face = which
  snippet it is written in — retiring the Story 5.6 `ioFace`-resolver sketch). The generic `Panel` stays
  the **fallback** for un-authored gear.
- **Plumbing.** A **device-UI registry** (`typeId → component`, else `Panel`; mirrors
  `skin.ts`/`focus.ts`); a **`DeviceHandle` context** packaging App's existing per-device
  `valueFor`/`readingFor`/`onParam` so a faceplate binds by id with no `postMessage` plumbing; **bound
  wrappers** `Control`/`Socket`/`Reading` (reference an id, pick the widget by descriptor `kind`, keep
  `Jack`'s `data-jack` anchor measurement working wherever placed); a **`Chassis`** primitive owning the
  shared bezel + 3-D flip (and setting the handle context) so a device authors only face _contents_. The
  generic `Panel` is **rebuilt on `Chassis`** (one flip implementation). _Rejected: leaving `Panel`
  untouched_ (two flip impls to keep in sync).
- **Preamp physics deferred — why the 8i6 shows only gain + power (settled after a code check).** The 8i6
  reuses the existing `GainStage` for its preamps; **INST/AIR/PAD/48V are omitted** because none is
  honestly modelable in today's engine, and the "no cosmetic controls" ethos forbids faking them:
  - **INST/hi-Z** would change the preamp's input impedance, but the loading divider is **baked at
    compile** into the edge transform (`schedule.rs`, from the port's static `InputZ`) — so a hi-Z switch
    is **structural** (needs a recompile-on-toggle, like repatching), not a smoothed `set_param`. Its
    effect is **latent anyway** (no hi-Z sources exist until Epic 5), so it would be recompile plumbing for
    zero audible payoff now.
  - **48V/phantom** is architecturally **inert on a preamp**: the existing `CondenserMic` self-emits
    phantom when its *own* flag is set (phantom flows upstream through the pull-based DAG and is
    approximated at the mic), so a preamp-side switch has **no path to reach the mic**. Real phantom supply
    needs an upstream side-graph (Epic-5 work, like the planned ground/clock side-graphs).
  - **AIR** (analog high-shelf) needs a new analog filter; **PAD** (in-`process` attenuation) *is* cleanly
    modelable now but adds little without a hot source.

  All four ride in with **Epic 5 (5.1)** when the preamp gets real physics — the switches appear when they
  are honest. _Rejected: a `MicPreamp` node carrying declared-but-inert INST/48V params now_ — a code check
  showed both would be cosmetic-or-latent, exactly what the ethos forbids. Recorded in `IMPROVEMENTS.md`.
- **`scarlett_8i6` catalog entry — minimal but honest.** Multi-node chassis reusing existing nodes: 2×
  `GainStage` preamps (gain + power) → 2× `AdConverter` (the digital "USB send"); a digital "USB return" → `DaConverter` →
  monitor + phones gain stages → analog outs; MIDI in/out via `EventThru`. Exposed face: **front** = 2
  combo inputs + a headphone out; **back** = line outs, digital send/return, MIDI, power. _Known
  simplifications (not bugs):_ USB is modeled as separate per-lane digital ports (our connector model is
  mono-per-lane; one-connector-many-channels is a 5.6 fidelity case), and **S/PDIF is deferred** (only
  real ports get jacks).
- **Focus via the registry (generalized — decision 2a).** The registry replaces the hardcoded `typeId ===
  "synth_voice"` (Screen) and `surface === "console"` branches in **both** render sites (in-world `item`
  and the focus overlay); a custom faceplate renders in the focus overlay too. Focusability = **has a
  registered focus surface** OR **is playable** (an events input ⇒ keybed, still derived — retiring
  `focus.ts`'s hardcoded `FocusSurface` kinds); `Console` becomes `channel_strip`'s registered focus
  component and the synth's ADSR `Screen` its registered embellishment. The keybed is still appended for
  instruments and the global-keyboard retirement (Story 4.8) is preserved. A larger bespoke focus surface
  (a DAW/touch display) is now expressible but **built only when a device needs it**.
- **Brand identity.** `skin.ts` gains an `accent`/`chassis` colour; the 8i6 faceplate reads it for its
  border/chassis via scoped CSS + tokens, and the **top-down floor-plan tile** (App's `item` snippet,
  `.plan-tile`, currently plain `--ae-bg-chip`) reads it too — so the red chassis shows from above. One
  value, both views.
- **Consistency guardrails (against N bespoke snowflakes).** Shared widgets + a small set of **layout
  primitives** (`Section`/`Legend`/`ButtonCluster`/`Silkscreen`, extracted while building the 8i6) +
  token-only colours/type. A **Vitest mount test per registered faceplate** asserts it references only
  **valid ids** and **places every param/port** — the web mirror of the Rust
  `catalog_aligns_with_exposed_face` guard.

- **Task 5.7.1 — `scarlett_8i6` catalog entry (`devices`).** Multi-node entry (2× `GainStage` preamps →
  2× AD; digital return → DA → monitor/phones gains; MIDI `EventThru`) with UI metadata
  (labels/kinds/connectors) positionally aligned to the exposed face. _Done:_ `catalog_aligns_with_exposed_face`
  + `descriptors_carry_engine_truth` pass; an `instantiate` test pins the multi-node port/param remap (as
  `channel_strip` does); the descriptor serializes camelCase; the device renders (via the **generic
  Panel**, pre-faceplate) in-browser.
- **Task 5.7.2 — Faceplate plumbing + `Chassis` + `Panel` refactor.** Add the `DeviceHandle` context, the
  `Chassis` primitive (bezel + flip + sets context), and the `Control`/`Socket`/`Reading` bound wrappers;
  **rebuild the generic `Panel` on `Chassis`** using the wrappers. _Done:_ every existing device renders
  and flips **identically** (fallback parity), knobs/faders/switches/jacks still drive the engine and
  patch; `pnpm check`/`typecheck`/`build` green; verified in-browser.
- **Task 5.7.3 — Device-UI registry + both render sites + focus generalization.** `device-ui.ts` registry
  (`typeId → component`, else `Panel`); wire it into the in-world `item` snippet **and** the focus overlay,
  replacing the hardcoded synth-Screen and console branches; rework `focus.ts` so focusability =
  registered-focus-surface ∨ playable, with `Console` and the synth `Screen` registered. Keybed still
  appended for instruments. _Done:_ synth (in-world + focus + keybed) and `channel_strip` (Console focus)
  behave as before, now via the registry; no hardcoded `typeId`/`surface` branches remain in App's render
  sites; in-browser parity.
- **Task 5.7.4 — `Scarlett8i6.svelte` faceplate + brand + primitives + guardrail.** The bespoke component:
  **front** (2 combo inputs with gain knobs, monitor + phones knobs, headphone jack, plus the power
  switch(es) the exposed face carries) and **back** (line outs, digital send/return, MIDI, power) laid out
  in scoped CSS; red chassis via a new `skin.accent`, threaded to the faceplate border **and** the
  top-down `.plan-tile`. Extract the `Section`/`Legend`/`ButtonCluster`/`Silkscreen` primitives and any new
  widget (a labeled toggle button / indicator LED) it needs. Add the **Vitest mount-test guardrail**
  (valid-ids + full param/port coverage per registered faceplate). _Done:_ zoomed in, the 8i6 reads as a
  simplified Focusrite — mixed front/back I/O, red chassis (in elevation **and** top view), section legends
  — its controls drive the live engine and its jacks patch with correct cable anchors; the mount test
  passes; full Rust gate + web `check`/`typecheck`/`test`/`build` green; verified in-browser by eye.

_Validate:_ un-authored devices render/flip **identically** through the generic `Panel` fallback (rebuilt
on `Chassis`); the **Scarlett 8i6** renders as a bespoke faceplate with **mixed-face I/O**, **red chassis**
in both the wall elevation and the top-down plan, and **section legends**, its controls driving the live
engine and its jacks patching with correct cable anchors; the preamps expose only **gain + power** (part 1;
INST/AIR/PAD/48V arrive in task **5.7.6**); the
**focus overlay** renders custom faceplates (and `Console` for
`channel_strip`) via the registry with the synth keybed intact; the **mount-test guardrail** proves every
registered faceplate references only valid ids and places all params/ports; the **layer rule holds** (no
layout vocabulary on the Rust descriptor); the full Rust gate (`cargo fmt --check && cargo lint && cargo
test && cargo wasm && cargo docs`) plus web `check`/`typecheck`/`test`/`build` pass; verified in-browser.

_Design notes — extended scope (part 2, folded in after 5.7.1–5.7.4 landed):_ The proving 8i6 is
deliberately reduced, and building it surfaced engine concepts missing to make it — or any faithful
interface — real. These are folded in as the tasks below rather than scattered to later stories (they were
first captured in `docs/IMPROVEMENTS.md`, now promoted here). Each carries its own rationale + done-state;
the larger ones (multichannel digital, routing, preamp physics) may be split into sub-tasks by the executor.

- **Task 5.7.5 — Device-level power gate (framework, not per-node).** Today each `GainStage` carries its own
  `powered` param, so a multi-node device exposes one power switch *per stage* — the 8i6 already shows 4,
  and adding line-I/O stages (5.7.8) would multiply them. Introduce a **device-level power** the faceplate
  presents once (a real interface is bus-powered — a single state), the per-node gates becoming an
  implementation detail driven by it (or one framework-level gate applied at the device boundary). This is
  the "generic framework-level power" deferral first noted in Epic 4 / Story 4.2. _Done:_ the 8i6 presents a
  single power control that silences the whole device (de-clicked, no recompile); single-node devices'
  power still works; catalog-alignment + engine tests green.
- **Task 5.7.6 — Preamp physics: INST / PAD / AIR / 48V.** The honest backing for the 8i6's front-panel
  switches (omitted in part 1 because none was modelable in today's engine). Each needs real work, in
  rough order of ease:
  - **PAD** — input attenuation: a smoothed in-`process` multiply (like `powered`), audible immediately.
  - **INST / hi-Z** — switches the preamp's **input impedance**. The loading divider is **baked at
    compile** (`schedule.rs`, from the port's static `InputZ`), so this needs either a recompile-on-toggle
    (structural, like repatching) or a runtime-re-solvable divider driven by a param. Audible payoff is
    latent until Epic-5 hi-Z sources exist, but the impedance change is real. Oracle: line-Z vs inst-Z
    divider loss against a constructed high-output-impedance source (§9).
  - **AIR** — an analog **high-shelf** filter (new analog DSP; EQ is digital-only today).
  - **48V phantom** — the hard one: `CondenserMic` self-emits phantom (it flows upstream through the
    pull-based DAG, approximated at the mic), so a preamp switch has **no path to reach the mic**. Real
    phantom needs a small **upstream phantom-supply side-graph** (mirroring the planned ground/clock
    side-graphs) — which is an Epic-5 decision-level piece; **48V may stay deferred** if that side-graph
    isn't built here, while INST/PAD/AIR land. _Done:_ the preamp exposes gain + the modeled switches with
    hand-calc oracles where analog; the 8i6 faceplate shows them; engine gate green.
- **Task 5.7.7 — Multichannel digital ports.** Every digital port today is a single mono lane
  (`lane_count() == 1`); a real USB (or ADAT/S-PDIF) connector carries **many channels bidirectionally over
  one physical connector**. Add a **multichannel digital port/lane** concept (a port with N lanes behind one
  jack) so an interface's USB is one connector, not the several mono digital ports the 8i6 fakes now. This
  is the Epic-1-deferred "multichannel digital ports (ADAT 8-lane etc.)" item — large; touches the port/lane
  model, `compile`, and the wasm/UI descriptor. _Done:_ a device declares a multichannel digital port; the
  8i6's USB becomes one; existing mono digital paths unchanged; engine + `cargo wasm` gate green.
- **Task 5.7.8 — 8i6 full analog I/O + S/PDIF.** With device power (5.7.5) and multichannel digital (5.7.7)
  in place, grow the reduced 8i6 to the real unit: add the **rear line inputs** and the **additional line
  outputs** (so it reads as ~8-in/6-out, not 2-in/1-line-out) and **S/PDIF** in/out (deferred in part 1).
  Extend the catalog entry (more AD/DA/gain nodes) and place the new I/O on the faceplate (front vs back per
  the real panel). _Done:_ the 8i6's exposed face matches the real unit's I/O count; catalog-alignment +
  `instantiate` remap tests updated; the faceplate places everything (guardrail green); in-browser.
  - _Also delivered here — the `computer` peer + round-trip latency._ Shipped a minimal `computer` USB peer
    (8-lane send in, 6-lane return out; per-lane send meters; an 8→6 loopback `Matrix`, default send 1→
    return 1). Closing the monitoring loop through it (8i6 → computer → 8i6 → speaker) is a graph cycle in
    the delay-free DAG engine, so added a **delayed-edge** primitive: `Graph::connect_delayed` marks an edge
    that is **cut from the topological sort** and served from the persistent output pool (its pre-loop copy
    reads last block's value), giving exactly **one block of round-trip latency** — physically the DAW's
    playback trailing its input. The `computer` declares its USB output a latency source
    (`CatalogEntry.delayed_outputs`); `build_patch` wires such edges delayed. **The schedule stays a strict
    DAG** — feedback is expressed as bounded latency, not a same-block solve. The default scene is now this
    playable loop. Oracles: a delayed two-node loop compiles (an undelayed one still errors), the delay is
    exactly one block, and the full 8i6↔computer loop builds and is audible.
- **Task 5.7.9 — Runtime routing matrix (engine concept + focus-view UI).** Interfaces and mixers route any
  input to any output through an internal **matrix** (the 8i6's is Focusrite Control). We have **no
  runtime-configurable routing** — internal wiring is fixed `InternalEdge`s and inter-device signal is fixed
  graph edges. Add a **routing abstraction**: per the routing-seam note in `crates/devices/src/catalog.rs`,
  runtime-switchable routing "is **not** a topology change — it lives inside a node behind a control param",
  so a **params-driven matrix node** (route input *i* → output *j* via matrix params, no recompile) is the
  intended shape (vs. user-repatching, which recompiles). Surface it in the **focus view** as a matrix grid
  (rows = inputs, cols = outputs), registered as the interface's focus surface (the part-1 registry already
  supports per-device focus surfaces). _Done:_ a routable node routes inputs → outputs at runtime via matrix
  params; a focus-view matrix grid drives them; engine + web gate green; in-browser.
- **Task 5.7.10 — Device dimensions pass.** The catalog `FormFactor` boxes are rough guesses (the 8i6 was
  authored 210×50×150 mm; several devices likely approximate). Do a pass against real gear so the spatial
  world's relative sizes read right. Small. _Done:_ dimensions reviewed/corrected; `catalog_carries_sane_form_factors`
  green; the spatial layout looks right in-browser.

_Validate (part 2):_ the 8i6 is a **faithful** interface — full analog + digital I/O (multichannel USB,
S/PDIF, rear line ins, line outs), a **single power** control (not per-stage), honest **INST/PAD/AIR** (and
48V if the phantom side-graph lands here), and a **routing matrix** in its focus view assigning inputs to
outputs at runtime; a `computer` USB peer completes the **playable monitoring loop** — sound travels
mic/synth → preamp → AD → USB → computer → USB return → DA → monitor, closed through a **delayed edge**
(one block of round-trip latency; the schedule stays a DAG); device **dimensions** corrected; the full Rust
gate (`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`) plus web
`check`/`typecheck`/`test`/`build` pass; verified in-browser.

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

---

## Epic 6 — Device Workbench (developer tooling)

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

- **Story 6.1 — Engine-session extraction.** — ✅ **Complete** (elaborated below). Pulled the
  engine/UI plumbing out of the `App.svelte` monolith into a shared session layer both views consume
  (`SceneSession` + `PatchController`). Pure refactor — the scene view behaves **identically** (the
  Story's validate gate). This is the load-bearing story; the workbench is only as solid as this seam.
- **Story 6.2 — Route + workbench shell.** — ✅ **Complete** (elaborated below). The app's first URL
  routing (`/devices/<typeId>` vs the scene view), a suspended-context boot that gets the catalog pre-
  gesture (resume-on-interaction), and a **dedicated** bench stage: mm grid + rack-unit ruler from the
  descriptor's form factor, both faces rendered simultaneously, live params/meters through the shared
  session. Device-only (silent) — the rig lands in 6.3. Unknown `typeId` → a catalog index page.
- **Story 6.3 — Bench patching + URL-persisted temp scene.** — ✅ **Complete** (elaborated below).
  **No auto-rig** (revised at planning): the bench carries a **fixed supporting cast** — synth source,
  DA, speaker — around the device-under-test, and the user patches source→DUT→monitor **by hand** with
  cables. The tap (`patch.output`) is set by **clicking an analog output jack** (default: the DUT's first
  analog out). Patching drives the **existing** `PatchController` + `cable-view`/`jack-anchors` (never a
  second cable impl — the Epic's load-bearing watch-out), which forces decoupling `cable-view` from the
  scene-spatial `LayoutCtx` behind a small injected layout interface. The whole temp scene (rig + param
  overrides + tap) serializes to the URL query as `base64url(JSON)` (path = `typeId`), versioned +
  regenerate-on-mismatch, debounced `replaceState`. **Also here — bench stage-surface upgrade:** a
  `WorldApi`-shaped surface (`clientToSurface` dividing out the scale) to measure jack anchors + draw
  cables, plus **cursor-anchored zoom** (scroll-anchored; keep the current scrollbar pan — no drag-to-pan,
  no fit/reset). Consequence of no-auto-rig: audibility is **user-driven** (patch the chain, then play via
  the reused keybed), softening the Epic's "pre-patched … audio after one gesture" phrasing.
- **Story 6.4 — Debug surface + the hot loop.** A bench debug panel: every param (value + descriptor
  range), every readout, engine health (overruns, worst render ms, event/param drops), connection
  losses, config-vs-param distinction (recompiles made visible), and a seed control for deterministic
  noise comparisons. Close the dev loop: a `wasm:watch` script (rebuild on Rust save → Vite full
  reload → URL restore → auto-resume). Candidate stretch: the bench **signal generator** device if the
  synth proves too blunt an oracle.

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
