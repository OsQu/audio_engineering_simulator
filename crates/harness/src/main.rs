//! Render/CLI test harness for driving the engine offline.
//!
//! This is the visualization *demo* (a detour after Story 1.3): it drives a sine through a
//! real compiled schedule and plots the resulting voltage in the terminal, so amplitude,
//! rail clipping, cable rolloff, and the device noise floor can be *seen*, not just asserted
//! in unit tests. The WAV render driver and offline scenarios proper arrive in Epic 2.

mod sine;

use engine::{
    AnalogRate, Cable, Decimator, Farads, GainStage, Graph, InputZ, NoiseDensity, Ohms, TestSource,
    VoltageBuffer, Volts, compile, kaiser_beta, volts_to_dbu,
};
use sine::SineSource;
use textplots::{Chart, Plot, Shape};

/// Tone frequency for the waveform scenarios.
const FREQ_HZ: f64 = 1_000.0;
/// Samples per `process` block.
const BLOCK_LEN: usize = 384;
/// How many blocks the waveform scenarios render — looping `process` proves the tone is
/// continuous across block boundaries (the `SineSource` carries its phase over), and gives
/// ~3 cycles to plot.
const BLOCKS: usize = 3;

/// A high analog rate: 384 kHz ⇒ 384 samples per cycle of a 1 kHz tone.
fn rate() -> AnalogRate {
    AnalogRate::new(384_000.0)
}

fn main() {
    scenario_clean_gain();
    scenario_clipping();
    scenario_cable_rolloff();
    scenario_noise_floor();
    scenario_fir_antialias();
}

