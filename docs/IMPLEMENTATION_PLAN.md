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

**Detail gradient (concretely):** Epics 1–6 are built — their completed Stories carry full design notes
and per-task delivery records, now **archived to `EPIC_<N>_NOTES.md`** with only a summary kept here
(Epic 4's one deferred Story, 4.7, keeps its coarse sketch; Epic 6's Story 6.4 is implemented, pending
verify + commit). **Epic 7 — the remaining "breadth & challenges" roadmap — stays coarse at Story level**;
its Tasks get written when we reach them. Don't over-plan work whose shape the earlier work will change.

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

## Epic 5 — Device Fidelity & the Digital Hub — ✅ **Complete**

**Progress:** Stories 5.7 ✅, 5.8 ✅, 5.9 (part 1) ✅, 5.10 ✅, 5.11 ✅ — **Epic 5 complete.** What began as
the open-ended "Breadth & Challenges" grab-bag became, in the doing, a focused and coherent arc: making one
interface — a Focusrite **Scarlett 8i6** — a *faithful* device, then growing the **`computer`** it plugs
into from a fixed monitoring loopback into a **minimal DAW**. Each capability landed as an engine/`devices`
_concept_ the web layer then consumed **by id**, the layer rule intact throughout. The rest of the original
"breadth & challenges" roadmap (more devices, networked audio, clock domains, the challenge framework, a
schematic view) was never really "catch-all improvements" — it is roadmap-scale, and moved to **Epic 7**
with its design notes intact.

**Goal (delivered):** grow device fidelity and the digital medium on the proven engine + UI — a faithful
audio interface, honest +48 V phantom power, bidirectional digital links, a computer that adapts to what's
plugged into it and records/plays multitrack audio — with every new capability an engine/`devices` concept
the web layer consumes by id, and the Epic-4/5 layer rule (catalog = specs, web = look & feel) never
violated.

> **Full framing, design notes, rejected alternatives, per-task delivery records, and deviations for every
> Story live in [`EPIC_5_NOTES.md`](./EPIC_5_NOTES.md).** This section keeps only the decisions and the
> delivered surface that constrain later work — enough to make good follow-up decisions without re-deriving
> Epic 5. Go to the notes when a decision turns on _why_ something was built the way it was, or you need the
> exact behavior of a shipped piece.

### What Epic 5 delivered (engine + devices + web surface)

- **Per-device faceplate system (web).** A device-UI registry (`typeId → component`, else the generic
  `Panel` fallback), a `DeviceHandle` context + a `Chassis` bezel (one flip implementation), **bound
  widgets** that reference an id (`Control`/`Socket`/`Reading`/`ConfigSwitch`/`ConfigButton`/`DuplexSocket`),
  focus surfaces generalized per device, layout primitives (`Section`/`Legend`/`ButtonCluster`/`Silkscreen`),
  and a static coverage guardrail (`web/test/faceplate.test.ts`) proving every exposed param/port is placed
  and only valid ids referenced. Proven on the branded, mixed-face Scarlett 8i6.
- **Preamp & interface physics (engine/devices).** Device-level power (a catalog **param group** binding one
  control to N node params; `powered` gates on `AdConverter`/`DaConverter`); a `MicPreamp` node (PAD −10 dB,
  AIR high-shelf, INST/hi-Z as a structural config, and a **balanced difference-first front-end**);
  multichannel digital ports (`DigitalMux`/`DigitalDemux`, `AudioFormat.channels`, a `Combo` connector); a
  params-driven `Matrix` routing node (route/mix/mute per crosspoint, no recompile); corrected device
  dimensions.
- **48 V phantom, emergent (engine).** `PhantomSupply`/`PhantomLoad` port declarations + a **compile-time DC
  operating-point solve** (`electrical/phantom.rs`, reusing `divider_gain`); the **unbalanced→balanced edge
  rule** (cold leg grounded to 0 V — TS-in-combo physics); a `condenser_mic` device with a deterministic
  capsule test tone riding the resolved pedestal. The mic's self-`powered` flag is **deleted** — the last
  in-domain label on the signal path.
- **Duplex digital transport + topology-derived delay (engine/devices/web).** `Graph::connect_delayed` +
  `compile`'s auto-break of any residual **digital** cycle (`topo::reaches`, delaying the lowest-index edge
  → one block of round-trip latency; an all-analog cycle still errors); `CatalogEntry.duplex_links` /
  `PortDescriptor.duplex_partner`, `Connection.duplex`, `build_patch`'s two-edge expansion; the
  `DuplexSocket` web widget (one USB-C jack carrying both directions).
- **Dynamic computer I/O (engine/devices/web).** A lane-port `Matrix` + a multichannel `DigitalMeter`;
  channel counts as **structural config** (`usb_sends`/`usb_returns`) written by host-side enumeration from
  the attached interface's **published** port `channels`; a **per-instance, config-aware descriptor** over
  wasm (`describe_device(type_id, configs)`), pushed to the UI as a `deviceDescriptors` map on each build.
- **The `computer` as a minimal DAW (engine/devices/web).** A file-byte transport seam (per-track
  `ByteRing`s + a wasm-safe hand-rolled WAV codec); a `u64` **digital-domain `Transport`/playhead**; the
  `MultitrackRecorder` channel-strip node (per-track fader, arm/record-enable/monitor, **overdub**); a
  `DawControl` trait + defaulted `Node::daw()` hook + `SceneEngine` DAW seams; and the web OPFS storage
  worker + host track model + the DAW **mixer** focus surface (transport + strips + routing + waveform).
  `SCHEMA_VERSION` reached **18**.

### Decisions that bind later work

