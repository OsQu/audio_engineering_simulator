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

## Epic 1 — Headless Voltage Engine

**Progress:** Stories 1.1–1.7 ✅ done — **Epic 1 complete.** Scaffold/types, electrical primitives,
runnable engine, analog-chain physics, balanced lines & common-mode, AD/DA converters & the
signal-carrier seam (1.1–1.6), and input lanes & a playable voice (1.7). The carrier set grew from one
buffer type to three: analog voltage (`VoltageBuffer`), digital audio (`SampleBuffer`, per-converter
rate/bit-depth/clock), and sparse MIDI/control events (`Lane::Events`) — plus the smoothed control-param
side-channel. The capstone plays a note end-to-end through `analog → AD → digital → DA → analog`. 229
engine tests green; hot path zero-alloc throughout. **Next: Epic 2 — offline render to WAV (the audio
oracle).**

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

### Story 1.3 — Minimal runnable engine *(first end-to-end milestone)* — ✅ **Done**
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

- ✅ **Task 1.3.1** — `Node` trait (declare ports, `process(block)` over voltage), internal-state pattern. *(Renamed from `Device`; "device" reserved for the chassis grouping above.)*
- ✅ **Task 1.3.2** — `Graph`: nodes + typed connections, validation (no dangling/duplicate), construct-in-code API.
- ✅ **Task 1.3.3** — Minimal device set: test source with real `Zout`, a gain/preamp stage (with a rail voltage), a passive summing node.
- ✅ **Task 1.3.4** — Topological sort of the DAG.
- ✅ **Task 1.3.5** — `compile(graph) -> Schedule`: topo order + buffer/scratch allocation from a pool.
- ✅ **Task 1.3.6** — `schedule.process(out_block)` loop at the oversampled rate; zero-alloc verified (counting-allocator integration test asserts no allocation in hot path).
- ✅ **Task 1.3.7** — Schedule-swap seam: rebuild off-path, swap the owned `Box` (ownership handoff, not atomics), exercised single-threaded — proves scene-reload won't stall. *(The lock-free cross-thread channel is deferred to Epic 3, where a real second thread exists to test it.)*

*Validate (✅ met):* a `source → gain → sum` chain runs end-to-end; steady-state output voltages match hand calc (1.9509 V = 1.0 · 2 · 0.990099 · 0.985222); hot path provably allocation-free. **The engine is now runnable.**

*Delivered:* the `Node` trait + minimal set (`TestSource`, `GainStage` with rail clip, `PassiveSum`);
`Graph` (arena of `Box<dyn Node>` + typed edges, construct-in-code) and `NodeId`; Kahn topological
sort (cycle-rejecting); fan-out edge solve (`fan_out_gains`, parallel branch loading); `compile(graph)
-> Schedule` (wiring validation via `CompileError`, two-pool buffer allocation, baked `EdgeTransform`
= divider gain × optional cable one-pole, flat step list); zero-alloc, panic-free, `unsafe`-free
`Schedule::process` (two-pool design + `self`-destructure for disjoint borrows; proven by a
counting-allocator integration test); `ScheduleSlot` ownership-handoff swap seam. `Device → Node`
rename, and the multi-stage / dynamic-routing / param-vs-recompile design (notes above) settled. 97
engine tests green.

### Story 1.4 — Analog-chain physics — ✅ **Done**
*Goal:* prove the single-conductor headline phenomena emerge from the voltage math, on real chains.
*Watch out:* these are "tests are the oracle" cases (§3.5) — you can't hear them. Each test asserts a
number you computed by hand, with the hand calc in a comment.

*Design notes (settled):*
- **Noise is specified as a spectral density** (V/√Hz), not a wideband RMS. White noise at the
  analog rate has a flat one-sided PSD over `[0, fs/2]`, so `D = σ/√(fs/2)` ⇒ the per-sample draw is
  `σ = D·√(fs/2)` and the wideband RMS on the wire is `σ`. *(Sanity: `D = 10 nV/√Hz` at `fs = 384 kHz`
  → `σ ≈ 4.4 µV` — the "µV" floor the plan calls for.)* Chosen over RMS-on-the-wire because it's
  **rate-independent in-band**: when the AD band-limits to audio `B` in Story 1.6, in-band noise becomes
  `D·√B` and the oversampling SNR gain falls out of the physics with **no remodel** (PROJECT_PLAN §2,
  "no throwaway parameter-only model to migrate later").
- **RNG threading — seed is a *run* parameter.** `compile` gains a **positional** `seed: u64` (decided
  positional for now, not a `CompileOptions` struct): it builds a root [`Rng`] and `split()`s an
  independent child into **every node, in node-index order**, via a new **optional** `Node::seed(rng)`
  hook (default no-op, so existing nodes are untouched). Splitting in index order — and handing even
  deterministic nodes their (unused) split — keeps each node's stream **stable regardless of topology or
  which neighbours are noisy**, which is what makes the SNR test's same-seed comparison exact. A node
  installs its stream only if it actually has a floor (`GainStage::seed` keeps the `Rng` only when its
  `NoiseDensity != ZERO`). Same seed ⇒ identical run; re-compile/swap with the same seed reproduces.
  (Honors the settled "splittable per-device" RNG model.)
