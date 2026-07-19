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

**Detail gradient (concretely):** Epics 1–4 and 6 are built — their completed Stories carry full design
notes and per-task delivery records, now **archived to `EPIC_<N>_NOTES.md`** with only a summary kept
here (Epic 4's one deferred Story, 4.7, keeps its coarse sketch; Epic 6's Story 6.4 is implemented, pending
verify + commit). **Epic 5 stays at Story level** — its Tasks get written when we reach them. Don't
over-plan work whose shape the earlier work will change.

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
  machinery, with the transport/subscription model as the net-new piece. _The link/forwarding layer
  **beneath** this (bidirectional links + simplified L2 switching) is **Story 5.9** — the two stack:
  5.9 is layer 2, 5.2 is IP-and-up._
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
- **Story 5.8** — 48V phantom power: balanced preamp front-end, a patchable condenser mic, and the
  compile-time phantom DC operating-point solve (retiring the mic's self-`powered` flag).
  ✅ **Complete** (on `e5-s8/phantom-power`, Validate met; see the Story block below).
- **Story 5.9** — **Bidirectional digital transport & simplified L2 forwarding.** Part 1 — **duplex
  single-connector links** (USB-C = one jack, both directions) + **topology-derived delay** (`compile`
  auto-breaks digital cycles) — ✅ **delivered & verified in-browser** (see the Story 5.9 block below).
  The L2 forwarding layer proper — addressed flows, switches, clock domains — remains **deferred**
  (feeds Story 5.2/5.3; the design work is parked in the Story 5.9 block).
- **Story 5.10** — **Dynamic computer I/O**: the computer stops hardcoding the 8i6's USB shape and
  adapts to the attached interface's **published** channel counts (lane-aware engine nodes +
  config-driven host enumeration). ✅ **Done** (see the Story block below).
- **Story 5.11** — **Computer as a minimal DAW**: the computer grows arbitrary mono tracks that arm to
  USB sends, record to WAV files on disk (OPFS), and play back to USB returns through an in-sim routing
  matrix + simple level mixer, with a transport (play/stop/record) clocked by the **in-simulation digital
  domain**. Host is dumb byte storage; the sim owns all audio. 🚧 **In progress** (see the Story block below).

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

### Story 5.8 — 48V Phantom Power — ✅ **Complete**

_Goal:_ Make +48 V phantom power **honest end-to-end**: a patchable **condenser mic** whose power
arrives from the device it's plugged into, a **balanced preamp front-end** that separates the phantom
pedestal from the audio the way real hardware does, and a **48V switch on the 8i6** that actually
reaches the mic. Retires the last in-domain label on the signal path — `CondenserMic`'s self-asserted
`powered` flag (condenser.rs's documented "informed approximation") — by replacing it with a
**compile-time DC operating-point solve** over the patch's real connectivity. Anchors to
PROJECT_PLAN §2 (phantom emerges from voltage physics, never a flag), §5.3 (local solve at compile /
per-sample forward), and the Epic-4/5 layer rule (catalog owns specs, web owns look & feel). Closes the
one switch Story 5.7.6 left deferred ("48V may stay deferred if that side-graph isn't built here").

_Watch out:_

- **The DC solve is compile-time physics, not a label.** The interconnect is linear (nonlinearity lives
  in devices) and the phantom network is static (48 V behind fixed resistors), so **superposition**
  splits the problem: solve the DC bias network once at compile (the operating point — SPICE `.OP`),
  superpose the AC signal per-sample forward as today (`osku_physics_concepts.md` §17). Do **not** try
  to push per-sample voltage backwards through the pull DAG — the stationary DC axis is exactly what
  compile-time solving is for, the same trade the loading divider already makes.
- **Common-mode rejection cannot be applied after a nonlinearity.** The 48 V pedestal must be removed
  **before** the preamp's gain/rail clamp (`clamp(48·g, ±10)` on both legs pins the rail and
  annihilates the differential — §17). Difference-first is *exact* at the ideal-CMRR altitude.
- **Hot-path contracts hold:** the resolution pass allocates/fails only in `compile`; `process` stays
  alloc-free, panic-free, branch-light. The capsule tone goes through the seeded/deterministic
  machinery (no ambient entropy; a phase accumulator, not `sin` of wall-clock).
- **Don't break the default scene:** the synth (unbalanced) feeds the 8i6 preamp today. Making the
  preamp balanced requires the unbal→bal edge rule to land **first**, and a regression oracle that the
  synth→preamp loading gain is *numerically unchanged*.
- **Scope guards:** single-edge resolution only (mic directly into the supplying input); no finite
  CMRR; no per-sample supply modulation; no back-driven DC into non-phantom sources (48V engaged with a
  synth plugged in simply resolves nothing — the real-world "mostly harmless" case, simplified).

_Design notes (settled at planning):_

- **48V is a structural config (recompile-on-toggle), like INST.** The DC network *is* topology; when
  it changes, recompile — the same pattern as repatching and the INST impedance switch, riding the
  existing `ConfigDescriptor`/schedule-swap machinery. Acoustically safe: the pedestal cancels at every
  balanced receiver in both states, so the swap can't click. _Rejected: a runtime 48V param with a
  compile-baked cross-node "supply feed" scaling the mic's pedestal through the supplier's smoother_ —
  it buys the seconds-long RC charge-up ramp (mic audibly fades in) but needs a new cross-node param
  mechanism; recorded as a possible later upgrade, not this story.
- **Resolution lives in engine `compile`, declared via `Node`-trait hooks.** Nodes declare phantom
  roles per port — a supply (`48 V` behind `6.8 kΩ` per leg, engaged or not) and a load (a DC
  resistance + minimum operating volts). These are **circuit-topology declarations**, the same class of
  port-fact as `InputZ`/`OutputZ` — not labels on the signal. `compile` walks analog edges; where an
  engaged supply faces a declared load it solves the DC divider (reusing the §5.3 local-solve
  primitives) and hands the producer its terminal volts via a `prepare`-like hook (default no-op).
  _Rejected: resolving in `build_patch`_ — less engine churn but puts electrical physics in the product
  layer, against the layer rule.
- **Sag and dead-mic emerge from the solve.** Supply = 48 V behind 2×6.8 kΩ (3.4 kΩ effective for
  common-mode current) + the connection's cable series R + the mic's DC load (constant-resistance
  model). Hand calc at a ~3 mA-class load (12.7 kΩ), no cable: `48·12 700/(3 400+12 700) = 37.86 V`.
  The mic runs iff terminal volts ≥ its declared minimum — a threshold in the mic's own electronics,
  the same species as rail clipping, not a flag. _Rejected: constant-current load_ — makes the solve
  nonlinear (iterative) for no audible payoff; constant-R keeps it a divider.
- **Unbalanced-into-balanced is an edge rule, not an adapter node.** A 1-conductor output into a
  2-conductor analog input becomes legal in `compile`: the hot lane gets the normal divider transform,
  the cold lane is **grounded (0 V)** — literally what a TS plug in a combo jack does (sleeve shorts
  the cold pin). The differential divider formula is unchanged (`Zin/(Zout+Rc+Zin)`), so existing
  unbalanced sources keep their exact gain. _Rejected: a build-inserted adapter node_ — electrically
  wrong: its own Z faces split one loading divider into two near-unity ones, silently deleting the
  loading loss.
- **`MicPreamp` grows a balanced front-end: difference-first, then the existing chain.** Input becomes
  `InputZ::balanced(...)` (2 conductors); `process` computes `s = V+ − V−` (exact pedestal/hum
  cancellation at ideal CMRR), then PAD → gain → rail → AIR → power as today (2-in/1-out node shape:
  the `BalancedReceiver` precedent). The per-leg coupling-caps topology becomes distinguishable only
  under finite CMRR — deferred with it. The declared `InputZ` lumps the phantom feed network's AC
  loading, as `InputZ` already lumps everything.
- **The capsule is a declared boundary stand-in: a deterministic test tone.** Acoustics are out of
  scope (PROJECT_PLAN §2), so `CondenserMic`'s capsule emits an internal sine (level + frequency
  params, mic-level ~10 mV default, phase-accumulator deterministic), riding the *resolved* pedestal:
  `V± = V_dc ± s/2`. Toggling 48V audibly kills/raises the tone — the story's in-browser payoff.