- **The layer rule held throughout (the headline).** The Rust catalog gained **no layout/UI vocabulary** — a
  faceplate references params/ports/readouts **by id**; all appearance lives in the web component. A device
  optionally registers **its own Svelte faceplate** (composing shared widgets + `--ae-*` tokens); the generic
  `Panel` is the fallback. The front/back-face problem **dissolves** (a jack's face = which snippet it's
  written in) — retiring the mixed-face `ioFace`-resolver sketch (the first case of Epic 7's Story 7.6).
- **Feedback is bounded latency, never a same-block solve.** A digital cycle is broken in `compile` by a
  **delayed edge** (an ideal digital copy carrying one block of round-trip latency; the `delayed_outputs`
  hint places it on the physically-correct leg — playback trails input). An all-analog cycle has nowhere to
  carry the latency and still rejects as `CompileError::Cycle`. **The schedule stays a strict DAG.**
- **A bias network is compile-time physics via superposition.** The interconnect is linear and the phantom
  network is static, so solve the **DC operating point once at compile** (SPICE `.OP`) and superpose the AC
  signal per-sample forward as today — never push per-sample voltage *backward* through the pull DAG.
  Common-mode rejection must be applied **before** any nonlinearity (difference-first, exact at ideal CMRR).
- **Runtime routing is a params-driven node, not a topology change** (the `Matrix` crossbar). User repatching
  recompiles; crosspoint changes don't. This is the between-device switch precedent for Epic 7's networking.
- **Channel counts and track counts are structural config, derived from the model** (`usb_sends` /
  `usb_returns` / `track_count`, config→recompile). No parallel channel constant anywhere — a computer's
  shape comes from the attached interface's published face.
- **The digital domain owns its own clock, timeline, and mix.** The DAW transport advances a
  **digital-domain** sample counter (`Transport::advance(frames)` fed the *runtime* digital lane length =
  "the DAW follows the interface clock"), never the host `AudioContext.currentTime` or the analog
  `sample_pos`. **Host = dumb byte storage** (opaque WAV bytes to OPFS); the sim owns the WAV codec, routing,
  and levels.
- **Overdub and multiple computers emerge from per-track / per-device state, never special-cased.** Playback
  and record are independent per-track concerns on one rolling playhead; takes are keyed by
  `(deviceId, track)` so a second computer needs no new code path.

### Deferred — decided, not gaps (where each lands)

- **Story 5.9's L2 forwarding proper** — addressed flows, switches, per-flow/per-direction channel checks →
  **Epic 7** (it feeds networked audio + clock domains). The full idea-gathering is preserved in
  `EPIC_5_NOTES.md` under Story 5.9's _Deferred_ block.
- **Clock domains, async-boundary FIFO slip, word-clock master/slave, fractional SRC** — one clock domain
  today (`ClockDomainId::SINGLE`; cross-rate rejected as `ClockCrossingUnsupported`). The emergent
  clock-distribution model → **Epic 7**. The delayed edge and `Transport::advance(frames)` were built
  rate-agnostic precisely so this is cleanly deferable.
- **Ground-topology-emergent hum** — `Cable::with_hum` stays a manual phenomenological stand-in; the
  grounding side-graph + "fix the hum" challenge → **Epic 7** (the design note is carried there).
- **Finite CMRR, the "air link" / acoustic-transduction story, record-latency compensation, stereo tracks,
  the SAB byte-ring transport** — each recorded in `EPIC_5_NOTES.md` with its resume trigger.
- **Two tactical Story-5.9 open decisions** (SCHEMA_VERSION bump for duplex; making the web `wouldCreateCycle`
  check domain-aware) → `docs/IMPROVEMENTS.md`.

### Story-by-story (status + the one thing each settled)

- **5.7 — Per-device faceplate UIs** ✅ — the faceplate registry + a faithful Scarlett 8i6 (+ device power,
  preamp physics, multichannel USB, routing matrix, a `computer` USB peer, the round-trip delayed edge).
  Settled: **component over coordinate-map** — a device authors its own Svelte faceplate, and the
  front/back-face problem dissolves.
- **5.8 — 48 V phantom power** ✅ — a patchable condenser mic + a balanced preamp front-end + a compile-time
  DC operating-point solve. Settled: **phantom emerges from a compile-time solve** (superposition; DC at
  compile, AC per-sample forward), retiring the mic's self-`powered` flag.
- **5.9 (part 1) — Duplex digital transport & topology-derived delay** ✅ — one USB-C jack carries both
  directions; `compile` auto-breaks digital feedback loops. Settled: **a duplex link is an authoring
  construct that lowers to two unidirectional engine edges** (the DAG invariant holds); L2 forwarding → Epic 7.
- **5.10 — Dynamic computer I/O** ✅ — the computer re-enumerates to the attached interface's published
  channel counts. Settled: **channel counts derive from the model** (config-driven node sizing + host-side
  enumeration), never a parallel constant.
- **5.11 — Computer as a minimal DAW** ✅ — multitrack record → route → level → play, clocked by the in-sim
  digital domain, with overdub. Settled: **the mix/routing/transport lives in the sim; the host is dumb byte
  storage** — a channel-strip mixer (per-track fader → bus crossbar).

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

---

## Epic 7 — Breadth & Challenges

**Goal:** grow device coverage and the medium (routing, studio wiring, live sound scaling toward large
venues), deepen DSP and AD/DA, and add the game layer. This is the continuation of `PROJECT_PLAN.md` §9
Stage 5 — the roadmap-scale "breadth & challenges" work that outlived Epic 5's device-fidelity/DAW arc.
Epic 5 delivered the fidelity slice (a faithful interface + the computer-as-DAW); these are the genuinely
big, mostly-independent efforts that remain.

**Exit criteria:** the same engine credibly supports studio, routing, and live-sound scenarios; structured
challenges layer on top of the sandbox.

**Watch-outs:** multi-core only if profiling at scale demands it (single core covers stadium on the napkin).
Keep device transforms understandable — spend the realism budget on the volts-and-converters layer.

**Notes:** The stories in this epic are **not related to each other** unless otherwise stated. We can do
them in any order or only do part of them. Kept deliberately coarse — each is elaborated to Task level (with
hand-calc oracles and watch-outs) only when we pick it up, per the "flesh out on arrival" convention.

_Tasks to be elaborated when we reach this Epic._

- **Story 7.1** — More devices: deeper mixer, more processors, patchbay, more converters.
- **Story 7.2** — Routing & live-sound scenarios at scale (multi-core partition of the schedule if
  needed). Includes **networked audio** (Dante/AES67) as a carrier: digital-audio sample streams over
  an IP transport with its own routing, subscriptions, latency, and a network clock (PTP) — modeled
  "TCP/IP layer upwards" (network behavior + encoding), reusing the `Sample` lane and the clock-domain
  machinery, with the transport/subscription model as the net-new piece. _The link/forwarding layer
  **beneath** this (bidirectional links + simplified L2 switching) was **started in Story 5.9**: part 1
  (duplex single-connector links + topology-derived delay) is delivered; the L2 forwarding **proper**
  (addressed flows, an L2 switch, per-flow/per-direction channel checks) was **deferred from 5.9 to here** —
  the idea-gathering is preserved in `EPIC_5_NOTES.md` under Story 5.9's Deferred block. The two stack: L2 is
  the link/forwarding layer, 7.2 is IP-and-up._
- **Story 7.3** — Deeper DSP and deeper AD/DA modeling as needed. Includes **clock domains, sync, and
  sample-rate conversion** — the payoff of the "crossing any clock = resample" rate model (Story 1.1)
  and the carrier/clock seam (Story 1.6). This is where the **emergent clock model** (the clock-domains
  decision below) is built: per-domain oscillators as real rates against the analog continuum, elastic
  FIFOs at async boundaries that genuinely slip, word-clock master/slave, recovered-vs-dedicated
  clocking, and a **fractional sample-rate converter** as the honest fix (also what lets a 44.1 k
  device meet a 48 k one). "Fix the clocking" (set a master, slave the rest) becomes a diagnostic
  challenge alongside "fix the hum". _Epic 5 built the DAW's transport rate-agnostic (`advance(frames)`
  fed the runtime lane length) precisely so this lands with zero transport redesign._
