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

## Epic 8 — The Acoustic Domain & Micing — 🚧 **In progress** (8.1, 8.2 done; 8.3 in progress)

**Progress:** Story 8.1 ✅ **Done** (the headless acoustic carrier, vox source & mic transduction); Story 8.2
✅ **Done** (the web/product mirror of the carrier + the micing-scenario exit gate — **394 engine + 65 devices
+ 283 web tests green**); Story 8.3 🚧 **In progress** (planned to Task level — speakers as acoustic sources +
acoustic feedback). This Epic activates
the **"air link" seam** both microphones' docs have long anticipated ("a real acoustic path… with a
pressure↔volts transduction seam at each end; until then the sine is the seam"). It adds a fourth signal
carrier — **acoustic sound pressure** — so that an instrument is a *source you mic*, not a line-out: the
richest lesson of the medium (mic choice, phantom, gain staging from mic level, PAD, the dynamic-vs-condenser
gap) becomes real. Sound *generation* stays deliberately trivial here (a sine at a chosen loudness);
recognizable **SoundFont/multisample instruments are a separate future Epic**, not this one.

**Goal:** make the acoustic side a first-class, physically-honest carrier — sound pressure in **Pascals**,
oversampled against the same analog continuum as volts, measured in **dB SPL** exactly as volts are measured
in dBu — and turn the two microphones into genuine **acoustic→analog transducers** fed by a patchable sound
source. Loudness at the source (whisper vs. shout, an acoustic guitar vs. a cranked amp) drives the whole
front-end gain-staging story, and it all emerges from `v = pressure × sensitivity` + the existing phantom /
balanced / noise-floor physics. Anchored to `PROJECT_PLAN.md` §4 (the "instrument / source" concept, made
real) and §5.5 (see the scope note below).

**Exit criteria:** you can place a sound source in the studio, mic it with a condenser or a dynamic, and the
recorded level, the required preamp gain, and the clipping/PAD behavior all match hand calculations and
real-world intuition across the SPL range — with the acoustic carrier compile-validated like every other
domain and the hot path still zero-alloc / panic-free.

**Watch-outs:** keep the realism budget on volts-and-converters — **air is ideal** (no propagation, no room,
no polar response, no distance); pressure rides at the **`AnalogRate`**, so do **not** invent an acoustic
"sample rate" or any global acoustic↔analog rate relationship (§7). SPL is a *measurement* — a buffer holding
dB SPL is a category error, exactly as a buffer holding dBu would be.

### Decisions that bind this Epic (settled in design discussion)

- **Acoustic is a real carrier, not a fudge.** Sound pressure is `Pascals` (a unit newtype like `Volts`),
  stored linearly in a `PressureBuffer` oversampled at the one `AnalogRate` (pressure is continuous-time
  analog — **not** a digital rate). **SPL** (ref 20 µPa) is a measurement helper, never storage — the anchor
  is **1 Pa = 94 dB SPL** (≈93.98). This mirrors the volts/dBu split exactly.
- **The microphone is the acoustic→analog converter** — the input-side bookend to the speaker. Converters
  remain the *only* cross-domain bridge (§5.6): an acoustic edge may only join acoustic ports; acoustic↔analog
  is rejected at `compile` as `DomainMismatch` (free — the check already compares `domain()`). Unlike the
  AD/DA, the transduction is a **pointwise multiply by sensitivity (V/Pa)** — same rate, no anti-alias FIR,
  no resample.
- **Air is ideal.** The acoustic edge is a **unity copy** (`EdgeKind::AcousticRoute`) with **no impedance
  divider** — air is not a Thévenin/load network. Distance/inverse-square attenuation, propagation delay, and
  polar/off-axis response are **deferred** (distance is where "move the mic closer/further" becomes real).
- **Scope relationship to `PROJECT_PLAN.md` §5.5.** That section keeps acoustics out of scope on the
  **output** side (the speaker stays in volts; no V→SPL, no air→ear). This Epic adds an **input-side** acoustic
  carrier — a source's sound pressure into a mic capsule — with air idealized to a direct handoff; and, in
  **Story 8.3**, a matching **output-side** radiation stage: a *distinct* voltage→acoustic converter (a powered
  monitor) that radiates `pressure = drive_volts × sensitivity`, so a device can be **mic'd**. This is a
  deliberate, scoped extension of §5.5 (settled at 8.3 planning): the **voltage-capture `Speaker` terminus stays
  untouched** (the §5.5 output tap in volts — the whole `da→spk` capture path is unchanged); what's new is
  **speaker→air→mic**, a closed device-to-device loop, **still not air→ear** (no listener, no room, no
  propagation/distance). It is *not* instrument realism (§2): the source is a sine, the bar is "recognizable +
  real dynamics," and the machine-gun effect is acceptable. _(A one-line note in PROJECT_PLAN §4/§5.5 recording
  this input+output acoustic carrier — device-to-device only — is a reasonable follow-up.)_