- **Noise is input-referred** on the gain stage: `out = clamp((in + n)·gain, ±rail)` — the "the preamp
  sets your SNR" lesson, composing correctly with the existing rail clip.
- **SNR-down-a-chain is shown with unity-gain buffer stages**, so each stage's *uncorrelated* noise
  accumulates in quadrature (`σ_total = √(σ₁² + σ₂²)`) with no gain bookkeeping muddying the hand calc —
  uncorrelated-noise-adds-in-power *is* the lesson.
- **Cable pickup is deferred to Story 1.5.** Pickup is interference coupled *onto the wire* from the
  environment; its broadband-random half is redundant with the device noise floor added here (both are
  just additive Gaussian noise, differing only in injection point), and its signature payoff — 50/60 Hz
  hum and the **common-mode rejection** that cancels it on balanced lines — only makes sense alongside
  balanced ports. So all *coupled-onto-the-wire* phenomena (random pickup, hum, CMRR) land together in
  1.5, and 1.4.1 stays focused on internally-generated device noise. *(Moved from the original 1.4.1.)*
- **AC test signals are `#[cfg(test)]` helpers** (a free-running sine source, a DC-offset source) in
  `test_util` — enough to drive AC through a real compiled patch for 1.4.2/1.4.3 without pulling the
  real *event-driven* oscillator forward from Story 1.7, where it belongs.
- **`DcBlocker` is a standalone public node** — a one-pole **high-pass** (the AC-coupling series-cap RC
  that real inputs/outputs have), the dual of the existing `OnePole` low-pass. DC-blocking is a
  first-class analog phenomenon, so it's a composable node rather than a property folded into others.

- ✅ **Task 1.4.1** — Device noise floors as a spectral density (µV-scale on the wire); SNR degrades down a
  chain as predicted (uncorrelated noise adds in quadrature). *(Cable pickup moved to Story 1.5.)*
- ✅ **Task 1.4.2** — DC offset rides the AC; a DC-blocking HPF removes it.
- ✅ **Task 1.4.3** — Headroom & clipping at the rail voltage (physical, in volts).

*Delivered (1.4.1):* `NoiseDensity` newtype (V/√Hz, `repr(transparent)` like its peers) with
`per_sample_sigma` = `D·√(fs/2)`; an optional `Node::seed(rng)` hook (default no-op) and a `seed`
parameter on `compile` that splits an independent per-node `Rng` stream in node-index order (reproducible
runs); `GainStage::with_noise` adding an input-referred floor (`out = clamp((in + n)·gain, ±rail)`) on a
still-zero-alloc/panic-free hot path (the `no_alloc` test now covers the Gaussian draw). Tests on compiled
chains: floor matches `σ` (4.38 µV @ 10 nV/√Hz, 384 kHz) and noise adds in quadrature down the chain
(`√2·σ`, −3.01 dB SNR per equal stage). 103 engine tests green.

*Delivered (1.4.2):* `DcBlocker`, a standalone one-pole **high-pass** node = the dual of the cable's
`OnePole`, computed as `out = x − lowpass(x)` (the inner low-pass tracks the DC/low content; subtracting
it leaves the AC — a zero at DC, the same matched pole). It **reuses** `OnePole` via a new per-sample
`OnePole::step` seam rather than duplicating the recurrence (one pole, two filters, no inheritance). Filter
coefficients are rate-dependent, so `compile` gained a `Node::prepare(rate)` hook (default no-op, the
companion to `seed`) that bakes the pole off the hot path — `process` only steps it (still zero-alloc /
panic-free; the `no_alloc` chain now includes a blocker). A `#[cfg(test)]` `SineSource` (AC on a DC
pedestal, free-running phase) was added to `test_util` to drive moving signals through a compiled patch.
Tests: −3 dB at the corner, ~unity a decade above, rolloff below, DC driven to zero; and end-to-end on a
compiled chain a 2 V-DC-offset 1 kHz tone comes out mean ≈ 0 with the AC RMS (0.7071 V) intact. 113 engine
tests green.

*Delivered (1.4.3):* the rail clip was already in `GainStage` (Task 1.4.1), so this task proved its
*consequences* numerically. A `headroom_db(peak, rail)` level helper (`20·log10(rail/peak)`) makes headroom
a number; a dependency-free single-bin DFT `tone_amplitude` (in `test_util`, no FFT crate) reads named
harmonics as the distortion oracle. Tests on compiled patches: clip-onset at `amp = rail/(divider·gain)`
(clean below, flat-topped exactly at the rail above); a sub-rail sine stays distortion-free (3rd harmonic
< 1 % of fundamental) at the predicted headroom; and a hard-overdriven sine becomes a square wave with the
textbook odd-harmonic spectrum (3rd = ⅓, 5th = ⅕ of the fundamental, no even harmonics). 118 engine tests
green.

