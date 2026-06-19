//! Render/CLI test harness for driving the engine offline.
//!
//! This is the visualization *demo* (a detour after Story 1.3): it drives a sine through a
//! real compiled schedule and plots the resulting voltage in the terminal, so amplitude,
//! rail clipping, and cable rolloff can be *seen*, not just asserted in unit tests. The WAV
//! render driver and offline scenarios proper arrive in Epic 2.

mod sine;

use engine::{AnalogRate, GainStage, Graph, InputZ, Ohms, VoltageBuffer, Volts, compile};
use sine::SineSource;
use textplots::{Chart, Plot, Shape};

/// Tone frequency for the waveform scenarios.
const FREQ_HZ: f64 = 1_000.0;
/// Samples per `process` block.
const BLOCK_LEN: usize = 384;
/// How many blocks to render — looping `process` proves the tone is continuous across block
/// boundaries (the `SineSource` carries its phase over), and gives ~3 cycles to plot.
const BLOCKS: usize = 3;

/// A high analog rate: 384 kHz ⇒ 384 samples per cycle of a 1 kHz tone.
fn rate() -> AnalogRate {
    AnalogRate::new(384_000.0)
}

fn main() {
    scenario_clean_gain();
    scenario_clipping();
}

/// Scenario 1 — clean gain. `SineSource(1 V) → GainStage(×2)`, well under the 10 V rail.
///
/// The connection loads the 100 Ω source against the stage's 10 kΩ input, an edge gain of
/// `10000 / (100 + 10000) = 0.990099`, so the output peak is `1.0 · 0.990099 · 2 = 1.980 V`.
fn scenario_clean_gain() {
    println!("\n=== Scenario 1: clean gain (×2, well under the 10 V rail) ===");
    let input = render(source_only(Volts::new(1.0)));
    let output = render(chain(Volts::new(1.0)));
    println!(
        "input peak  {:.3} V (open-circuit source)\noutput peak {:.3} V (expected 1.980 = 1.0 · 0.990099 · 2)",
        peak(&input),
        peak(&output),
    );
    plot("Input — SineSource open-circuit, 1 V peak", &input, None);
    plot(
        "Output — ×2 after loading, a clean ~1.98 V sine",
        &output,
        None,
    );
}

/// Scenario 2 — clipping at the rail (the visual payoff). Same chain, source amp **8 V**.
///
/// Wanted peak `8 · 0.990099 · 2 ≈ 15.84 V` exceeds the 10 V rail, so the output **flat-tops
/// at ±10 V** — clipping that emerges from the rail in volts, not from a flag. Both charts
/// share a fixed ±11 V scale so the squared-off top sits visibly below the input's round crest.
fn scenario_clipping() {
    println!("\n=== Scenario 2: clipping at the ±10 V rail ===");
    let input = render(source_only(Volts::new(8.0)));
    let output = render(chain(Volts::new(8.0)));
    println!(
        "input peak  {:.3} V (8 V sine)\noutput peak {:.3} V (clamped to the 10 V rail; wanted ≈15.84 V)",
        peak(&input),
        peak(&output),
    );
    let scale = Some((-11.0, 11.0));
    plot("Input — 8 V sine (round crest)", &input, scale);
    plot(
        "Output — wanted ~15.8 V, flat-topped at ±10 V",
        &output,
        scale,
    );
}

/// A graph of just the source, tapped — the open-circuit input signal, run through the same
/// engine path as the output for an honest comparison.
fn source_only(amp: Volts) -> Graph {
    let mut g = Graph::new();
    let src = g.add(SineSource::new(amp, FREQ_HZ, Ohms::new(100.0)));
    g.set_output(src, 0);
    g
}

/// The full `SineSource → GainStage` chain, tapped at the gain output.
fn chain(amp: Volts) -> Graph {
    let mut g = Graph::new();
    let src = g.add(SineSource::new(amp, FREQ_HZ, Ohms::new(100.0)));
    let stage = g.add(GainStage::new(
        2.0,
        Volts::new(10.0),
        InputZ::new(Ohms::new(10_000.0)),
        Ohms::new(150.0),
    ));
    g.connect(src, 0, stage, 0);
    g.set_output(stage, 0);
    g
}

/// Compile a graph and render `BLOCKS` blocks into one contiguous waveform. Off the hot path,
/// so allocating the result `Vec` and `expect`-ing the compile are fine here.
fn render(graph: Graph) -> Vec<f32> {
    let mut schedule = compile(graph, BLOCK_LEN, rate()).expect("a valid chain should compile");
    let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
    let mut samples = Vec::with_capacity(BLOCK_LEN * BLOCKS);
    for _ in 0..BLOCKS {
        schedule.process(&mut out);
        samples.extend_from_slice(out.as_slice());
    }
    samples
}

/// Peak (max absolute) voltage over a waveform.
fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, &v| m.max(v.abs()))
}

/// Plot a waveform: x = sample index, y = volts. `y_range` fixes the vertical scale (for
/// comparing two charts on one scale); `None` auto-scales to the data.
fn plot(title: &str, samples: &[f32], y_range: Option<(f32, f32)>) {
    println!("\n{title}");
    // textplots takes (x, y) pairs; x is the sample index across all rendered blocks.
    let points: Vec<(f32, f32)> = samples
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f32, v))
        .collect();
    let shape = Shape::Lines(&points);
    let xmax = samples.len() as f32;
    let mut chart = match y_range {
        Some((lo, hi)) => Chart::new_with_y_range(120, 60, 0.0, xmax, lo, hi),
        None => Chart::new(120, 60, 0.0, xmax),
    };
    chart.lineplot(&shape).nice();
}
