# Epic 2 — Offline Render — Design Notes & Delivery Record

Companion archive to `IMPLEMENTATION_PLAN.md`. Epic 2 is **substantially complete** — Stories 2.1
and 2.2 are done (254 engine tests + 5 render integration tests green); Story 2.3 (the golden-file
regression harness + converter-payoff demos) is **deferred**. The plan keeps a tight summary of
Epic 2; **this file is the full record** — the *settled design notes* (with the rejected alternatives
and the reasoning that justified each choice), the per-task breakdown, and the per-task *Delivered*
notes.

Read this when a later epic's design decision turns on **why** Epic 2 was built the way it was, or
when you need the exact API/behavior of something Epic 2 shipped. For *what exists and what binds you
going forward*, the plan's Epic 2 summary is enough; come here for the depth behind it.

The plan's section ordering (Goal → Watch out → Design notes → Tasks → Validate → Delivered) is
preserved per story.

---

## Epic 2 framing (as originally written)

**Goal:** reach the audio oracle without real-time infrastructure — the *same* engine (driven block by
block via `Schedule::process_io`) rendered flat-out into a WAV. First real DSP and a trivial speaker so
there's something meaningful to hear.

**Exit criteria:** build a chain, render it, and the result sounds correct; DSP and converter behavior
validated by listening **and** numeric-oracle tests. *(The originally-planned golden-file regression
layer is deferred to 2.3 — added if/when drift surfaces; the numeric oracles are the standing guard.)*

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

---

## Story 2.1 — Offline render to WAV + speaker terminus *(first sound)*

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

---

## Story 2.2 — First DSP devices: 3-band EQ + compressor (digital domain)

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

---

## Story 2.3 — Golden-file harness + converter-payoff demos — ⏸️ **Deferred**

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