*Validate:* ✅ SNR-down-the-chain (1.4.1), DC removal (1.4.2), and clip-onset voltages (1.4.3) all match
hand calcs on running patches.

### Story 1.5 — Balanced lines, cable pickup & common-mode physics — ✅ **Done**
*Goal:* two-conductor balanced lines, the interference that couples onto cables, and everything that
rides common-mode. Isolated as its own story because common-mode modeling is a distinct risk worth
proving on its own.
*Watch out:* the receiver takes V+ − V−; interference coupling equally to both must cancel. Pickup,
phantom, and hum are voltages riding the conductors (common-mode DC/AC) — not flags.

*Design notes (settled):*
- **Representation: "buffer = conductor."** A port declares a **conductor count** (1 = unbalanced,
  2 = balanced); the schedule pool allocates one flat `f32` `VoltageBuffer` **per conductor**, so
  `in_len`/`out_len` simply start counting conductors and the existing contiguous-range slicing and
  SIMD-friendly flat lanes are untouched. New logic stays confined to the electrical faces (they gain
  a conductor count), the edge solve (per-conductor map + common-mode injection), and the balanced
  driver/receiver nodes. *Rejected:* a polymorphic-width `VoltageBuffer` (breaks the flat-`f32`/SIMD
  story protected since 1.1), and a pre-diagonalized `(differential, common-mode)` basis (mathematically
  equivalent but "knows the answer" — CMRR should emerge from a literal `V+ − V−`, not from storing the
  differential lane directly). **Unbalanced is the degenerate 1-conductor case** (cold leg grounded), so
  balanced→unbalanced mismatches — a real hum source — emerge rather than being special-cased.