- **Deferred — the "air link" story (separate, not this one):** the missing abstraction from a
  vibrating source (speaker cone, vocal cords, string) over air to the capsule. Not accurate
  acoustics — a simple "analog wave over the air" carrier with a transduction seam at each end
  (pressure↔volts is the declared boundary). Noted at planning: acoustic feedback (mic hears speaker)
  is already expressible via the delayed-edge primitive — one block ≈ 2.7 ms ≈ 0.93 m of air at
  343 m/s, so the forced latency is nearly physical — and the open question is how an air path is
  "patched" (proximity/geometry, not a cable). Howlround as an emergent challenge is the payoff.
- **Known simplifications (not bugs):** single-edge phantom resolution (through-a-patchbay deferred
  with 5.1); fan-out from one mic into two supplying inputs unresolved (deferred, real-world-weird);
  no DC back-drive onto non-load sources; the ramp-up transient (structural toggle is instant).

- **Task 5.8.1 — Unbal→bal edge rule (`engine::compile`).** Make a 1-conductor analog output into a
  2-conductor analog input legal: hot lane = the normal baked divider transform, cold lane = 0 V
  (TS-in-combo physics). Conductor inference, pool sizing, and interference coupling (pickup/hum still
  lands on both conductors) updated. _Done:_ oracle — the divider gain equals the 1→1 case exactly
  (hand calc in comment); a driven balanced receiver recovers the full signal (`s − 0 = s`); existing
  1→1 and 2→2 paths byte-identical; engine gate green.
- **Task 5.8.2 — `MicPreamp` balanced front-end.** Input face → `InputZ::balanced`; `process` takes
  `V+ − V−` first, then the existing PAD/gain/rail/AIR/power chain. The 8i6 catalog entry keeps
  working via 5.8.1 (synth still plugs in). _Done:_ oracles — a `48.005/47.995` pair comes out as
  `gain·0.01` with **zero** DC (pedestal rejected before the clamp); common-mode hum injected on both
  legs cancels; the default scene's synth→preamp gain is numerically unchanged (regression hand calc);
  engine + devices tests green.
- **Task 5.8.3 — Phantom declarations + the compile-time DC solve.** `Node`-trait hooks for supply
  (volts, per-leg feed R, engaged) and load (DC resistance, minimum volts); `compile` resolves each
  analog edge's operating point and delivers terminal volts to the producer via a default-no-op hook.
  `CondenserMic` consumes it: pedestal = resolved volts, dead below its minimum — **the `powered`
  flag is deleted**. `MicPreamp` grows the supply declaration (engaged via constructor ← config).
  _Done:_ oracles — `48·12 700/16 100 = 37.86 V` (no cable), sag grows with cable R (hand calc), a
  long/lossy enough run drops below the minimum ⇒ silent mic, no supply (or disengaged) ⇒ 0 V ⇒
  silent, supply facing a non-load ⇒ no-op; engine gate green.
- **Task 5.8.4 — Capsule test tone.** `CondenserMic`'s capsule emits a deterministic internal sine
  (smoothed level + frequency params, ~10 mV default) differentially on the resolved pedestal:
  `V± = V_dc ± s/2`. _Done:_ oracles — output common-mode = `V_dc` exactly, differential amplitude =
  the level param; deterministic across runs (same seed ⇒ identical buffers); no-alloc test still
  green.
- **Task 5.8.5 — `condenser_mic` catalog device + the 8i6 48V config.** Catalog entry (balanced XLR
  out, level/freq params, phantom-load declaration, sane `FormFactor`); the 8i6 gains a **48V**
  `ConfigDescriptor` (one switch, both preamps — matching the real unit) mapped to the preamps'
  supply-engaged constructor arg. _Done:_ `catalog_aligns_with_exposed_face` +
  `descriptors_carry_engine_truth` + an `instantiate` remap test cover the mic; toggling the 8i6's
  48V config rebuilds with supplies engaged; devices tests green.
- **Task 5.8.6 — Web: 48V switch + mic in the world.** The 8i6 faceplate presents the 48V toggle (the
  INST config-toggle pattern; button + LED); the mic renders via the generic `Panel` fallback (no
  bespoke faceplate — it earns one later); an XLR cable patches mic → combo 1. _Done:_ in-browser —
  48V off ⇒ silence, on ⇒ the capsule tone through preamp → AD → computer loop → monitors; faceplate
  mount-test guardrail green; `pnpm run format` + web `check`/`typecheck`/`test`/`build` green.