- **Story 7.4** — Challenge / diagnostic-scenario framework on the sandbox. Includes the
  **ground-topology-derived hum** decision below ("fix the hum" is a named challenge scenario).
- **Story 7.5** — Optional schematic / node-graph view over the same model.
- **Story 7.6** — Device fidelity & the corner cases it forces. As devices get more faithful (a real
  audio interface, a proper mixer with sends/inserts, a patchbay), they break simplifying assumptions the
  earlier UI baked in; this is where those get collected and resolved (distinct from 7.1's breadth-of-
  coverage — this is depth-of-realism). The layer rule holds throughout: the **catalog owns which ports
  exist** (specs), the **web owns how they're drawn** (look & feel). _First known case — mixed-face I/O —
  was resolved by **Story 5.7**:_ once a device authors its own faceplate as a Svelte component, face
  assignment is simply _which face's markup you write a jack into_, so the per-device `ioFace` problem
  dissolved (no per-port resolver needed). Further fidelity corner cases accrue here as they surface.

_Decision — ground-loop hum should become emergent from grounding topology._
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
  ground-lift controls — naturally introduced alongside Story 7.1 (patchbay/wiring) and consumed by the
  "fix the hum" diagnostic in Story 7.4. ROI is high then (the heart of the troubleshooting lesson), low now.

_Decision — clock domains and their failures emerge from a clock-distribution side-graph + real
per-domain rates._ Through Story 1.6 there is a single internal clock domain
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
  devices and the fractional resampler (this Story). Story 5.11's DAW transport already rides
  `advance(frames)` so "the DAW follows the interface clock" is the current mechanism, not a future one.

---

## Epic 8 — The Acoustic Domain & Micing — ✅ **Complete**

**Progress:** Stories 8.1 ✅, 8.2 ✅, 8.3 ✅ — **Epic 8 complete.** It added a **fourth signal carrier —
acoustic sound pressure (Pascals @ the analog rate)** — so an instrument is *a source you mic*, not a
line-out: 8.1 stood the carrier up headless (a `SoundSource`/vox + the two mics as acoustic→analog
transducers), 8.2 surfaced it through the product layer (the web mirror + the micing gain-staging scenario),
and 8.3 closed the loop (a `PoweredMonitor` voltage→acoustic transducer, with acoustic feedback / howlround
emerging from a delayed air edge). Sound *generation* stayed deliberately trivial (a sine at a chosen
loudness); recognizable SoundFont/multisample instruments remain a separate future epic. **405 engine + 67
devices + 283 web tests green.**

**Goal (delivered):** make the acoustic side a first-class, physically-honest carrier — pressure in
**Pascals**, oversampled against the same analog continuum as volts, measured in **dB SPL** exactly as volts
are in dBu — with the two mics as genuine **acoustic→analog transducers**, a patchable sound source, and a
powered monitor radiating **voltage→acoustic**, so loudness at the source and gain in the chain drive the
whole front-end story (mic choice, phantom, gain staging, the pad, feedback) — all emergent from
`v = pressure · sensitivity` + the existing physics.

> **Full framing, design notes, rejected alternatives, per-task delivery records, and deviations for every
> Story live in [`EPIC_8_NOTES.md`](./EPIC_8_NOTES.md).** This section keeps only the decisions and the
> delivered surface that constrain later work — enough to make good follow-up decisions without re-deriving
> Epic 8. Go to the notes when a decision turns on _why_ something was built the way it was, or you need the
> exact behavior of a shipped piece.

### What Epic 8 delivered (engine + devices + web surface)

- **The acoustic carrier (engine).** `Pascals` (a `repr(transparent)` unit newtype like `Volts`) +
  `spl_to_pascals`/`pascals_to_spl` (ref 20 µPa, anchor **1 Pa = 93.98 dB SPL**); a `PressureBuffer` (linear
  Pa @ the one `AnalogRate`, mono) + `Domain::Acoustic` / `Lane::Pressure` / `AcousticFace` on the port
  enums; `EdgeKind::AcousticRoute` — a **unity pressure copy** (ideal air, no divider/cable), the acoustic
  peer of `DigitalRoute`. Acoustic↔analog is rejected at compile as `DomainMismatch` (free — the domain check
  already existed).
- **Sources & transducers (engine).** A `SoundSource` node (the first node that *emits* pressure — a sine at
  a LEVEL peak-pressure knob + a FREQ knob, deterministic phase); the `CondenserMic`/`DynamicMic` became
  **acoustic→analog transducers** (`differential = pressure · sensitivity`, V/Pa), their self-tone removed —
  an unpatched mic is silent, and the condenser-vs-dynamic ~14 dB gap is now **emergent** from sensitivity
  (10 vs 2 mV/Pa). The `PoweredMonitor` node (Story 8.3) is the output-side mirror: **`pressure = drive_volts
  · sensitivity · volume`** (Pa/V), a bridging line input + a mono acoustic output, distinct from the
  voltage-capture `Speaker` terminus (which is untouched).
- **Acoustic feedback (engine).** Acoustic edges are **delay-eligible** at `compile`'s cycle-break, and the
  policy **prefers the acoustic (air) edge** over a digital one on the same cycle — so a `mic → preamp →
  monitor → air → mic` loop auto-breaks by delaying the physically-honest air hop. Howlround (loop gain L ≥ 1)
  then **emerges** from the delayed loop and **self-limits at the rail** — never special-cased.
- **The product layer (devices + web).** `vox` and `powered_monitor` catalog devices + the mics' acoustic
  capsule ports; `Connector::Acoustic` (mates only with itself), `PortKind::Acoustic`, `build_patch`'s
  acoustic-edge expansion. Web: `"acoustic"` added to the `PortDomain`/`PortKind`/`Connector` unions, an
  `--ae-signal-acoustic` token, a **dashed** acoustic cable + an "ideal air (no cable)" inspector, and a
  dB-SPL-aware `format.ts` (display keyed on **unit**, not taper). All descriptor-driven — the generic
  `Panel` renders vox/mic/monitor with no bespoke faceplates.

### Decisions that bind later work

- **Acoustic is a real carrier, not a fudge.** Sound pressure is `Pascals` in a `PressureBuffer` at the **one
  `AnalogRate`** (pressure is continuous-time analog — never a digital rate); **SPL is a measurement helper,
  never storage** (a buffer holding dB SPL is a category error, exactly as one holding dBu). Mirrors the
  volts/dBu split.
- **The transducers are the only acoustic↔analog bridge** (a mic in, a monitor out), a **pointwise multiply
  by sensitivity** — same rate, no anti-alias FIR, no resample (unlike the AD/DA). An acoustic edge joins only
  acoustic ports; the cross-domain rejection is free.
- **Air is ideal.** The acoustic edge is a unity copy — no impedance divider, no distance, no propagation, no
  polar response. The one-block delay used to break a feedback loop is a **cycle-break mechanism**
  (block-size-dependent), **not** a calibrated propagation time.