- **Balanced is not a separate system — per-conductor processors compose via a framework lift.** A
  balanced pair is just two wires; everything *between* the conductor-changing elements (the driver's
  split, the receiver's difference) is ordinary per-conductor physics — AC-coupling DC blockers, gain,
  cable rolloff, noise — and must compose on the pair, never be reimplemented as "balanced" variants
  (that would make "balanced" a label, the very thing §5.4 forbids). So a node author writes a
  single-conductor transform **once**; the compiler **infers** each per-conductor node's conductor
  multiplicity from the wiring (anchored by the driver/receiver/source faces) and **replicates** the node
  per leg — one independent state copy each, identical coefficients — by wrapping it in an internal
  `Lifted` lane-node. The only genuinely balanced-specific elements are the conductor-changing ones.
  CMRR rejection then emerges from **leg symmetry** (identical per-leg processing), and the deferred
  **finite CMRR is exactly leg asymmetry** — so the deferral stays forward-compatible. Opt-in via
  `Node::per_conductor()`; `Node::replicate()` mints a fresh per-leg instance. *Rejected:* a per-node
  "balanced constructor" (keeps "balanced" out of every node's API), and compiler magic without an opt-in
  (a node must declare itself per-conductor and how to replicate).
- **CMRR is ideal-only; finite CMRR is deferred (possibly never).** Symmetric legs ⇒ the common-mode
  voltage is identical on both conductors ⇒ it cancels exactly at the receiver difference — infinite
  rejection, limited only by float precision. The headline lesson (balanced rejects, unbalanced passes
  at 0 dB) needs nothing more. **Finite, realistic CMRR** would require modeling **leg-impedance
  imbalance** so a common-mode voltage partly *converts* to differential and a sliver survives the
  subtraction — deeper fidelity per §5.3 that does not earn its complexity at this simulation's accuracy.
  Decided **deferred to later / possibly never**: revisit only if a concrete scenario needs a finite CMRR
  figure. (Mirrors the resonance deferral in Story 1.2 — a documented decision, not a gap.)
- **Pickup/hum couple onto the *edge* as common-mode** — the RNG-on-edges seam. `compile` splits an
  independent, index-stable child `Rng` into each edge (parallel to the per-node seeding in step 7); the
  edge step draws/generates the interference per sample and adds the **same value to every conductor** of
  that edge, alloc-free like `GainStage`'s noise floor. Pickup is Gaussian (1.5.2); hum is a deterministic
  50/60 Hz generator with seeded phase (1.5.5) — same injection point, different generator.
- **Phantom is modeled at the source node, with the supply direction abstracted.** Physically +48 V flows
  *upstream* (preamp → mic) against the pull-based DAG, so a phantom-powered condenser source simply
  *emits* +48 V common-mode DC on both conductors when powered (its AC signal differentially), and is
  silent when unpowered ("draws it" = the node gates on power). The upstream supply path and any
  current-draw/voltage-sag are an informed approximation flagged per §5.3 and deferred.

*Implementation phasing (dependency order; task numbers unchanged):* 1.5.1 (substrate) → **per-conductor
lift detour (below)** → 1.5.2 (pickup + edge-injection seam) → **1.5.4 (CMRR — the validation capstone of
pickup + balanced, pulled ahead)** → 1.5.5 (hum, reuses the seam) → **1.5.3 (phantom — orthogonal, depends
only on the substrate, moved last)**.

*Detour — per-conductor lift (substrate generalization, after 1.5.1):* built as its own short track
before the remaining phases because it changes `compile` and the `Node` trait. **(D1)** `per_conductor()`
/ `replicate()` trait hooks + the internal `Lifted` lane-wrapper (holds M independent single-conductor
instances, fans `prepare`/`seed`/`process` across them). **(D2)** conductor-multiplicity inference in
`compile` (a fixpoint over edges anchored by fixed faces; conflicts are `ConductorMismatch`) + wrapping
each liftable node with M>1 in `Lifted`, before pool allocation so everything downstream is unchanged.
**(D3)** `DcBlocker` opts in, proving an ordinary processor composes on a balanced pair — per-leg DC
blocking, differential audio survives — which also de-risks phantom (1.5.3).

- ✅ **Task 1.5.1** — Two-conductor (V+, V−) balanced ports + receiver difference; extend the solve.
- ✅ **Task 1.5.2** — Cable pickup: broadband EMI as a noise voltage coupled *onto* the conductor(s) — the
  RNG-on-edges seam (an optional pickup density on the cable, edge gets its own split stream). Additive
  on unbalanced; sets up the rejection contrast. *(Moved here from Story 1.4.)*
- ✅ **Task 1.5.3** — Phantom +48 V as common-mode DC; condenser source draws it.
- ✅ **Task 1.5.4** — Balanced CMRR vs. unbalanced (**ideal rejection only** — see the deferral above):
  pickup/hum coupling equally to both conductors cancels at the receiver difference on balanced, passes
  on unbalanced (no rejection).
- ✅ **Task 1.5.5** — Ground-loop hum (50/60 Hz common-mode): rejected on balanced, audible on unbalanced.
  *`Cable::with_hum` is a **manual** injection for now — a stand-in for hum whose **appearance** will later
  emerge from a ground-topology cycle-detection pass (see the Epic 5 decision); amplitude stays
  phenomenological.*

*Validate (✅ met):* ideal common-mode rejection on balanced (bit-exact zero) vs. full pass-through on
unbalanced (pickup and hum) match hand calcs; phantom present common-mode (48 V), absent differentially.

*Delivered:* balanced lines as **two conductors** (`InputZ`/`OutputZ::balanced`, "buffer = conductor" in
the schedule pool; a balanced edge bakes one transform per conductor) with `BalancedDriver` (split) /
`BalancedReceiver` (`V+ − V−`). The **per-conductor lift** (detour) — `Node::per_conductor()` /
`replicate()` + the internal `Lifted` lane-wrapper + conductor-multiplicity inference in `compile` — lets
ordinary single-conductor processors (e.g. `DcBlocker`) compose on the pair, so "balanced" is never a flag;
common-mode rejection emerges from identical per-leg processing (finite CMRR = leg asymmetry, deferred).
Interference couples on the **edge** as common-mode via a per-edge seeded stream (salted off the compile
seed, split in edge-index order, identical clone per conductor): `Cable::with_pickup` (Gaussian) and
`Cable::with_hum` (deterministic 50/60 Hz, seeded phase). `CondenserMic` emits +48 V phantom common-mode
when powered. Tests prove ideal CMRR is bit-exact (identical common-mode on both legs cancels at `V+ − V−`)
while unbalanced passes the µV pickup / hum tone / 48 V; hot path stays zero-alloc (no-alloc test covers the
balanced edge, the lifted blocker, pickup, and hum). 142 engine tests green.

### Story 1.6 — AD/DA converters & the signal-carrier seam *(the second carrier)* — ✅ **Done**
*Goal:* the pedagogically rich modeled converters crossing volts ↔ digital samples, on top of a
proven analog base — **and** the architectural generalization they force: from one buffer type to an
**open set of signal carriers**. This is the second pivotal story (after 1.3): where the "two distinct
signal types, never conflated" discipline finally gets its second producer, and where the seam for
MIDI (1.7) and networked audio (Epic 5) is laid.
*Watch out:* use **polyphase** decimation/interpolation (compute only at the rate you need) — naive
filtering at the oversampled rate is ~8× wasteful. Reference voltage → dBFS mapping must be explicit.
Keep buffers **linear**; dBFS is a measurement helper, never storage. Don't bake a *closed* two-type
model — the carrier set is open.

*Design notes (settled — pressure-tested against a real multi-I/O interface device):*
- **Carriers, not buffer types — an open `Lane` enum.** A node's ports flow one of several carriers;
  the schedule pool holds a `Lane { Voltage(VoltageBuffer), Sample(SampleBuffer), … }` and
  `Node::process(&[Lane], &mut [Lane])` keeps a **single signature**. *Rejected:* **typed lanes**
  (per-domain `VoltageNode`/`SampleNode` traits + typed pools) — its premise that a node is
  single-domain fails for the central product device (an interface with analog + ADAT + SPDIF + MIDI
  ports is inherently heterogeneous), and it scales combinatorially with carriers and bridges.
  *Rejected:* a **closed 2-variant** enum — MIDI is not a dense sample buffer at all (it's sparse
  events), so the lane representation must admit non-buffer carriers from the start. The enum's cost
  is one `match` per step (per block, not per sample) plus a one-line `voltage()` accessor per node
  port; domain-compatibility joins the other checks at the single `compile` validation gate
  (consistent with `ConductorMismatch`, cycle detection) — `process` stays total.
- **Ports carry a domain; edges carry a domain-appropriate transform.** A port is a **per-direction
  enum wrapping the existing face** (the `Port` name reserved since Story 1.2): `InputPort {
  Analog(InputZ), Digital(DigitalFace) }` and `OutputPort { Analog(OutputZ), Digital(DigitalFace) }`,
  with `DigitalFace { format: AudioFormat, /* clock role 1.7+ */ }`. **`InputZ`/`OutputZ` are
  unchanged** — zero churn to the analog impedance API and source-vs-load stays type-distinct; the
  per-direction asymmetry (analog z differs by direction, digital format doesn't) is honest.
  *Rejected:* a single `Port { domain, face }` over a bare `Ohms` — it would touch every analog
  constructor. Each port exposes `domain()` and **`lane_count()`** (conductors for analog, channels
  for digital), which **unifies pool allocation** while leaving the balanced **lift** semantics an
  analog-face property (digital nodes are `per_conductor() == false`, so conductors and channels are
  never conflated). The pool is `Vec<Lane>`; `Node::process(&[Lane], &mut [Lane])` keeps one signature
  and reads each lane through a hot-path `lane.voltage()` / `.sample()` accessor whose wrong arm is
  `unreachable!` — dead code, since `compile` validated every edge's domain (panic-freedom rests on
  the compile guarantee, as elsewhere).
- **Converters bridge domains *inside* a node, never on an edge.** The AD has analog input ports and
  digital output ports and crosses the boundary in its own `process`; the edges feeding it are analog,
  those leaving it digital. So the edge invariant is simply **every edge connects same-domain ports** —
  any cross-domain edge is a `CompileError::DomainMismatch`. `Step::Edge` generalizes to `EdgeKind {
  Analog(EdgeTransform), DigitalRoute, … }`: the analog electrical solve (`fan_out_gains` +
  `EdgeTransform`) is one arm, unchanged; a digital edge is a copy (a clock-domain check / SRC arrives
  in Epic 5). `compile` allocates each port's lanes into the `Lane` pool sized per domain (analog at
  `block_len`, digital at `block_len/M`, validating the integer ratio and `block_len % M == 0` there).
- **The multi-I/O device is a *group of nodes*, not one node.** The interface decomposes into preamps
  (V→V), internal AD (V→Sample), digital router (Sample→Sample), internal DA (Sample→V), MIDI routing
  (events→events) — separate nodes sharing a logical-device identity (the UI/save-load grouping,
  Epic 4). Only the converters/bridges straddle domains; everything else stays single-domain. (Honors
  the "one chassis → many nodes" model from Story 1.3.)
- **The 1.6 converters are mono, and the AD *core* is single-ended.** The AD samples one voltage —
  its analog input is single-ended (`InputPort::Analog`, 1 conductor) and it emits one digital
  channel; the DA the reverse. This is honest about real hardware: a balanced line/mic input is
  balanced at the **input stage** (an instrumentation-amp / mic preamp that takes `V+ − V−` *and*
  applies gain — even differential ADC chips difference before sampling), *then* the single-ended
  result reaches the converter core. So the realistic **balanced input is modeled as a separate node**
  in front of the AD — the proven `BalancedReceiver` (Story 1.5), plus preamp gain — reusing 1.5 with
  no duplicated differencing; at the device level the interface still presents balanced jacks, owned
  by that input-stage node (chassis-is-many-nodes). Accordingly the 1.6 **end-to-end capstone chain is
  balanced-fronted** (`balanced source → BalancedReceiver → AD → DA → …`) for realism, while the
  converter **unit tests stay single-ended** to isolate the converter (a clean known voltage at the
  AD, no receiver divider in the +4 dBu = −18 dBFS hand calc). Multichannel digital ports (ADAT's 8
  lanes, etc.) are deferred to Epic 5; `lane_count()` on every 1.6 digital port is 1.
- **`SampleBuffer` stores linear normalized samples** (±1.0 = full scale), parallel to
  `VoltageBuffer`, and carries a **distinct `SampleRate` newtype** (not the analog `AnalogRate` — the
  type system must stop you conflating the two), a `BitDepth`, and a **`ClockDomainId`** (see clocks
  below). dBFS is a conversion helper (volts/samples ↔ dBFS), never the storage format.
- **dBFS calibration is per-converter and peak-referenced.** 0 dBFS = a sample of ±1.0 (peak). The AD
  holds a **reference (full-scale) voltage** — its own property, not a global constant (mirrors
  per-converter rate). The headline "+4 dBu = −18 dBFS" must pin the peak/RMS convention in the test's
  hand calc: +4 dBu = 1.228 V RMS = 1.737 V peak; −18 dBFS peak ⇒ full-scale ≈ 13.8 V peak.
- **Anti-alias / reconstruction filtering is a steep linear-phase FIR** (windowed-sinc), the
  **designed digital decimation/interpolation filter** of a modern oversampling converter — the steep
  filter really is digital in today's hardware; the gentle analog pre-filter sits up near the analog
  Nyquist and we don't need it (nothing generates content above it). This is **new infrastructure**
  (the engine has only the IIR `OnePole` so far): a **Kaiser-windowed sinc**, linear-phase (symmetric
  taps → constant group delay), taps designed at `compile`, zero-alloc ring-buffer convolution in
  `process`, `f64` accumulator, denormal flush. *Nominal spec:* passband to ~0.45·Fs_dig, stopband
  from the digital Nyquist (0.5·Fs_dig), ≥~96 dB stopband / ~0.01 dB passband ripple; the Kaiser order
  formula then sets the tap count (a narrow ~20→24 kHz transition over a 384 kHz stream ⇒ ~1000 taps
  at 96 dB — paid once at compile; a wider transition or lower stopband trades fidelity for fewer
  taps). The demonstrable
  **"weak AA filter" knob is the tap count** (a converter parameter): a short kernel widens the
  transition and lifts the stopband floor, so content above Nyquist leaks and folds back audibly.
  **Polyphase** from the start: an L-tap FIR splits into M phases of L/M taps; the AD runs one phase
  per output sample, the DA one phase per input sample. The DA reconstruction filter reuses the design.
- **Integer-divide rate constraint (for now).** A converter's sample rate must integer-divide the
  analog rate (`M` integer), so polyphase is clean and the digital block is `block_len/M`; the analog
  rate is chosen accordingly (e.g. 8×48 k = 384 k). Mixing rationally-unrelated families (44.1 k with
  48 k) needs a **fractional** resampler — deferred to Story 5.3, where async clock boundaries first
  exist. `compile` rejects a non-integer ratio rather than guessing.
- **Quantization: mid-tread, full-scale clamp, seeded non-subtractive TPDF dither.** Round-to-nearest
  with a level at zero (**mid-tread** — silence stays silent), step `Δ = FS / 2^(N−1)`, **hard-clamped
  at full scale** (a digital "over" clips — the real ADC behavior). **TPDF dither** of ±1 LSB (the sum
  of two independent ±½-LSB uniform draws) is added before rounding, from the node's seeded `Rng` (via
  the `seed` hook, so it's deterministic) — non-subtractive, as real converters do; it decorrelates the
  error into a **flat noise floor** (the "low bit depth ⇒ measurable quantization noise" payoff)
  instead of correlated harmonics. *Test oracle (1.6.5):* dithered noise variance = `Δ²/12 + Δ²/6 =
  Δ²/4` ⇒ RMS `Δ/2`, so a full-scale sine has SNR ≈ `6.02·N − 3.0` dB — flat, not harmonic. Exercise
  N = 16 (measurably noisy) and N = 24 (≈transparent). *Rejected for the first cut:* naive rounding
  (correlated harmonic distortion — a different, less faithful artifact).
- **Clocks: one domain now, the emergent model later.** A `SampleBuffer` carries a `ClockDomainId`
  that is the **identity of the oscillator that produced it** (+ its real rate), *not* a label to
  compare. In 1.6 the AD and DA share the single internal device clock, so there is exactly one domain
  and no async boundary — none of the emergent machinery is built. But the representation is chosen to
  **grow into** it without a rewrite: a clock is a real frequency against the analog continuum (a
  phase accumulator), clock distribution is a compile-time side-graph, and the failure of an unlocked
  link **emerges** as a finite-FIFO over/underflow (clicks/slips) — all built in **Epic 5** (5.3)
  alongside SRC, where multiple devices/domains first coexist. Physical-layer decode (line coding,
  PLL) is **out of scope** (internal circuitry, §2 Non-Goals).

- ✅ **Task 1.6.1** — The carrier seam: `Lane` enum (Voltage, Sample), `SampleBuffer` (linear, +
  `SampleRate` / `BitDepth` / `ClockDomainId`), domain-tagged `Port` / faces, `EdgeKind { Analog,
  DigitalRoute }`, per-domain pool sizing and cross-domain validation in `compile`, and the
  `Node::process(&[Lane], &mut [Lane])` migration (existing nodes read Voltage lanes). No converter
  yet — just the plumbing, with the existing analog tests green through the generalized types.
- ✅ **Task 1.6.2** — FIR primitive: windowed-sinc tap design at `compile`, zero-alloc polyphase
  convolution in `process`, `f64` accumulator, denormal flush. Tested against hand-computed filter
  responses; the "weak filter" parameter exercised. Built as `Decimator`; `Interpolator` (its
  upsampling mirror) followed in 1.6.4.
- ✅ **Task 1.6.3** — AD: polyphase anti-alias decimation (integer `M`), seeded-TPDF-dither quantization
  at variable bit depth, reference-voltage → dBFS calibration. Output is a `Sample` lane; opens the
  (single) clock domain.
- ✅ **Task 1.6.4** — DA: polyphase interpolation (`Interpolator`: zero-stuff ×`M` + reconstruction
  FIR, gain-scaled for unity passband) + de-calibration → `Voltage` lane.
- ✅ **Task 1.6.5** — Calibration & artifact tests, on compiled chains through the seam: "+4 dBu =
  −18 dBFS" holds (peak-referenced hand calc); weak AA filter ⇒ measurable aliasing fold-back; low
  bit depth ⇒ measurable quantization **noise floor** (TPDF RMS `Δ/2`, SNR ≈ `6.02·N − 3`, not
  harmonic distortion); plus the balanced-fronted capstone.

*Validate (✅ met):* calibration mapping exact; aliasing and quantization artifacts measurable and
matching prediction; the full chain `analog → AD → digital → DA → analog` (capstone balanced-fronted)
runs through the generalized carrier seam with the analog physics from Stories 1.2–1.5 intact —
because the analog chain underneath is already proven, a failure here is the converter's or the seam's.

### Story 1.7 — Input lanes & a playable voice (headless) — ✅ **Done**
*Goal:* the two-lane input system and a simple monophonic synth voice, exercised without audio output —
closing Epic 1 and adding the **third carrier** (sparse MIDI/control events).
*Watch out:* keep the two input lanes **genuinely separate** — they are different mechanisms, not two
flavors of one. **Events are a routed carrier** (a `Lane`, with edges, addressable between devices);
**control params are a host→node side-channel** (knob settings, not routed, latest-wins, smoothed). The
oscillator lives in the oversampled analog domain — alias products from an analog-rate oscillator fold
around the *analog* Nyquist (e.g. ~192 kHz at an 8×48 k rate), land out of the audio band, and the AD
decimation filter (Story 1.6) removes them. So **no band-limiting tricks are needed** here — it's the
same converter payoff, not a special case.

*Design notes (settled — decisions taken before fleshing the story out):*
- **Events are the third signal carrier — `Lane::Events`** (sparse timestamped messages, not a dense
  buffer), added to the seam from Story 1.6 alongside a `Domain::Events`, an `EdgeKind::EventRoute`, and
  domain-tagged event ports. This is the carrier seam, kept **separate from the queue that fills it**
  (its own task), mirroring how 1.6.1 separated the seam from the converters that used it.
- **Events ride the existing input lanes; only control params change the `process` signature.** Because
  events are a carrier, a node consumes them through its ordinary `inputs: &[Lane]` (an `Events` input
  port), exactly like voltage or samples. So the synth voice is `events-in → voltage-out` with no special
  argument. Control params are *not* a carrier, so they enter by a new argument:
  **`Node::process(&mut self, params: &Params, inputs: &[Lane], outputs: &mut [Lane])`**. This is the
  `schedule.process(out, &control_queue, &event_queue)` shape from the Epic intro — `control_queue` feeds
  the param store, `event_queue` feeds source nodes' event input lanes. *(Migration: ~120 existing
  `process` call sites gain a `&Params`; a `Params::EMPTY` default keeps non-param tests terse.)*