_Validate:_ a condenser mic patched into the 8i6's combo input is **silent until the 8i6's 48V switch
is engaged** and audible through the full monitoring loop after; the pedestal is genuinely present on
the wire (common-mode `V_dc`) and **cancels at the balanced front-end before any nonlinearity**; sag
emerges from the DC divider with hand-calc oracles (`37.86 V` at the reference load; below-minimum ⇒
dead mic); the synth still plugs into the same combo jack with **numerically unchanged** gain; the
mic's self-`powered` flag is gone from the engine; the full Rust gate (`cargo fmt --check && cargo
lint && cargo test && cargo wasm && cargo docs`) plus web `check`/`typecheck`/`test`/`build` pass;
verified in-browser.

*Delivered:* all six tasks as planned (one commit each), Validate met in-browser (mic silent →
48V on → capsule tone through the monitoring loop → off → silent). Deviations from plan: none
structural; the notable in-task design choices, for future reference:

- **5.8.1 (grounding edge):** the cold leg is an ordinary `EdgeTransform` with `gain = 0` (no new
  `EdgeKind` variant, no second hot-path arm), its source index clamped to the hot lane so nothing
  can index out of bounds; step-7b interference already cloned onto every conductor, so
  common-mode pickup/hum on the grounding edge came free. Matched edges bake byte-identically.
- **5.8.2 (balanced front-end):** `MicPreamp::new` kept its `z_in: InputZ` signature with a
  construction-time balanced assert (the rail-check precedent) rather than switching to bare
  `Ohms` — honest about the declared face, no call-site ripple.
- **5.8.3 (DC solve):** declarations landed as `PhantomSupply`/`PhantomLoad` in
  `electrical/phantom.rs` with the solve as `PhantomSupply::terminal_volts` **reusing
  `divider_gain`** (compile stays a thin walk); `MicPreamp` grew a chainable
  `with_phantom(engaged)` builder (default off) instead of a positional bool; the fan-in guard
  (`CompileError::PhantomFanIn`) counts only *engaged* supplies.
- **5.8.4 (capsule tone):** the oscillator **free-runs while dead** (power folds into a per-block
  gate multiplier — no per-sample branch, phase continuity on re-power); unprepared ⇒ phase step 0.
  Noted at review: when the air-link story lands, **FREQ dies with the internal sine** while
  **LEVEL mutates into capsule sensitivity** (the pressure→volts mV/Pa figure, likely a catalog
  spec rather than a knob).
- **5.8.5 (catalog):** the both-preamps-from-one-key test reads the existing input-meter Peak
  readouts (one build proves both channels); devices-crate tests kept the house abs-diff style
  (no new `approx` dev-dep).
- **5.8.6 (web):** a new mm-scaled `ConfigButton` hardware widget (the rem-scaled `ConfigSwitch`
  stays a focus-surface aesthetic) with a **red** engaged lamp (phantom hardware convention); the
  interactive 48V lives on the faceplate only (real front-panel button), trivially addable to the
  Focusrite Control surface later; the mic renders via the generic `Panel` fallback with a
  one-line skin. Field note: a config widget whose key misses the descriptor renders nothing —
  a stale WASM artifact thus shows as a zero-width div; a dev-mode warning in that branch is a
  candidate hardening (IMPROVEMENTS.md).

### Story 5.9 (part 1) — Duplex digital transport & topology-derived delay — ✅ **Delivered**

_The broader Story 5.9 is a **simplified OSI-Layer-2** link/forwarding layer beneath Story 5.2's
IP/subscription (Dante/AES67) layer. **Part 1** — bidirectional single-connector links + automatic
latency insertion — is delivered here; the L2 forwarding proper (addressed flows, switches, clock
domains) stays deferred to 5.2/5.3 (see "Deferred" below, where the idea-gathering is kept intact)._

_Goal:_ Digital connections were unidirectional (one `OutputPort` → one `InputPort`, an
`EdgeKind::DigitalRoute` sample copy), so a duplex link (USB-C, Ethernet) had to be authored as two
separate one-way cables. Make one physical connector carry **both directions** (the 8i6/computer USB as
one USB-C jack), and make digital feedback loops **robust** — a monitoring loop through a duplex link
must build and sound without the author hand-placing a delayed edge.

_Calibration — most of this was already right:_ per-strand fidelity is correctly **absent** (a digital
edge is an ideal lossless copy — no `Lifted`/per-conductor one-pole/common-mode; that machinery is
analog-only); multichannel-behind-one-connector already existed (`AudioFormat.channels`,
`DigitalMux`/`DigitalDemux`, the 8i6's 8-ch send + 6-ch return). The **only** trait digital inherited
from the analog cable was the **unidirectional edge** — and that is the **DAG invariant** (§5), not a
leftover: a wire carrying live data both ways in one block is a cycle. So a duplex link is an
**authoring-layer** construct that lowers to two engine edges; the engine datapath stays unidirectional.

_Design decisions (settled before building):_

- **#2 delay is topology-derived, in `compile` (keep the hint).** `compile` auto-breaks any residual
  **digital** cycle by delaying the lowest-index digital edge on it (deterministic; a delayed edge is an
  ideal digital copy carrying one block of round-trip latency, physical only on a buffered digital link).
  An all-analog cycle has nowhere to carry the latency and still rejects as `CompileError::Cycle`.
  `delayed_outputs` stays a **hint**: a build-declared latent output (the DAW/computer) is pre-marked
  `delayed` and excluded, so `compile` only breaks what the author didn't — and the block of latency
  lands on the physically-correct leg (playback trails input). _Rejected: pure topology (drop
  `delayed_outputs`)_ — arbitrary latency placement within a loop. _Rejected: keep it device-declared_ —
  fragile to topology the author didn't foresee (a duplex link between two non-latent devices never
  breaks).
- **#1 fidelity: one duplex jack.** A device declares an `(output, input)` pair as one connector; the web
  draws one USB-C jack and one cable; the engine still sees two edges.

_Shipped:_

- **#2 (engine).** `topo::reaches` (compile-time reachability); an auto-break loop in `compile` (step 5c)
  that delays a digital edge on each residual cycle; `CompileError::Cycle` re-documented as the
  unbreakable (analog-feedback) case. Oracles: a digital 2-cycle of ideal edges auto-compiles; an
  auto-broken digital self-loop integrates one block/block (identical to a manual delayed edge); an
  all-analog cycle still errors; the computer monitoring loop's latency still lands on the playback leg.
- **#1 (Rust core).** `CatalogEntry.duplex_links: &[(out_id, in_id)]` (8i6 `(0,7)`, computer `(0,0)`) →
  `PortDescriptor.duplex_partner` (JS `duplexPartner?`, `skip_serializing_if` so one-way jacks omit it);
  `Connection.duplex: bool`; `build_patch` expands a duplex connection into **both** directed edges (the
  reverse via each jack's partner), erroring `BuildError::NotDuplex` on a non-duplex port; connection-loss
  accounting stays 1:1 with scene connections (the digital reverse leg is untracked). Oracle: one duplex
  USB connection builds the full mic→8i6→computer→8i6→speaker loop and is audible; a duplex flag on a
  one-way jack errors.
- **#1 (web).** `DuplexSocket.svelte` renders the 8i6/computer USB as one jack; `Jack` carries a
  `data-jack-alt` for the partner leg and `jack-anchors` registers it at the same centre, so a cable
  anchors either direction at the one jack; `evaluateConnection` joins two duplex jacks into a
  `duplex:true` connection and **skips the feedback-loop check** (a duplex link is a cycle by design);
  `patching` carries `duplexPartner` through; the faceplate guardrail credits a `DuplexSocket` as placing
  both its ports.
- **Follow-up bug fixed in-story.** `commitCable` rebuilt the stored `Connection` from `from`/`to` only,
  **silently dropping `duplex`** → every duplex cable committed one-way (no return leg → silent). Fixed to
  spread the whole verdict connection; regression test in `scene-ops.test.ts`. Found via a user-reported
  no-sound patch; the engine/build side was correct from the first commit — the symptom was UI
  persistence.

_Validate:_ the 8i6 and computer each show **one** USB-C jack; plugging **one** cable authors a `duplex`
connection that `build_patch` expands to both edges, and the mic monitoring loop closes and sounds; a
UI-authored digital 2-cycle auto-compiles while an analog cycle still errors; the full Rust gate (`cargo
fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`) plus web `check`/`typecheck`/`test`
pass; verified in-browser (delete + re-plug the USB-C cable → sound).

*Delivered:* #1 + #2 across five commits on `e5-s9/duplex-digital-transport`; Validate met in-browser.
Notable finding: the one-way-cable symptom was a UI persistence bug (`commitCable` dropping `duplex`),
not an engine/build defect — the duplex expansion + auto-break were correct throughout.

_Deferred to 5.2 / 5.3 (the rest of the L2 vision — idea-gathering kept intact):_

- **(c) Addressed flows — the net-new carrier concept.** The lane model is positional (source lane _k_ →
  dest lane _k_), right for interfaces/ADAT/USB (a fixed compile-time bundle). An L2 **switch** forwards by
  **address**, not position: an ingress flow carries a destination, the switch picks egress from a
  forwarding/subscription table. Nothing in `SampleBuffer`/`Lane` carries a flow identity. The heart of the
  networking work — decide whether it reuses `Sample` lanes + a routing table or gets its own addressed
  carrier.
- **(d) Channel-count equality is interface-only.** `build_patch`'s `ChannelCountMismatch` (equal counts)
  is right for a point-to-point cable, wrong for a switch (forwards a subset) and for per-direction duplex
  — it becomes a per-flow/per-direction check, not an edge invariant.
- **(e) Clock domains are the true prerequisite (5.3).** Two independently-clocked devices on one link are
  an async boundary; the return leg's latency is really elastic-FIFO slip, not a fixed block (today: one
  clock domain, cross-rate rejected as `ClockCrossingUnsupported`). The one-block delayed edge is a
  stand-in; "clock not locked → dropouts" depends on 5.3.
- **Design leanings for the switch (confirm at 5.2 planning):** subscription changes as
  **recompile-on-change** (structural, like INST/48V) to keep the DAG static; the within-device 14×14
  `Matrix` is the runtime-routing precedent for the between-device table; make consequences **emergent**
  (per-hop latency, bandwidth exhaustion → dropouts, missing subscription → silence, clock-not-locked);
  **skip** MAC/VLAN/QoS/frame encoding unless they produce an audible/measurable consequence (no flagging —
  §4/§9).

_Open decisions (non-blocking):_

- **(i)** `SCHEMA_VERSION` was **not** bumped for duplex, so a pre-duplex saved scene with a one-way USB
  link loads verbatim (silent monitoring). Left as-is because a one-way USB is a *legal* patch — decide
  whether to bump (force-discard stale benches) when it bites.
- **(ii)** the web's `wouldCreateCycle` still rejects a **one-way** digital loop authored by drag even
  though the engine (post-#2) would auto-break it; only duplex bypasses it today. Make the web cycle check
  domain-aware if UI-authored digital loops become a case.

### Story 5.10 — Dynamic computer I/O — ✅ **Done**

_Goal:_ The `computer` hardcodes the 8i6's USB shape — 8 sends / 6 returns baked into its catalog
entry as 11 nodes, 22 hand-wired internal edges, and statically authored meter/grid labels. A real
computer has no channel count of its own: it enumerates whatever the attached interface's driver
publishes. Make the computer **adapt to the attached interface**. The publication half already exists —
`PortDescriptor.channels` is engine truth derived from the port face's `lane_count()` — so this story
builds the consumption half: the computer's shape is **derived from the published face of what's
plugged in** (PROJECT_PLAN's derive-from-the-model rule; no parallel channel constant), with the
catalog owning specs and the web owning enumeration + look & feel (the Epic-4/5 layer rule). Payoff:
any future interface (a 2i2, a big rack unit) works against the same computer with no new peer device.

_Watch out:_

- **Hot-path contracts hold:** sized nodes allocate at construction only; `process` stays
  alloc-free/panic-free (per-lane loops, no per-N dispatch or growth).
- **Id stability is the trap.** The meter's per-lane readout ids must keep `(0, 1)` at n = 1 (the
  8i6's own meters address `PEAK_DBFS`/`RMS_DBFS`); matrix crosspoint ids are row-major `i·m + j`, so
  a change of M **reshuffles every id** — never remap saved params across a re-enumeration, reset
  them (design note below).
- **The "exposed face is config-independent" invariant is deliberately retired** (`instantiate`/
  `describe` docs, `descriptors()` built from `DeviceConfig::EMPTY`). Update the docs and the
  alignment-pinning tests to the new contract — don't work around them.
- **Don't touch the 8i6's own matrix face.** Its 14×14 `Matrix` genuinely needs mono ports (inputs
  arrive on separate wires from separate ADs/preamps); only the computer wants the lane-port variant.
- The type-level `descriptors()` catalog listing keeps working, showing the default (EMPTY-config)
  face.
- **Scope guards:** one USB port per computer (no hub); computer↔computer USB does **not** enumerate
  (both keep their configured/default shape — the equal-count check still guards the edge); per
  5.9's deferred (d), `ChannelCountMismatch` equality stays the right point-to-point rule and the
  backstop for hand-authored patches.

_Design notes (settled at planning):_

- **Multichannel-port nodes keep the topology shape fixed — no imperative-builder `CatalogEntry`.**
  The computer's node count only varied with N because meters/matrix speak mono ports (forcing the
  demux + N meter nodes + mux). With lane-aware nodes the entry is **two nodes and one internal
  edge** regardless of counts — meter-bank(N) → lane-matrix(N→M); the demux/mux disappear (they
  existed only to adapt lanes to mono ports). The static entry suffices: `NodeBuilder` already
  receives `&DeviceConfig`, so the builders construct sized nodes. _Rejected: the catalog module
  doc's anticipated "imperative-builder variant"_ — dynamic node/edge lists are heavier machinery
  than the problem needs; it stays available for a future genuinely-variable topology.
- **Channel counts are structural config (`usb_sends` / `usb_returns`), written by host-side
  enumeration.** When the UI connects/disconnects the USB duplex cable it reads the interface's
  published port `channels` and writes the computer instance's config (rebuilding via the existing
  config→recompile path). A loaded patch never enumerates — config serializes in the patch, so the
  IR stays self-describing. _Rejected: `build_patch` negotiation (sizing the computer from whatever
  is cabled to it)_ — breaks the compositional per-device expansion and makes a patch's meaning
  depend on inference rather than what's written.
- **Unattached default: 2×2 — the built-in sound card.** Realistic (a DAW with no interface shows
  the built-in device). Existing computer tests set 8×6 config explicitly; pre-story bench URLs that
  relied on the implicit 8×6 break (accepted, not a bug). _Rejected: keeping 8×6_ — the default
  would encode one specific interface.
- **Loopback default: diagonal over min(N, M)** (send k → return k) — the 8i6 matrix's own
  identity-default philosophy; every return carries signal out of the box. _Rejected: today's
  first-two-lanes-only loopback._
- **The config keys are hidden (host-driven).** Not declared in `configs`/`ConfigDescriptor` —
  undeclared keys already flow through the IR and `DeviceConfig::get_or` (no new `ConfigKind`
  widget needed); the faceplate displays the *detected* counts read-only, derived from the
  per-instance descriptor. You don't type your computer's channel count; you plug in an interface.
- **The descriptor becomes per-instance.** `describe` grows a config-aware path exported over wasm
  (`describe_device(type_id, configs)`); the type catalog keeps the default face. Readout labels
  ("Send k Peak/RMS") and the `GridSpec` row/col names are **generated** from the built face's
  counts — the same synthesis precedent as the grid's crosspoint labels.
- **Re-enumeration resets the matrix params to the loopback default.** Crosspoint ids reshuffle with
  M, so remapping saved `ParamSetting`s is wrong-headed — and a reset is what a real DAW does when
  the I/O device changes. Within one saved patch, config + params serialize together, so ids stay
  self-consistent.
- **Enumeration lives in web TS (`scene-ops`).** An authoring-layer act like
  `commitCable`/`disconnect`, sitting next to them with Vitest coverage; Rust's
  `ChannelCountMismatch` stays the backstop. _Rejected: a devices-crate helper over wasm_ — extra
  marshalling for ~10 lines of logic the TS layer already has the data for.

- **Task 5.10.1 — Engine: multichannel `DigitalMeter`.** Grow the meter to N lanes: one N-lane
  input, one N-lane exact-passthrough output, per-lane Peak/RMS readouts at ids `(2k, 2k+1)` — n = 1
  preserves today's face and the `PEAK_DBFS`/`RMS_DBFS` constants (keep `new` mono or grow an arg;
  call-site ripple is small either way). _Done:_ oracles — distinct per-lane constants read back as
  hand-calc'd dBFS (lanes at 1.0 / 0.5 / 0.25 → 0 / −6.02 / −12.04 dBFS peak, calc in comment; a
  full-scale sine's RMS = peak − 3.01 dB); n = 1 matches today's meter exactly; no-alloc test still
  green; engine gate green.
- **Task 5.10.2 — Engine: lane-port `Matrix`.** A construction variant whose face is one N-lane
  input / one M-lane output (instead of N + M mono ports), sharing the crosspoint-param and
  f64-summing core; crosspoint ids unchanged (row-major `i·m + j`). _Done:_ oracles — hand-calc'd
  gain-weighted sums across lanes match the mono-port matrix under the same gains; diagonal defaults
  route k → k; engine gate green.
- **Task 5.10.3 — Devices: the config-driven computer.** Rewrite the entry to
  meter-bank(N) → lane-matrix(N→M) with N/M from `usb_sends`/`usb_returns` (default 2×2, diagonal
  loopback); generated readout + grid labels sized from the built face; retire the
  config-independent-face invariant in the `instantiate`/`describe` docs; existing computer loop
  tests set 8×6 config explicitly. _Done:_ oracles — the default computer expands 2×2 (4 readouts,
  4 crosspoints, USB port channels 2/2); an 8×6-config computer reproduces today's playable-loop and
  duplex-cable tests unchanged; a mismatched hand-authored patch still errors
  `ChannelCountMismatch`; alignment-pinning tests updated; devices gate green.
  **Note (pulled forward from 5.10.4):** `build_patch`'s pre-compile channel-count check read the
  config-blind *type* descriptor (computer always 2×2), so an 8×6 loop was wrongly rejected. Fixed by
  making `describe` config-aware and adding the `describe_device(type_id, config)` seam, then keying
  `build_patch`'s `descs` by scene id from each device's config (dropping the `types` indirection). So
  the Rust-side config-aware descriptor already exists and is tested here.
- **Task 5.10.4 — wasm: export the per-instance descriptor.** The config-aware `describe` +
  `describe_device(type_id, config)` seam **already landed in 5.10.3** (build_patch needed it), so this
  task is now just the **wasm export** of `describe_device(type_id, configs)` + the **TS mirror** in
  `catalog.ts`. _Done:_ the descriptor for an 8×6-config computer carries 16 readouts / 48 grid params
  / 8-ch + 6-ch USB port faces over the wasm boundary; the EMPTY-config type catalog is unchanged;
  `cargo wasm` + full gate green.
- **Task 5.10.5 — Web: enumeration + adaptive faceplate.** `scene-ops` enumeration on USB duplex
  connect/disconnect (and interface removal): peer's published `channels` → computer config, matrix
  params reset to default, rebuild; `Computer.svelte`/`ComputerMixer.svelte` consume the
  per-instance descriptor (meters/grid resize; detected counts displayed read-only). _Done:_ Vitest
  for enumeration (connect 8i6 → 8×6; disconnect → 2×2; computer↔computer → no-op); in-browser —
  plugging the 8i6's single USB-C re-enumerates the computer (meters/grid resize) and the monitoring
  loop still sounds through the diagonal loopback, unplugging returns it to 2×2; `pnpm run format` +
  web `check`/`typecheck`/`test` green. **Note:** enumeration is a *gesture* (connect/disconnect), but
  the default scene ships the 8i6 pre-cabled to the computer — a loaded patch never enumerates, so the
  default scene **authors** the computer's 8×6 config (`scene-store.ts`, `SCHEMA_VERSION` 16→17). The
  per-instance descriptor is delivered to the UI by the worklet pushing a `deviceDescriptors` map (by
  scene id) on build and each hot-swap → `session.deviceDescriptors` → `session.descriptorOf(id)`, which
  the faceplate/focus render sites prefer over the static type catalog.

_Validate:_ an unattached computer presents **2×2** (its built-in audio); plugging the 8i6's single
USB-C cable **re-enumerates** it to 8 sends / 6 returns — the send meters and routing grid resize, and
the monitoring loop closes and sounds through the diagonal loopback; unplugging returns it to 2×2;
every channel count derives from the interface's **published** port face (no parallel constant
anywhere); the full Rust gate (`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo
docs`) plus web `check`/`typecheck`/`test` pass; verified in-browser.

### Story 5.11 — Computer as a minimal DAW — 🚧 **In progress**

_Goal:_ Bring the `computer` forward from a fixed monitoring loopback into a **minimal, honest DAW** — the
digital hub a real audio interface plugs into. It grows an **arbitrary number of mono tracks**; each track
**arms** to a USB send (the interface's inputs), **records** its audio to a **file on disk**, and **plays
that file back** to a USB return (the interface's outputs), through an **in-simulation routing matrix** and
a **simple level mixer**. A single **transport** (play / stop / record) drives it, clocked by the
**in-simulation digital clock domain** — not the host's capture clock. Anchors to PROJECT_PLAN §4 (the
computer is a black-box device on real digital ports), §5.6 (clock is a real rate inside the sim, not a
label), and §2's "**Not a DAW**" non-goal — which this story respects by staying strictly at the
**signal-path** altitude (record → route → level → play), the routing/monitoring/gain-staging lesson, with
production features (timeline editing, clip slicing, automation, tempo/grid, per-track plugins) explicitly
out. Retires the `computer`'s diagonal-loopback `Matrix` default (Story 5.10) in favour of a multitrack recorder.

_Why it serves the learning goal (reconciling §2's non-goal):_ record-arm, send/return assignment, input
monitoring, the routing matrix, and gain-staging in and out of the interface are **core audio-engineering
routing knowledge** — impractical to explore with real gear. This is that lesson made hands-on, not a music
production tool. The scope ceiling below is the guardrail that keeps it honest to the non-goal.

_Watch out:_

- **Clock: the transport runs on the in-sim digital domain, never the host.** The one running counter today
  (`Schedule::sample_pos`, `schedule.rs:318`) is the **analog-rate** external-event clock; `SampleBuffer`
  carries **no** position (only `SampleRate`/`BitDepth`/`ClockDomainId`, always `ClockDomainId::SINGLE`).
  The DAW playhead is a **new digital-domain sample counter** advancing the digital block length (**128
  samples @ 48 kHz**, analog 1024 ÷ M = 8; `alloc_lane`, `schedule.rs:1181-1228`) per processed block when
  playing. Frame it as the digital domain's **own** counter (forward-compatible with Story 5.3, where the
  DAW clock can drift from the interface clock) — do **not** map it onto the host `AudioContext.currentTime`
  or the analog `sample_pos`. Epic 3 already concluded a transport must ride the engine's own sample clock +
  a shared reference, not host time (`EPIC_3_NOTES.md:391-398`).
- **Host = dumb byte storage; the sim owns all audio.** The **only** thing crossing the sim↔host boundary is
  **opaque file bytes** ("append these bytes to file X" / "read bytes of file X" → OPFS). The `computer`
  (in the sim) **WAV-encodes** recorded samples and **WAV-decodes** playback bytes itself, in the digital
  domain; format, rate, timeline, and mix never leak to the host. A real DAW writes WAVs to disk; OPFS is
  that disk. _One nuance, not a layer break:_ the UI may decode a stored WAV **host-side purely to draw a
  waveform thumbnail** — a filesystem read for display, not the host doing audio.
- **No unbounded engine buffer; the audio thread never blocks on disk.** Recording **streams per block**:
  `process` writes the block's samples to a **pre-allocated ring** (zero-alloc); off the hot path the sim
  WAV-encodes into an **outbound byte ring** the host drains to OPFS asynchronously. Playback is the mirror
  (host fills an **inbound byte ring** ahead of the playhead; the sim decodes per block into a playback
  ring; `process` reads it). Disk slowness surfaces as an honest under/overrun, not an audio glitch. The
  whole take **never** lives in engine memory.
- **Hot-path contracts hold.** `process` stays zero-alloc / panic-free / branch-light (sample rings only).
  WAV encode/decode and ring servicing run **off** `process` (like the readout-lane refresh / capture),
  with pre-sized scratch — no allocation on any path once compiled. Fixed known format ⇒ decode is a total
  fixed transform.
- **Determinism holds.** Playback bytes are **external input** on the same footing as note events — same
  fed stream ⇒ identical output. Recording is a pure tap (no effect on the live signal). No ambient entropy.
- **Don't break the default scene.** The mic/synth → 8i6 → computer → 8i6 → monitor loop must keep sounding:
  the default computer ships **one track input-monitoring send 1 → master (return 1)**, transport stopped,
  nothing recorded. This retires the all-diagonal loopback (a behaviour change — bump `SCHEMA_VERSION`). The
  **delayed USB-return output** (`delayed_outputs`, one block of round-trip latency) is unchanged.
- **Mono now, stereo later — foundation open, node mono-baked (honest status after 5.11.3).** The
  _primitives_ are all channel-agnostic and stereo-ready: `WavSpec { channels }` (interleaved WAV),
  `ByteRing` (byte-agnostic — interleaved L/R streams with frame = `channels × 4`, tears still impossible),
  `Transport` (channel-agnostic), and the **per-track** level fader (one fader = a whole stereo track). The
  `MultitrackRecorder` node itself, however, **bakes one-lane-per-track today** (`input` is one send lane per
  track; one playback lane + one ring per track; a single-4-byte-frame per-sample loop; `set_input(track,
  lane)`) — and the crossbar `Matrix` would likewise route a stereo pair as two lanes —
  chosen for a simpler, fully-tested mono node, since the epic is mono-only and there is **no stereo source
  to exercise a stereo track end-to-end**. Going stereo is therefore a **contained node-local refactor**
  (per-track lane _list_ + `channels` count → interleaved-PCM rings → an inner per-channel `process` loop +
  `set_input(track, &[lane])`), ~40 lines confined to `multitrack.rs` and its callers — **no change to
  `Transport`, `ByteRing`, `wav`, the schedule, or any other node.** Build it when a stereo source/use
  actually exists; not speculative infra now. _(Supersedes the earlier "a track owns a list of lanes (1
  today); no API that assumes one-lane-per-track" wording — the node does assume it; the foundation doesn't.)_
- **Scope guards:** signal-path + a simple level mixer (faders) only. **OUT:** timeline editing, clip
  slicing/arranging, automation, tempo/grid/quantize, per-track inserts/plugins, undo, multi-clip-per-track.
  One transport, tracks share it. No punch ranges — record = write from the playhead while transport rolls.

_Design notes (settled at planning):_

- **The mixing/routing/transport lives in the simulation, not the host (the headline decision).** The
  `computer` device owns the routing matrix (track → return), the per-track + master **level faders**, the
  input monitoring, and the digital-clock playhead. The host supplies only **raw file bytes** and issues
  transport **commands**; it does no audio. This aligns with the clocking call (the digital domain owns its
  timeline and mix bus) and the Epic-4/5 layer rule (engine = signal, web = app/UI + now the disk).
  _Rejected: a host-side mixer_ (engine exposes send lanes + injects pre-mixed return streams) — thinner
  engine, but the mixer/timeline wouldn't be "in the sim" and the transport clock would drift toward host
  ownership, against §5.6.
- **One bidirectional file-byte seam, not two audio taps.** Earlier framing had the engine stream raw audio
  samples to the host (record) and take samples back (playback) — which leaks audio format/rate/timeline to
  the host. Superseded: the seam is **opaque bytes for file storage** (SPSC-shaped byte rings each
  direction, bounded, under/overrun-on-pressure, serviced off the hot path), the host an OPFS filesystem.
  The sim owns the WAV codec. This also keeps the deferred **Story 4.7 waveform probe** independent — it's a
  different (display) mechanism, unbuilt here.
- **The transport playhead is a `u64` digital-domain counter, host-mirrored, engine-authoritative.** The
  engine advances it deterministically (128/block **while rolling**); the host predicts it (knows start
  position + roll state, same block advance) to pre-fill/drain byte rings without per-block feedback; on
  seek/stop the host resyncs; the engine's count is truth if they ever diverge (e.g. an overrun). This is
  the "carry a shared clock reference over the transport" conclusion (`EPIC_3_NOTES.md:391-398`), realized
  on the digital clock. Resembles the event `sample_pos`/`drain_due` shape (`schedule.rs:451-488`) — a due
  region `[pos, pos+block]` — but on the digital rate.
- **Overdub is the correctness bar: transport = rolling/stopped + an _independent_ record-enable (not a
  play-vs-record mode).** A real DAW records new tracks **while playing back already-recorded ones**. So
  playback and record are **independent per-track concerns processed every rolling block on the one
  playhead** — not mutually-exclusive global modes. While rolling: every track holding file data at the
  playhead **plays** (decode → route/sum → returns) **and**, in the very same block, every **armed +
  record-enabled** track **captures** its assigned send to its own file. This must **emerge** from the
  per-track structure (a track that plays and a track that records are just different per-track states in
  the same recorder loop), not be special-cased. _Rejected: a global record mode that supersedes playback_ — it
  would forbid overdubbing, the core multitrack act. Consequence for the seam (below): the byte streams are
  **per-track**, so N tracks can play distinct files while another writes its own. Known simplification: an
  overdubbed take lands at the **monitored** position — offset by the uncompensated round-trip latency (AD +
  USB + delayed return); record-latency compensation is out of scope (a later/5.3-era concern).
- **Clock provenance: the DAW's rate is the interface's, carried in the data — the transport hardcodes no
  rate (traced from code at planning).** What sets the digital rate is the **converter**, not the transport:
  the analog rate is the single `compile(graph, block_len, analog_rate, seed)` parameter (384 kHz); each
  `AdConverter`/`DaConverter` carries its own `SampleRate` (a distinct newtype from `AnalogRate`) and the AD
  **stamps** it onto the `SampleBuffer` it produces (`ad.rs` "opens a clock domain"); `compile`'s `alloc_lane`
  (`schedule.rs`) then derives `M = analog/digital`, sizes the digital lane to `block_len / M` (128 @ 48 kHz),
  and every digital→digital edge is compile-checked equal-rate (`ClockCrossingUnsupported` otherwise). So
  `Transport::advance(frames)` takes the frame count as an **argument** — the recorder passes the **runtime
  digital lane length** (`SampleBuffer::len()` of its USB lanes = `block_len / M` for whatever the interface
  feeds), never a `128` constant. This models **"the DAW follows the interface clock"** (the realistic USB
  case — the interface is the master, the computer slaves), so the "external clock → interface → DAW" story is
  the current mechanism, not a future one, and needs zero transport redesign. **What is _not_ emergent yet —
  all Story 5.3, already scoped there, none blocked by 5.11:** (1) a device declaring a **clock source**
  (`Internal`/`RecoverFrom`/`WordClock`) — `port.rs` flags `DigitalFace` as where the clock role will live;
  (2) `ClockDomainId::SINGLE` is hardcoded in `alloc_lane` (one domain ⇒ no drift, no async-boundary FIFO/
  slip); (3) cross-rate edges are **rejected, not resampled** (no SRC); (4) the DAW node rates are statically
  authored at 48 kHz in the catalog rather than **derived** from the interface's published port rate (the
  rate-axis analogue of 5.10's channel-count derivation — a no-op today since everything is 48 kHz). Building
  5.11 on the rate-agnostic `advance(frames)` keeps all four cleanly deferable.
- **Tracks are config-driven node sizing, exactly like 5.10's USB channels.** A hidden `track_count` config
  (written by the web track model) sizes the recorder; per-track routing/level/arm/monitor are runtime params
  (no recompile), the `Matrix` runtime-routing precedent (5.7.9). Adding/removing a track is a structural
  config change → recompile-on-change (the INST/48V/usb-channels pattern). _Rejected: a fixed max track
  count_ — encodes an arbitrary ceiling; config-driven is the established idiom.
- **Crossbar router + record/playback tracks (the headline routing decision, settled mid-5.11.3).** Routing
  and record/playback are **separate concerns**, wired as a linear chain:
  `DigitalMeter(N sends)` (kept — input meters) → **`MultitrackRecorder(N → N+T)`** → **`Matrix(N+T → M)`**
  (the crossbar) → USB out (delayed). The **`MultitrackRecorder`** is a tape-machine: it **records** armed
  send lanes to files and **plays back** track files, owning the [`Transport`]; its output bus is the **N
  sends passed through** (so the mixer can monitor a live input) **+ T track playbacks** (silent unless
  rolling). It does **no** routing/levels/monitoring/summing — those are the **`Matrix`** crossbar's
  `(sends + track playbacks) → returns` crosspoint gains (the "simple mixer"). _Rejected: fusing routing into
  one-in-one-out tracks_ (each track a mono strip with a single input→single output) — it can't **fan out**
  (a track to master *and* an aux send at once), forces a duplicate "track" per routing path, and conflates a
  recorder with a routing wire. The crossbar expresses **many-to-few** (30 tracks → a 2-lane master, each a
  crosspoint), **fan-out / aux sends** (extra crosspoints; an outboard loop is `send→aux-return` out and
  `send→master` back in), and monitoring (`send→return`) uniformly — matching real mixer+multitrack gear —
  and reuses the `Matrix` we already have (its pure-loopback default is retired for the crossbar default:
  `send 0 → return 0` and each `playback → return 0` at unity, keeping the default scene's monitoring loop
  audible). Track count is independent of the interface's channel count.
- **WAV codec is a small hand-rolled wasm-safe writer/reader in the engine** (a fixed canonical 44-byte
  header + PCM). Format is **32-bit IEEE float** (`WAVE_FORMAT_IEEE_FLOAT`, tag 3) — matches the DAW's own
  `f32` `SampleBuffer` storage, so encode→decode is **bit-exact** (no quantization step). Mono now, a
  `channels` field for stereo later. Decode is **total** (foreign bytes from host storage → `WavError`, never
  a panic; unknown chunks skipped). Stored files are real WAVs (nice-to-have: downloadable/inspectable).
  _Rejected: host-side WAV encoding_ — puts audio semantics in the host, against the seam decision.
  _Rejected: reusing `hound` (the community standard, already a `harness` dep) in the engine_ — it would
  compile to `wasm32` (pure Rust over `Read`/`Write`/`Seek`, supports float), but fits this use poorly: (a)
  its `WavWriter`+`finalize()`-`Seek` shape means holding the **whole take in a `Cursor<Vec<u8>>` in WASM
  memory** until finalize — exactly what the per-block streaming-to-disk model forbids; (b) it allocates and
  isn't shaped for the pre-sized, alloc-free, per-quantum framing this seam needs next to the audio thread;
  (c) the engine is deliberately dependency-lean and wasm-clean (the reason `hound` was quarantined to
  `harness`), and a dep to save ~90 lines of a *fixed-format* header + `f32::to_le_bytes` is a bad trade; (d)
  a crate's value is decoding **arbitrary** WAV variants, but we only ever decode files we wrote at one fixed
  format — variant-handling we'd never exercise. **Tipping point:** if we ever import arbitrary user WAVs
  (odd bit depths, ADPCM, extensible headers), switch to `hound` (or `symphonia` for broad decode) rather
  than grow a hand parser.
- **The "simple mixer" is the `Matrix` crossbar — no new mixing concept.** Routing and level are one and the
  same: the crossbar's per-crosspoint gains `(N sends + T track playbacks) → M returns`, surfaced in the
  focus view (the 5.7.9 `RoutingGrid` precedent). A track's "fader" is its crosspoint gain to master; an aux
  send is another crosspoint. Setting levels + routing only; no EQ/dynamics/pan.
- **Storage transport: `postMessage` byte chunks to start, SAB later if needed.** The byte rings cross the
  worklet↔main boundary; begin with per-quantum `postMessage` of byte chunks (the current param/event
  transport shape), promote to a `SharedArrayBuffer` ring (the deferred Epic-3 SAB work) only if disk-rate
  bulk transfer demands it. Recorded per-block payload is small (128 frames). The `ByteRing` is hand-rolled
  and **single-threaded-shaped today** (like `EventQueue` is just a `Vec`), with a custom **all-or-nothing
  whole-frame** `write`/`read` guarantee (a slow consumer drops a whole block, a slow producer underruns a
  whole block — PCM never tears). _Rejected (for now): an audio-SPSC crate (`rtrb`/`ringbuf`)_ — `rtrb` gives
  element streaming **without** the whole-frame framing guarantee, and it's lock-free machinery not needed
  until the SAB upgrade. The `write`/`read` interface is shaped so `rtrb` can be evaluated to slot **beneath**
  it when that upgrade lands.
- **Known simplifications (not bugs):** mono tracks only; one clip per track (record replaces); no timeline
  editing/seeking-into-a-clip beyond transport seek; input monitoring is a per-track gate (not latency-
  compensated against the round-trip); the DAW clock is still the single domain today (drift vs. the
  interface is a Story 5.3 concern); OPFS-only (native harness may use real files behind the same seam).

- **Task 5.11.1 — Engine: the file-byte transport seam.** Bounded, SPSC-shaped **per-track byte streams**
  in each direction between the recorder and the host (outbound = each recording track's bytes to store;
  inbound = each playing track's file bytes) — **indexed by track** so N tracks play distinct files while
  another writes its own (the overdub requirement), serviced **off** the hot path (drop/underrun-on-pressure,
  no `process` involvement), host side opaque. Plus the wasm-safe **WAV codec** (writer/reader, minimal
  header + PCM, or raw f32). _Done:_ oracles — a known sample block WAV-encodes then decodes back
  **sample-exact** (hand-checked header + PCM bytes in a comment); two tracks' inbound streams decode
  independently without cross-talk while a third's outbound stream fills; ring under/overrun is bounded and
  lossy-not-panicky; no-alloc test green (rings pre-allocated; `process` untouched); `cargo wasm` green (no
  native-only dep).
- **Task 5.11.2 — Engine: transport + digital-domain playhead.** A `u64` digital-domain sample counter with
  a **rolling/stopped** transport + **seek**, and an **independent record-enable** (per-track arm decides who
  writes). Advances the digital block length per processed block **while rolling** (playback *and* any armed
  record happen on the same rolling playhead — no play-vs-record mode); recording writes are further gated by
  per-track arm + record-enable, playback reads by whether a track has file data. _Done:_ oracles — playhead
  advances **128/block @ 48 kHz** while rolling and holds while stopped (hand calc: analog 1024 ÷ M = 8);
  seek repositions exactly; record-enable toggles writes **without** stopping playback (the overdub gate);
  deterministic across runs.
- **Task 5.11.3 — Engine: the `MultitrackRecorder` node (record/playback + transport).** ✅ **Done.** A
  tape-machine node: `T` mono tracks, `N` send lanes in → `N + T` lanes out (the N sends **passed through**
  for the mixer to monitor, then one **playback** lane per track). Per track = **playback**(stream its file
  from the inbound ring to its output lane while rolling) + **record**(stream its assigned send to the
  outbound ring while rolling + record-enabled + armed). **No routing/levels/monitoring/summing** — those are
  the downstream `Matrix` crossbar's job (settled mid-task; see the crossbar note above). Owns the
  [`Transport`], advancing it by the **runtime** lane length. _Delivered oracles (6):_ ports = N in / N+T out,
  **no params**; sends pass through to lanes 0..N; a track's playback appears on lane `N+t` only while rolling
  (silent stopped); record captures the armed send only when rolling + record-enabled; **overdub oracle** —
  track 0's playback lane carries its file **unchanged** while track 1 records its send in the same block;
  transport advances by the runtime lane length. Alloc-free (rings pre-allocated; stack `[u8;4]`), panic-free.
- **Task 5.11.4 — Devices: rebuild `computer` as the DAW.** ✅ **Done.** Rewrote the catalog entry to the
  crossbar chain `DigitalMeter(N)` → `MultitrackRecorder(N → N+T)` → `Matrix::new_single_ports(N+T → M)` → USB
  out (delayed), with `T` from a hidden `track_count` config (default **1**) and USB N/M from 5.10's
  `usb_sends`/`usb_returns` (default 2×2). The `Matrix` default is the **crossbar loopback**: `send 0 → return
  0` and each `playback → return 0` at unity (keeps the mic/synth monitoring loop audible), everything else 0
  — retiring the old diagonal send-k→return-k default. Generated grid labels `"In i → Return j"` (rows = the
  N+T mixer inputs, cols = M returns) + the per-lane send meters. _Infra fix:_ `describe` now sizes the grid's
  **rows from the matrix's own crosspoint count** (`crosspoints / m_out`), not the input-port face — because a
  crossbar's inputs (N+T) exceed the device's input ports (N); `GridAxis::Named` self-sizes so the 8i6's
  hand-named 14×14 is untouched (the alignment guard was updated to mirror this). _Delivered oracles:_ default
  computer = meter + recorder(1 track) + a 3×2 crossbar, USB in/out = 2/2, 6 crosspoints, still audible in the
  playable-loop test; `track_count`=4 → an (N+4)×M crossbar; `configured` 8×6 → a 9×6 crossbar (54
  crosspoints); `ChannelCountMismatch` + duplex backstops intact; **the deferred 5.11.3 no-alloc proof landed
  here** — a direct `no_alloc.rs` check of the recorder's rolling+recording ring paths. Full Rust gate green
  (engine 360 + devices 60). **Note for 5.11.6:** the crossbar retires the diagonal default and reshapes the
  matrix crosspoint ids ((N+T)×M), so a saved scene's stored matrix `ParamSetting`s are stale → **bump
  `SCHEMA_VERSION`** (discard + rebuild) when the web lands.
- **Task 5.11.5 — wasm: export the DAW seams.** On `SceneEngine`: drain the outbound byte ring / fill the
  inbound byte ring (zero-copy views, by device id), transport commands (play/stop/record/seek), a playhead
  getter, and the per-instance descriptor already carrying the track face. _Done:_ the byte rings and
  transport round-trip across the wasm boundary for an N-track computer; the EMPTY-config type catalog
  unchanged; `cargo wasm` + full Rust gate green.
- **Task 5.11.6 — Web: OPFS storage + track model + transport UI + level mixer + waveform.** OPFS-backed
  take files (worker + sync access handles) draining/filling the byte rings around the playhead; a host-side
  **track model** (create/remove tracks → `track_count` config + recompile; arm; input/output assign; level);
  **transport controls** (record/play/stop); the **simple level mixer** (faders = recorder params) + routing in
  the focus view (the 5.7.9 `RoutingGrid` precedent); a **waveform** view decoding stored WAVs host-side for
  display only. Vitest for the host track/transport/storage logic (incl. concurrent OPFS read of playing
  files + write of a recording file via per-file sync access handles). _Done:_ in-browser — arm a track to
  the mic/synth send, hit record, stop, play it back through the monitoring loop and hear it; **then
  overdub — with that take playing, arm a second track and record it, and hear both together on the next
  playback** (the DAW pressure test); a track sums into master; unplug/replug still sound; `pnpm run format`
  + web `check`/`typecheck`/`test` green.

_Validate:_ the `computer` presents an **arbitrary number of mono tracks**; a track **armed** to a USB send
**records to a WAV file on disk (OPFS)** while the transport rolls, and **plays that file back** to a USB
return through the **in-sim routing matrix + level mixer**, audible through the interface monitoring loop;
the **transport** (rolling/stopped + record-enable) is clocked by the **in-simulation digital domain**
(playhead advances 128 digital samples/block, never touching the host clock); **overdub works — a new track
records while already-recorded tracks play back on the same playhead**, and both are heard together
afterward (the DAW pressure test); the **only** sim↔host data is **opaque file bytes** (host stores; the sim
owns WAV encode/decode, routing, levels, timeline); the recorded take **never lives whole in engine memory**
and the audio thread never blocks on disk; the default scene's mic/synth loop
still sounds (one default monitor track); mono-now with a lane-list door open for stereo; hot path stays
zero-alloc / panic-free and deterministic; the full Rust gate (`cargo fmt --check && cargo lint && cargo
test && cargo wasm && cargo docs`) plus web `check`/`typecheck`/`test` pass; verified in-browser.

---

## Epic 6 — Device Workbench (developer tooling) — 🚧 **6.1–6.3 complete; 6.4 implemented (verify + commit pending)**

**Progress:** Stories 6.1 ✅, 6.2 ✅, 6.3 ✅ done; **6.4 implemented and green** (Rust + web gates) with
in-browser verification + commit pending. The scene view's engine/UI plumbing was extracted into a shared
**session layer** (`SceneSession` + `PatchController`) that a second view root consumes unchanged; on it
sits a single-device **workbench** at `/devices/<typeId>` — the device on a real-mm grid (both faces),
hand-patchable to a synth source + DA + speaker cast, every param/config/meter live, a filter+pin **debug
panel**, a **MIDI monitor**, **device focus mode**, and a temp scene that lives **in the URL** so a
Rust-rebuild → reload restores the bench (pins included). **Next: verify 6.4 in-browser + commit; then the
open threads (the knob-ring in `IMPROVEMENTS.md`, the `wasm:watch` rebuild-loop).**

**Goal (delivered):** a focused single-device development view — build/test/debug each new Stage-5 device on
a mm grid, front + back, instantly patchable to a source + monitor, every param and meter exposed, with the
temp scene in the URL so the Rust-rebuild → reload loop restores itself. Developer tooling in service of
Epic 5 breadth (not a PROJECT_PLAN §9 roadmap stage); shares Epic 5's ordering freedom — its stories can
interleave with Epic 5 work.

> **Full design notes, rejected alternatives, per-task delivery records, and deviations live in
> [`EPIC_6_NOTES.md`](./EPIC_6_NOTES.md).** This section keeps only the decisions and the delivered surface
> that constrain later work — enough to make good follow-up decisions without re-deriving Epic 6.

### What Epic 6 delivered (web surface; engine largely untouched)

- **The session seam (6.1):** `SceneSession` (`web/src/session.svelte.ts`) — the view-agnostic engine
  consumer (lifecycle, live readout state, the authoritative `Scene`, param/config/note/hot-swap lanes) —
  and `PatchController` (the patching glue: drag + click-to-pick + cross-view pending, the jack-anchor store
  + `measure(worldApi)`). App and the workbench both construct these; `App.svelte` keeps only scene-view UI
  state. The codebase's first `.svelte.ts` rune-class modules; pure decision logic stays in the node-tested
  `*.ts` modules.
- **Routing + suspended boot (6.2):** a hand-rolled zero-dep router (`router.svelte.ts` + `Root.svelte`) —
  `/devices/<typeId>` → `Workbench`, else the scene view. The engine boots on a **suspended** AudioContext
  so the catalog arrives pre-gesture (`resume()` decoupled from `start()`; `resume()` on first interaction).
  `BenchStage.svelte` renders one device both faces on a mm + rack-U grid via the same faceplate widgets.
- **Hand-patchable bench + URL scene (6.3):** a fixed **supporting cast** (synth source · DA · speaker)
  around the DUT, **unwired** — the user patches source→DUT→monitor by hand via the shared
  `PatchController`/`cable-view` (decoupled from the scene-spatial `LayoutCtx` behind an injected
  `CableLayout`; `WorldApi`/`SurfacePoint` hoisted to `world-api.ts`); a "Listen" tap selector; a reused
  keybed with a "Send to" selector; and the whole temp scene serialized to the URL query (`url-scene.ts`,
  `base64url(JSON)`, versioned + regenerate-on-mismatch, debounced `replaceState`).
- **Debug surface, MIDI, meters, focus (6.4):** a session-driven `DebugPanel.svelte` in a collapsible
  **right-hand drawer** — always-on header (output peak · monitored tap · signal-path latency ·
  connection losses) + a filter+pin **watch-list** over every device's params/configs/readouts (pins in
  `scene.ui.benchWatch`, URL-persisted) + a **MIDI monitor** (access status · held notes · routed-event
  log). **Web MIDI** wired into the bench (it was missing). **Device focus mode** on the bench (the DUT's
  focus surface — for the 8i6 the routing matrix). Engine side: `SceneSession.latencyMs`, `midiLog` +
  `noteName`, and the 8i6's two inline `VuMeter` input readouts (In 1/In 2 VU + peak-dBu). Plus the
  `wasm:watch` hot loop + a Vite full-reload plugin.

### Decisions that bind later work

- **One plumbing path, two views.** The workbench consumes the *same* `SceneSession` + `PatchController` +
  `cable-view` + widgets as the scene view — never a forked copy. Anything the bench needs from App was
  *extracted*, not duplicated (the `CableLayout` interface, `world-api.ts`, `keyboard-input.svelte.ts`).
- **Spaces/racks/placement are scene-view UI; the bench is a flat both-faces layout.** The bench has empty
  spatial `ui`; it can't reuse `WorldView`, so shared geometry rides an injected layout interface instead.
- **The URL is the bench's one persistence home** (disposable, versioned, regenerate-on-mismatch) — the
  temp scene, param overrides, tap, **and** debug pins all round-trip through it; no localStorage for bench
  state. Debounced `replaceState` (no history spam).
- **Layer rule holds for dev tooling.** The catalog gains no workbench/debug vocabulary; the bench + panel
  are pure descriptor/reading consumers keyed by id.
- **Hot-path panic-freedom is enforced at build, not by careful callers.** A non-analog output tap is
  rejected in `build_patch` (`BuildError::OutputTapNotAnalog`) rather than trapping in `render_quantum`
  (6.2.5) — a CLAUDE.md §6 fix the dev bench forced.
- **Bench audibility is user-driven, by design.** No auto-rig (you patch by hand) and the DUT boots at its
  node defaults (the 8i6 boots **powered-off**) — so there's silence until patched + powered + routed; not
  a bug.

### Deferred — decided, not gaps

- **Engine-health surface, seed control, App-panel dedup, a bench signal-generator device** — deferred out
  of 6.4 (health stays on `session.health`; the panel is audio-parameters only).
- **The 8i6 gain-knob level ring** — the input readouts exist (6.4); rendering the ring is a web-only
  follow-up recorded in `docs/IMPROVEMENTS.md`.
- **`wasm:watch` rebuild-loop** — the watcher reacts to the build's own `web/public/*` output; a source-only
  watch / ignore rule is the parked fix.
- **Hard-coded spaces** (describe them in the scene file; no user-created spaces) — an open `IMPROVEMENTS.md`
  item, not Epic-6 scope.

### Story-by-story (status + the one thing each settled)

- **6.1 — Engine-session extraction** ✅ — pulled the engine/UI plumbing out of `App.svelte` into
  `SceneSession` + `PatchController`. Settled: **one shared consumer surface** both views construct; a pure
  refactor, scene view behavior-identical.
- **6.2 — Route + workbench shell** ✅ — first URL routing + a **suspended-context boot** so the catalog
  arrives pre-gesture; a dedicated bench stage (both faces, mm/rack-U grid). Settled: boot a known-good
  bootstrap then resolve `<typeId>`; `resume()` decoupled from `start()`; a non-analog tap is a build-time
  rejection (6.2.5).
- **6.3 — Bench patching + URL-persisted temp scene** ✅ — **no auto-rig**: a fixed cast patched by hand via
  the shared cable machinery (decoupled from `LayoutCtx`); the temp scene round-trips through the URL.
  Settled: hand-patching is a first-class bench feature; the URL is the one persistence home.
- **6.4 — Debug surface + the hot loop** 🚧 *implemented; verify + commit pending* — a filter+pin watch-list
  + an always-on header, the `wasm:watch` loop, **plus** (emergent) Web MIDI + a MIDI monitor, 8i6 input
  readouts, a right-hand drawer, and bench device-focus mode. Settled: pins live in `scene.ui`
  (URL-persisted); the panel is audio-parameters only (health/seed excluded).