/// Scenario 1 — clean gain. `SineSource(1 V) → GainStage(×2)`, well under the 10 V rail.
///
/// The connection loads the 100 Ω source against the stage's 10 kΩ input, an edge gain of
/// `10000 / (100 + 10000) = 0.990099`, so the output peak is `1.0 · 0.990099 · 2 = 1.980 V`.
fn scenario_clean_gain() {
    println!("\n=== Scenario 1: clean gain (×2, well under the 10 V rail) ===");
    let input = render(source_only(Volts::new(1.0), FREQ_HZ), BLOCKS);
    let output = render(chain(Volts::new(1.0)), BLOCKS);
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
    let input = render(source_only(Volts::new(8.0), FREQ_HZ), BLOCKS);
    let output = render(chain(Volts::new(8.0)), BLOCKS);
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

// --- Scenario 3: cable treble rolloff -------------------------------------------------------
//
// A moderately high source impedance into a high-Z load, so the resistive divider is ≈unity
// (a ~0 dB passband) and the cable's shunt capacitance — not loading loss — is what curves the
// response. The R·C is picked so the −3 dB corner lands mid-sweep, clearly in-band.

/// Source output impedance for the sweep.
const SWEEP_SRC_ZOUT: f32 = 2_200.0;
/// Load input impedance for the sweep (high-Z ⇒ negligible resistive divider loss).
const SWEEP_LOAD_ZIN: f32 = 1_000_000.0;
/// Sweep range and resolution.
const SWEEP_FMIN: f64 = 100.0;
const SWEEP_FMAX: f64 = 30_000.0;
const SWEEP_POINTS: usize = 56;
/// Periods rendered per frequency; the first half is discarded as the filter-settling
/// transient and the steady second half is RMS-measured (mirrors `engine`'s `measure_gain`).
const SWEEP_PERIODS: usize = 128;

/// The demo cable: series R + shunt C, sized for a treble corner around a few kHz.
fn sweep_cable() -> Cable {
    Cable::new(Ohms::new(100.0), Farads::new(22e-9))
}

/// Scenario 3 — cable treble rolloff. `SineSource → cable → high-Z load`, swept in frequency.
///
/// Measures `RMS(out)/RMS(in)` per frequency and plots it in dB against `log10(freq)`. The
/// shunt capacitance forms a one-pole low-pass with the Thévenin resistance it sees,
/// `R_thev = (Zout + R_cable) ∥ Zin`, cornering at `f_c = 1/(2π·R_thev·C)`: flat (~0 dB) below,
/// rolling off ~−6 dB/octave above, −3 dB at `f_c`.
fn scenario_cable_rolloff() {
    println!("\n=== Scenario 3: cable treble rolloff (frequency sweep) ===");

    // Sweep: log-spaced frequencies, the observed gain (dB) at each. Everything here comes out
    // of the compiled schedule — we drive a sine in and measure what comes out; no analytic
    // expectation is computed. (The engine's unit tests already prove the RC corner; this is
    // for the eyes.)
    let mut curve = Vec::with_capacity(SWEEP_POINTS);
    for i in 0..SWEEP_POINTS {
        let frac = i as f64 / (SWEEP_POINTS - 1) as f64;
        let freq = SWEEP_FMIN * (SWEEP_FMAX / SWEEP_FMIN).powf(frac);

        // Enough whole blocks to cover SWEEP_PERIODS cycles of this frequency.
        let total = (rate().as_hz() / freq * SWEEP_PERIODS as f64).ceil() as usize;
        let blocks = total.div_ceil(BLOCK_LEN);
        let out = render(chain_cabled(Volts::new(1.0), freq), blocks);
        let inp = render(source_only(Volts::new(1.0), freq), blocks);
        let half = out.len() / 2; // discard the settling transient
        let gain = rms(&out[half..]) / rms(&inp[half..]);
        let db = 20.0 * f64::from(gain).log10();
        curve.push((freq, db));
    }

    // Passband and corner are read off the *measured* curve, not computed. The passband is the
    // gain at the lowest swept frequency (far below the corner); the −3 dB corner is 3.01 dB
    // below that, and we find where the curve crosses it.
    let passband_db = curve[0].1;
    let corner_db = passband_db - 3.0103;
    let measured_corner = measured_crossover(&curve, corner_db);
    println!(
        "passband {passband_db:.3} dB (observed at {:.0} Hz)",
        curve[0].0
    );
    match measured_corner {
        Some(f) => println!("measured −3 dB corner ≈ {f:.0} Hz"),
        None => println!("(−3 dB crossover not found within the swept range)"),
    }

    let points: Vec<(f32, f32)> = curve
        .iter()
        .map(|&(f, db)| (f.log10() as f32, db as f32))
        .collect();
    plot_response(
        "Gain (dB, y) vs log10(frequency) (x: 2=100 Hz · 3=1 kHz · 4=10 kHz) — flat line is −3 dB",
        &points,
        corner_db as f32,
        SWEEP_FMIN.log10() as f32,
        SWEEP_FMAX.log10() as f32,
    );
}

// --- Scenario 4: noise floor revealed by gain ----------------------------------------------
//
// Feed *silence* into a preamp that has an input-referred noise floor, then crank the gain.
// With 0 V in, the output is just the amplified floor: out = (0 + n)·gain = n·gain. So the
// hiss that's invisible at unity gain rises 1:1 with the gain you dial in — the lesson that
// your noise floor (and SNR) is set at the first gain stage, the preamp. The noise stream is
// seeded, so every gain replays the *same* hiss, only louder.

/// Input-referred noise density of the demo preamp: 10 nV/√Hz (a modest device floor; matches
/// the engine's noise tests). At 384 kHz this is σ = D·√(fs/2) ≈ 4.38 µV RMS at the input.
const NOISE_DENSITY: f32 = 10e-9;
/// Gains to dial through, in dB — each 20 dB (×10) hotter than the last, so the floor should
/// climb a clean 20 dB per step.
const NOISE_GAINS_DB: [f32; 3] = [20.0, 40.0, 60.0];
/// Samples rendered for the floor — more than the waveform scenarios so the RMS estimate of a
/// random signal settles. 8 × 384 = 3072 samples.
const NOISE_BLOCKS: usize = 8;

/// Scenario 4 — the device noise floor, made visible by gain. Silence into a noisy preamp,
/// rendered at a few gains.
///
/// The output is the preamp's own input-referred floor times the gain, so the measured output
/// RMS should track `gain · σ` (and rise 1:1 in dB with the gain). All three gains are plotted
/// on one shared scale set by the loudest, so the hiss visibly lifts off the zero line.
fn scenario_noise_floor() {
    println!("\n=== Scenario 4: noise floor revealed by gain (silence in, crank the preamp) ===");

    // The input-referred floor in volts: σ = D·√(fs/2). With silence in, the output RMS noise
    // is just gain·σ — there's no signal for it to ride on.
    let sigma_in = NoiseDensity::new(NOISE_DENSITY).per_sample_sigma(rate());
    println!(
        "input-referred floor σ = {:.3} µV RMS ({:.1} dBu) at {:.0} nV/√Hz, {:.0} kHz",
        sigma_in * 1e6,
        volts_to_dbu(Volts::new(sigma_in)),
        NOISE_DENSITY * 1e9,
        rate().as_hz() / 1000.0,
    );

    let mut waveforms = Vec::with_capacity(NOISE_GAINS_DB.len());
    for &db in &NOISE_GAINS_DB {
        let gain = 10f32.powf(db / 20.0);
        let out = render(noisy_preamp(gain), NOISE_BLOCKS);
        let measured = rms(&out);
        let expected = gain * sigma_in;
        println!(
            "  +{db:>2.0} dB (×{gain:<5.0}): floor {:>8.1} µV RMS measured ({:>6.1} dBu) — expected {:>8.1} µV",
            measured * 1e6,
            volts_to_dbu(Volts::new(measured)),
            expected * 1e6,
        );
        waveforms.push((db, out));
    }

    // One shared vertical scale, set by the loudest render (+10 % headroom), so the lower gains
    // read as near-flat lines and the top gain fills the band — the floor lifting as you crank.
    let top_peak = waveforms.last().map_or(1.0, |(_, w)| peak(w));
    let scale = Some((-top_peak * 1.1, top_peak * 1.1));
    for (db, w) in &waveforms {
        plot(
            &format!("Output floor at +{db:.0} dB gain (volts; shared scale set by the loudest)"),
            w,
            scale,
        );
    }
}

// --- Scenario 5: the FIR anti-alias filter (decimation 384 kHz → 48 kHz) -------------------
//
// Before dropping to 48 kHz the converter must remove everything above the 24 kHz decimated
// Nyquist, or it folds back (aliases). This sweeps an input tone from the passband up into the
// stopband and plots how much survives decimation, for a STRONG (many-tap) filter and a WEAK
// (few-tap) one: the strong filter cliffs at Nyquist, the weak one leaks — and that leak *is*
// audible aliasing. Then it shows a single out-of-band tone in the time domain: rejected by the
// strong filter, folded back to an 8 kHz wave by the weak one. The FIR is a standalone primitive
// here (the AD that wraps it is Story 1.6.3); we drive `Decimator` directly.

/// Decimation factor for the demo: 384 kHz → 48 kHz.
const M_FIR: usize = 8;
/// Tap counts for the strong (steep) and weak (leaky) filters — the "weak filter" knob.
const FIR_TAPS_STRONG: usize = 161;
const FIR_TAPS_WEAK: usize = 13;
/// Frequency-sweep range and resolution.
const FIR_FMIN: f64 = 1_000.0;
const FIR_FMAX: f64 = 90_000.0;
const FIR_POINTS: usize = 56;
/// High-rate samples rendered per swept frequency (a multiple of `M_FIR`).
const FIR_SWEEP_LEN: usize = 16_384;

fn scenario_fir_antialias() {
    println!("\n=== Scenario 5: FIR anti-alias filter, decimating 384 kHz → 48 kHz ===");
    let beta = kaiser_beta(80.0);
    let lo_nyquist = rate().as_hz() / (2.0 * M_FIR as f64); // 24 kHz

    // Sweep: log-spaced input frequencies, the level (dB, ref input) surviving decimation through
    // each filter. Everything is measured off the real decimator output — no analytic curve.
    let mut strong = Vec::with_capacity(FIR_POINTS);
    let mut weak = Vec::with_capacity(FIR_POINTS);
    for i in 0..FIR_POINTS {
        let frac = i as f64 / (FIR_POINTS - 1) as f64;
        let freq = FIR_FMIN * (FIR_FMAX / FIR_FMIN).powf(frac);
        let input = tone(freq, 1.0, FIR_SWEEP_LEN);
        let in_rms = rms(&input);
        let x = freq.log10() as f32;
        strong.push((
            x,
            gain_db(&mut aa_filter(FIR_TAPS_STRONG, beta), &input, in_rms),
        ));
        weak.push((
            x,
            gain_db(&mut aa_filter(FIR_TAPS_WEAK, beta), &input, in_rms),
        ));
    }

    println!("  level surviving decimation (dB, ref input):");
    for &probe in &[4_000.0_f64, 40_000.0] {
        let input = tone(probe, 1.0, FIR_SWEEP_LEN);
        let in_rms = rms(&input);
        let s = gain_db(&mut aa_filter(FIR_TAPS_STRONG, beta), &input, in_rms);
        let w = gain_db(&mut aa_filter(FIR_TAPS_WEAK, beta), &input, in_rms);
        let band = if probe < lo_nyquist {
            "passband"
        } else {
            "stopband"
        };
        println!("    {probe:>6.0} Hz ({band}):  strong {s:>7.1} dB    weak {w:>7.1} dB");
    }

    plot_fir(
        "Surviving level (dB, y) vs log10(freq) — strong (161 taps) vs weak (13 taps); vertical = 24 kHz Nyquist",
        &strong,
        &weak,
        lo_nyquist.log10() as f32,
        FIR_FMIN.log10() as f32,
        FIR_FMAX.log10() as f32,
    );

    // Time domain: one 40 kHz tone (above Nyquist) decimated. Strong → ≈ silence; weak → it folds
    // back to a 48 − 40 = 8 kHz alias. We plot a short **window** of the settled output — the 8 kHz
    // alias is only 6 samples/cycle at 48 kHz, so showing the whole tail (~170 cycles) would smear
    // into a solid band; ~8 cycles resolves the wave. Shared ±1.1 scale.
    let probe = tone(40_000.0, 1.0, FIR_SWEEP_LEN);
    let strong_out = decimate(&mut aa_filter(FIR_TAPS_STRONG, beta), &probe);
    let weak_out = decimate(&mut aa_filter(FIR_TAPS_WEAK, beta), &probe);
    let start = strong_out.len() / 2; // past the filter-settling transient
    let win = 48; // ~8 cycles of the 8 kHz alias at 48 kHz (6 samples/cycle)
    let scale = Some((-1.1, 1.1));
    println!("\n  a 40 kHz tone (above Nyquist) after decimation — 48-sample (~1 ms) window:");
    plot(
        "Strong filter — out-of-band tone rejected (≈ silence)",
        &strong_out[start..start + win],
        scale,
    );
    plot(
        "Weak filter — folds back to an 8 kHz alias (≈8 cycles shown)",
        &weak_out[start..start + win],
        scale,
    );
}

/// An anti-alias decimator with `num_taps` taps at the demo's 8× factor.
fn aa_filter(num_taps: usize, beta: f64) -> Decimator {
    Decimator::lowpass(num_taps, M_FIR, beta)
}

/// A high-rate sine of `len` samples at `freq` Hz, peak `amp`. The FIR works on raw `f32`
/// (it's domain-agnostic numbers), so this bypasses the graph.
fn tone(freq: f64, amp: f32, len: usize) -> Vec<f32> {
    let dt = rate().seconds_per_sample();
    let omega = std::f64::consts::TAU * freq;
    (0..len)
        .map(|n| (f64::from(amp) * (omega * n as f64 * dt).sin()) as f32)
        .collect()
}

/// Decimate `input` into a fresh `Vec` (off the hot path; allocation is fine).
fn decimate(dec: &mut Decimator, input: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0; input.len() / dec.factor()];
    dec.process(input, &mut out);
    out
}