- **Control params: framework smooths and delivers (decided).** A node *declares* its params like it
  declares ports — `Node::params() -> &[ParamDecl]` where `ParamDecl { id, default, range, smooth }`. The
  **schedule owns** the latest-wins queue and a smoothed-value store; it drains the queue at block start
  and hands already-smoothed values to `process` via the `Params` accessor (`params.get(GAIN_ID)`). De-zipper
  is written **once, in the framework**, never re-implemented per node (avoids the "balanced as a label"
  anti-pattern, §5.4). *Open sub-decision for 1.7.3:* per-block step vs within-block linear ramp — default
  to a **within-block linear ramp** (start→target across the block, recomputed per block) so there are no
  instantaneous jumps, which is the honest reading of "per-block de-zippering."
- **Param addressing is `(NodeId, ParamId)`; event addressing is `(NodeId, event-input-port)`.** A control
  param targets one node's one declared param. An external event targets one node's event-input port (the
  same lane an inter-device event edge would write to — "feed the same event lane, one carrier"); `compile`
  resolves both to pool/store indices, the same way the output tap is resolved today.
- **The queues are single-threaded seams now (decided), lock-free-shaped, validated cross-thread in Epic 3.**
  Headless there is no consumer thread, so — exactly as the schedule-swap seam (1.3.7) was built as a
  single-threaded ownership handoff and its lock-free cross-thread form deferred — the param and event
  queues are SPSC-shaped but exercised single-threaded. True lock-free validation lands in Epic 3 with a
  real AudioWorklet thread. *(This resolves the apparent "lock-free now" wording against the 1.3 deferral.)*
