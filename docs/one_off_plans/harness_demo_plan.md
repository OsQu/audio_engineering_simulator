# Harness visualization demo — plan (detour)

A **detour** after Story 1.3 (the engine is now runnable, headless). Goal: a small, _tangible_
demo that drives a sine through a **real compiled schedule** and renders the voltage in the
terminal — so you can _see_ amplitude (gain + loading), **rail clipping**, and **cable treble
rolloff** instead of only asserting them in unit tests. First taste of "real-time interactive is
the north star." Lives in the `harness` crate (its placeholder purpose: the render/CLI crate).

> This is throwaway-ish scaffolding that predates Epic 2's real WAV render driver and Story 1.7's
> oscillator — keep it small; it may be superseded/refactored then.

## Decisions (settled with Oskari)

- **Plot with `textplots`** (terminal Braille line plots). **Harness-only** dependency — `harness`
  is a native binary crate, so this does **not** touch the `engine` crate or its `wasm32` build.
  The engine stays dependency-clean.
- **Committed** in `crates/harness` (not scratch).
- **`SineSource` is a demo `Node` defined _in the harness_, not the engine** — the real oscillator
  is Story 1.7. Mark it clearly as demo scaffolding.
- Native-only (harness isn't compiled to wasm), so a plotting dep there is fine.

## Engine public API the harness consumes

All re-exported from `engine` (crate root). Signatures:

```rust
// graph building
Graph::new() -> Graph
Graph::add<N: Node + 'static>(&mut self, node: N) -> NodeId
Graph::connect(&mut self, from: NodeId, out_port: usize, to: NodeId, in_port: usize)
Graph::connect_cabled(&mut self, from: NodeId, out_port: usize, to: NodeId, in_port: usize, cable: Cable)
Graph::set_output(&mut self, node: NodeId, out_port: usize)

// compile + run
compile(graph: Graph, block_len: usize, rate: AnalogRate) -> Result<Schedule, CompileError>
Schedule::process(&mut self, out: &mut VoltageBuffer)   // fills `out`; hot path
Schedule::block_len(&self) -> usize
ScheduleSlot::{new(Schedule), process(&mut out), install(Box<Schedule>) -> Box<Schedule>, block_len()}

// nodes (engine-provided)
TestSource::new(level: Volts, z_out: Ohms)                                   // DC only
GainStage::new(gain: f32, rail: Volts, z_in: InputZ, z_out: Ohms)            // clamps at ±rail
PassiveSum::new(inputs: Vec<InputZ>, z_out: Ohms)                            // unity sum

// Node trait (to impl SineSource)
trait Node {
    fn inputs(&self) -> &[InputZ];
    fn outputs(&self) -> &[OutputZ];
    fn process(&mut self, inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]);
}

// values / types
Volts::new(f32) / .get() -> f32
Ohms::new(f32),  InputZ::new(Ohms),  OutputZ::new(Ohms)
AnalogRate::new(f64) / .as_hz() -> f64 / .seconds_per_sample() -> f64
Cable::new(r: Ohms, c: Farads),  Farads::new(f32)
VoltageBuffer::zeros(len: usize, rate: AnalogRate) / .as_slice() -> &[f32]
    / .as_mut_slice() -> &mut [f32] / .fill(Volts) / .rate() -> AnalogRate / .len()
```

## `SineSource` — demo node (define in harness)

- 0 inputs, 1 output with a real `Zout` (so loading still applies downstream).
- Fields: `amp: Volts`, `freq_hz: f64`, `outputs: [OutputZ; 1]`, `phase: f64` (radians).
- `process`: read `dt = outputs[0].rate().seconds_per_sample()` (the buffer carries the analog
  rate `compile` sized it with); fill `outputs[0]` with `amp·sin(phase)`, advancing
  `phase += 2π·freq_hz·dt` per sample. **Persist `phase` across blocks** (continuous tone); wrap
  it mod `2π` to keep the `f64` small.
- A quick `#[cfg(test)]` test: one block of a 1 kHz sine has the expected peak ≈ amp and ~the
  expected period.

## Scenarios (each a real compile → process)

Use a high analog rate, e.g. `AnalogRate::new(384_000.0)`. At 384 kHz a 1 kHz sine is 384
samples/cycle → plot ~2–3 cycles (~800–1200 samples; `textplots` downsamples to terminal width).

1. **Clean gain.** `SineSource(1.0 V, 1 kHz, Zout 100 Ω)` → `GainStage(×2, rail 10 V, Zin 10 kΩ,
Zout 150 Ω)`, tap the gain output. Edge gain = `10000/(100+10000) = 0.990099`. Expect a clean
   sine, peak ≈ `1.0 · 0.990099 · 2 = 1.980 V` (well under the 10 V rail). Plot input vs output.

2. **Clipping at the rail (the visual payoff).** Same chain, `SineSource` amp = **8 V**. Wanted
   peak = `8 · 0.990099 · 2 ≈ 15.84 V` > 10 V rail → output **flat-tops at ±10 V**. The plot shows
   the squared-off wave — clipping emerging from the rail, not a flag.

3. **Cable treble rolloff (frequency sweep).** `SineSource` → `connect_cabled(Cable(R, C))` →
   `GainStage`/load, tapped. Sweep `freq` (e.g. log-spaced 100 Hz … 30 kHz), measure
   `RMS(out)/RMS(in)` per frequency, plot **gain (dB) vs log-frequency**. You should see ~0 dB in
   the passband and a −3 dB corner at `f_c = 1/(2π·R_thev·C)`, `R_thev = (Zout + R_cable) ∥ Zin`,
   rolling off above. (Reuse the corner math from `electrical/cable.rs`; pick `R`,`C` for a corner
   around a few kHz so it's visible in-band.) Discard a settling transient before measuring RMS,
   like `test_util::measure_gain` does.

## Plotting with `textplots`

- Add to **`crates/harness/Cargo.toml`** `[dependencies]`: `textplots = "0.8"` (check latest;
  use context7/docs to confirm the API).
- Rough API: `Chart::new(width, height, x_min, x_max).lineplot(&Shape::Lines(&points)).display();`
  where `points: &[(f32, f32)]` = `(x, y)`. For waveforms `x` = time or sample index, `y` = volts;
  for the sweep `x` = log10(freq), `y` = gain dB.
- Build the `Vec<(f32, f32)>` by `enumerate()`-ing the output slice.

## Task breakdown (one-by-one; green gate incl. `cargo fmt`; stop after each for Oskari to commit)

1. **textplots dep + `SineSource`** demo node (+ its unit test).
2. **Scenarios 1 & 2** — binary builds the chain, processes a block, plots input vs output;
   shows clean then clipped (flat-tops). `cargo run -p harness`.
3. **Scenario 3** — frequency sweep + gain-vs-freq plot.
4. _(optional)_ `--csv` flag to dump samples for external plotting (gnuplot/Python).

**Validate:** `cargo run -p harness` renders the plots; the clipped wave visibly flat-tops; the
sweep shows the rolloff. (No new numeric oracle needed — the unit tests already prove the physics;
this is for the eyes.)

## Workflow reminders (for the executing session)

- **Task-by-task.** After each task, run the full gate —
  `cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs` (cargo aliases exist)
  — then **STOP**; Oskari verifies and commits himself, then says continue. Review his commit
  message. See memories `run-fmt-before-handoff` and `dev-workflow`.
- **Teach inline.** New Rust here: defining a **binary crate** (`main`), implementing an
  **external trait** (`engine::Node`) in a _consumer_ crate, pulling in a **dependency**, and
  building `&[(f32,f32)]` point data. Explain on the lines and append to `docs/rust_concepts.md`.
- `cargo` prefix note: this shell needs `source "$HOME/.cargo/env" &&` before cargo.
- `harness/src/main.rs` is currently a placeholder — replace it.
- Keep the engine dep-clean: `textplots` goes **only** in `harness`.

```

```
