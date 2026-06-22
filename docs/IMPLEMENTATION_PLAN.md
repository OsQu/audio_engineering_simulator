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
  exercised single-threaded today. → **Epic 3** (real AudioWorklet thread).
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

**Progress:** Story 2.1 ✅ done — **first audible render**: a played note runs through `synth → (AD → DA →)
speaker`, the speaker voltage is captured off-sim-clock to 48 kHz, and written to a float32 WAV. Stories
2.2 (first DSP) and 2.3 (golden harness + converter-payoff demos) remain.

**Goal:** reach the audio oracle without real-time infrastructure — the *same* engine (driven block by
block via `Schedule::process_io`) rendered flat-out into a WAV. First real DSP and a trivial speaker so
there's something meaningful to hear.

**Exit criteria:** build a chain, render it, and the result sounds correct; DSP and converter behavior
validated by listening **and** golden-file tests.

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

### Story 2.2 — First DSP devices: 3-band EQ + compressor (digital domain)
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

- **Task 2.2.1** — `dsp` module + `Biquad` primitive (TDF-II, `f64`, denormal-flushed, zero-alloc
  `process`) + RBJ designers (peaking / low-shelf / high-shelf); promote `flush_denormal` to a shared spot.
  Tests: a **0 dB** band is unity at every frequency; a **+6 dB peaking** band reads ≈ 2.0 (linear) at its
  center freq and ≈ unity a decade away; shelf asymptotes hit the design gain at DC / Nyquist. (Magnitude
  via `measure_gain`-style single-bin probe at the digital rate.)
- **Task 2.2.2** — `ThreeBandEq` node: three biquads in series, digital in/out, designed at `prepare` from
  `self.rate`. Tests: an all-0-dB EQ is transparent (unity, all bands); a +6 dB LF shelf boosts a low tone
  while leaving a high tone ≈ unchanged; the mid peak bumps a tone at its center.
- **Task 2.2.3** — `Compressor` node: peak envelope follower (attack/release coeffs `a = 1 − e^(−1/(τ·fs))`)
  → dB gain computer (threshold / ratio / soft knee) → manual makeup. Tests: **static curve** — below
  threshold is unity × makeup; above, a hand-calc'd point holds (e.g. ratio 4:1, threshold −10 dBFS, −2 dBFS
  in ⇒ −8 dBFS out, i.e. −6 dB gain reduction); **attack timing** — a step input drives the envelope to
  ≈ 63% (1 − 1/e) in ≈ τ samples; release symmetric on signal removal.
- **Task 2.2.4** — Harness render scenarios: insert the EQ and the compressor between the modeled AD and DA
  on the played-note patch; render to `renders/*.wav`. Validate by **ear** plus a numeric check (compressor
  reduces peak/RMS by the expected amount; EQ shifts spectral balance the expected way).

*Validate:* `cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs` all green; the EQ
and the compressor each carry hand-calc unit oracles; a rendered WAV demonstrates each by ear; the run stays
deterministic (seed / block_len / rate pinned). Hot path stays zero-alloc (the `no_alloc` test covers the
new nodes once they're in a patch).

### Story 2.3 — Golden-file harness + converter-payoff demos
*Goal:* lock down the epic's renders and demonstrate the converter payoff by ear. Build the **golden-file
regression harness** (tolerance + spectral comparison, deterministic fixed patches, a `--bless`
regeneration path) and the **payoff demo renders**: aliasing via a weak AA filter and quantization noise
via low bit depth — reusing the Story-1.6 tap-count and bit-depth knobs on the *modeled* AD. *Watch out:*
artifacts must originate in the modeled converters, never the transparent implicit capture; golden refs
are blobs in-repo (size) blessed on the documented reference target. *Absorbs old 2.3.2 + 2.3.3.*

---

## Epic 3 — Real-Time Playback (the north star)

**Goal:** the engine live in the browser — turn knobs and play an instrument with low latency, glitch-free.
Engine-in-AudioWorklet (WASM), shallow render-ahead, lock-free param + event lanes across the thread boundary.

**Exit criteria:** a running patch is audible in real time, stable (no dropouts under normal use),
with knob changes and note playing responsive at low latency (~5–12 ms target).

**Watch-outs:** the hot-path contracts (zero-alloc, lock-free, panic-free, denormal flush) become
non-negotiable here. SharedArrayBuffer needs COOP/COEP headers on the serving origin. Measure latency,
don't assume it.

- **Task 3.1.1** — WASM build of the engine (`wasm32` + SIMD), size/perf sanity, minimal JS bindings.
- **Task 3.2.1** — AudioWorklet host: instantiate engine in the worklet, process per quantum, shallow render-ahead.
- **Task 3.2.2** — COOP/COEP serving setup + SharedArrayBuffer for the param/event lanes.
- **Task 3.3.1** — Live control: UI/keyboard → control-param queue across the thread boundary.
- **Task 3.3.2** — Live events: Web MIDI + computer keyboard → timestamped event queue (playing).
- **Task 3.4.1** — Glitch-free hardening: panic-free audit of the hot path, denormal flush, schedule hot-swap under load.
- **Task 3.4.2** — Latency measurement + tuning (render-ahead depth vs. responsiveness).

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