- **Events lane is fixed-capacity with a defined overflow policy.** Unlike Voltage/Sample lanes (sized
  `block_len` / `block_len/M`), an `Events` lane is a sparse `(sample_offset, message)` list. `compile`
  pre-allocates a **bounded** capacity (a converter/graph parameter); on overflow within a block the hot
  path **drops** excess events (never allocates, never panics) — and `log`s nothing on the audio path. The
  bound is the new thing to size honestly; default generous (e.g. 256 events/block).
- **The voice is monophonic (decided), last-note priority.** One oscillator + one ADSR envelope. Pitch
  comes from note-on; gate/note-off drives the release. Polyphony / voice allocation (note→voice
  assignment, stealing) is a categorically larger concern, **deferred** past Epic 1 — a documented decision,
  not a gap. The voice exercises **both** input lanes: pitch & gate from events, envelope times / level from
  control params.
- **Event messages for now: note-on {note, velocity}, note-off, gate.** MIDI **CC** (a continuous
  controller that would drive a *param* — events→param) blurs the two-lane separation and is **deferred**;
  the lanes stay genuinely separate (note events only) this story.

- ✅ **Task 1.7.1** — *Events carrier seam (the third carrier).* `Lane::Events` (sparse, bounded-capacity)
  + `Domain::Events` + `EventMessage` (note-on/off, gate) + event-tagged `InputPort`/`OutputPort` +
  `EdgeKind::EventRoute`; `compile` validates event↔event edges (a cross-domain event edge is
  `DomainMismatch`), sizes/pre-allocates the bounded event lanes, and defines the drop-on-overflow policy.
  No queue, no voice — just plumbing, with all existing analog/digital tests green through the extended types.