/// Level (dB) surviving decimation through `dec`, relative to the input RMS, measured on the
/// steady tail (the first half is dropped as the filter transient). Floored so a fully rejected
/// tone plots as a finite −120 dB rather than −∞.
fn gain_db(dec: &mut Decimator, input: &[f32], in_rms: f32) -> f32 {
    let out = decimate(dec, input);
    let tail = &out[out.len() / 2..];
    let g = rms(tail) / in_rms;
    if g <= 1e-6 { -120.0 } else { 20.0 * g.log10() }
}

// --- graph builders -------------------------------------------------------------------------

/// A graph of just the source, tapped — the open-circuit input signal, run through the same
/// engine path as the output for an honest comparison.
fn source_only(amp: Volts, freq: f64) -> Graph {
    let mut g = Graph::new();
    let src = g.add(SineSource::new(amp, freq, Ohms::new(100.0)));
    g.set_output(src, 0);
    g
}

/// The full `SineSource → GainStage` chain, tapped at the gain output (scenarios 1 & 2).
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

/// `SineSource → cable → unity buffered load`, tapped at the load (scenario 3). The load is a
/// ×1 `GainStage` with a high rail (never clips) presenting a high-Z input; tapping it gives
/// the divided-and-filtered voltage the cable delivers.
fn chain_cabled(amp: Volts, freq: f64) -> Graph {
    let mut g = Graph::new();
    let src = g.add(SineSource::new(amp, freq, Ohms::new(SWEEP_SRC_ZOUT)));
    let load = g.add(GainStage::new(
        1.0,
        Volts::new(1_000.0),
        InputZ::new(Ohms::new(SWEEP_LOAD_ZIN)),
        Ohms::new(150.0),
    ));
    g.connect_cabled(src, 0, load, 0, sweep_cable());
    g.set_output(load, 0);
    g
}