- **Output-side acoustics are device→air→device only.** Story 8.3 deliberately extended `PROJECT_PLAN` §5.5
  (recorded there): a `PoweredMonitor` radiates so a device can be **mic'd**, but the voltage-capture
  `Speaker` terminus stays in volts and there is **still no air→ear** (no listener, no room).
- **Feedback emerges, never scaffolded.** No feedback detector, no gain limiter — howlround is the geometric
  consequence of loop gain ≥ 1 through the delayed air edge, self-limited by the rail clip; a dynamic resists
  feedback a condenser can't, straight from the sensitivity gap.
- **dB SPL is display, never stored.** The web converts Pa ↔ dB SPL for the vox loudness knob; the engine
  param stays linear `Pascals`. `format.ts` display is keyed on **unit** (`"dB"` gain / `"dB SPL"` pressure /
  else number+unit), decoupled from the knob **taper**.

### Deferred — decided, not gaps

- **Distance → inverse-square attenuation + propagation delay** ("move the mic closer/farther") — the air
  edge is unity and the one-block delay is a cycle-break, not calibrated time. The natural next acoustic depth.
- **Polar / off-axis response, proximity/placement-based pickup, speaker frequency-response / cabinet
  coloration** (flat for now), and **bespoke skeuomorphic vox/mic/monitor faceplates** (the generic `Panel`
  covers them).
- **Air → ear / room acoustics** — out of scope by design; the model is device-to-device.
- **SoundFont / multisample instruments** — a separate future epic (no rustysynth, no sample assets, no
  polyphony machinery here).

### Story-by-story (status + the one thing each settled)

- **8.1 — The acoustic carrier, vox source & mic transduction (headless)** ✅ — `Pascals` / `PressureBuffer` /
  `Domain::Acoustic` / `AcousticRoute`, a `SoundSource`, and the mics as transducers. Settled: **acoustic is a
  first-class fourth carrier at the analog rate; SPL is a measurement, and the mics' sensitivity gap is
  emergent** (the self-tone is gone — an unpatched mic is silent).