- ✅ **Task 1.7.2** — *Timestamped event queue + delivery.* The schedule's external event queue
  (single-threaded SPSC seam), block-bucketed by sample offset and merged with any inter-device event-edge
  output into the target node's event input lane; `process` begins taking `&event_queue`. Note-on/off/gate
  land at sample offsets within the block (sub-block-accurate dispatch is the consuming node's job, reading
  the ordered events from its lane).
- ✅ **Task 1.7.3** — *Control-param system.* `ParamDecl` + `Node::params()`, the latest-wins param queue
  (single-threaded seam), the framework-owned smoothed store with within-block de-zipper ramp, and the
  `params: &Params` argument threaded into `Node::process` (with `Params::EMPTY`). Retrofit one existing
  node (`GainStage` gain) to read a declared param — the de-zipper demo target.
- ✅ **Task 1.7.4** — *Monophonic synth voice.* Oscillator + ADSR envelope as a cross-domain source node
  (`events-in → voltage-out`, real `Zout`), last-note priority. Pitch & gate from the event lane; envelope
  times / output level as declared control params. First node consuming `Lane::Events`.
- ✅ **Task 1.7.5** — *End-to-end headless test:* "play a note" through `analog → AD → digital → DA → analog`,
  asserting sample-accurate onset (silence until the trigger sample, signal after), correct fundamental
  (DFT bin), and a swept gain param that de-zippers without a discontinuity.

