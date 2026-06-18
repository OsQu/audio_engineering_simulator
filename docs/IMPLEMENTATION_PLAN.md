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

**Detail gradient (intentional):** Epic 1 is broken to Task level because we start there.
Epics 2–3 have Tasks but expect churn. Epics 4–5 stay at Story level — their Tasks get
written when we reach them. Don't over-plan work whose shape the earlier work will change.

**Branch convention:** one branch per **Story**, `e<epic>-s<story>/<short-story-slug>`,
e.g. `e1-s2/electrical-primitives`. Its Tasks are commits on that branch; PR (or
fast-forward) to `main` and delete on merge once the Story's *Validate* gate is green.

### Architecture decisions baked into this plan

These were settled in design discussion and constrain every Epic:

- **Engine in Rust**, native for dev/test, `wasm32` + SIMD for the browser. **UI in TypeScript.**
- **Two distinct signal types**, never conflated: `VoltageBuffer` (oversampled, volts, analog)
  vs `SampleBuffer` (base-rate, dBFS, digital). The AD/DA converters are the *only* bridge.
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

## Epic 1 — Headless Voltage Engine

**Goal:** the novel, risky core, built and validated headless. A graph of devices and cables
propagating oversampled voltage in the analog domain, crossing the AD/DA boundary into and back
out of digital, with all physical behavior *emerging* from the voltage math and asserted by tests.

**Exit criteria:** a defined patch runs end-to-end `analog → AD → digital → DA → analog`;
voltage and conversion behavior is asserted by tests and matches hand calculations.

**Epic-wide watch-outs:** resist building UI or audio output here. Observation = tests, numeric
asserts, and the ability to define a graph in code. Keep the hot-path discipline from day one even
though nothing is real-time yet — retrofitting zero-alloc/panic-free later is painful.

**Story ordering is validation-first.** The runnable engine comes early (Story 1.3) so every later
story validates real phenomena on real chains, not in isolation. Each story below ends with an
explicit *Validate* gate — don't start the next story until it's green. Phenomena are validated in
the story where their prerequisites first exist, never batched at the end.

### Story 1.1 — Scaffold & core numeric types — ✅ **Done**
*Goal:* a Cargo workspace and the **analog** type vocabulary everything else builds on. We model
the analog world first — continuous-proxy voltage only. Digital buffers, sample rates, and word
clocks are deliberately **not** here; they emerge with the AD/DA converters (Story 1.6).

*Rate model (settled):* there is **one fundamental clock** — the analog rate, our proxy for
"continuous." It is a parameter, never a constant. There is **no global oversample factor and no
global digital base rate**. Digital sample rates are per-converter, emergent, and stamped onto the
data when an AD converter produces it — so the types here must not be able to express a global
analog↔digital rate relationship, because there isn't one. Crossing any clock boundary (analog→digital
at an AD, digital→digital between mismatched converters) is a *resample*, and those phenomena emerge
later rather than being designed in now.

*Scalar policy (settled):* **`f32` storage**, with `f64` reached for only where precision demands it
(accumulators — summing nodes, filter state, the future AD anti-alias filter). This keeps the WASM/SIMD
path cheap while protecting error-sensitive accumulation. Decide it here because every buffer depends on it.

*Watch out:* buffers store **linear** values — `VoltageBuffer` holds linear volts. dB (and later dBFS)
are *measurement units* realized by conversion helpers, never a storage format. Get this right now: a
buffer that stores dB is a category error. Derive everything from the physical (volts) model, not the
other way around.