- **Sensitivity calibration is backward-consistent.** Mic sensitivities are chosen so the pre-Epic-8 mic
  defaults reproduce at 94 dB SPL: condenser **~10 mV/Pa** (10 mV @ 1 Pa == the old 0.01 V default), dynamic
  **~2 mV/Pa** (== 0.002 V). The dynamic's **~14 dB deficit** (⇒ ~14 dB more preamp gain) becomes *emergent*
  from transducer efficiency, no longer an asserted LEVEL.
- **Air is patched as an "acoustic cable"** in the web layer — a distinct/dashed acoustic↔acoustic link
  reusing the existing patch machinery (the engine only ever sees an acoustic edge). Proximity/placement-based
  pickup is deferred.
- **SoundFont / multisample instruments are a separate future Epic** — no rustysynth, no sample assets, no
  polyphony machinery here.

### Story 8.1 — The acoustic carrier, vox source & mic transduction (headless engine) — ✅ **Done**

*Goal:* stand up the acoustic carrier end to end in the headless engine and prove the transduction physics by
hand calc — a `SoundSource` emitting sound pressure at a chosen SPL, carried over an ideal acoustic edge into
a microphone that transduces it to a balanced mic-level voltage, with the existing phantom / passive /
noise-floor behavior riding on top. No devices/UI yet (that is Story 8.2); this is the engine-before-product
slice, validated with tests as the oracle for the physical domain (§9).

*Watch out:*
- **Hot-path contract holds** (`CLAUDE.md` §6): `SoundSource` and the mic transduction are zero-alloc,
  panic-free, denormal-safe; the `no_alloc` counting-allocator test must stay green.
- **Determinism:** `SoundSource`'s oscillator is an `f64` phase accumulator with no ambient entropy (like
  `SynthVoice`/the mics); same seed ⇒ identical output.
- **One rate:** `PressureBuffer` is at `AnalogRate` — do not add a pressure rate or an acoustic↔analog rate
  relationship (§7). The acoustic lane is the same continuous proxy as the voltage lane.
- **Don't drop coverage when removing the self-tone:** the two mics' existing capsule-tone tests must be
  *rewritten* to drive the mic from a pressure input and assert transduction, not deleted.

*Design notes (settled at planning):*
- **The mics lose their self-tone.** The deterministic capsule sine and its `FREQ` / `LEVEL`-as-volts params
  are **removed** and replaced by a single `SENSITIVITY` (V/Pa) param; the mic now transduces whatever pressure
  arrives on its acoustic input, and an **unpatched mic is silent** (only its self-noise applies).
  *Rejected:* keeping the tone behind a "built-in test oscillator" toggle — the seam comments always framed the
  tone as a stand-in "until the air link," and a mic emitting a tone into a silent room is dishonest.
