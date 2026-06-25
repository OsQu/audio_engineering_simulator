# Audio physics & DSP concepts reference

A running reference of the electronics, signal, and DSP ideas we've covered while building
this project, organized by topic (not by when we hit them). It's the analog-domain
companion to `rust_concepts.md`: that one is *how to write it in Rust*, this one is *what
we're modeling and why*. Use it to skip re-explaining things already covered.

> Status: covers the voltage-native model, impedance/Thévenin & the local solve (incl. the
> open/short extremes, global-vs-local, feedback/algebraic loops, buffering & pushback, and
> ground loops), cables, filters (one-pole & 2nd-order), poles, filter implementation,
> distortion, rail clipping, sampling/decimation/aliasing, windowed-sinc FIR, quantization &
> dither, and group delay/latency — through Epic 3 (real-time).

## Contents
1. [The voltage-native model](#1-the-voltage-native-model)
2. [Decibels & level references](#2-decibels--level-references)
3. [Impedance, Thévenin & the local solve](#3-impedance-thévenin--the-local-solve)
4. [Cables: series R + shunt C](#4-cables-series-r--shunt-c)
5. [Filters: what they are](#5-filters-what-they-are)
6. [One-pole low-pass (rolloff)](#6-one-pole-low-pass-rolloff)
7. [Second-order resonant low-pass](#7-second-order-resonant-low-pass)
8. [Poles & zeros](#8-poles--zeros)
9. [Filter implementation](#9-filter-implementation)
10. [Distortion](#10-distortion)
11. [Rail clipping & headroom](#11-rail-clipping--headroom)
12. [Sampling, decimation & aliasing](#12-sampling-decimation--aliasing)
13. [Windowed-sinc FIR & the Kaiser window](#13-windowed-sinc-fir--the-kaiser-window)
14. [Quantization & dBFS calibration](#14-quantization--dbfs-calibration)
15. [Dither](#15-dither)
16. [Group delay & latency](#16-group-delay--latency)

---

## 1. The voltage-native model

The engine models the analog signal as a **real, oversampled voltage waveform in volts** —
not a buffer with metadata. Levels, impedance loss, clipping, noise, DC, and hum **emerge
from the voltage math**; they're never flagged or special-cased.

**"Emergent" ≠ "simulated at the lowest level."** It means: express the model in real
physical quantities (volts, ohms, farads) and let observable behavior fall out of the math.
The craft is choosing the **altitude** where (a) the inputs are genuine physical quantities
and (b) every *audible* effect emerges — then refusing to pay for fidelity that changes
nothing you can hear.

**Lumped vs. distributed.** Real components are spatially distributed (a cable is distributed
R-L-C along its length). We use **lumped-element** approximations (one R, one C). This is
valid — not lazy — when the part is **electrically short**: its physical length ≪ the
signal's wavelength. At audio, a 20 kHz wavelength in cable is ~10 km, so meters of cable are
thousands of times shorter → the distributed model collapses to the lumped one. Distributed
effects (reflections, propagation delay) only appear at RF or km-scale runs.

**Banned: the scalar shortcut.** Representing an effect by its *result* (e.g. "cable = −2 dB")
instead of its *cause* (R and C, with the loss emerging) is the one thing the philosophy
forbids.

## 2. Decibels & level references

A **decibel** is a logarithmic ratio. For voltage (an amplitude, not a power):

```
   dB = 20 · log10(V / V_ref)        V = V_ref · 10^(dB/20)
```

- **Ratios:** ×2 ≈ **+6 dB**, ×10 = **+20 dB**, ×0.5 ≈ **−6 dB**, ÷√2 ≈ **−3 dB**.
- A **gain ratio** in dB uses the two signal voltages directly (`20·log10(Vout/Vin)`); the
  reference cancels, so it's *not* a dBu/dBV measurement.

**Level references** fix `V_ref` to an absolute voltage so a level can be quoted in dB:
- **dBu** — `0 dBu = √0.6 ≈ 0.7746 V` (the RMS voltage delivering 1 mW into 600 Ω; often
  rounded to 0.775 V). `+4 dBu ≈ 1.23 V` is the pro nominal level.
- **dBV** — `0 dBV = 1 V`. `−10 dBV ≈ 0.316 V` is the consumer nominal level.
- The same voltage reads ~2.2 dB hotter on dBu than dBV (because 0 dBu < 0 dBV).
- **dBFS** (digital full scale) is deliberately *not* an analog concept — it's owned by the
  AD converter (Story 1.6), where a reference voltage maps volts → dBFS.

In code these are *measurement helpers* (`dbu_to_volts`, `volts_to_dbu`, …); buffers always
store **linear** volts. A buffer holding dB would be a category error.

## 3. Impedance, Thévenin & the local solve

**Impedance (Z, ohms)** is how much a thing opposes current flow under a voltage, via
**Ohm's law** `V = I·Z`. We model it as **resistive (real)** for now (`Ohms` newtype);
reactive (frequency-dependent) impedance is deferred (see §7–8).

The key physical idea: **no real source is ideal.** When you draw current from it, its
terminal voltage *sags*. We model that as a resistance *inside* the source, in series with a
perfect source:
- **Output impedance `Zout`** — the source's internal series resistance. **Low Zout = a
  "stiff" source** (holds its voltage under load); **high Zout = weak** (sags easily). A wall
  outlet ≈ 0 Ω; a guitar pickup is high-Z and sags the moment it's loaded.
- **Input impedance `Zin`** — how much current an input draws. **High Zin = a light load**
  (little current); **low Zin = a heavy load**.
- Water analogy: voltage = pressure, current = flow, impedance = how narrow the pipe is.
  `Zout` is a constriction in the source's outlet; `Zin` is the receiver's intake.

**Thévenin equivalent** — any linear output reduces to an ideal voltage source in series with
`Zout` (`Thevenin { v_src, z_out }`). `v_src` is the **open-circuit** voltage (into an
infinite load); a real load pulls it down. `Zin` is `InputZ`.

**The local solve (voltage divider).** Because pro devices buffer their I/O (low Zout, high
Zin, no back-loading), connections solve **locally** — no global nodal/SPICE solve. The
voltage a receiver sees:

```
   V_in = V_src · Zin / (Zout + Zcable + Zin)
```

From this one relationship:
- **Bridging (good):** `Zin ≫ Zout` → gain ≈ 1, negligible loss. *Modern pro audio.*
  (e.g. 100 Ω source → 10 kΩ input → −0.09 dB.)
- **Matching:** `Zin = Zout` → gain = 0.5 → exactly **−6 dB**. *Vintage 600 Ω.*
- **Loaded down (bad):** high-Z source into a low-Z input → gain ≪ 1, big loss.
  (e.g. 10 kΩ source → 600 Ω input → **−25 dB**.) This is bridging done backwards.
- **Fan-out (splitter):** several inputs on one output = their `Zin`s in **parallel**
  (`Ohms::parallel`, `(a·b)/(a+b)`), which lowers the combined load.

The gain is **resistive and constant** → computed once at `compile`, not per sample. The
per-sample `v_src` is multiplied in separately (the Story 1.3 seam).

### Why the divider works (physical derivation)

Source, cable, and load form a **single series loop**. Three facts derive the formula:
1. **Ohm's law:** voltage across any resistor = `I · Z`.
2. **One current (series):** one path, no branches → the *same* `I` flows through `Zout`,
   `Zcable`, `Zin` (Kirchhoff's current law).
3. **Drops sum to the source:** `v_src = V_out + V_cable + V_in` (energy around the loop).

Substitute (1) into (3) with the shared `I` from (2):
```
   v_src = I·Zout + I·Zcable + I·Zin = I·(Zout + Zcable + Zin)
   ⇒  I    = v_src / (Zout + Zcable + Zin)              ← find the current first
   ⇒  V_in = I·Zin = v_src · Zin / (Zout + Zcable + Zin) ← then the drop across Zin
```

**Intuition:** `v_src` is a fixed voltage *budget*; the series resistances, all carrying the
same current, each claim a share **proportional to their size**. The load gets the share
proportional to its own `Zin`. **Where the lost voltage goes:** the drops across `Zout` and
`Zcable` are dissipated as **heat** in those resistances — signal that doesn't reach the load
literally warms the source. Fan-out loses more because a parallel load draws more total
current → bigger drop across `Zout` → less left for the node.

### Gain: passive ≤ 1 vs. active > 1

The divider is **passive** (no power source) so by energy conservation its gain is **always
≤ 1** — it can only lose. **Amplification (gain > 1) needs an active device + power supply:**
the input signal doesn't *become* the output, it **controls** a much larger voltage released
from the rails (a transistor/op-amp as a valve; the rails as the reservoir). The extra energy
comes from the supply, not the signal — which is why a high-`Zin` amp input can draw almost
no power yet produce a big output.

Modeled as a **voltage-controlled voltage source**: `V_out = G·V_in` (G = gain knob), which
becomes the device's `v_src` behind a low `Zout`, then feeds the next divider. So the two
compose: `realized = G · V_in  ×  divider_gain` (active >1 × passive ≤1). This is the
**Story 1.3.3** preamp; the rails are also where it *clips* (§11). Nuance: a **step-up
transformer** gives passive voltage gain by **trading voltage for current** (`V_out = N·V_in`,
`I_out = I_in/N`) — power gain still ≤ 1; reactive, so deferred. Rule: **passive ⇒ no power
gain**; a resistive divider ⇒ no voltage gain either.

**Two conductors, two meanings:** lumping a cable's *shield* capacitance (§4) is fine; but
modeling two *signal* conductors (balanced V+/V−, Story 1.5) is a different axis that makes
common-mode rejection, hum immunity, and phantom power *emerge* — done deliberately there.

### Open vs. short circuit, and the 1/Z intuition

Current is `I = V/Z` — **impedance is in the denominator**, so it throttles current, never
multiplies it. The two extremes bound everything in between:

```
   Z → ∞  (open circuit):  I = V/∞ = 0      a gap, no path — zero current, full voltage
   Z → 0  (short circuit): I = V/0 → max    limited only by the source's own Zout
```

So the "huge number" intuition belongs to the **short** (low Z), not the open. An open circuit
is a literal gap electrons can't cross → no current. Across a Thévenin output:

| Load | Current | Terminal voltage |
|---|---|---|
| open (∞) | 0 | **`v_src`** (max V) — *this* is why `v_src` is the open-circuit voltage |
| typical | mid | `v_src · Zin/(Zout+Zin)` |
| short (0) | `v_src/Zout` (max I) | 0 |

A voltmeter reads `≈ v_src` precisely because its huge `Zin` is nearly an open circuit:
measuring open-circuit voltage just means loading lightly enough that the droop is negligible
(bridging used as a measurement trick).

### What we're solving — global vs. local

The physical question a patch poses is **what voltage appears at every node** (especially each
device input). Everything is coupled: a load changes a voltage, which changes currents, which
change other nodes.

- **Global solve (nodal analysis / SPICE):** one equation per node, solved **simultaneously**
  — a single matrix for the whole circuit. Needed when nodes are mutually dependent; expensive,
  redone on any change. A per-sample matrix solve at the oversampled rate would be brutal.
- **Local solve (ours):** because buffered I/O makes signal flow **one-directional with no
  feedback**, each connection is an **independent divider**, solved in **topological order**
  (source → its edge → next device → …). Closed-form, no matrix; gains baked at `compile`.

Each edge's divider depends only on its three fixed impedances — never on anything *downstream
of the load* — so no step needs a later step's result, and one forward pass suffices. The
**topological sort is the visible fingerprint of "local"**: a global solve has no per-node
order.

### When the local solve breaks: feedback & unbuffered pushback

Two distinct ways the one-directional assumption fails:

**1. Feedback loops (graph cycles).** If a later output routes back to an earlier input, a node
depends on itself → simultaneous equations. The topo sort can't order a cycle (→
`CompileError::Cycle`). Nuance: only a **delay-free (algebraic) loop** truly forces a
simultaneous solve. A loop with **memory/delay** in it (every real feedback effect — echo
feedback, IIR) reads *last sample's* value, already known, so you "cut" the loop at the delay
and the forward walk still works. And you'd only globalize the **looped cluster** (a strongly-
connected component), not the whole patch.

**2. Unbuffered pushback.** In a **passive** device (no active stage — a pot, a resistor pad, a
passive tone stack) input and output share **one current path**. The load's current flows
through the device in the **same forward direction** (current never reverses) — but sharing the
path means the **load impedance reflects to the input**: `Zin_seen = f(Zload)` (a series-R
device: `Zin_seen = R + Zload`). The input face is no longer constant, so the `source→device`
edge couples to the `device→load` edge **even with no graph cycle**. Solving it needs an
impedance pass upstream, then voltages downstream — a coupled solve.

**What buffering does.** An **active** follower (op-amp/transistor — high `Zin`, low `Zout`)
splits the one path into **two current loops**: a tiny input loop, and an output loop whose
current comes from the **power supply**. It carries the *voltage* forward without sharing
*current*, so the load can't reflect upstream and `Zin` stays a constant. That severing is the
whole basis of the local solve. Modeling an unbuffered device with a fixed `Zin` silently
treats it *as if* buffered — the load-dependent tone simply won't emerge (the deferred
reactive/loading class, §7).

**A transformer is not a buffer.** It's passive: it *transforms/isolates* (voltage & impedance
by turns ratio, galvanic isolation, balanced↔unbalanced) but **reflects** the load rather than
hiding it — `Z_primary = Z_secondary·(Np/Ns)²`. The downstream impedance still shows through,
scaled. Buffering hides the load (active, fed from a rail); a transformer passes it transformed.

### Ground loops (preview of Story 1.5)

Two devices joined by the signal cable **and** a second path (both chassis to mains earth) form
a conductor loop. Building "ground" is not one equipotential — earth points differ by a
**50/60 Hz** hum voltage `V_ground(t)`. The two devices' references differ, and that difference
lands **in series with the signal**: `v_received = v_signal + V_hum`.

- **Buffering doesn't help.** It fixes impedance, not the *reference* mismatch — good low-`Zout`
  gear hums just as happily. The cure is a **balanced** interconnect or **breaking the loop**
  (lift ground; an isolation transformer's galvanic isolation severs the DC path).
- **Modeled as a common-mode injection, not a global earth solve.** Add `V_hum` equally to both
  conductors of a balanced line; the differential receiver (`V+ − V−`) cancels it (finite CMRR
  → a little leaks), while an unbalanced line adds it straight to the signal → audible hum. We
  model the *effect* (a common-mode source), so it stays local — the same trick as phantom power
  (a common-mode DC injection). Seeded phase keeps it deterministic.

## 4. Cables: series R + shunt C

A cable is modeled as **series resistance R** + **shunt capacitance C**:

- **Series** = in-line; the signal flows *through* it (Zout, R_cable). Drops voltage along
  the path.
- **Shunt** = bridged across the line to ground; offers a *side path* (the cable's
  conductor-to-shield capacitance). The signal does **not** flow through it.

A **shunt cap to ground makes a low-pass**: at DC the cap is open (no effect, full signal);
at high frequency its impedance `1/(2πfC)` collapses → it shunts highs to ground → treble
lost. Longer cable → more capacitance → lower corner → darker. (The "buffer your pedalboard"
lesson, straight from the wire's geometry.)

**Series vs. shunt flips the filter** — same two parts, swap roles:

| Topology | Filter | Where |
|---|---|---|
| series **R**, shunt **C** | low-pass (treble loss) | the cable |
| series **C**, shunt **R** | high-pass (kills DC) | DC blocker (Story 1.4.2) |

A series cap blocks DC because at 0 Hz the cap is an open circuit — DC can't pass, AC sails
through.

## 5. Filters: what they are

A **filter** is a system whose **gain depends on frequency**. Feed it a sine; it changes
that sine's amplitude and phase by an amount set by the sine's frequency.

**LTI** (Linear + Time-Invariant) is why the frequency response is a *complete* description:
an LTI system turns a sine into the *same-frequency* sine (only amplitude/phase change) — it
can't distort the shape. Since any signal is a sum of sines (Fourier), knowing what the
filter does to each sine tells you what it does to everything.

**Emergent vs. designed** (both appear in this engine):
- **Emergent** — falls out of R/L/C physics in the volts domain (cable LPF, DC blocker,
  pickup resonance). We specify *components*; the response emerges.
- **Designed** — a DSP device we author to a target response (biquad EQ in Epic 2, the AD
  anti-alias filter). We specify the *response* directly.

**Three views of the same filter** (switch between them constantly):
- **Frequency** — gain & phase vs. f. *"What will I hear?"*
- **Time** — impulse/step response. *"Transient, latency, ringing?"*
- **Pole–zero** — the algebraic skeleton (§8). *"How do I design/analyze it?"*

**IIR vs. FIR:** a filter whose output depends on *past outputs* (feedback) is **IIR**
(infinite impulse response) — our emergent analog filters are IIR (the cap voltage feeds
back). Depending only on past *inputs* is FIR.

> **Want (Oskari):** a proper from-scratch crash course on filters as its own session at some
> point — the three views together, IIR vs. FIR, phase, resonance/Q, and how design maps to
> code. Not now. Until then these filter sections (5–9, 13, 16) are the running notes.

## 6. One-pole low-pass (rolloff)

The simplest filter: one energy store (a cap) → **one pole** → a gentle rolloff, **no peak**
possible (one store can only dissipate energy, not slosh it).

```
   corner   f_c = 1 / (2π·R·C)          (R = the Thévenin resistance the cap sees)
   at f_c   gain = 1/√2 = 0.707  →  −3 dB   ("half-power point"), phase −45°
   above    slope → −6 dB/octave = −20 dB/decade   (gain halves per octave)
   |H(f)| = 1 / √(1 + (f/f_c)²)
```

For our cable, the cap sees `R_thev = (Zout + R_cable) ∥ Zin`, so
`f_c = 1/(2π·R_thev·C)`. **The divider gain × a unity-DC-gain one-pole reproduces the full
shunt-C divider response exactly** — the resistive loss and the rolloff are separable, not an
approximation:

```
   V_in/V_src = [Zin/(Zs+Zin)] · 1/(1 + s·C·(Zs∥Zin))      Zs = Zout + R_cable
                └ constant gain ┘   └──── one-pole LPF ────┘
```

Time domain: a step in → a smooth exponential settle, time constant `τ = RC` (63% in one τ).
No overshoot.

## 7. Second-order resonant low-pass

Add a **second** energy store (an inductor L) → **two poles** → steeper rolloff
(**−12 dB/oct**) **and** the possibility of **resonance**. L (magnetic field) and C (electric
field) trade energy back and forth at:

```
   f_0 = 1 / (2π·√(LC))
```

The only thing damping that exchange is R. Light damping → the circuit responds *more* at
`f_0` than at DC → a **peak** before the rolloff. Sharpness is the **Q factor**:

| Q | Behavior |
|---|---|
| < 0.5 | overdamped — no peak (≈ two stacked one-poles) |
| 0.707 | maximally flat (Butterworth) — the boundary, no peak |
| > 0.707 | underdamped — a peak at ≈ `f_0`, taller with Q; rings in time |

Time domain: a high-Q step response **overshoots and rings** (a decaying sinusoid). *Peak in
frequency ⟺ ringing in time* — the same poles.

**Guitar example:** a passive pickup is an inductor (~2–5 H) + the cable's C → an RLC
resonant low-pass, peak around 2–5 kHz (its "presence"). A longer cable raises C → lowers
`f_0` and Q → the bright peak slides down and flattens. A **buffer** (low Zout) isolates the
pickup from the cable so length stops detuning the tone — the "buffer your pedalboard" lesson.

**Why our Story 1.2 model is one-pole:** we model the source as a *resistance*, not an
inductor — so only the cap stores energy → one pole → treble loss but **no resonance peak**.
Adding a reactive (inductive) `Zout` later turns it 2nd-order and the same cable C gives the
peak for free. What's deferred is the narrow but signature class of *emergent, cross-device,
load-dependent resonance* (pickup tone, ribbon-mic loading, transformer character, speaker
damping). Resonance *inside* a device stays available as designed DSP.

## 8. Poles & zeros

A filter is a ratio of polynomials in **complex frequency** `s = σ + jω`:

```
   H(s) = N(s) / D(s)
```

- **Zeros** = roots of `N(s)` → `H = 0` there (a notch/null).
- **Poles** = roots of `D(s)` → `H → ∞` there: the filter's **natural modes**; they set
  where the response rises (a corner, or a resonant peak).
- **Order = number of poles = number of energy stores.**

Steady-state sine response = evaluate at `s = jω` (walk up the imaginary axis). The
**s-plane** tells you behavior from a pole's location:

```
              jω  (= oscillation frequency)
               │      × ← complex pole: decaying oscillation (ringing);
               │     ╱     closer to the jω axis ⇒ higher Q ⇒ taller peak
   ────────────┼──────────── σ  (= decay rate)
          ×    │
   real pole on −σ axis: pure exponential decay, NO oscillation (our one-pole, at −ω_c)
        LEFT half = STABLE (decays)   │   RIGHT half = UNSTABLE (grows)
```

- **Real part σ** = decay rate (more negative → settles faster; on the axis → pure
  oscillator; right half → unstable).
- **Imag part ω** ≈ resonant frequency.
- A single **real** pole → rolloff, no peak. A **complex-conjugate pair** → ringing/resonance.

**Why a near-axis pole makes a peak** (geometric): `|H(jω)| = ∏(dist to zeros) / ∏(dist to
poles)`. Sweeping ω up the axis, passing *near a pole* shrinks a denominator distance →
gain spikes. Passing near a *zero* → a notch.

**Examples:**
- One-pole LPF: `H = ω_c/(s + ω_c)`, pole at `−ω_c`. At ω=0 → gain 1; at ω=ω_c →
  `ω_c/(ω_c√2)=0.707` → −3 dB.
- DC-blocking HPF: `H = s/(s + ω_c)` — same pole, **plus a zero at s=0** that nulls DC.

**Discrete (z-plane):** sampling maps the s-plane to the z-plane; **stability = poles inside
the unit circle** (`|z| < 1`). The one-pole's discrete pole sits at `z = 1 − a` (§9), always
in `[0,1)` for `a ∈ (0,1]` → unconditionally stable.

## 9. Filter implementation

From continuous circuit to a per-sample line. The one-pole RC obeys:

```
   RC·(dy/dt) + y = x          (y = capacitor voltage = output)
```

Whatever the discretization, the per-sample recurrence is the same one-line leaky integrator
(`dt = 1/rate`, `AnalogRate::seconds_per_sample`):

```
   y[n] = y[n−1] + a·(x[n] − y[n−1])        a ∈ [0, 1]
```

`a` is the fraction of the gap to the input the output closes each sample; only the *formula
for `a`* differs between methods:

| coefficient | method | corner accuracy |
|---|---|---|
| `a = dt/(RC + dt)` | backward Euler | crude — ~4% low at a 16 kHz corner (384 kHz) |
| **`a = 1 − e^(−dt/RC)`** | **matched / exact pole** ← *what we ship* | ~0.3% at the same corner |

The **matched** form places the discrete pole at the analog pole's exact image (`e^(−dt/RC)`),
so the −3 dB point lands on `1/(2π·RC)` to a fraction of a percent even at a treble corner —
which is what lets the corner test assert a realistic ~16 kHz corner tightly. The `exp` is paid
**once** at construction, so the hot path is identical to backward Euler. Both are stable
(`a ≤ 1`) and degrade gracefully: `RC→0 ⇒ a→1` (pass-through, e.g. no cable), `RC→∞ ⇒ a→0`
(frozen). (Bilinear-with-prewarp also matches one corner exactly but misbehaves at the
no-cable limit, so we skip it.) See `OnePole::new` in `electrical/cable.rs`.

**The compile/process split & hot-path discipline** (this filter is the engine's first
stateful processor — the template every later one follows):
- `a` computed **once** at construction from R, C, rate; `process` does no division/validation.
- State (`y`) is **pre-allocated in the struct**, kept in **`f64`** (it's an accumulator —
  the scalar policy's f64 case); the buffer stays `f32`.
- The per-sample loop is **zero-alloc, panic-free** (iterate `as_mut_slice`, no indexing),
  with **denormals flushed** (decaying tails slip into denormal floats → CPU stalls → fatal
  in a real-time worklet).
- Pull state into a local around the loop (`let mut y = self.y; … self.y = y;`) so it stays
  in a register.

Unit check: `ohms · farads = seconds`, so `RC` and `dt` add cleanly.

## 10. Distortion

"Distortion" is several different things with different homes:

| Kind | What it is | Home |
|---|---|---|
| **Rail clipping** | a voltage ceiling (§11) | device transfer, in volts |
| **Saturation** | a soft nonlinear curve (tube/tape/transformer) | device transfer, in volts |
| **Current-sag / loading** | source can't supply current into a low-Z load | *deferred* |

**Architecture rule: the interconnect solve stays LINEAR.** Distortion is a **device-owned
nonlinear transfer function applied in the volts domain**, *upstream* of the Thévenin `v_src`:

```
   inside device:  gain → [nonlinear transfer: clip/saturate] → v_src
   between devices: v_src ──[Zout]──[cable]──► Zin     (linear divider + one-pole)
```

- **Don't make Thévenin nonlinear.** Thévenin is a *linear* equivalent; a nonlinear source +
  load needs a coupled per-sample (Newton) solve = the SPICE we ruled out, and it breaks the
  zero-alloc hot path. Current-sag is also low-value (pro gear buffers I/O), so it's skipped.
- It's still **volts-native**, so artifacts (harmonics, clip shape) **emerge** — not faked.
- **Oversampling pays off:** doing distortion in the oversampled analog domain keeps its new
  harmonics below the analog Nyquist, so the AD anti-alias filter (1.6) band-limits them
  correctly — no aliasing fold-back. (Distorting in the digital domain wouldn't get this.)
- **Load-dependent distortion** (e.g. a tube amp into a speaker) — if ever needed — is handled
  by **parameterizing that device's transfer with its compile-time-known load**, not a
  per-sample coupled solve (impedances are fixed at `compile`).

## 11. Rail clipping & headroom

Active devices run on a DC **power-supply rail** (e.g. ±15 V). The output **cannot swing past
the rail** — there's no more voltage available (in practice it clips ~1–2 V short, since the
output stage needs voltage across it). The rail is a hard, physical voltage ceiling.

**Headroom** = the gap (in dB) between nominal level and the rail. Pro gear runs ~+4 dBu
(≈1.23 V RMS) on big rails (≈±13.5 V usable) → ~18+ dB of headroom for transients. This is
why **gain staging** matters: stay high above the noise floor (good SNR) but below the rail
(no clipping) — the sweet spot between the two, which the sim teaches emergently (noise floor
in 1.4.1, rail in 1.4.3).

**Clipping → harmonics.** When peaks hit the rail they **flat-top**:

```
       ╭─╮                  ┌──┐   ← flat-topped at +rail
      ╱   ╲                ╱    ╲
   ──╱─────╲──────  →   ──╱──────╲──────
    ╱       ╲            ╱        ╲
             ╰─╯                   └──┘  ← flat-bottomed at −rail
```

A flat-topped sine approaches a **square wave** = fundamental + **odd harmonics** (3rd, 5th,
…). Harder clip → more square-like → more harmonics → harsher. The sharp corners are
high-frequency energy.

- **Hard clip** (op-amp/transistor): abrupt clamp → many high-order odd harmonics → harsh.
- **Soft clip / saturation** (tube/tape/transformer): the curve bends gradually → fewer,
  lower harmonics → "warm." Often **asymmetric**, adding **even** harmonics (2nd = an octave),
  musically consonant. (Symmetric clip → odd only; asymmetry or a DC offset → even harmonics.)

**Model:** a clamp on the voltage — `y = clamp(x, −rail, +rail)` (hard) or a `tanh`-like curve
(soft). The rail is a real voltage, the signal is a real voltage, so the artifact emerges; we
never flag "clipped."

**Analog rail clip ≠ digital clip.** Rail clipping hits a *voltage* limit (volts, pre-AD).
**Digital clipping** hits *full scale* (0 dBFS, the largest code, post-AD). Different ceilings
on opposite sides of the converter; the AD's reference-voltage→dBFS calibration (Story 1.6)
relates them ("how many dB below the rail is 0 dBFS").

## 12. Sampling, decimation & aliasing

The AD crosses analog → digital. The analog domain is a **heavily oversampled** voltage stream
(the continuous proxy, e.g. 384 kHz); the AD drops it to the converter's own **digital rate**
(e.g. 48 kHz) by **decimation** — keeping one sample of every `M = analog_rate / digital_rate`.

You can't *just* drop samples. **Nyquist–Shannon:** a stream at rate `fs` can only represent
frequencies up to `fs/2` (its Nyquist). Anything above folds back — **aliasing**: a tone at
`f > fs/2` reappears as a phantom at `|k·fs − f|`, indistinguishable from a real tone there and
**unremovable** afterward.

```
   384 kHz analog, decimate ×8 → 48 kHz (Nyquist 24 kHz)
   a 40 kHz tone, naively decimated → folds to |48 − 40| = 8 kHz  ← a phantom in-band
```

So decimation is **filter-then-drop**: a low-pass removes everything above the new Nyquist
*first* (the **anti-alias filter**, §13), then you keep every `M`-th sample. This is the
modern-converter architecture — the steep filter is digital, run on the oversampled stream; a
real converter's gentle *analog* pre-filter sits up near the analog Nyquist and we don't model
it (nothing in our world generates above it). The DA is the mirror: **interpolate** (upsample
×M + reconstruction low-pass) back to the analog rate.

**Polyphase efficiency.** Decimation computes only the outputs it keeps — one length-`L` FIR dot
product per *retained* sample, never the `M−1` discarded ones. Same result as filter-all-then-drop,
`M`× cheaper. (`M` must be an integer for now; arbitrary ratios — a 44.1k device into a 48k one —
are a fractional resample, deferred to Epic 5.) See `fir::Decimator`; `compile` enforces the
integer ratio and `block_len % M == 0`.

## 13. Windowed-sinc FIR & the Kaiser window

A **FIR** (finite impulse response) filter is **feed-forward**: `y[n] = Σ b[k]·x[n−k]` — a
weighted sum of the last `L` inputs (the **taps**), no feedback. Contrast the cable's recursive
[one-pole](#6-one-pole-low-pass-rolloff) (IIR). The FIR's payoff is that it can be made
**arbitrarily steep** and **linear-phase** (symmetric taps ⇒ every frequency delayed equally,
constant group delay = `(L−1)/2` samples) — exactly what an anti-alias brick wall needs.

The ideal brick-wall low-pass has impulse response **`sinc`** (`sin(πx)/(πx)`): flat to the
cutoff, zero above. But `sinc` is *infinitely* long; truncating it rings (Gibbs ripple). So you
**window** it — taper the ends to a soft stop:

```
   tap[n] = 2·fc · sinc(2·fc·(n − center)) · w[n]      (then normalize Σtap = 1 ⇒ unity DC gain)
```

The **Kaiser window** `w[n]` is the near-optimal adjustable taper, controlled by one parameter
**β**: larger β ⇒ deeper **stopband attenuation** but a **wider transition band** (the two always
trade). Kaiser's empirical formulas give β from a target stopband (dB) and the **tap count `L`**
from (stopband, transition width):

```
   β ≈ 0.1102·(A − 8.7)   (A = stopband dB, A > 50)
   L ≈ (A − 8) / (2.285 · Δω)     (Δω = transition width, rad/sample)
```

So a narrow transition costs taps: a ~20→24 kHz transition at ~96 dB over a 384 kHz stream ≈
**~1000 taps**. **Tap count is the demonstrable "weak filter" knob** — a short kernel widens the
transition and lifts the stopband floor, so out-of-band content leaks past Nyquist and aliases
(harness Scenario 5: 161 taps rejects a 40 kHz tone by ~89 dB; 13 taps by only ~6 dB → audible
fold-back). Taps are designed once at construction (a Bessel `I₀` for the window); `process` only
multiplies and accumulates. See `fir::design_lowpass`, `kaiser_beta`, `bessel_i0`.

## 14. Quantization & dBFS calibration

After band-limiting and decimating, the AD does the irreversibly-digital step: **quantize** each
sample onto a finite grid of `2^bits` levels. The step is

```
   Δ = FS / 2^(bits−1)          (signed PCM word; FS = full-scale magnitude)
```

We use a **mid-tread** quantizer (round-to-nearest, with a level *at zero* — so silence stays
silent: `q = round(x/Δ)·Δ`), and **hard-clamp at full scale** (a digital "over" clips at the
largest code — distinct from analog rail clipping, §11, which is a *voltage* limit pre-AD).

**dBFS calibration.** Samples are stored **normalized**, `±1.0 = full scale`. The bridge from
volts is the converter's **reference voltage** = the peak volts that map to 0 dBFS:

```
   sample = volts / reference          dBFS = 20·log10(|sample|)
```

This is the only place the analog and digital level worlds meet. Pick the reference to set the
alignment: a **13.80 V-peak** reference puts **+4 dBu** (1.737 V peak) at **−18 dBFS** — the
standard pro headroom calibration ("+4 = −18"). It's *owned by the AD*, not a global constant
(each converter has its own). See `BitDepth::step`, `AdConverter`, `sample_to_dbfs`.

**Quantization noise.** Rounding error is bounded by `±Δ/2`; modeled as uniform over that range
it has variance `Δ²/12`, giving the ideal full-scale-sine SNR `≈ 6.02·N + 1.76 dB` — the famous
**~6 dB per bit**. (16-bit ≈ 98 dB; 24-bit ≈ 146 dB.) But plain rounding's error is *correlated*
with the signal → §15.

## 15. Dither

Plain quantization rounds deterministically, so the error is **correlated** with the signal: it
shows up as **harmonic distortion**, and low-level signals quantize into ugly "chunky" steps (a
fade-out granulates instead of smoothly vanishing). **Dither** fixes this by adding a small random
signal *before* quantizing, which **decorrelates** the error into a flat, signal-independent
**noise floor** — trading distortion for benign hiss. The ear tolerates steady hiss far better
than correlated distortion, and dither lets you hear signals *below* one LSB (their information
survives in the noise's statistics). It's what every real converter does.

**TPDF dither** (triangular probability density), **±1 LSB peak**, is the standard: form it as the
**sum of two independent uniform** draws each over `±½ LSB`. That triangular shape is the minimum
that makes the first *two* moments of the error (mean and variance) signal-independent — full
decorrelation. It's **non-subtractive** (added before quantizing, never removed), as in real
hardware.

```
   q = round( (x + tpdf) / Δ ) · Δ          tpdf = (u₁ − ½ + u₂ − ½)·Δ,  u ∼ Uniform[0,1)
```

**The cost:** total noise variance = quantizer `Δ²/12` + dither `Δ²/6` = `Δ²/4` ⇒ RMS `Δ/2`, so
the dithered full-scale-sine SNR is `≈ 6.02·N − 3.0 dB` — about 3 dB worse than undithered, a
trade gladly made for clean low-level behavior. The dither is drawn from the **seeded** per-node
RNG, so a render is reproducible. See `AdConverter::process`.

## 16. Group delay & latency

A filter doesn't only change *how loud* each frequency is — it also **delays the signal in time**.
**Group delay** is the name for how long. It's the *time view* of a filter (§5), and it's where
filter behavior becomes **latency**.

**Why an FIR delays.** A FIR output is a weighted sum of the last `L` input samples (the taps). Our
converter taps are **symmetric** — a hump, biggest in the middle, tapering to both ends — so the
kernel's "center of mass" sits in the **middle** of the window. The output is therefore a smoothed
copy of the input *as it was at the middle sample*, i.e. shifted later in time by **half the window**:

```
   group delay = (L − 1) / 2   samples            (symmetric / linear-phase FIR)
```

Concretely: feed a single spike into a symmetric `L`-tap filter and the output's peak lands
`(L−1)/2` samples after the spike. For our 161-tap converters that's exactly **80 samples**.

**Linear phase = one number.** "Symmetric taps" has a formal name, **linear phase**, and its payoff
is that **every frequency is delayed by the same amount**. So the waveform isn't smeared — it just
arrives late, whole — and the delay collapses to a *single* number `(L−1)/2`, not a per-frequency
curve. This is a reason the converters use FIRs. An **IIR** filter (the cable one-pole §6, the biquad
§7) is *not* linear-phase: its group delay **varies with frequency**, so it has no single delay
figure and it does smear transients slightly. (Feed-forward FIRs can be linear-phase; feedback IIRs
can't — the tradeoff for the IIR's cheapness and steepness-per-order.)

**Samples → time.** Group delay comes out in samples; divide by the sample rate for time:

```
   delay (seconds) = delay (samples) / sample_rate
```

**Our chain (Story 3.4).** Three matched linear-phase FIRs sit in series, each 161 taps → 80 samples,
all at the **384 kHz** analog rate:

| filter | role | delay |
| --- | --- | --- |
| AD decimator | analog volts → 48 kHz digital | 80 |
| DA interpolator | 48 kHz digital → analog volts | 80 |
| capture decimator | speaker volts → host samples | 80 |

```
   80 × 3 = 240 samples  →  240 / 384 000 Hz = 0.625 ms
```

That **0.625 ms** is the engine's fixed *signal-path* latency. It's small but real, and it **grows
as chains get longer** — so we measure it honestly rather than ignore it. (A decimator's taps tick at
its *input* rate, an interpolator's at its *output* rate; in this chain those are all 384 kHz, so the
three delays share one unit and simply add.)

**The latency budget.** Playing a note → hearing it sums three independent parts: the browser's output
buffer (the dominant chunk, measured live), the **note-stamping quantum** (a played note fires at the
next engine block, ≤ ~2.7 ms — the input-side granularity), and this **signal-path group delay**
(~0.6 ms). The page reports all three. See `RtEngine::signal_path_latency_ms` and `Decimator::group_delay`.