- ✅ **Task 1.1.1** — Cargo workspace + crate layout: `engine` (core lib) now, plus placeholder members for the future `wasm-bindings` and a render/CLI test-harness crate so the workspace shape doesn't churn later. CI: `cargo test` / `clippy` / `fmt` **and `cargo check --target wasm32-unknown-unknown`** from day one (catch non-portable code — threads, `std::time`, incidental allocs — before Epic 3). Crate-level lint config (deny the relevant clippy groups), note `panic = "abort"` intent for the WASM profile. Test conventions (`approx` for float asserts).
- ✅ **Task 1.1.2** — Scalar policy (`f32` storage / `f64` accumulation) made concrete. `Volts` newtype. `VoltageBuffer` (linear volts, single-conductor for now) carried at the one `AnalogRate` — the engine's fundamental continuous-proxy clock, a constructor parameter, never a constant. No oversample-factor field anywhere.
- ✅ **Task 1.1.3** — Analog level conversions with tests: dBu↔V (0 dBu = 0.775 V), dBV↔V (−10 dBV ≈ 0.316 V). Pure linear↔log helpers; buffers stay linear. *(dBFS and the reference-voltage→dBFS calibration are a digital-domain concept owned by the AD — deferred to Story 1.6, where they emerge from the converter.)*
- ✅ **Task 1.1.4** — Seeded deterministic RNG abstraction: **uniform + Gaussian** draws (thermal/device noise is normal-distributed), and **splittable / sub-seedable** so each device gets its own independent, reproducible stream. Reproducibility test: same seed ⇒ identical sequences; independent streams stay uncorrelated yet stable.