- **The source node is generic** (`SoundSource`): a sine at `FREQ` (Hz) emitted at a loudness knob in **dB SPL**
  (log — the engineer's unit; stored/emitted as Pa). Kept generic so a future drum/amp/guitar reuses it; the
  catalog (Story 8.2) labels one instance "vox".
- **The acoustic edge is a unity copy** (`EdgeKind::AcousticRoute`, mirroring `DigitalRoute`) — no divider, no
  cable R·C: air is ideal.
- **The carrier extends the open seams additively** — `Domain::Acoustic`, `Lane::Acoustic(PressureBuffer)` +
  `pressure()`/`pressure_mut()`, `AcousticFace` + `InputPort::Acoustic`/`OutputPort::Acoustic`
  (`lane_count() == 1`). No existing node's ports change except the two mics gaining one acoustic input.

*Tasks:*
- **Task 8.1.1 — `Pascals` unit + SPL↔Pa conversions.** A `repr(transparent)` newtype mirroring `Volts`, and
  `spl_to_pascals` / `pascals_to_spl` (ref 20 µPa) alongside the other level helpers. *Validate (hand calc):*
  1 Pa = 93.98 dB SPL; 94 dB SPL ⇒ ≈1 Pa; 0 dB SPL ⇒ 20 µPa; 30 dB SPL ⇒ 632 µPa; 120 dB SPL ⇒ 20 Pa.
- **Task 8.1.2 — `PressureBuffer` + the acoustic carrier seam.** `PressureBuffer` (linear Pa @ `AnalogRate`,
  mirroring `VoltageBuffer`); `Domain::Acoustic`; `Lane::Acoustic` + typed accessors; `AcousticFace` and the
  `InputPort`/`OutputPort` acoustic arms (`domain()`, `lane_count() == 1`, `acoustic()`, `From` impls). No node
  yet. *Validate:* the workspace compiles; unit tests for `domain()`/`lane_count()` and the lane accessors.
- **Task 8.1.3 — compile: the `AcousticRoute` edge + lane pool.** Add `EdgeKind::AcousticRoute` (unity
  pressure copy, mirroring `DigitalRoute`) and the acoustic arm in the lane-pool allocation. *Validate:* using
  scaffold test nodes (like `digital_seam`/`event_seam`), an acoustic edge copies pressure src→dst unchanged,
  and an acoustic↔analog edge is rejected as `DomainMismatch`.
- **Task 8.1.4 — the `SoundSource` node (vox engine node).** A sine at `FREQ`, emitted at a `LEVEL` knob in
  dB SPL → Pa → `Lane::Acoustic`; one acoustic output, no inputs; deterministic phase accumulator; zero-alloc,
  panic-free. *Validate (hand calc):* the emitted peak pressure is ≈1 Pa at 94 dB SPL and 20 Pa at 120 dB SPL
  (read back via `pascals_to_spl` on the block peak); silent at construction until prepared.
- **Task 8.1.5 — the mics become transducers.** `CondenserMic` and `DynamicMic` each gain one acoustic input;
  `process` computes the differential `s = pressure × sensitivity`, keeping the phantom pedestal / passive-0
  common-mode / power-gate logic on top; the self-tone + `FREQ`/`LEVEL`-volts params are replaced by
  `SENSITIVITY` (V/Pa). Existing capsule-tone tests rewritten to drive from a pressure input. *Validate (hand
  calc):* condenser 10 mV/Pa ⇒ 10 mV differential at 94 dB SPL (== old default), dynamic 2 mV/Pa ⇒ 2 mV
  (~14 dB less); whisper 30 dB SPL ⇒ ~6.3 µV; cranked amp 120 dB SPL ⇒ 200 mV; a dead/unpowered condenser is
  still silent regardless of input pressure.

*Validate (Story 8.1):* ✅ **met** — a headless `SoundSource → acoustic edge → mic → balanced receiver` chain
recovers `differential = pressure × sensitivity` to hand-calc tolerance across the oracle SPLs, the
condenser-vs-dynamic sensitivity gap is confirmed numerically, and the full gate is green
(`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`, `no_alloc` included).

*Delivered:* the acoustic carrier is a first-class fourth domain, end to end, headless.
- **Task 8.1.1** — `Pascals` newtype (`signal/pascals.rs`, mirroring `Volts`) + `spl_to_pascals` /
  `pascals_to_spl` (`level.rs`, ref 20 µPa). Anchor **1 Pa = 93.98 dB SPL** and the 30/94/120 dB oracles
  asserted. SPL is a measurement helper, never storage.
- **Task 8.1.2 + 8.1.3** — landed **together** (one indivisible compile unit: adding `Domain::Acoustic` /
  `Lane::Pressure` forces every exhaustive match — edge-kind assignment, the copy loop, the pool allocator —
  to gain its acoustic arm, so the workspace can't compile without the edge in place). `PressureBuffer`
  (`signal/pressure.rs`, linear Pa @ `AnalogRate`, mono), `Domain::Acoustic`, `Lane::Pressure` +
  `pressure()`/`pressure_mut()`, `AcousticFace` + `InputPort`/`OutputPort::Acoustic` (`lane_count() == 1`).
  `EdgeKind::AcousticRoute` = a **unity pressure copy** (ideal air — no divider, no cable R·C, the acoustic
  peer of `DigitalRoute`); `alloc_lane` grows an acoustic arm; the existing domain-match check rejects
  acoustic↔analog **for free**. `acoustic_seam` white-box tests prove the mono lane, the unity copy, and the
  `DomainMismatch`.
- **Task 8.1.4** — `SoundSource` node (`node/sound_source.rs`), the first node that *emits* pressure: a sine
  at `FREQ` radiating at a `LEVEL` = **peak pressure in Pascals** (linear, like `GainStage`'s gain — the
  dB-SPL knob is the UI's concern). Deterministic free-running phase; zero-alloc/panic-free. Oracles: 94 dB
  SPL ⇒ ≈1 Pa peak, 120 dB ⇒ 20 Pa.
- **Task 8.1.5** — both mics became **acoustic→analog transducers**: one acoustic input, `differential =
  pressure × sensitivity`, the self-tone oscillator + `LEVEL`/`FREQ` params **removed** and replaced by a
  fixed `SENSITIVITY` (V/Pa) — mics now have **no control params**; an unpatched mic is silent. Condenser
  keeps its phantom pedestal / power-gate on top; dynamic stays passive (0 CM, always alive). Sensitivities
  (condenser 10 mV/Pa, dynamic 2 mV/Pa) reproduce the old defaults at 94 dB SPL, and the **~14 dB gap is now
  emergent** from transducer efficiency. The engine phantom-integration tests were adapted (pedestal tests use
  an unconnected capsule; the audio-recovery test is fed by a `SoundSource`).

*Deviations / notes (not bugs):*
- **A minimal `vox` catalog device was pulled forward from Story 8.2.** Two rich `devices`-crate integration
  tests (phantom through the 8i6) depended on the mic's now-removed self-tone; `build_patch` handles acoustic
  connections for free, so feeding a `vox` at exactly 1 Pa into the capsule keeps their 10 mV hand-calcs
  verbatim. **Only the device's *existence*** (with engine-native Pa/Hz params) moved earlier — its
  skeuomorphic **faceplate, the acoustic-cable rendering, and the dB-SPL loudness knob stay in 8.2.** Likewise
  `Connector::Acoustic` / `PortKind::Acoustic` are **declarations only** (styling is 8.2).
- **Precision boundary:** a whisper's ~6 µV differential is below `f32`'s resolution when it rides the
  condenser's ~37.86 V phantom pedestal (ULP ≈ 4 µV), so the small-signal SPL-range oracle is asserted on the
  **pedestal-free dynamic mic** (exact), and the condenser's transduction magnitude is asserted at the 94 dB
  anchor with a loose tolerance. This is the same f32-cancellation reality the pre-8.1 pedestal tests already
  noted, not a new limitation.
- **Web untouched:** the Rust gate is green, but with acoustic ports + the `vox` device now in the catalog the
  browser harness won't render them correctly until Story 8.2 (the `pnpm` checks aren't part of the cargo gate).

### Story 8.2 — Micing in the studio (web + scenarios) — ✅ **Done**

*Goal:* surface the acoustic carrier through the product layer so you can **place a source, mic it, and hear the
front-end come alive** — drag a vox and a mic into the studio, wire the capsule with an acoustic cable, and watch
loudness (whisper → shout → cranked amp) drive the gain-staging story (`PROJECT_PLAN.md` §4, §7). Almost all of
the *engine/devices* surface this story once owned was **pulled forward by Story 8.1** (the `vox` device, the mics'
acoustic capsule ports + descriptors, `Connector::Acoustic` / `PortKind::Acoustic`, `build_patch`'s acoustic-edge
expansion, and the free acoustic↔analog `DomainMismatch`), so 8.2 is now essentially a **`web/` story** — the
client-side mirror of the new carrier — plus the numeric validation and a small connector test.

*Watch out:*
- **The web mirrors legality, it doesn't own it** (Epic 4 decision): `build_patch` is authoritative. Keep the
  `catalog.ts` unions and `connectorsCompatible` a faithful mirror of the Rust — a drift shows as the UI allowing a
  patch the engine then rejects (or vice-versa).
- **`format.ts` couples display to taper today** (`taper === "log"` ⇒ `20·log10(value)` dB) — correct only because
  every current log param is a voltage-gain. The vox breaks that: Loudness is dB **SPL** over *pressure*, Pitch is a
  log-travel **Hz** knob. Decouple **display (by unit) from travel (by taper)** or both vox knobs read wrong.
- **dB SPL is a display, never storage** — the engine param stays linear `Pascals`; the web converts `Pa ↔ dB SPL`
  (ref 20 µPa, `1 Pa = 94 dB SPL`) for the knob only. Don't push dB SPL back into the scene/param value.
- **Air stays ideal** — the acoustic cable is a *rendering* distinction with no pickable properties (no R·C, no
  length); distance/attenuation/polar remain deferred. Don't add it to the R·C cable catalog.
- Run `pnpm run format` + `pnpm run check` + `pnpm run typecheck` after web edits (the cargo gate doesn't cover `web/`).

*Design notes (settled at planning):*
- **Acoustic cable = rendering-only, no options.** A distinct **dashed** acoustic↔acoustic link, reusing the
  existing patch machinery; the engine only ever sees the unity `AcousticRoute` edge. *Rejected:* a
  pickable cable-catalog entry (like the R·C presets) — it would imply parameters (length/attenuation) the Epic
  explicitly defers, over-building for ideal air.
- **Loudness knob shows dB SPL, ~0–130.** The web maps the vox's linear `Pascals` param to a dB-SPL readout
  (`20·log10(Pa / 20e-6)`, silence = −∞) and back; the existing **log taper's travel is reused as-is** (it already
  maps "dB below max, snapping to silence at the bottom", which is the right feel — the engine ceiling is 100 Pa ≈
  134 dB SPL). *Rejected:* a raw-Pascals knob (not how loudness is discussed; poor feel).
- **No bespoke faceplates.** The generic descriptor-driven `Panel` already renders the vox (Loudness + Pitch knobs,
  acoustic output jack) and the mics (acoustic capsule input jack + XLR output) — so the story needs **jack/cable
  styling + the dB-SPL knob fix**, not new Svelte faceplate components. *(A skeuomorphic bespoke `Vox`/mic faceplate
  is optional future polish, deferred — this is a deviation from the coarse sketch's "vox faceplate" wording,
  justified: the fallback Panel + shared widgets already cover it, per the Epic-5 component-over-coordinate rule.)*
- **Validation = a durable Rust scene test + manual by-eye** (no new web test infra, per the project's deferred-e2e
  stance): the numbers are asserted Rust-side (tests are the oracle for the analog domain); the browser check is a
  human confirmation the UI patches and renders.

*Tasks:*
- **Task 8.2.1 — Web carrier types + legality mirror.** Add `"acoustic"` to `PortDomain`, `PortKind`, and
  `Connector` in `web/src/catalog.ts`; confirm `connectorsCompatible` (equality already covers it) and the
  `connections.ts`/`patching.ts` legality (`domain` match + `wouldCreateCycle`, both carrier-agnostic) accept a
  vox→mic-capsule patch and reject acoustic↔analog. Add the missing acoustic case to the Rust
  `connectors_compatible_matrix` test. *Validate:* client-side, a vox→capsule patch reads legal and an
  acoustic↔line patch reads illegal (mirroring `build_patch`); `pnpm run typecheck` green; the Rust connector test
  passes.
- **Task 8.2.2 — Acoustic jack & cable styling.** A signal-palette token (`--ae-signal-acoustic`) + `Jack.svelte`
  styling for `data-kind="acoustic"` / `data-domain="acoustic"`; a **dashed** acoustic cable in `cable-view.ts`; a
  `CableInspector.svelte` acoustic branch mirroring the digital "ideal (no cable)" line. *Validate (by-eye):* the
  vox's Air out and the mics' Capsule in render as distinct acoustic jacks, and the acoustic cable draws dashed with
  an "ideal — no cable" inspector.
- **Task 8.2.3 — dB-SPL loudness knob.** Decouple `format.ts` display from taper — format **by unit** (`"dB"` ⇒
  gain dB as today; new `"dB SPL"` ⇒ `20·log10(value/20e-6)`, `−∞` at 0; everything else ⇒ number + unit, so a
  log-taper Hz knob reads Hz); set the vox Loudness param's unit to `"dB SPL"` in the catalog. *Validate:* the vox
  Loudness knob reads **94 dB SPL at 1 Pa** and Pitch reads Hz; a `format.ts` Vitest unit test covers the dB/dB-SPL/
  unit branches (cheap, pure logic — no browser).
- **Task 8.2.4 — The micing scenario (the exit gate).** A Rust harness/devices scene-level integration test:
  `vox → mic → 8i6 preamp → AD`, asserting the gain-staging by hand calc — a whisper (30 dB SPL) sits near the
  preamp noise floor, a cranked amp (120 dB SPL) needs the −10 dB PAD to avoid clipping, and a dynamic mic reads
  ~14 dB below a condenser on the same source at the same gain. Plus a manual in-browser pass building the chain.
  *Validate:* the exit criteria below, green.

*Validate (Story 8.2):* ✅ **met** — in the browser you can drag a vox and a condenser (or dynamic) into the studio, wire the
capsule with an acoustic cable, and drive the loudness knob (in dB SPL) to watch the front-end gain-staging emerge
— whisper near the noise floor, cranked amp needing the PAD, dynamic needing more gain than a condenser; the
supporting numbers are pinned by the Rust scene test; the full cargo gate **and** `pnpm run check`/`typecheck`/`format`
are green. Deferred (record, don't build): distance/inverse-square attenuation, propagation delay, polar/off-axis
response, proximity/placement-based pickup, and bespoke skeuomorphic vox/mic faceplates.

*Delivered:* the acoustic carrier is now surfaced through the product layer — you can place a vox, mic it, and hear
the front-end gain-staging come alive. Since Story 8.1 pulled the engine/devices surface forward, this was
essentially a `web/` story plus the numeric exit gate.
- **Task 8.2.1** — web carrier types + legality mirror (`web/src/catalog.ts`): `"acoustic"` added to `PortDomain`,
  `PortKind`, and `Connector`. `connectorsCompatible` (equality) mates acoustic only with itself; the
  `connections.ts`/`patching.ts` legality is carrier-agnostic, so it accepts a vox→capsule patch and rejects
  acoustic↔analog for free. Mirrored Rust-side by an acoustic arm in the `connectors_compatible_matrix` test, and
  covered web-side by two `connections.test.ts` cases (legal acoustic edge, illegal acoustic→analog) plus a
  `cableAllowed("acoustic") === false` assertion (air carries no R·C cable).
- **Task 8.2.2** — acoustic jack & cable styling: a `--ae-signal-acoustic` palette token (a light sky-cyan, kept
  clear of mic-blue and digital-teal) in `tokens.css`; `Jack.svelte` rings the acoustic connector with it; a
  **dashed** acoustic cable in `Cable.svelte` (all three strokes dashed together so the shadow doesn't fill the
  gaps — it reads as intangible ideal air); a `CableInspector.svelte` acoustic branch showing "acoustic link —
  ideal air (no cable)", mirroring the digital ideal-link line.
- **Task 8.2.3** — dB-SPL loudness knob: `format.ts` display is now keyed on **unit**, not taper — `"dB"` ⇒ gain
  `20·log₁₀` dB (as before), the new `"dB SPL"` ⇒ `20·log₁₀(p / 20µPa)` (−∞ at 0), everything else ⇒ number + unit
  (so a log-taper Hz knob reads Hz). The vox Loudness param's unit became `"dB SPL"` in the catalog (linear Pa
  stays the stored value; the log taper's dB-below-max travel is reused as-is). A `format.test.ts` Vitest covers
  the switch/dB/dB-SPL/unit branches. (Same commit also shrank the vox from a 1.7 m person-sized tower to a
  compact ~200 mm desktop box, on the scale of a mic — the old size dwarfed the gear and was awkward to patch.)
- **Task 8.2.4** — the micing scenario, the exit gate: `micing_scenario_gain_staging` in `build.rs`, a
  `vox → mic → 8i6 preamp` scene test reading the post-preamp **In 1 Peak** meter (dBu) and asserting the
  gain-staging by hand calc. A whisper (30 dB SPL) is buried at the −60 floor — below the AD LSB and the
  condenser's f32 phantom-pedestal cancellation floor (you need a sensitive mic + lots of gain). At 94 dB SPL /
  default gain, the condenser reads −29.89 dBu and the dynamic −43.91 dBu — the **~14 dB gap emergent** from
  transducer efficiency (5× sensitivity) plus the deeper dynamic loading divider. A cranked amp (120 dB SPL) at
  ×100 gain pins the 10 V rail (+22.22 dBu, ~5.9 dB clipped) without the PAD and lands clean at +18.11 dBu with
  it. Nothing special-cased — it all falls out of `v = pressure × sensitivity` + the existing dividers/rail.

*Deviations / notes (not bugs):*
- **No new web faceplates or e2e test infra**, as planned — the generic descriptor-driven `Panel` + shared widgets
  already render the vox and mics, so the story was jack/cable styling + the dB-SPL fix, not new Svelte components.
  The gain-staging numbers are pinned by the durable Rust scene test (tests are the oracle for the analog domain);
  the browser is a manual by-eye confirmation.
- **The whisper case asserts the buried −60 floor, not a precise recovered magnitude** — at high gain the
  condenser's amplified phantom-pedestal residual dominates a signal that small (the documented Story-8.1 f32
  boundary); the physical gain-staging lesson is carried by the 94 dB and 120 dB cases.

### Story 8.3 — Speakers as acoustic sources & acoustic feedback — 🚧 **In progress**

*Goal:* close the loop — let a speaker **radiate sound pressure you can mic**, the output-side mirror of the
microphone, so **acoustic feedback (howlround / Larsen) emerges from the physics** the moment you patch a
`mic → preamp → monitor → air → mic` loop and push the gain up. A new voltage→acoustic **converter** node (a
powered monitor) transduces `pressure = drive_volts × sensitivity (Pa/V)`, **distinct** from the voltage-capture
`Speaker` terminus (which stays in volts, untouched — §5.5's output tap is unchanged; see the Epic-8 scope bullet
above). The enabler is that the acoustic domain, like digital, can **carry a one-block delay to break a graph
cycle** — and because a small delay in the air hop is *physically true*, that is exactly where the loop's latency
belongs. The "ring out the monitors" / gain-before-feedback lesson is then not special-cased: it falls out of
`v = pressure × sensitivity` + the delayed air edge. Anchored to `PROJECT_PLAN.md` §4 (a device as a *source you
mic*) and the §5.5 scope extension settled at planning (device-to-device radiation, still no air→ear).

*Watch out:*
- **Feedback must *emerge*, never be scaffolded.** No feedback-detector, no gain-limiter, no special demo device —
  howlround is the geometric consequence of loop gain ≥ 1 through the delayed loop. The realistic self-limit is
  the **rail clip** (the loop stops growing when the preamp/monitor pins its rail) — that too is emergent, not
  coded.
- **The latency lands on the *air* edge, always.** The existing auto-break (§5.9, `schedule.rs` step 5c) cuts the
  *lowest-index* breakable edge, gated to `Domain::DigitalAudio`. A realistic loop through the 8i6 crosses the
  interface's **AD→matrix→DA** (digital edges) *and* the air edge, so lowest-index could land the delay on a
  digital edge. Change the policy to **prefer an acoustic edge** among the breakable edges on a cycle (air carries
  the physical delay), falling back to digital — so the delay is physically honest regardless of what else is in
  the loop. This is a touch more than the coarse sketch's "one-condition change," and deliberately so.
- **Hot-path contract holds** (`CLAUDE.md` §6): the converter's `process` is zero-alloc, panic-free,
  denormal-safe; the delayed-edge machinery is already alloc-free (the `delayed_steps` copy path is
  carrier-agnostic — it copies a pressure lane exactly as it copies a sample lane). `no_alloc` stays green.
- **Air stays ideal, still.** The air edge is the unity `AcousticRoute` from 8.1 — the one-block delay is a
  **cycle-break mechanism** (block-size-dependent), **not** a calibrated propagation time. Distance→delay-line and
  inverse-square attenuation stay deferred (they pair with 8.2's deferred distance work).
- **Don't touch the `Speaker` terminus.** The whole `da→spk` capture path depends on it staying a pure
  voltage→voltage tap. The radiator is a **new, separate** device/node.

*Design notes (settled at planning):*
- **A new "powered monitor" device, not an extension of `Speaker`.** One new catalog device: an analog **line
  input** (¼", line-level) → an **acoustic output** (`Connector::Acoustic`, mates only with a mic capsule) with a
  **Volume** knob (a log-taper attenuator from 0 dB, exactly like the 8i6's monitor amp). *Rejected:* adding an
  acoustic output to the existing `speaker` device (muddies the §5.5 voltage terminus and risks the capture path);
  *rejected:* a bespoke guitar-combo-amp or a separate power-amp + passive-cab pair (more surface than the lesson
  needs — the 8i6 preamp already supplies the loop gain, per the planning decision to keep new surface minimal).
- **The converter node mirrors the dynamic mic.** `inputs: [InputZ…]` (voltage in), `outputs: [AcousticFace…]`
  (mono pressure out), `process`: `pressure = drive_volts × sensitivity × volume`. Passive, no phantom — the exact
  reverse of `dynamic.rs`. It reads `inputs[0].voltage()`, writes `outputs[0].pressure_mut()`.
- **Sensitivity calibrated so the *feedback-resistance* lesson emerges.** Target **≈ 1 Pa/V** (pinned exactly by
  hand-calc in the tests against the 8i6 preamp's real gain range + dividers): loop gain
  `L = mic_sensitivity(V/Pa) × preamp_gain × dividers × monitor_sensitivity(Pa/V) × volume`. At ≈1 Pa/V a
  **condenser** (10 mV/Pa) crosses unity around **+40 dB** of preamp gain, while a **dynamic** (2 mV/Pa) needs
  ≈ +54 dB — the same ~14 dB sensitivity gap now showing up as *"the dynamic is harder to ring"* (why loud stages
  favor dynamics). Not special-cased — it's the 8.1 sensitivity gap seen through the loop.
- **Prefer-acoustic cycle-break** (see Watch out): the delay always lands on the air hop. A pure-**analog** cycle
  (no digital *and* no acoustic edge) still correctly rejects as `CompileError::Cycle` — analog feedback has
  nowhere to carry latency. The `Cycle` doc comments (`schedule.rs` ~138, ~209) get updated from "no digital edge"
  to "no digital-or-acoustic edge."
- **Validation = engine hand-calc tests + a devices scenario + manual by-eye** (house style, deferred-e2e): the
  *mechanism* is proved at the engine level with raw nodes (exact gains, the air edge unambiguously the delayed
  leg); the *product loop* is proved through real catalog devices via `build_patch`; the browser is a human
  confirmation the loop patches and rings. No new web test infra.

*Tasks:*
- **Task 8.3.1 — The powered-monitor converter node (engine).** A new `node/…` (e.g. `PoweredMonitor`): analog
  voltage input + acoustic output, `pressure = drive_volts × sensitivity × volume`, a `Volume` param (log taper,
  attenuator from 0 dB). Zero-alloc / panic-free / denormal-safe; `no_alloc` green. *Validate (hand calc):* 1 V
  drive × sensitivity × unity volume = the calibrated peak pressure (asserted in Pa **and** its dB SPL); volume
  −6 dB halves the pressure; a `declares_faces` test confirms `inputs()[0].domain() == Analog` and
  `outputs()[0].domain() == Acoustic` (`lane_count() == 1`, mono).
- **Task 8.3.2 — Acoustic edges delay-eligible; the air edge carries the latency (engine cycle-break).** Extend
  `schedule.rs` step 5c's breakable filter to `DigitalAudio || Acoustic`, and **prefer an acoustic edge** when a
  cycle has both; update the `CompileError::Cycle` doc comments. *Validate:* a raw-node
  `DynamicMic → GainStage → PoweredMonitor → [acoustic edge] → DynamicMic` loop compiles (no `Cycle`) with
  **exactly one delayed edge = the acoustic one**; with **sub-unity** loop gain a step reaches the geometric
  steady state `1/(1−L)` and a transient re-enters **one block later** scaled by `L` (hand-calc); with
  **super-unity** gain successive block peaks **grow** (ratio `L`) toward the rail, then self-limit at the clip;
  a **mixed** digital+acoustic cycle delays the acoustic edge; a **pure-analog** cycle still rejects as `Cycle`.
- **Task 8.3.3 — The `powered_monitor` catalog device + web (devices).** A `CatalogEntry` (line input ¼" +
  acoustic output `Connector::Acoustic`, a Volume param) building the 8.3.1 node. Confirm `build_patch` assembles
  a `mic → 8i6 → powered_monitor → [acoustic] → mic` loop, **compiles auto-broken with the delay on the air
  edge** (proving prefer-acoustic end-to-end, even across the 8i6's digital edges). Web renders via the generic
  descriptor-driven `Panel` + 8.2's acoustic jack/cable styling (no new Svelte). *Validate (by-eye):* the monitor
  renders with a line jack + a dashed acoustic out; the loop patches; routing the 8i6 for direct monitoring and
  raising the gain makes the input meter climb toward feedback. `pnpm run check`/`typecheck`/`format` green.
- **Task 8.3.4 — The feedback scenario (the exit gate).** A harness/devices scene-level integration test pinning
  the product-chain numbers by hand calc: the loop gain `L` for a condenser vs. a dynamic through the 8i6 preamp
  + the monitor; assert **below unity** the loop is bounded (settles), **above unity** the block peaks grow toward
  the rail and self-limit (howlround, emergent), and the **condenser rings at a lower gain than the dynamic**
  (the ~14 dB feedback-resistance gap). Plus a manual in-browser pass building and ringing the loop. *Validate:*
  the exit criteria below, green.

*Validate (Story 8.3):* a `mic → preamp → monitor → air → mic` patch **compiles** (auto-broken, the one block of
latency sitting on the **air** edge); a below-unity loop settles to a bounded, one-block-trailing steady state;
pushing the gain past unity makes the level **build toward feedback** and self-limit at the rail — with the
condenser ringing at a lower gain than the dynamic — all pinned by hand-calc Rust tests; an all-analog cycle with
**no** delay-eligible edge still rejects as `Cycle`; the full cargo gate **and** `pnpm run
check`/`typecheck`/`format` are green; and in the browser you can build the loop and hear/see it ring. *Deferred
(record, don't build):* a **distance→delay-line** + inverse-square attenuation (the one-block delay is a
cycle-break mechanism, not calibrated propagation — pairs with 8.2's deferred distance work); speaker
frequency-response / cabinet coloration (flat for now); and **air→ear / room acoustics** (out of scope — the loop
is device-to-device).