/// A silent source into a noisy preamp at linear voltage gain `gain`, tapped at the preamp
/// output (scenario 4). [`TestSource`] emits 0 V (DC silence) from a real 100 Ω face, so the
/// only thing on the wire is the preamp's own input-referred floor, amplified. The rail is far
/// above any gain here, so the floor never clips.
fn noisy_preamp(gain: f32) -> Graph {
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(100.0)));
    let pre = g.add(
        GainStage::new(
            gain,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
        .with_noise(NoiseDensity::new(NOISE_DENSITY)),
    );
    g.connect(src, 0, pre, 0);
    g.set_output(pre, 0);
    g
}

// --- rendering & measurement ----------------------------------------------------------------

/// Compile a graph and render `blocks` blocks into one contiguous waveform. Off the hot path,
/// so allocating the result `Vec` and `expect`-ing the compile are fine here.
fn render(graph: Graph, blocks: usize) -> Vec<f32> {
    let mut schedule = compile(graph, BLOCK_LEN, rate(), 0).expect("a valid chain should compile");
    let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
    let mut samples = Vec::with_capacity(BLOCK_LEN * blocks);
    for _ in 0..blocks {
        schedule.process(&mut out);
        samples.extend_from_slice(out.as_slice());
    }
    samples
}

/// Peak (max absolute) voltage over a waveform.
fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, &v| m.max(v.abs()))
}