*Validate (✅ met):* a note triggers sample-accurately and produces the expected output (onset + fundamental)
through the full chain; a swept control param de-zippers without discontinuities. **Epic 1 exit met.**

*Delivered:* the **third carrier** end to end. `Lane::Events` / `Domain::Events` / `EventMessage` +
`EventFace` event ports + `EdgeKind::EventRoute` (1.7.1); an external `EventQueue` (single-threaded SPSC
seam) with absolute-sample timestamps, block-bucketed delivery into **open** event inputs resolved via
`Schedule::event_input`, and `process_with_events` (1.7.2). The **control-param** lane: `ParamDecl` /
`Node::params()`, a framework-owned `Smoother` store with a within-block linear-ramp de-zipper, a
latest-wins `ParamQueue`, `Params` handed to a new `Node::process(params, …)` signature (migrated across
every node + harness, `Params::EMPTY` fallback), and `Schedule::param` / `process_io` / `process_with_params`
(1.7.3). `SynthVoice` — a monophonic sawtooth + ADSR voice consuming **both** lanes (pitch/gate from events,
level/ADSR as smoothed params), last-note priority (1.7.4). The capstone plays A4 through
`analog → AD → digital → DA → analog` with the fundamental matching the hand calc `(2/π)·sustain·level`,
proves causal silence-before-onset across the converters, and shows the level de-zippering end to end (1.7.5).
Hot path stays zero-alloc (the `no_alloc` test now drives the voice via `process_io` with both queues).
229 engine tests green. **Epic 1 is complete: a defined patch runs end-to-end `analog → AD → digital →
DA → analog`, with all voltage/conversion/event/param behavior asserted against hand calcs.**

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