- **8.2 — Micing in the studio (web + scenarios)** ✅ — the client-side mirror of the carrier (acoustic
  domain/kind/connector, dashed cable, dB-SPL knob) + the micing gain-staging scenario. Settled: **the web
  mirrors legality (it doesn't own it), and dB SPL is a display keyed on unit, decoupled from taper.**
- **8.3 — Speakers as acoustic sources & acoustic feedback** ✅ — a `PoweredMonitor` voltage→acoustic
  transducer + acoustic edges made delay-eligible (air edge preferred). Settled: **howlround emerges from a
  delayed air loop and self-limits at the rail — feedback is never special-cased**, and the one-block delay
  belongs on the physically-honest air hop.

---

## Epic 9 — Instruments: SoundFont-Based MIDI Voices — 🚧 **In progress (9.1 ✅; 9.2 ✅; 9.3 🚧)**

**Progress:** Story 9.1 ✅ **done — the feasibility spike returned GO**: `rustysynth` v1.3.6 (MIT) renders a
recognizable GM note and its render path is **zero-alloc** (even including the first render, with a chord
held, and through a voice-stealing burst past its 64-voice pool, reverb/chorus on), **panic-free** on the
note path under adversarial input, and **compiles for `wasm32-unknown-unknown`** loading the font from a
`Cursor<&[u8]>`. No fork / `OxiSynth` fallback needed. Story 9.2 ✅ **done**: the `SoundFontSource` node +
headless `→ DaConverter →` volts chassis — events become a recognizable, correctly-pitched, polyphonic,
velocity-sensitive GM voice, zero-alloc and panic-free on the node's own `process`. Story 9.3 🚧 **in
progress** — *elaborated below*: a shared parsed-bank asset cache, asset-aware device construction, and
browser→worklet→WASM delivery of the `.sf2`, with preset selection kept as structural config. **Next
after 9.3: Story 9.4 — the audible payoff (instrument devices + web).** Stories 9.4–9.5 stay coarse,
fleshed out on arrival. This is the epic Epic 8
explicitly deferred ("SoundFont / multisample instruments —
a separate future epic (no rustysynth, no sample assets, no polyphony machinery here)"). It gives the
simulator **recognizable played instruments** — an electric guitar you patch into a dist pedal + amp, an
acoustic guitar / horn you mic — using a General-MIDI SoundFont played by a ready-made Rust synth
([`rustysynth`](https://crates.io/crates/rustysynth), MIT), wrapped as a black-box node behind the engine's
event lane and internal DA.

**Goal:** make instruments a first-class *source you play* — MIDI note-on/off + velocity in, a recognizable
GM voice out — while staying inside every non-negotiable (`PROJECT_PLAN` §2/§6: **instruments are
recognizable, not realistic**; the realism budget stays on the volts-and-converters layer). An instrument
is modeled honestly as a **digital sound module** (a rompler / digital piano): SoundFont sample playback at
its own digital rate → an **internal DA** → an analog line-out (volts) or an acoustic radiator (Pascals).
Interaction is correct by carrier, not by special case: an electric-guitar patch comes out a **hi-Z
instrument out in volts** (its dirt comes from the pedal/amp downstream — analog nodes, where the realism
budget belongs); an acoustic-guitar / horn patch comes out an **acoustic output** you mic with the Epic-8
condenser/dynamic mics.

**Exit criteria:** you can place an instrument device, play it (QWERTY / Web MIDI / focus keybed) and hear a
recognizable polyphonic GM voice; the electric-guitar instrument drives a distortion chain audibly, and the
acoustic instrument is mic'd through the Epic-8 acoustic carrier; all glitch-free and inside the zero-alloc
/ panic-free / deterministic hot-path contract, verified in-browser.

**Watch-outs:**
- **The hot-path contract is the whole risk.** `process` must not allocate or panic
  (`crates/engine/tests/no_alloc.rs` enforces zero-alloc over 128 blocks). `rustysynth` pre-allocates its
  voice pool at construction and its `render` path shows no `Vec` growth / I/O / time — but this is a claim
  to **verify under the counting allocator**, not assume. Story 9.1 exists to prove it *before* anything is
  built on it.
- **Do not resample at a fractional ratio.** Pin the synth's internal rate to **48 kHz** so 48 k → 384 k is
  exactly the existing **8×** `Interpolator` / `DaConverter` path; 44.1 k would force a fractional resampler
  we don't have and don't want here.
- **The event lane stays note-only.** No CC / pitch-bend / program-change on the event lane (it would blur
  the two-input-lane separation — `CLAUDE.md` §5). Instrument/preset **selection is a device config/param**,
  not a MIDI CC.
- **Assets are a construct-time concern, never a hot-path one.** SoundFont bytes (~6 MB) load and parse at
  construction; `process` only reads pre-built tables. The `NodeBuilder`/`DeviceConfig` (scalar-only)
  cannot carry the blob — a host→node byte seam is required (Story 9.3).
- **Engine leanness.** `rustysynth` is a notable dependency in the otherwise-lean `engine`; justified
  because **nodes live in `engine`** and it is std-only, dep-free, MIT, and wasm-buildable — matching
  engine's portability constraints. Keep it isolated behind the one node module.

**Notes (relation to the deferral & the carrier model):** this realizes `PROJECT_PLAN` §4's
*Instrument / source* and §6's *Instruments* as a recognizable-not-realistic voice. Nothing about the
analog/voltage spine changes: the instrument is a **digital device with a converter**, so "digital only
exists after a converter" holds — the converter simply lives *inside* the black-box chassis, exactly as
`PROJECT_PLAN` §5.6's "one chassis, many nodes" allows.

### Settled architecture decisions (at epic planning)

- **An instrument is a digital sound module, not a magic analog source.** It plays SoundFont samples at a
  digital rate and crosses an **internal DA** to analog — modeled as a **two-node chassis**
  (`SoundFontSource` emitting on the `Sample` lane @ 48 kHz → `DigitalRoute` → the existing `DaConverter` →
  analog out), reusing the shipped DA/`Interpolator` path wholesale. *Rejected:* a source that emits volts
  directly (it would hide a converter as a fudge and re-implement DA badly, breaking "converters are the
  only domain bridge").
- **Wrap `rustysynth` as the black-box DSP core; do not write an SF2 sampler.** The black-box philosophy
  (`PROJECT_PLAN` §3) explicitly permits delegating internal DSP; an SF2 engine (generators, modulators,
  loop points, filters, per-voice envelopes) is large and buys nothing given instruments are non-realistic.
  **Confirmed by the 9.1 spike — no fork or fallback needed** (`rustysynth` v1.3.6 is zero-alloc + panic-free
  on the render path as-is). The [`OxiSynth`](https://github.com/PolyMeilex/OxiSynth) / fixed-pool-fork
  fallback is therefore **not exercised** and recorded only as a contingency if a future need arises.
- **Polyphony and velocity come from the black box, not from us.** This satisfies Epic 1's deferred
  *polyphony / voice allocation* item **by delegation** — `rustysynth` owns voice allocation / stealing and
  velocity layers. The engine's `EventMessage::NoteOn { note, velocity }` already carries velocity (today's
  `SynthVoice` ignores it; the sampler uses it).
- **48 kHz internal, 8× to the 384 kHz analog rate.** Fixed integer ratio; one engine block (1024 analog
  samples) = 128 frames @ 48 kHz — precisely the AD/DA block contract already in place. Stereo synth output
  is summed to mono (engine is mono, epic-wide).
- **v1 event scope: note-on/off + velocity + per-device preset selection.** No CC / pitch-bend / program
  change on the event lane; preset choice is a config/param. Extending `EventMessage` for pitch-bend /
  sustain is a **9.5 (optional/later)** decision, deliberately deferred.
- **One synth core, two output faces.** The electric-guitar instrument exposes a **voltage instrument-out**
  (hi-Z); the acoustic instrument exposes an **acoustic-out** (Pa via a sample→transducer, reusing the
  Epic-8 `PoweredMonitor` pattern) to be mic'd. Same `SoundFontSource` core; the face and the loaded GM
  preset differ per device.
- **Asset delivery is construction context, not a live node-control lane.** The host fetches the `.sf2`
  before initial patch compilation and transfers it across the worklet/WASM boundary under a stable
  **asset id**. `SceneEngine` owns the parsed-bank cache across schedule swaps; asset-aware device builders
  borrow it and each instrument constructs independent synth/voice state over the shared immutable bank.
  *Rejected:* a `DawControl`-style upload keyed by device id — the node needs the bank to construct its
  synth, an unloaded/silent node is a bad intermediate state, and identical instruments would duplicate the
  same parse/sample data. `SoundFont::new` reads from a `Cursor<&[u8]>` on wasm (no `File`). Default GM set:
  **TimGM6mb (~6 MB)**
  (the font `rustysynth`'s own example ships; loads cleanly); *not* FluidR3 (~140 MB). **The font is a real
  compatibility decision, not a free variable** — `rustysynth` runs a structural `SanityCheckFailed` gate at
  load, and the 9.1 spike found the tiny FluidSynth demo font (VintageDreamsWaves) *rejected* by it while
  TimGM6mb loaded. Validate any candidate font against `SoundFont::new` before adopting it.
- **Determinism preserved.** SoundFont playback is deterministic (no RNG; `rustysynth` reverb/chorus are
  deterministic DSP, and none of the render path touches `Instant`/`SystemTime`/threads). Same seed ⇒ same
  output holds.

### Stories

Coarse until picked up, per "flesh out on arrival." Stories 9.1–9.3 are elaborated below.

- **Story 9.1** — SoundFont hot-path feasibility spike (**the go/no-go gate**) — *elaborated below.*
- **Story 9.2** — The `SoundFontSource` node + internal-DA chassis (headless): events → 48 kHz render →
  mono → 8× → volts; monophonic-through-polyphonic notes proven by tests; zero-alloc/panic-free green.
  — *elaborated below.*
- **Story 9.3** — Asset-delivery seam: host fetches the `.sf2`, bytes cross the worklet/WASM boundary
  under a stable asset id; parse once, cache the parsed bank across recompiles, and feed it to fallible
  asset-aware device construction; preset selection stays structural config. — *elaborated below.*
- **Story 9.4** — Instrument devices + web: an electric-guitar device (voltage instrument-out → dist pedal
  + amp chain) and an acoustic device (acoustic-out → mic'd); catalog entries, faceplates, focus keybed,
  played in-browser. The epic's audible payoff.
- **Story 9.5** *(optional/later)* — depth: velocity curves, more GM instruments, and — if wanted — an
  `EventMessage` extension for pitch-bend / sustain-pedal (revisits the note-only decision).

### Story 9.1 — SoundFont hot-path feasibility spike — ✅ **Done** (verdict: **GO**)

*Goal:* prove — **before** building anything on it — that `rustysynth` can serve as the black-box synth
core inside the engine's non-negotiable hot path: zero-alloc and panic-free in steady state under the
counting allocator, wasm-buildable loading from bytes, deterministic, and producing a recognizable GM note.
This is a **spike**, not production wiring: throwaway harness code that answers go/no-go and records the
verdict + any required mitigation (fork / `OxiSynth` fallback).

*Watch out:*
- The whole epic rests on this gate — do not soften it. "Looks alloc-free in the source" is not the bar;
  **the counting allocator asserting zero over N blocks with active voices** is.
- Steady state means **voices already active** (a chord held) — a note-on that activates a pooled voice
  must not grow anything; verify with notes held across the measured blocks, and also across a
  voice-stealing event (more note-ons than `maximum_polyphony`).
- wasm has no `File` — load the SoundFont from a `Cursor<&[u8]>`; confirm `cargo wasm` compiles the engine
  with `rustysynth` in its dep tree.
- This spike may allocate/parse freely at **construction** (that's allowed); the assertion is only on the
  `render` path.

*Design notes (settled at planning):*
- **Spike lives behind a native test + a wasm build check**, not in the shipped node yet — the
  `SoundFontSource` node is Story 9.2. Keep `rustysynth` isolated so a fallback swap is cheap.
- **Reuse the `no_alloc.rs` counting-allocator harness pattern** rather than inventing a new one — the same
  `#[global_allocator]` guard the engine already trusts.
- A **small GM SoundFont test asset** is vendored for the spike (TimGM6mb or a trimmed GM set); the license
  of the bundled font is checked and recorded (distinct from `rustysynth`'s own MIT).
- Panic audit is concrete: `rustysynth`'s `note_on` bounds-checks channel but has at least one unchecked
  index (`self.sound_font.presets[preset]` after a lookup) — feed adversarial inputs (out-of-range
  key/velocity, empty/garbage soundfont at construction, exhausted polyphony) and record whether any
  reaches a panic on the render/note path; if so, define the clamp/guard wrapper.

- **Task 9.1.1** — Vendor `rustysynth` + a small GM SoundFont asset; a throwaway native test loads the
  font, `note_on` a middle-C GM piano, renders N frames, and asserts non-silent, finite output. *Done:*
  the library plays a recognizable note in-repo; the bundled font's license is recorded.
- **Task 9.1.2** — **The zero-alloc gate.** Wrap a steady-state `render` loop (a chord held across ≥128
  render blocks, plus a voice-stealing burst) in the `no_alloc.rs` counting-allocator harness; assert zero
  allocations on the render path. *Done:* the assertion is green, or the exact allocation site + a
  mitigation (fixed-pool fork) is documented as the go/no-go verdict.
- **Task 9.1.3** — **The panic-free gate.** Drive adversarial inputs (out-of-range key/velocity, exhausted
  polyphony, degenerate soundfont) through `note_on`/`render`; confirm no panic on the render/note path, or
  specify the clamping wrapper the node will apply. *Done:* the panic surface is characterized and either
  clean or wrapped.
- **Task 9.1.4** — **The wasm gate.** Confirm `cargo wasm` compiles `engine` with `rustysynth` in the dep
  tree, and that the SoundFont loads from a `Cursor<&[u8]>` (not `File`). *Done:* the wasm portability check
  is green loading from bytes.

*Validate:* `rustysynth` renders a recognizable GM note in-repo, verified **zero-alloc and panic-free on the
render path** under the counting allocator (chord held + voice-stealing), and `cargo wasm` compiles the
engine with it loading a SoundFont from bytes — the go/no-go verdict (and, if no-go, the fork / `OxiSynth`
fallback decision) recorded.

*Delivered (spike — GO):* proven with `rustysynth` **v1.3.6** (MIT) + **TimGM6mb.sf2** (~6 MB GM, the font
`rustysynth`'s example uses). All four gates green, in a throwaway integration test + a throwaway wasm crate
(both removed after the verdict was recorded — kept only in commit history, per the "spike is throwaway"
rule):
- **9.1.1** — plays a recognizable middle-C GM note (finite, non-silent). Synth defaults observed: **64-voice
  polyphony, 64-frame block, reverb+chorus on**.
- **9.1.2 — the go/no-go gate: PASS.** A counting `#[global_allocator]` (the `no_alloc.rs` pattern) recorded
  **zero** allocations on the render path — *including the very first `render`*, across 128 blocks with a
  C-E-G chord held, **and** through a voice-stealing burst (80 note-ons past the 64-voice pool), with
  reverb/chorus enabled. `rustysynth` pre-allocates its voice pool at construction and grows nothing in
  `render`; it drops into the hot path **as-is** — no fork, no `OxiSynth` fallback.
- **9.1.3 — panic-free (note path).** Adversarial `note_on(99, 200, 300)`, `note_on(0, -5, -1)`,
  `note_off(50, 999)` + `render` did not panic. **Nuance:** this exercised only the *note* path; the known
  unchecked `self.sound_font.presets[preset]` index is on the **program-change** path, which v1 never drives
  (preset is set once at construction, per the note-only event scope). **If 9.5 ever adds program-change via
  events, that index needs a guard** — recorded so it isn't forgotten.
- **9.1.4 — wasm: PASS.** `rustysynth` compiles for `wasm32-unknown-unknown` and loads the font from a
  `Cursor<&[u8]>` (bytes, not `File`) — the exact browser path.
- **Learning — font choice is a compatibility decision, not a free variable:** `rustysynth` runs a
  structural `SanityCheckFailed` gate at load; the tiny FluidSynth demo font (VintageDreamsWaves) was
  *rejected*, TimGM6mb accepted. Validate any candidate `.sf2` against `SoundFont::new` before adopting it.
- **Consequence for 9.2+:** `rustysynth` is **not** carried as an engine dependency yet — it enters as a real
  `[dependencies]` entry when the `SoundFontSource` node lands in Story 9.2, isolated behind that one module.

### Story 9.2 — The `SoundFontSource` node + internal-DA chassis (headless) — ✅ **Done**

*Goal:* build the black-box instrument core as a real engine node and prove the whole digital→analog chassis
headless. `SoundFontSource` consumes the **event lane** (note-on/off + velocity), renders a GM voice through
`rustysynth` at **48 kHz**, folds stereo→mono, and emits on the **`Sample` lane**; the existing `DaConverter`
takes it **8× → 384 kHz volts**. This realizes the epic's "an instrument is a digital sound module with an
internal DA" decision (`PROJECT_PLAN` §5.6 "one chassis, many nodes") end to end, with everything **inside the
zero-alloc / panic-free / deterministic hot-path contract** — the risk the 9.1 spike de-risked for raw
`rustysynth`, now re-proven on the *node's own* `process` (event drain + chunked render + fold). No devices,
no faceplates, no byte seam yet — the font is fed straight in for tests (asset delivery is 9.3; the
voltage/acoustic output faces + catalog are 9.4).

*Watch out:*
- **The hot path is the whole risk (again).** The spike proved raw `render()` over 128-frame slices is
  zero-alloc; 9.2 must re-prove it for the *node's* `process` — event draining, **sub-sliced** renders at
  arbitrary event frames, and the stereo→mono fold — under `no_alloc.rs`. Verify, don't assume the spike covers it.
- **rustysynth quantizes note starts to its internal `block_size`** (default **64** frames ≈ 1.3 ms). A
  `note_on` between `render` calls only takes effect at the next internal block boundary — so sub-slicing our
  renders at event frames buys ordering + timing down to `block_size`, not to the individual frame. This is the
  honest control-rate of a sample module, **not a bug**; documented so the sample-accuracy expectation is calibrated.
- **`prepare()` gives only the analog rate, never the block length** — so a heap scratch buffer for rustysynth's
  stereo output can't be sized at compile. Render in fixed-size **stack** chunks (`[f32; 64]`) instead; no heap
  scratch, and the chunk loop *is* the event-sub-slicing loop.
- **The source writes `Sample` with no AD in front of it**, so it must **clamp its own mono output to ±1.0** —
  it is the module's internal full-scale digital bus, and rustysynth can exceed ±1 on reverb/transient peaks.
- **Keep `rustysynth` out of engine's *public* API.** The constructor takes **bytes** and parses internally
  (fallible: wrap `SanityCheckFailed`); no `rustysynth::*` type appears in a `pub` signature. Isolated behind
  the one `node/sound_font.rs` module.
- **Novel node topology:** event-in + digital-out (the `SynthVoice` is event-in/analog-out; the AD is
  analog-in/digital-out). It's the first of its kind — verify the schedule partitions/sizes it correctly (it
  reuses the AD's `DigitalFace` lane-sizing path, so this should be free, but confirm at build).

*Design notes (settled at planning):*
- **Timing: sub-slice the render at event digital-frames.** In `process`, drain events in order; for each event
  at digital frame `f = offset / M`, render frames `[cursor, f)` (in `[f32; 64]` stack chunks, folding
  L+R→mono), then apply the clamped `note_on`/`note_off`; render the remainder after the last event.
  Granularity resolves to rustysynth's `block_size` (kept at the default **64**). *Rejected:* block-rate MIDI
  (apply all events at frame 0, one full render) — simpler but throws away the event lane's sample-accuracy
  entirely (quantizes to the whole ~2.67 ms engine block), against the two-lane ethos (`CLAUDE.md` §5). Finer
  `block_size` (≈16) was also rejected for v1: ~4× the reverb/chorus/env overhead for sub-ms gains nobody asked for.
- **Stereo→mono fold = `0.5·(L+R)`.** The engine is mono epic-wide; averaging (not summing) keeps a centered
  voice at unity rather than doubling it.
- **Reverb/chorus OFF for the headless chassis.** Cleaner release-to-silence assertions (no reverb tail) and
  lower DSP; both are deterministic either way (the determinism claim holds on or off). Richness is a 9.4 call.
- **Source is param-free in 9.2.** Preset selection is a 9.3 config concern; loudness rides the DA reference /
  downstream gain; velocity rides the event. `Gate(bool)` events are **ignored** (a soundfont note needs a
  pitch a bare gate can't supply). `note_on` key/velocity are **clamped to 0–127** as the safe wrapper (the
  9.1 panic audit found the note path clean, but clamping costs nothing and future-proofs).
- **Font fed straight in for tests.** `SoundFontSource::new(&sf2_bytes, preset)` parses at construction; tests
  read `assets/soundfonts/TimGM6mb.sf2` (a **shared** repo-root asset the 9.4 web build will reuse) via
  `CARGO_MANIFEST_DIR` at runtime. `rustysynth` enters `crates/engine/Cargo.toml` as a real dependency here.
- **Headless DA chassis is provisional.** For 9.2 the `DaConverter` reference/`z_out` are chosen for clean test
  assertions (a 1.0 V reference ⇒ full-scale ±1 → ±1 V); the real hi-Z instrument-out vs acoustic-out faces and
  their calibration are 9.4 — **not a bug** that 9.2's numbers are stand-ins.

- **Task 9.2.1** — `rustysynth` dep + the shared `TimGM6mb.sf2` asset + the `SoundFontSource` node skeleton
  (event-in / `Sample`-out @ 48 kHz, construct-time fallible parse, clamped note wrapper, chunked stereo→mono
  render). *Done:* a native test drives the node alone — note-on middle C, run `process`, assert the `Sample`
  lane is finite and non-silent; garbage bytes → a construction error, not a panic.
- **Task 9.2.2** — The headless chassis end-to-end: `SoundFontSource → DaConverter` in a `Graph` (digital edge,
  8× → 384 kHz). *Done:* silent-until-triggered; a played note is finite non-silent volts; its fundamental
  matches the played pitch (hand-checked via `tone_amplitude`); a mid-block note-on is silent before its
  (block-quantized) frame; note-off releases toward silence.
- **Task 9.2.3** — Polyphony + velocity from the black box. *Done:* a held chord sounds multiple simultaneous
  pitches (each fundamental present); higher velocity ⇒ larger output — proven by tests.
- **Task 9.2.4** — The hot-path gates on the *node's* `process`. *Done:* `no_alloc.rs` extended with the
  `SoundFontSource → DA` chassis driven by events asserts **zero** allocations over ≥128 blocks (chord held +
  voice-steal burst); adversarial events (out-of-range key/velocity, exhausted polyphony, note-off for absent
  notes) are panic-free; `cargo wasm` still compiles `engine` with `rustysynth` in the tree.

*Program design:* `docs/epics/epic-9/story-9.2-soundfont-source.md`

*Validate:* ✅ **met** — the headless `SoundFontSource → DaConverter` chassis turns note-on/off + velocity
events into a recognizable, correctly-pitched, polyphonic GM voice in **volts**, verified **zero-alloc and
panic-free on the node's `process`** under the counting allocator (chord held + voice-stealing), deterministic,
with `cargo wasm` green — the full pre-push gate passing.

*Delivered:* the `SoundFontSource` node (`crates/engine/src/node/sound_font.rs`) wrapping `rustysynth` v1.3.6
(now a real, isolated `engine` dependency) — event-in (`EventFace`) → digital `Sample`-out @ 48 kHz — plus the
headless chassis proven end-to-end through the shipped `DaConverter` (8× → 384 kHz volts). All four tasks green
under the full pre-push gate.
- **9.2.1** — the node: construct-time fallible parse (`SoundFontError`, no `rustysynth` type in any public
  signature); font read from a `Cursor` (the wasm path); events drained and the render **sub-sliced at each
  event's digital frame**, rendered in `[f32; 64]` **stack** chunks (no heap scratch — `prepare` isn't told the
  block length), folded L+R→mono (`0.5·(L+R)`), **clamped to ±1.0** (the node is the module's internal
  full-scale digital bus). Node-alone tests in `crates/engine/tests/soundfont.rs`.
- **9.2.2** — chassis wired in-test (`SoundFontSource → DaConverter`, `connect_ideal` → a `DigitalRoute`):
  silent-until-triggered, finite audible volts, **in-tune** (A4's 440 Hz bin ≫ a detuned 550 Hz bin), timing
  honored (with a guard band), note-off releases.
- **9.2.3** — polyphony proven by a **differential** test (a held chord keeps the non-last notes ringing, which
  a monophonic voice couldn't); velocity scales sustained RMS.
- **9.2.4** — the hot-path gates on the node's *own* `process`: `no_alloc.rs` extended with the chassis (chord
  held + 80-note-on steal burst) asserts **zero** allocations over 128 blocks; adversarial events (out-of-range
  note/velocity, note-off for an absent note, exhausted polyphony) are panic-free and finite; `cargo wasm`
  builds `engine` + `rustysynth` for wasm32.

*Deviations & known simplifications (not bugs):*
- **Reverb/chorus left off** and the **source is param-free** (fixed GM preset at construction, no LEVEL) — the
  richness pass, preset **selection**, and the byte seam are 9.3/9.4. Loudness currently rides the DA reference.
- **Provisional 1 V DA reference** in the test chassis (full scale → ±1 V) — the real hi-Z instrument-out vs
  acoustic-out faces and their calibration are Story 9.4.
- **Note timing quantizes to `rustysynth`'s internal `block_size`** (64 frames ≈ 1.3 ms) — the honest
  control-rate of a sample module; sub-slicing honors ordering, not the individual frame.
- **Pitch/release tests use the drawbar organ (GM 16)**, a sustained preset the single-bin DFT reads cleanly;
  the default piano's decay would muddy them. The "plays a note" test still uses the piano.
- **`test_util` is `#[cfg(test)]`-internal**, so `soundfont.rs` (an integration test) carries small local
  `rms`/`tone_amplitude` copies.
- **Correction to a 9.1 note:** the `self.sound_font.presets[preset]` index `rustysynth`'s `note_on` performs is
  actually **safe** — `preset` there is a *resolved* lookup value with a GM fallback, not the raw program number
  (`set_patch` just stores the number). So a future 9.5 program-change-via-events would **not** need a guard on
  that index (channel-range is already checked internally).

### Story 9.3 — Shared SoundFont assets across the browser boundary — 🚧 **In progress**

*Goal:* turn Story 9.2's test-only `SoundFontSource::new(&bytes, preset)` construction into the real
asset seam Story 9.4's devices can build on. The browser fetches the vendored `.sf2` once, transfers it
through the AudioWorklet boundary before the initial patch compiles, and WASM parses it once into a
long-lived cache under a stable asset id. Device construction borrows that cache: every instrument gets
independent mutable synth/voice state and a structurally selected GM preset, while all instances and later
schedule recompiles share the same immutable parsed sample bank. This Story delivers infrastructure and
its end-to-end proof; the player-visible electric/acoustic devices and faceplates remain Story 9.4.

*Watch out:*
- **Asset work is cold-path only.** Fetching, byte transfer, parsing, cache mutation, synth construction,
  and error formatting happen before rendering or during a structural rebuild — never in `process`.
  `SoundFontSource::process` must remain zero-alloc, panic-free, deterministic, and byte-I/O-free.
- **The initial patch cannot race its assets.** An asset-dependent device must not compile as a silent
  placeholder and wake up later. Required assets arrive before `build_patch`; missing/rejected assets are
  legible build errors and the current live scene survives a failed hot-swap.
- **Share only immutable bank data.** A parsed SoundFont may be shared (`Arc` internally), but each device
  must own a fresh `Synthesizer`; sharing one synth would leak notes, envelopes, and voice stealing between
  instruments.
- **Do not bloat the WASM binary or duplicate the source asset.** `TimGM6mb.sf2` stays the single tracked
  repo asset and is staged as a generated web artifact for fetch; it is not `include_bytes!`-embedded in
  Rust and not committed a second time under `web/public`.
- **Distribution provenance is a real gate.** Story 9.1 says the font's license was recorded, but no
  license/attribution file exists beside the asset today. Verify and record its redistributable terms
  before treating the browser-shipped font as complete.

*Design notes (settled at planning):*
- **Split the immutable bank from the mutable player.** Add an opaque, cloneable engine `SoundFontBank`
  that owns `Arc<rustysynth::SoundFont>`; parsing bytes builds the bank once. `SoundFontSource::from_bank`
  constructs a fresh `Synthesizer` from it for each device. No `rustysynth` type enters a public signature.
- **Cache by stable asset id, not device id.** The cache maps a content identity such as `tim_gm_6mb` to
  one parsed bank. Several guitars/horns share it while keeping their own preset and synth state; renaming
  or removing a scene device does not affect asset lifetime. *Rejected:* per-device caching, which conflates
  resource identity with placement identity and duplicates the same ~6 MB parse/sample data.
- **`SceneEngine` owns cache lifetime across schedules.** The cache sits above `BuiltScene`, so
  `load_patch`/config changes can build a replacement schedule from cloned bank handles without reparsing.
  The built schedule owns the handles it needs; swapping/dropping a schedule cannot invalidate the cache.
- **Device building becomes asset-aware and fallible.** Assets are explicit construction context rather
  than globals. The ordinary asset-free catalog entries remain simple; an asset-backed builder reports a
  missing bank or synth-construction failure through `BuildError`. Descriptor generation must not parse the
  font merely to discover the node's fixed ports.
- **Preset stays structural and per device.** A scalar `preset` config is normalized to GM program
  `0..=127` when the device builds a fresh synth; changing it follows the existing config→recompile path.
  It is not a smoothed param and does not add program-change to the event lane. The visible selector and
  concrete preset-bearing instrument entries arrive in 9.4; 9.3 proves the construction contract with a
  private fixture.
- **Preload the known v1 asset.** The main thread fetches the generated `/soundfonts/TimGM6mb.sf2` artifact
  once, carries its `ArrayBuffer` in the initial processor options, and the worklet supplies the asset
  bundle when constructing `SceneEngine`. Dynamic user-imported fonts and eviction are intentionally out of
  scope; the stable one-font manifest is enough for the planned instruments and leaves the cache additive.

- **Task 9.3.1** — Separate SoundFont parsing from voice construction: add `SoundFontBank`, construct
  independent `SoundFontSource`s from a shared bank, migrate the 9.2 tests/no-alloc chassis, and prove two
  sources share immutable data without sharing note state.
- **Task 9.3.2** — Add the asset-aware construction context: a stable-id bank library passed explicitly
  through device/patch building, fallible asset-backed builders with missing/build error variants, and the
  scalar preset normalization contract; prove it with a private fixture without publishing a 9.4 device.
- **Task 9.3.3** — Carry the default asset browser→worklet→WASM before initial compilation: stage the one
  tracked `.sf2` into generated web output, fetch/transfer it once, populate `SceneEngine`'s persistent cache,
  and record the asset's verified distribution license/provenance.
- **Task 9.3.4** — Prove the lifecycle end to end: initial construction and preset change use the cached
  bank, repeated patch hot-swaps do not reparse it, multiple sources remain state-independent, and
  missing/malformed assets fail legibly without replacing the live scene; run the full Rust/WASM/web gates.

*Program design:* `docs/epics/epic-9/story-9.3-soundfont-assets.md`

*Validate:* from the browser-facing initialization path, the one tracked TimGM asset is fetched and
transferred before patch construction, parsed exactly once under a stable asset id, and reused by
independent preset-selected sources across config changes and schedule hot-swaps. Missing or malformed
assets produce legible cold-path errors and never replace a live scene; the SoundFont render path remains
deterministic, zero-alloc, panic-free, and wasm-buildable, with the complete Rust and web gates green.
