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

## Epic 2 — Offline Render ("hear it" cheaply)

**Progress:** Stories 2.1 ✅ and 2.2 ✅ done; **2.3 deferred** (see below). 2.1 — **first audible render**:
a played note runs through `synth → (AD → DA →) speaker`, the speaker voltage is captured off-sim-clock to
48 kHz, and written to a float32 WAV. 2.2 — **first real DSP**: a `Biquad` primitive + RBJ designers, a
`ThreeBandEq` and a feed-forward `Compressor` (both pure-digital, between the modeled AD and DA), and
harness scenarios that render the note through each. The epic's behavior is validated by **numeric oracles**
(unit tests + the harness integration tests in `tests/render.rs`) and **by ear** via the render scenarios;
the **golden-file regression harness is deferred** until drift/quality issues actually appear (2.3).

**Goal:** reach the audio oracle without real-time infrastructure — the *same* engine (driven block by
block via `Schedule::process_io`) rendered flat-out into a WAV. First real DSP and a trivial speaker so
there's something meaningful to hear.

**Exit criteria:** build a chain, render it, and the result sounds correct; DSP and converter behavior
validated by listening **and** numeric-oracle tests. *(The originally-planned golden-file regression layer
is deferred to 2.3 — added if/when drift surfaces; the numeric oracles are the standing guard.)*

**Epic-wide watch-outs:** this is a **test harness, not a second engine** — the render driver is a loop
over `process_io` plus a file writer, nothing more. Determinism (seeded) is what makes golden-file tests
viable; pin *every* run parameter (seed, `block_len`, rate, patch) for a golden render. Keep it thin.
**Mono only** this epic (converters/lanes are mono; multichannel digital is deferred to Epic 5) — render
a mono WAV. The harness is native-only, so its deps (`textplots` today, a WAV writer next) never reach
the engine or its wasm32 build.