*Deferred to Story 1.6 (stated so it's a decision, not a gap):* `SampleBuffer`, per-converter
`sample_rate` / `bit_depth`, the dBFS/reference-voltage calibration, and word-clock concerns. The
"two distinct signal types, never conflated" discipline is honored when the second type first has a
producer (the AD) — until then nothing makes a digital buffer, so there is no conversion to police.

*Validate (✅ met):* dBu↔V and dBV↔V round-trip and fixed-level conversions match hand calcs; same seed ⇒
identical noise, and independent device streams are reproducible yet uncorrelated. Self-contained, no graph needed.

*Delivered:* engine public surface — `Volts`, `VoltageBuffer`, `AnalogRate`, the four dBu/dBV↔V
conversions, and a seeded splittable `Rng` (uniform + Gaussian). Cargo workspace, CI (incl. wasm32
check), lint policy, and cargo aliases in place. 31 tests green.

### Story 1.2 — Electrical primitives & local solve — ✅ **Done**
*Goal:* Thévenin sources, input impedances, the voltage-divider solve, and the electrical cable.
*Watch out:* the cable is a real **frequency-dependent** element (R + C → one-pole low-pass at the
oversampled rate), not a scalar loss — the "instrument into a long cable" lesson depends on it.

*Design notes (settled):*
- **Impedance is an `Ohms` newtype** (same discipline as `Volts`): series `Add`, a `parallel`
  combinator `(a·b)/(a+b)` for fan-out, finite/≥0 construct-time validation. The cable's
  capacitance is a `Farads` newtype.
- **Name the input descriptor `InputZ`, not `Port`.** `Port` is reserved for the Story 1.3 graph
  connection point, which will *contain* these electrical faces (`Thevenin` for an output, `InputZ`
  for an input). Keeps the layering clean.
- **The divider solves to a dimensionless gain**, not a voltage: `gain = Zin/(Zout+Zcable+Zin)`,
  impedance-only and compile-time-constant; the per-sample `v_src` is multiplied in by the caller.
  This is exactly the seam Story 1.3 needs — gain baked at `compile`, signal flowing through
  `process`.
- **Divider and cable-LPF compose exactly.** A shunt-C input divider factors into
  `[Zin/(Zs+Zin)] · 1/(1 + s·C·(Zs∥Zin))` (`Zs = Zout+Rcable`) — i.e. the constant resistive
  divider gain × a unity-DC-gain one-pole whose corner is `f_c = 1/(2π·R_thev·C)`,
  `R_thev = (Zout+Rcable)∥Zin`. So splitting 1.2.2 (resistive gain) from 1.2.3 (the LPF) is
  physically honest, not an approximation.
- **One-pole fidelity limit (accepted for now):** modeling the source as a resistive high-Z gives
  the treble-loss rolloff but **not** the inductive-pickup *resonance peak* (that needs a reactive
  `Zout` → a 2nd-order resonant low-pass). Deferred per §5.3 "decide per-feature whether deeper
  fidelity earns its complexity"; revisit with reactive source impedance later. What's lost is the
  narrow but signature class of *emergent, cross-device, load-dependent resonance* (passive pickup
  tone + volume/tone-knob interaction, ribbon/dynamic mic loading, transformer character, speaker
  damping, passive resonant EQ/crossovers) — none of it needed through Epic 2; it surfaces with the
  first reactive *device* (Epic 5 breadth). Resonance *inside* a device stays available as designed
  DSP (biquads), unaffected by this.
- **Keep the connection seam open (no implementation now):** frequency-shaping is conceptually a
  property of the whole **edge** (source Z + cable + load Z), not of the cable alone. Today it
  degenerates exactly to *constant resistive gain × a cable-owned one-pole*, which only holds with
  ≤1 reactive element on the edge; a reactive source makes the edge a 2nd-order transfer function
  whose coefficients depend on *both* endpoints. So `Ohms` stays a real scalar and the gain stays an
  `f32` for now — just don't let later code (esp. the Story 1.3 connection model) enshrine "an edge
  is a flat gain plus a cable LPF" as a permanent contract.

- ✅ **Task 1.2.1** — `electrical` module: `Ohms` newtype (series/parallel), `Thevenin { v_src, z_out }`
  output face, `InputZ { z_in }` input face. Construct-time only, single-conductor (balanced lands in 1.5).
- ✅ **Task 1.2.\*** — Test-signal helpers (`#[cfg(test)]`): `sine`, `rms`, and a `measure_gain` that
  drives a steady tone through a stateful filter and returns the steady-state amplitude ratio. Shared
  infra — Story 1.4 reuses it for SNR. (We now need real audio signals to test filter behavior.)
- ✅ **Task 1.2.2** — Voltage-divider gain solve `divider_gain(Zout, Zcable, InputZ) -> f32`
  (`V_in = V_src · gain`). Tests assert hand-calc ratios: bridging (gain ≈ 1, ≈0 dB), matching 600 Ω
  (gain = 0.5, −6.02 dB), fan-out as parallel `Zin`.
- ✅ **Task 1.2.3** — `Cable { r, c }` as series R + shunt C → a stateful one-pole LPF
  (matched/exact coefficient `a = 1 − e^(−dt/RC)`, `f64` state, zero-alloc/panic-free/denormal-flushed)
  at the oversampled rate. *(Matched, not naive backward-Euler `dt/(RC+dt)`: it places the discrete
  pole exactly, so the corner is accurate to a fraction of a percent even at a treble corner — at no
  hot-path cost, since the `exp` is computed once at construction.)* Tests via the helper: corner
  `≈ −3 dB` at the computed `f_c`; plus a **capstone** test — high-Z source → long cable → typical
  `InputZ` — asserting the resistive loss **and** the treble rolloff together, proving the divider +
  LPF compose.

*Validate (✅ met):* impedance/divider physics proven as unit tests on the solve before anything else
runs — bridging ≈0 dB, matching −6 dB, RC corner at the computed frequency, and the capstone showing
loss + rolloff compose.

*Delivered:* `electrical` module — `Ohms` (series/`parallel`), `Farads` (own module), `Thevenin`,
`InputZ`, `divider_gain`, and `Cable` + `OnePole` (matched-coefficient one-pole LPF, zero-alloc /
panic-free / denormal-flushed hot path). A `#[cfg(test)]` `test_util` (`sine` / `rms` / `measure_gain`),
reused from Story 1.4 on. Doc-link + bare-URL lints (`cargo docs`) added to the pre-push gate and CI.
62 engine tests green.

### Story 1.3 — Minimal runnable engine *(first end-to-end milestone)*
*Goal:* device + graph + schedule + block loop — the smallest thing that actually **runs** a patch.
This is the big de-risking moment: after this story we run real chains and validate everything on them.
*Watch out:* devices are black boxes (model observable I/O, not circuitry). This is where the zero-alloc
contract is won — arena/pool the buffers, the `process` loop must not allocate. Build the atomic-swap
seam now even though single-threaded.

*Design notes (settled):*
- **Node vs. device — naming.** The schedulable unit is a **`Node`** (a black-box processing element
  with electrical faces); the trait was renamed `Device → Node`. "Device" is reserved for the *physical
  chassis* (a mixer, an interface), which may map to **several** nodes. Matches audio-graph convention
  (Web Audio's `AudioNode`) and the graph's existing `NodeId`/`nodes` vocabulary.
- **One chassis → many nodes (deferred).** When a device's signal path *leaves and re-enters* the box —
  an **insert** (send → external gear → return) or a routed **audio interface** — it is not one atomic
  node but several **stages** of a path, scheduled separately. Modeling the whole chassis as one node
  manufactures a false cycle (`mixer → comp → mixer`), which cycle detection correctly rejects. The honest
  model splits at the seam into multiple nodes (state partitions cleanly: pre-insert vs post-insert). A
  *logical device* is then a **group of nodes** sharing identity — the grouping the UI/save-load uses (Epic 4+).
- **The schedulable unit is a *stage*** (a set of output ports + the input ports they're computed from). A
  simple node today is the **single-stage (N=1)** case — exactly `Node::process` (read-all-inputs,
  write-all-outputs). Multi-stage nodes declare internal **port-level dependencies**, which the compiler
  folds into the topo sort alongside the external cable edges.
- **Internal routing is dynamic, declared not hard-coded.** A device's internal dependency structure
  reflects its *current configuration* (route in1→out2 vs in1→out3), queried at `compile`. Re-routing that
  changes the dependency graph ⇒ **recompile off-path + atomic swap** (the 1.3.7 seam); cycle detection then
  validates the routing. A device may instead declare *conservative* (all-to-all) deps so routing becomes
  gain changes with **no recompile** — at the cost of coupling all its ports (unusable for re-entrant gear).
- **Parameters vs. structure.** A param that changes processing but not topology (a fader, EQ freq,
  threshold) is a value read inside `process` via the control lane (Story 1.7) — **no recompile**. `compile`
  owns *structure* (topo order, buffers, baked edge solves, stage graph); `process` owns *values*. Recompile
  is reserved for structural change (add/remove node, repatch a cable, reroute topology).
- **Decision: defer the multi-stage / node-grouping machinery** until the first device that needs it (insert
  mixer / routable interface). Single-stage `Node`s cover Story 1.3. The retrofit is additive (a
  `stages()`-style declaration defaulting to one all-ports stage) and localized to the `Node` trait +
  `compile` + `schedule`, with per-port buffers already in place — so deferring doesn't corner us. Don't bake
  "one schedule step per node" in as a permanent assumption.

- **Task 1.3.1** — `Node` trait (declare ports, `process(block)` over voltage), internal-state pattern. *(Renamed from `Device`; "device" reserved for the chassis grouping above.)*
- **Task 1.3.2** — `Graph`: nodes + typed connections, validation (no dangling/duplicate), construct-in-code API.
- **Task 1.3.3** — Minimal device set: test source with real `Zout`, a gain/preamp stage (with a rail voltage), a passive summing node.
- **Task 1.3.4** — Topological sort of the DAG.
- **Task 1.3.5** — `compile(graph) -> Schedule`: topo order + buffer/scratch allocation from a pool.
- **Task 1.3.6** — `schedule.process(out_block)` loop at the oversampled rate; zero-alloc verified (test/bench asserts no allocation in hot path).
- **Task 1.3.7** — Atomic schedule-swap seam (rebuild off-path, swap pointer), exercised single-threaded — proves scene-reload won't stall later.

*Validate:* a `source → gain → sum` chain runs end-to-end; steady-state output voltages match hand calc; hot path provably allocation-free. **The engine is now runnable.**

### Story 1.4 — Analog-chain physics
*Goal:* prove the single-conductor headline phenomena emerge from the voltage math, on real chains.
*Watch out:* these are "tests are the oracle" cases (§3.5) — you can't hear them. Each test asserts a
number you computed by hand, with the hand calc in a comment.

- **Task 1.4.1** — Device noise floors (µV) + cable pickup; SNR degrades down a chain as predicted.
- **Task 1.4.2** — DC offset rides the AC; a DC-blocking HPF removes it.
- **Task 1.4.3** — Headroom & clipping at the rail voltage (physical, in volts).

*Validate:* SNR-down-the-chain, DC removal, and clip-onset voltages all match hand calcs on a running patch.

### Story 1.5 — Balanced lines & common-mode physics
*Goal:* two-conductor balanced lines and everything that rides common-mode. Isolated as its own story
because common-mode modeling is a distinct risk worth proving on its own.
*Watch out:* the receiver takes V+ − V−; interference coupling equally to both must cancel. Phantom and
hum are common-mode DC/AC riding the same conductors — not flags.

- **Task 1.5.1** — Two-conductor (V+, V−) balanced ports + receiver difference; extend the solve.
- **Task 1.5.2** — Phantom +48 V as common-mode DC; condenser source draws it.
- **Task 1.5.3** — Balanced CMRR vs. unbalanced (no rejection).
- **Task 1.5.4** — Ground-loop hum (50/60 Hz common-mode): rejected on balanced, audible on unbalanced.

*Validate:* CMRR figure on balanced vs. unbalanced and hum rejection match hand calcs; phantom voltage present common-mode, absent differentially.

### Story 1.6 — AD/DA converters (the boundary)
*Goal:* the pedagogically rich modeled converters crossing volts ↔ dBFS, on top of a proven analog base.
*Watch out:* use **polyphase** decimation/interpolation (compute only at the rate you need) — naive
filtering at the oversampled rate is ~8× wasteful. Reference voltage → dBFS mapping must be explicit.

- **Task 1.6.1** — AD: polyphase anti-alias decimation, quantization (variable bit depth), reference-voltage → dBFS. Output is `SampleBuffer`.
- **Task 1.6.2** — DA: polyphase interpolation + reconstruction filter → `VoltageBuffer`.
- **Task 1.6.3** — Calibration & artifact tests: "+4 dBu = −18 dBFS" holds; weak AA filter ⇒ measurable aliasing fold-back; low bit depth ⇒ measurable quantization noise.

*Validate:* calibration mapping exact; aliasing and quantization artifacts measurable and matching prediction — because the analog chain underneath is already proven, a failure here is the converter's.

### Story 1.7 — Input lanes & a playable voice (headless)
*Goal:* the two-lane input system and a simple synth voice, exercised without audio output.
*Watch out:* keep control (smoothed) and events (sample-accurate) genuinely separate. The oscillator
lives in the oversampled analog domain — aliasing is handled by the AD filter, so no band-limiting tricks needed yet.

- **Task 1.7.1** — Lock-free control-param queue (latest-wins) + per-block de-zippering.
- **Task 1.7.2** — Timestamped event queue (note-on/off/gate) applied at sample offsets within a block.
- **Task 1.7.3** — Simple synth voice (oscillator + envelope) as a source device with real `Zout`, driven by events.
- **Task 1.7.4** — End-to-end headless test: "play a note" through `analog → AD → digital → DA → analog`, asserting expected output.

*Validate:* a note triggers sample-accurately and produces the expected output through the full chain; a swept control param de-zippers without discontinuities. **Epic exit met.**

---

## Epic 2 — Offline Render ("hear it" cheaply)

**Goal:** reach the audio oracle without real-time infrastructure — the *same* engine driven flat-out
into a WAV. First real DSP and a trivial speaker/air/ear stage so there's something meaningful to hear.

**Exit criteria:** build a chain, render it, and the result sounds correct; DSP and converter behavior
validated by listening **and** golden-file tests.

**Watch-outs:** this is a test harness, not a feature — do not build a second engine. Determinism
(seeded) is what makes golden-file tests viable. Keep it thin.

- **Task 2.1.1** — WAV render driver: drain `schedule.process` as fast as possible to a file; deterministic with seed.
- **Task 2.2.1** — First DSP: a filter (biquad) device.
- **Task 2.2.2** — First dynamics: a simple compressor device.
- **Task 2.3.1** — Trivial speaker (V → SPL via sensitivity + simple response curve) + air/ear (fixed attenuation) + internal AD plumbing to host format.
- **Task 2.3.2** — Converter-payoff demo renders: aliasing (weak AA filter) and quantization (low bit depth), audible.
- **Task 2.3.3** — Golden-file test harness: render fixed patches, assert output matches stored references.

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
- **Story 5.2** — Routing & live-sound scenarios at scale (multi-core partition of the schedule if needed).
- **Story 5.3** — Deeper DSP and deeper AD/DA modeling as needed. Includes **clock-crossing / sample-rate-conversion** scenarios: mismatched converters (e.g. a 44.1k device into a 48k device) resample at the boundary, with the real artifacts emerging — the payoff of the "crossing any clock = resample" rate model settled in Story 1.1. *(Assess scope when we arrive — likely depends on the fractional resampler that AD/DA in Story 1.6 may or may not have already needed.)*
- **Story 5.4** — Challenge / diagnostic-scenario framework on the sandbox.
- **Story 5.5** — Optional schematic / node-graph view over the same model.