/// Root-mean-square of a slice (f64 accumulation; empty ⇒ 0). The harness's own copy — the
/// engine's `rms` is test-only and not part of its public API.
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

/// Frequency (Hz) where a falling dB response first crosses `target_db`, by linear
/// interpolation in log-frequency between the two straddling points. `None` if it never does.
fn measured_crossover(curve: &[(f64, f64)], target_db: f64) -> Option<f64> {
    for pair in curve.windows(2) {
        let (f0, db0) = pair[0];
        let (f1, db1) = pair[1];
        if db0 >= target_db && db1 < target_db {
            // Interpolate the crossing in (log10 f, dB) space, then map back to Hz.
            let (l0, l1) = (f0.log10(), f1.log10());
            let t = (db0 - target_db) / (db0 - db1);
            return Some(10f64.powf(l0 + t * (l1 - l0)));
        }
    }
    None
}

// --- plotting -------------------------------------------------------------------------------

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

/// Plot a frequency response (`curve` already in (log10 Hz, dB)) with a horizontal reference
/// line at `corner_db`, on a fixed −24…+3 dB scale.
fn plot_response(title: &str, curve: &[(f32, f32)], corner_db: f32, x_min: f32, x_max: f32) {
    println!("\n{title}");
    let reference = [(x_min, corner_db), (x_max, corner_db)];
    let curve_shape = Shape::Lines(curve);
    let ref_shape = Shape::Lines(&reference);
    Chart::new_with_y_range(120, 60, x_min, x_max, -24.0, 3.0)
        .lineplot(&curve_shape)
        .lineplot(&ref_shape)
        .nice();
}

/// Plot two FIR responses (each in (log10 Hz, dB)) on a fixed −72…+3 dB scale, with a vertical
/// marker at the decimated Nyquist: first curve = strong filter, second = weak, third = marker.
fn plot_fir(
    title: &str,
    strong: &[(f32, f32)],
    weak: &[(f32, f32)],
    nyquist_log: f32,
    x_min: f32,
    x_max: f32,
) {
    println!("\n{title}");
    let marker = [(nyquist_log, -72.0), (nyquist_log, 3.0)];
    Chart::new_with_y_range(120, 60, x_min, x_max, -72.0, 3.0)
        .lineplot(&Shape::Lines(strong))
        .lineplot(&Shape::Lines(weak))
        .lineplot(&Shape::Lines(&marker))
        .nice();
}