**Settled this planning pass (decisions that shape the stories):**
- **The simulation ends in the analog domain at the speaker feed (volts); we do *not* simulate
  acoustics** (no air→ear, no "ear-as-microphone" node — PROJECT_PLAN §5.5's "*or nothing*"). The graph
  terminates at a thin **`Speaker`** voltage→voltage device (sensitivity + an optional simple response
  curve). The engine **output tap stays a voltage tap** — no Sample-lane tap is needed for output.
- **The host render is an *implicit capture*, outside the simulation.** The harness taps the speaker's
  analog voltage and resamples it to the host rate to produce WAV/real-time samples. This capture is
  **pure plumbing**: it carries **no `ClockDomainId`**, is on **no modeled-converter clock or sample
  rate**, and has no dBFS calibration role. It **reuses the FIR `Decimator`** so it stays transparent and
  adds no artifacts of its own (aliasing/quantization must come only from the *modeled* AD/DA under test).
  It maps volts→full-scale through a **fixed monitor reference** (deterministic, level-faithful), and for
  Epic 2 the **host rate integer-divides the analog rate** (e.g. 48 k from 384 k = ÷8); arbitrary host
  rates (44.1 k vs a 384 k clock ⇒ fractional resample) are deferred. *(This is the §5.1 "internal AD"
  role, minus the acoustic stage and minus node status — it lives in the harness.)*
- **First DSP lives in the digital domain** — biquad EQ and compressor operate on `SampleBuffer`, sitting
  between the modeled AD and DA (the "plugins/DAW" position). Avoids the ~8× oversample cost and exercises
  the digital lane. Analog (voltage-domain) outboard DSP is a later option, not Epic 2.
- **Golden-file comparison is tolerance + spectral-feature based** (per-sample max-abs-error epsilon plus
  RMS/THD/spectral checks), not bit-exact — robust across platforms (FMA contraction and libm `exp`/`sin`
  in coeff design are not bit-portable native↔wasm↔arch) and across harmless refactors. Pin the reference
  target in docs; provide a `--bless` regeneration path. Stories 2.1–2.2 validate with **numeric oracles**
  (reuse Epic 1's DFT/RMS/THD `test_util`); the golden harness is built in 2.3 once there's a lot to lock
  down.

> *Tasks for each Story below are fleshed out (to Task level + any remaining design notes) when we pick
> the Story up to build it — per the detail-gradient convention. The Goals, watch-outs, and settled
> decisions are recorded now.*

### Story 2.1 — Offline render to WAV + speaker terminus *(first sound)* — ✅ **Done**
*Goal:* the smallest thing that produces **a WAV you can listen to** — the audio-oracle-unlocked
milestone (the Epic-2 analogue of Story 1.3's "first runnable"). The render driver loops `process_io`
into a WAV writer; the graph gains a thin `Speaker` terminus; the harness performs the implicit capture
(transparent decimation, fixed monitor reference) to host samples. Validate by **ear** plus numeric
oracles (render the played-note patch, assert onset + fundamental).
*Watch out:* don't build a second engine (loop the existing `process_io`); keep the speaker trivial and
in volts (it produces voltage, not SPL); the implicit capture is harness-side and off-sim-clock.

*Design notes (settled at planning):*
- **The implicit capture is a harness-held `Decimator`.** `Decimator::lowpass(num_taps, M, beta)` already
  gives a transparent polyphase anti-alias decimator with unity passband; the capture is one instance at
  `M = analog_rate / host_rate`, fed the speaker-voltage block each call, **held stateful for the whole
  render** (its ring buffer carries across blocks — re-creating it per block would inject transients). No
  new DSP. It is *not* a graph node, carries no `ClockDomainId`, and is on no modeled-converter clock.
- **Volts → full scale via a fixed monitor reference**, then clamp to ±1.0. No per-render
  auto-normalization (it would break determinism and cross-render level comparison). Scaling is linear, so
  apply it after decimation.
- **Canonical render format is float32 WAV.** A PCM16 writer would add its *own* quantization noise — which
  would contaminate Story 2.3's measurement of quantization noise from the *modeled* AD. PCM16 stays an
  optional listening convenience only. WAV I/O via **`hound`** (harness-only dep; native, never reaches the
  engine/wasm build; also gives 2.3 its golden read-back).
- **The output tap is the Speaker's voltage output** — `process_io`'s `out` buffer *is* the speaker block,
  fed straight to the capture. No Sample-lane tap. The Speaker's output port is a benign terminus fiction
  (ideal `OutputZ`, nothing loads it) standing in for "what we hear."
- **The Speaker is flat (sensitivity gain only) this story** — a recognizable terminus device, voltage→
  voltage, no rail. A frequency-response curve is cosmetic and deferred (trivially added later via `OnePole`
  or the 2.2 biquad).
- **No PowerAmp node** (device breadth is Epic 5); the Speaker's sensitivity covers level, and `GainStage`
  stands in if a patch wants an explicit gain stage.
- **`block_len` must be a multiple of the capture's `M`** (mirrors the modeled-AD constraint), e.g.
  384-sample analog blocks with `M = 8` → 48 host samples per block at 48 k.
- **Latency is real:** the capture FIR — plus the modeled AD/DA in the patch — add fixed group delay, so
  the onset oracle offsets by the known compile-time latency (the Epic-1 capstone already does this for the
  converters; the capture stacks on top).
- **Numeric oracles live harness-local.** Engine's DFT/RMS helpers are `#[cfg(test)]` and unreachable
  cross-crate; reimplement a tiny single-bin DFT + RMS in the harness (same shape as Epic-1's
  `tone_amplitude`) rather than widening the engine's public API.
- **Artifacts:** rendered WAVs go to a **gitignored `renders/`**; the CLI stays scenario-function style (no
  arg-parser crate). Committed golden refs get their own dir in Story 2.3.

- ✅ **Task 2.1.1** — `Speaker` terminus node (engine): voltage→voltage sensitivity gain, 1 analog in / 1 analog out, ideal `OutputZ`, no rail. Unit test: passband gain = sensitivity.
- ✅ **Task 2.1.2** — Implicit capture (harness): a `Decimator::lowpass` at `M = analog/host` (transparent spec) + fixed monitor-reference volts→full-scale + clamp, held stateful across blocks. Test: a known sine in volts → expected normalized amplitude at the host rate.
- ✅ **Task 2.1.3** — WAV writer (harness, `hound`): mono, canonical float32. Header/round-trip test.
- ✅ **Task 2.1.4** — Render driver + first-sound scenario: compile a fixed patch (synth note → modeled AD → modeled DA → `Speaker`), loop `process_io` for N seconds into the capture → WAV; deterministic (seed/block_len/rate pinned); output to `renders/`.
- ✅ **Task 2.1.5** — Numeric-oracle validation test (harness integration): render the played-note patch, assert correct fundamental (DFT bin), onset after the known total latency, and non-silence/level.

*Validate (✅ met):* a fixed patch renders to a float32 WAV that **plays and sounds right by ear**, and a
harness test asserts the rendered fundamental, latency-offset onset, and level against hand calcs —
deterministic across runs. **First sound achieved.**

*Delivered:* the first audible render. **Engine:** `Speaker` — a flat voltage→voltage terminus node
(sensitivity gain, bridging `InputZ`, nominal terminus `OutputZ`, no rail). **Harness** restructured as
**lib + bin** so the integration tests and `main` share code: `capture::Capture` (a stateful harness-held
`Decimator` at `M = analog/host` + a fixed monitor-reference volts→±1.0 + clamp — transparent,
off-sim-clock, no `ClockDomainId`); `wav` (mono **float32** WAV via `hound`, file write + in-memory
round-trip); `render::render_to_samples` (loops `process_with_events`, feeds the tapped speaker voltage
through the capture, returns exactly `round(host_rate·seconds)` samples). Two `main` scenarios render A4 to
`renders/*.wav` — the full chain `synth → AD → DA → speaker` and a pure-analog `synth → speaker`
comparison (no quantization / converter delay). Three integration tests (on the analog-only patch, whose
pre-onset is true silence) assert the 440 Hz fundamental (≈ 0.45 = 2·0.7/π, dominating its harmonics),
causal onset (exact silence before the note), and bit-identical determinism. `hound` + `approx` are
harness-only deps (never reach the engine/wasm build). 243 tests green (engine's 232 unchanged).

*Absorbs old 2.1.1 + 2.3.1.*

### Story 2.2 — First DSP devices: 3-band EQ + compressor (digital domain) — ✅ **Done**
*Goal:* the first real DSP, validated by ear and numeric oracles — a **3-band EQ** and a **feed-forward
compressor**, both pure-digital nodes operating on `SampleBuffer` **between the modeled AD and DA** (the
"plugins/DAW" position). This exercises the digital lane and avoids the ~8× oversample cost of
voltage-domain DSP. *Watch out:* keep transforms understandable per §5.5 — feed-forward compressor, no
lookahead; the realism budget stays on the volts-and-converters layer. The compressor is the meatiest
single device in the epic. *Absorbs old 2.2.1 + 2.2.2.*

*Design notes (settled at planning):*
- **No new scheduling/compile work.** A pure-digital node declares `DigitalFace` in/out ports at its
  `SampleRate`; `compile` already sizes its lanes at `block_len / M`, validates the integer-divide +
  block-length constraints, and routes the `DigitalRoute` edges (same-clock-domain sample copies). It reads
  `inputs[0].sample()` and writes `outputs[0].sample_mut()` — the `DaConverter` read pattern. The story is
  **two nodes + one DSP primitive**, nothing in the graph/schedule.
- **New `dsp` module** (`dsp.rs` + `dsp/biquad.rs`), peer to `electrical` / `fir`, for digital DSP
  primitives. The module-private `flush_denormal` in `electrical/cable.rs` is **promoted** to a shared spot
  reachable by both analog and digital filters (it's currently re-implemented in `fir.rs` too).
- **`Biquad` primitive** — Transposed Direct Form II, `f64` coeffs + state, `step` / `process` zero-alloc
  and denormal-flushed (the [`OnePole`] *shape*, in the digital domain). RBJ-cookbook coefficient
  **designers** (peaking, low-shelf, high-shelf) take `(SampleRate, freq, Q/slope, gain_db)`. Note: unlike
  `OnePole` (an *edge* filter `compile` builds directly), the biquad is **node-owned** and bakes its coeffs
  in `prepare`.
- **Design coeffs from the node's own `SampleRate`, not `prepare`'s argument.** `Node::prepare(rate)` is
  handed the `AnalogRate` (the ~384 kHz oversample clock), which is **irrelevant to a pure-digital filter**.
  Both nodes store their `SampleRate` at construction (like AD/DA) and design against it; the `prepare`
  argument is unused (documented). The plan's earlier "coeffs designed at `prepare(rate)`" meant *the
  digital rate*, not this argument.
- **3-band EQ** — LF **low-shelf** + parametric **mid peak** + HF **high-shelf**, three biquads in series,
  single digital channel in/out. **Static** config: each band's freq/Q/gain set at construction, coeffs
  designed once at `prepare`. **No smoothed control params this epic** — safely smoothing biquad
  coefficients is a real problem and live knob-turning belongs to Epic 3 (real-time). Golden tests pin the
  config anyway.
- **Compressor** — feed-forward, **no lookahead** (lookahead = a delay buffer + added latency, deferred).
  Pipeline: **peak detector** (rectify → one-pole envelope with *switched* attack/release coefficients,
  baked at `prepare` — the `OnePole` recurrence with two coefficients) → **dB-domain gain computer**
  (threshold, ratio, soft-knee width; hard knee when width = 0) → **manual makeup gain**. The dB domain is
  the one spot that pays a `log10`/`pow` per envelope step — accepted, kept off the per-sample-where-possible
  path. Static config.
- **Mono only** (epic constraint) — single channel, no stereo-linked detection.
- **Validation:** engine `#[cfg(test)]` unit tests assert hand calcs (reusing `tone_amplitude` / `rms` from
  `test_util`); harness render scenarios are the ear (harness reuses its own DFT/RMS, per Story 2.1).

- ✅ **Task 2.2.1** — `dsp` module + `Biquad` primitive (TDF-II, `f64`, denormal-flushed, zero-alloc
  `process`) + RBJ designers (peaking / low-shelf / high-shelf); promote `flush_denormal` to a shared spot.
  Tests: a **0 dB** band is unity at every frequency; a **+6 dB peaking** band reads ≈ 2.0 (linear) at its
  center freq and ≈ unity a decade away; shelf asymptotes hit the design gain at DC / Nyquist. (Magnitude
  via `measure_gain`-style single-bin probe at the digital rate.)
- ✅ **Task 2.2.2** — `ThreeBandEq` node: three biquads in series, digital in/out, designed at `prepare` from
  `self.rate`. Tests: an all-0-dB EQ is transparent (unity, all bands); a +6 dB LF shelf boosts a low tone
  while leaving a high tone ≈ unchanged; the mid peak bumps a tone at its center.
- ✅ **Task 2.2.3** — `Compressor` node: peak envelope follower (attack/release coeffs `a = 1 − e^(−1/(τ·fs))`)
  → dB gain computer (threshold / ratio / soft knee) → manual makeup. Tests: **static curve** — below
  threshold is unity × makeup; above, a hand-calc'd point holds (e.g. ratio 4:1, threshold −10 dBFS, −2 dBFS
  in ⇒ −8 dBFS out, i.e. −6 dB gain reduction); **attack timing** — a step input drives the envelope to
  ≈ 63% (1 − 1/e) in ≈ τ samples; release symmetric on signal removal.
- ✅ **Task 2.2.4** — Harness render scenarios: insert the EQ and the compressor between the modeled AD and DA
  on the played-note patch; render to `renders/*.wav`. Validate by **ear** plus a numeric check (compressor
  reduces peak/RMS by the expected amount; EQ shifts spectral balance the expected way).

*Validate (✅ met):* the full gate (`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo
docs`) is green; the EQ and the compressor each carry hand-calc unit oracles; rendered WAVs demonstrate each
by ear; the run stays deterministic (seed / block_len / rate pinned — converter dither is seeded too). Hot
path stays zero-alloc.

*Delivered:* the first real DSP, in a new `dsp` module peer to `electrical` / `fir`. **Primitives:**
`Biquad` (Transposed Direct Form II, `f64` coeffs + state, zero-alloc denormal-flushed `process`) with RBJ
designers `peaking` / `low_shelf` / `high_shelf`; `PeakEnvelope`, a rectify-then-switched-coefficient
attack/release follower (**extracted as its own primitive** rather than buried in the compressor — reusable
and independently timing-tested; a small deviation from the planned `dsp.rs + dsp/biquad.rs` shape, now
`dsp.rs + dsp/biquad.rs + dsp/envelope.rs`). `flush_denormal` promoted from `electrical/cable.rs` into
`dsp` and shared (both exported at the crate root with `Biquad`). **Nodes:** `ThreeBandEq` (LF shelf + mid
peak + HF shelf, three biquads in series, static config baked at `prepare` from its own `SampleRate`) and
`Compressor` (feed-forward, no lookahead; `PeakEnvelope` → dB gain computer with threshold / ratio / soft
knee → manual makeup; builder `with_knee` / `with_makeup`). Both pure-digital, one channel in/out, between
the modeled AD and DA — **no graph/schedule changes** (the Story 1.6 digital ports/edges carried them).
**Harness:** two listening scenarios (`synth → AD → EQ → DA → speaker` and `… → compressor → …`) writing
`renders/first_sound_eq.wav` / `first_sound_compressed.wav` (voice level halved for boost/makeup headroom),
plus two tolerance-based render-oracle tests (a −12 dB low shelf cuts the 440 Hz fundamental to < 60 %; 8:1
compression below threshold drops the sustain peak to < 60 %) via a shared `render_through` helper. **254
engine tests** (+22: 6 biquad, 5 envelope, 4 EQ, 7 compressor) and **5 render integration tests** (+2)
green.

### Story 2.3 — Golden-file harness + converter-payoff demos — ⏸️ **Deferred**
*Deferred (2026-06-23):* the standing **numeric oracles** (engine unit tests + the harness integration
tests in `tests/render.rs`) already pin the epic's behavior against hand calcs, and the render scenarios
cover the ear check. A golden-file *regression* layer only earns its keep once we're actually fighting
drift or quality regressions — so it's deferred until that need shows up, rather than built speculatively
now. The **converter-payoff demos** (aliasing, quantization) ride along with it and are deferred too; the
knobs they'd use already exist (`AdConverter::with_aa_taps`, `BitDepth`) and the naive-sawtooth voice has
the HF content aliasing needs, so picking this up later is cheap.

*If/when resumed, the design is settled (decided in the 2.3 assessment pass, superseding the epic-level
"per-sample epsilon + WAV blobs" note above):*
- **Feature-vector goldens, not waveform blobs.** Reduce each render to a small committed JSON of measured
  metrics (fundamental Hz + amplitude, broadband RMS, THD, peak, noise-floor; plus an alias-bin energy /
  quantization-noise figure for the payoff demos), compared per-metric with explicit tolerances. Rationale:
  dev is macOS-**ARM**, CI is Linux-**x86**, and coeff-design `sin`/`exp` + FMA contraction aren't
  bit-portable across them, so a per-sample epsilon would have to be too loose to guard well; physically
  meaningful metrics are portable, tiny in-repo, and survive harmless refactors.
- **`--bless` via a bin flag over a shared lib.** `cargo run -p harness -- --bless` regenerates all
  goldens; the reduce + (de)serialize logic lives in `harness::golden` so the bin and the read-and-compare
  tests share one path and can't drift (still no arg-parser crate — minimal `std::env::args()` in `main`).
  Goldens live in a committed dir (e.g. `crates/harness/tests/golden/*.json`), distinct from gitignored
  `renders/`.
- **Six renders locked down:** first_sound, first_sound_analog, eq, compressed, + the two payoff demos
  (aliasing_weak, quant_low). The textplots scenarios (1–6) aren't WAV renders and stay unguarded.
- **Payoff demos get numeric oracles, not just ear + golden:** aliasing asserts fold-bin energy appears
  with weak taps and is absent with strong; quantization asserts the broadband noise floor rises ~the
  expected per-bit amount vs a high-bit reference.
- **Spectral helper:** promote the harness's single-bin DFT (`bin_magnitude`/`rms`, today in
  `tests/render.rs`) into the lib and add a THD + broadband-noise-floor measure, shared by bless, the
  oracle tests, and the golden compare. The existing eq/compressor *semantic* oracles stay — golden is
  regression-on-top, not a replacement.
*Absorbs old 2.3.2 + 2.3.3.*

---

## Epic 3 — Real-Time Playback (the north star)

**Progress:** Story 3.1 ✅ done — the engine builds to WASM and the in-browser feasibility benchmark
clears the gate at **≈46× real-time** (in-worklet single-thread confirmed; the heaviest unknown in
PROJECT_PLAN §10 is retired). Next: **3.2 — first real-time sound** (Vite/TS harness + AudioWorklet).

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
  only ~3% (the chain is largely serial/recursive) — so explicit SIMD intrinsics are *not* pursued.**

> *Tasks below are a coarse sketch, fleshed out to Task level when each Story is picked up — per the
> detail-gradient convention (Epics 2–3 carry Tasks but expect churn). Goals, watch-outs, and the settled
> decisions are recorded now.*

### Story 3.1 — WASM engine + real-time feasibility spike — ✅ **Done**
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
