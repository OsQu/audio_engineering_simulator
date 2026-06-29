//! Render/CLI test harness for driving the engine offline.
//!
//! This is the visualization *demo* (a detour after Story 1.3): it drives a sine through a
//! real compiled schedule and plots the resulting voltage in the terminal, so amplitude,
//! rail clipping, cable rolloff, and the device noise floor can be *seen*, not just asserted
//! in unit tests. The WAV render driver and offline scenarios proper arrive in Epic 2.

mod sine;

use engine::{
    AdConverter, AnalogRate, BitDepth, Cable, ClockDomainId, Compressor, DaConverter, Decimator,
    EqBand, EventMessage, EventQueue, Farads, GainStage, Graph, InputZ, Lane, Node, NodeId,
    NoiseDensity, Ohms, Params, SampleBuffer, SampleRate, Speaker, SynthVoice, TestSource,
    ThreeBandEq, VoltageBuffer, Volts, compile, kaiser_beta, volts_to_dbu,
};
use harness::render::{RenderConfig, render_to_samples};
use harness::wav;
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
    scenario_saw_across_domains();
    scenario_first_sound();
    scenario_first_sound_analog();
    scenario_first_sound_eq();
    scenario_first_sound_compressed();
}

// --- Scenario 7: first sound — a played note rendered to a WAV -----------------------------
//
// The Epic-2 milestone: the whole journey made audible. A4 is played into the synth voice and
// rendered end to end through `synth → modeled AD → modeled DA → speaker`, then the speaker's
// tapped voltage is captured (off-sim-clock) to 48 kHz host samples and written to a float32 WAV
// you can actually listen to. Unlike the other scenarios this writes a file rather than plotting.

/// MIDI note for the first-sound render: A4 = 440 Hz.
const FS_NOTE: u8 = 69;
/// Host (WAV) sample rate — 48 kHz, an integer eighth of the 384 kHz analog rate.
const FS_HOST_RATE: f64 = 48_000.0;
/// Converter / monitor full-scale reference, in volts. The voice peaks around 0.7 V, so a 1 V
/// reference renders it a few dB below full scale — hot but unclipped.
const FS_REFERENCE_V: f32 = 1.0;
/// Render length and where the note releases (so the envelope's release is audible in the tail).
const FS_SECONDS: f64 = 1.0;
const FS_NOTE_OFF_S: f64 = 0.75;
/// Output paths (under the gitignored `renders/`, relative to the invocation directory): the full
/// chain through the modeled converters, and a pure-analog comparison straight to the speaker.
const FS_OUT_PATH: &str = "renders/first_sound.wav";
const FS_ANALOG_OUT_PATH: &str = "renders/first_sound_analog.wav";
/// The DSP scenarios: the same note through a digital EQ, and through a compressor — each inserted
/// between the modeled AD and DA (the "plugins in the DAW" position).
const FS_EQ_OUT_PATH: &str = "renders/first_sound_eq.wav";
const FS_COMPRESSED_OUT_PATH: &str = "renders/first_sound_compressed.wav";

/// `synth → AD → DA → speaker`, tapped at the speaker. Returns the graph and the voice node (its
/// event input is where the note is played).
fn first_sound_graph() -> (Graph, NodeId) {
    let host_rate = SampleRate::new(FS_HOST_RATE);
    let mut g = Graph::new();
    let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
    let ad = g.add(AdConverter::new(
        host_rate,
        BitDepth::new(16),
        Volts::new(FS_REFERENCE_V),
        Ohms::new(1e6),
    ));
    let da = g.add(DaConverter::new(
        host_rate,
        BitDepth::new(16),
        Volts::new(FS_REFERENCE_V),
        Ohms::new(150.0),
    ));
    let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
    g.connect_ideal(voice, 0, ad, 0);
    g.connect_ideal(ad, 0, da, 0);
    g.connect_ideal(da, 0, spk, 0);
    g.set_output(spk, 0);
    (g, voice)
}

/// `synth → speaker` — the same voice straight to the speaker, **no modeled AD/DA**. The only
/// band-limiting is the transparent capture (to the 24 kHz host Nyquist), so this is the cleaner
/// reference: no 16-bit quantization, no converter group delay. Returns the graph and voice node.
fn first_sound_analog_graph() -> (Graph, NodeId) {
    let mut g = Graph::new();
    let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
    let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
    g.connect_ideal(voice, 0, spk, 0);
    g.set_output(spk, 0);
    (g, voice)
}

/// `synth → AD → 3-band EQ → DA → speaker`. The EQ runs in the **digital** domain between the
/// modeled converters: a +6 dB low shelf (warmth), a −6 dB mid scoop, and a +6 dB high shelf (air),
/// so the rendered note is audibly recoloured versus the flat [`first_sound_graph`]. Returns the
/// graph and the voice node.
fn first_sound_eq_graph() -> (Graph, NodeId) {
    let host_rate = SampleRate::new(FS_HOST_RATE);
    let bits = BitDepth::new(16);
    let mut g = Graph::new();
    // Half level into the converters so the EQ's +6 dB boosts have headroom and don't clip the
    // capture (the flat render already sits near full scale).
    let voice = g.add(SynthVoice::new(Volts::new(0.5), Ohms::new(1.0)));
    let ad = g.add(AdConverter::new(
        host_rate,
        bits,
        Volts::new(FS_REFERENCE_V),
        Ohms::new(1e6),
    ));
    let eq = g.add(ThreeBandEq::new(
        host_rate,
        bits,
        EqBand::new(150.0, 0.707, 6.0),   // low shelf: +6 dB warmth
        EqBand::new(800.0, 1.0, -6.0),    // mid peak: −6 dB scoop
        EqBand::new(6_000.0, 0.707, 6.0), // high shelf: +6 dB air
    ));
    let da = g.add(DaConverter::new(
        host_rate,
        bits,
        Volts::new(FS_REFERENCE_V),
        Ohms::new(150.0),
    ));
    let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
    g.connect_ideal(voice, 0, ad, 0);
    g.connect_ideal(ad, 0, eq, 0);
    g.connect_ideal(eq, 0, da, 0);
    g.connect_ideal(da, 0, spk, 0);
    g.set_output(spk, 0);
    (g, voice)
}

/// `synth → AD → compressor → DA → speaker`. The compressor runs in the **digital** domain: a low
/// threshold and a 4:1 ratio squash the note's attack and sustain, then +6 dB of manual makeup
/// brings the level back up — so the render is denser and more even than the flat one. Returns the
/// graph and the voice node.
fn first_sound_compressed_graph() -> (Graph, NodeId) {
    let host_rate = SampleRate::new(FS_HOST_RATE);
    let bits = BitDepth::new(16);
    let mut g = Graph::new();
    // Half level into the converters so the +6 dB makeup gain has headroom over the note's onset
    // transient (which the 5 ms attack hasn't yet clamped) and doesn't clip the capture.
    let voice = g.add(SynthVoice::new(Volts::new(0.5), Ohms::new(1.0)));
    let ad = g.add(AdConverter::new(
        host_rate,
        bits,
        Volts::new(FS_REFERENCE_V),
        Ohms::new(1e6),
    ));
    let comp = g.add(
        // threshold −18 dBFS, 4:1, 5 ms attack / 80 ms release, soft knee, +6 dB makeup.
        Compressor::new(host_rate, bits, -18.0, 4.0, 5.0, 80.0)
            .with_knee(6.0)
            .with_makeup(6.0),
    );
    let da = g.add(DaConverter::new(
        host_rate,
        bits,
        Volts::new(FS_REFERENCE_V),
        Ohms::new(150.0),
    ));
    let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
    g.connect_ideal(voice, 0, ad, 0);
    g.connect_ideal(ad, 0, comp, 0);
    g.connect_ideal(comp, 0, da, 0);
    g.connect_ideal(da, 0, spk, 0);
    g.set_output(spk, 0);
    (g, voice)
}

/// Compile `graph` (tapped at the speaker), play [`FS_NOTE`] from the start, release it at
/// [`FS_NOTE_OFF_S`], render [`FS_SECONDS`] of host audio, and write it to `path`. `voice` is the
/// node whose event input receives the note. Event timestamps are absolute analog samples.
fn render_note_to_wav(graph: Graph, voice: NodeId, path: &str) {
    let host_rate = SampleRate::new(FS_HOST_RATE);
    let mut schedule = compile(graph, BLOCK_LEN, rate(), 0).expect("first-sound chain compiles");
    let ev = schedule
        .event_input(voice, 0)
        .expect("the voice's event input");

    let mut events = EventQueue::with_capacity(4);
    events.push(
        0,
        ev,
        EventMessage::NoteOn {
            note: FS_NOTE,
            velocity: 100,
        },
    );
    events.push(
        (FS_NOTE_OFF_S * rate().as_hz()) as u64,
        ev,
        EventMessage::NoteOff { note: FS_NOTE },
    );

    let cfg = RenderConfig {
        host_rate,
        full_scale_volts: FS_REFERENCE_V,
        seconds: FS_SECONDS,
    };
    let samples = render_to_samples(&mut schedule, rate(), &mut events, &cfg);

    std::fs::create_dir_all("renders").expect("create the renders/ directory");
    wav::write_mono_f32(path, &samples, host_rate).expect("write the WAV");

    println!(
        "  rendered {} samples ({:.2} s @ {:.0} kHz, peak {:.3}) → {path}",
        samples.len(),
        FS_SECONDS,
        FS_HOST_RATE / 1000.0,
        peak(&samples),
    );
}

fn scenario_first_sound() {
    println!("\n=== Scenario 7: first sound — A4 through synth → AD → DA → speaker, to a WAV ===");
    let (g, voice) = first_sound_graph();
    render_note_to_wav(g, voice, FS_OUT_PATH);
}

fn scenario_first_sound_analog() {
    println!(
        "\n=== Scenario 8: first sound (analog only) — A4 through synth → speaker, to a WAV ==="
    );
    let (g, voice) = first_sound_analog_graph();
    render_note_to_wav(g, voice, FS_ANALOG_OUT_PATH);
}

fn scenario_first_sound_eq() {
    println!(
        "\n=== Scenario 9: 3-band EQ — A4 through synth → AD → EQ → DA → speaker, to a WAV ==="
    );
    let (g, voice) = first_sound_eq_graph();
    render_note_to_wav(g, voice, FS_EQ_OUT_PATH);
}

fn scenario_first_sound_compressed() {
    println!(
        "\n=== Scenario 10: compressor — A4 through synth → AD → compressor → DA → speaker, to a WAV ==="
    );
    let (g, voice) = first_sound_compressed_graph();
    render_note_to_wav(g, voice, FS_COMPRESSED_OUT_PATH);
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

// --- Scenario 6: a sawtooth across the AD/DA boundary --------------------------------------
//
// The synth voice's sawtooth, shown in three domains: the oversampled **analog source** (sharp —
// harmonics all the way up), the **digital samples after the AD** (sampled at 48 kHz and
// band-limited to the 24 kHz Nyquist, so the ultrasonic harmonics that would alias are gone), and
// the **analog reconstruction after the DA** (smooth volts again). The AD and DA are driven
// directly as nodes (like `Decimator` in scenario 5, and the only way to *see* the digital domain
// — a schedule's internal lanes are private); the voice is rendered through a real compiled
// schedule with a note played into its event lane.

/// MIDI note for the saw demo: A4 = 440 Hz (872 analog / 109 digital samples per cycle).
const SAW_NOTE: u8 = 69;
/// Decimation factor: 384 kHz → 48 kHz.
const SAW_M: usize = 8;
/// Converter full-scale reference, in volts. At 1 V the digital sample values read on the same
/// scale as the volts, so the three plots are directly comparable in magnitude.
const SAW_REF_V: f32 = 1.0;
/// Analog samples rendered — a multiple of `BLOCK_LEN` (and so of `SAW_M`), long enough to settle
/// well past the envelope attack/decay (~5760 samples) before the displayed window.
const SAW_LEN: usize = 32 * BLOCK_LEN; // 12288

fn scenario_saw_across_domains() {
    println!("\n=== Scenario 6: a sawtooth voice across the AD/DA boundary ===");
    let digital_rate = SampleRate::new(rate().as_hz() / SAW_M as f64); // 48 kHz

    // analog source → AD → digital → DA → analog reconstruction.
    let analog_src = render_voice(SAW_NOTE, SAW_LEN);
    let digital = ad_convert(&analog_src, digital_rate);
    let analog_recon = da_convert(&digital, digital_rate);

    // ~3 cycles from each signal's steady tail (440 Hz ⇒ 872 analog / 109 digital samples per
    // cycle), on a shared ±0.9 scale (a touch above the 0.7 V sawtooth so the band-limiting
    // overshoot stays visible). The AD+DA group delays shift the reconstruction by ~160 analog
    // samples vs the source — the shape matches, the phase is offset.
    let a_cyc = (rate().as_hz() / 440.0) as usize;
    let d_cyc = (digital_rate.as_hz() / 440.0) as usize;
    let (a_win, d_win) = (a_cyc * 3, d_cyc * 3);
    let a_start = SAW_LEN - a_win - a_cyc;
    let d_start = digital.len() - d_win - d_cyc;
    let src = &analog_src[a_start..a_start + a_win];
    let dig = &digital[d_start..d_start + d_win];
    let recon = &analog_recon[a_start..a_start + a_win];

    // Steady-state (sustain) peaks, measured on the shown windows — sawtooth amplitude is the
    // voice's sustain·level = 0.7 V. The digital/reconstructed peaks run slightly higher: the AD's
    // band-limiting leaves a little Gibbs overshoot at the sawtooth's reset.
    println!(
        "  source peak {:.3} V  →  digital peak {:.3} (normalized, {:.0} V ref)  →  reconstructed peak {:.3} V",
        peak(src),
        peak(dig),
        SAW_REF_V,
        peak(recon),
    );

    let scale = Some((-0.9, 0.9));
    plot(
        "Analog source — the voice's sawtooth (384 kHz; sharp, all harmonics)",
        src,
        scale,
    );
    plot(
        "After AD — digital samples (48 kHz, band-limited to the 24 kHz Nyquist)",
        dig,
        scale,
    );
    plot(
        "After DA — reconstructed analog volts (384 kHz, smooth)",
        recon,
        scale,
    );
}

/// Render `len` analog samples of the synth voice holding `note`, through a real compiled schedule
/// with the note played into its event lane. `len` is a multiple of `BLOCK_LEN`.
fn render_voice(note: u8, len: usize) -> Vec<f32> {
    let mut g = Graph::new();
    let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
    g.set_output(voice, 0);
    let mut schedule = compile(g, BLOCK_LEN, rate(), 0).expect("voice chain compiles");
    let ev = schedule
        .event_input(voice, 0)
        .expect("the voice's event input");

    let mut events = EventQueue::with_capacity(4);
    events.push(
        0,
        ev,
        EventMessage::NoteOn {
            note,
            velocity: 100,
        },
    );
    let mut out = VoltageBuffer::zeros(BLOCK_LEN, rate());
    let mut samples = Vec::with_capacity(len);
    for b in 0..len / BLOCK_LEN {
        if b == 0 {
            schedule.process_with_events(&mut out, &mut events);
        } else {
            schedule.process(&mut out);
        }
        samples.extend_from_slice(out.as_slice());
    }
    samples
}

/// Drive an [`AdConverter`] directly over an analog waveform, returning its digital samples
/// (normalized, ±1 = full scale). Off the hot path, so building the lanes and the result `Vec` is
/// fine; `prepare` bakes the anti-alias FIR (no schedule to do it). Undithered for a clean trace.
fn ad_convert(analog: &[f32], digital_rate: SampleRate) -> Vec<f32> {
    let mut ad = AdConverter::new(
        digital_rate,
        BitDepth::new(16),
        Volts::new(SAW_REF_V),
        Ohms::new(1e6),
    );
    ad.prepare(rate());
    let input = [Lane::Voltage(VoltageBuffer::from_volts(
        analog.to_vec(),
        rate(),
    ))];
    let mut digital = [Lane::Sample(SampleBuffer::zeros(
        analog.len() / SAW_M,
        digital_rate,
        BitDepth::new(16),
        ClockDomainId::SINGLE,
    ))];
    ad.process(&Params::EMPTY, &input, &mut digital);
    digital[0].sample().as_slice().to_vec()
}

/// Drive a [`DaConverter`] directly over digital samples, returning the reconstructed analog volts
/// (`SAW_M`× longer). `prepare` bakes the reconstruction FIR.
fn da_convert(digital: &[f32], digital_rate: SampleRate) -> Vec<f32> {
    let mut da = DaConverter::new(
        digital_rate,
        BitDepth::new(16),
        Volts::new(SAW_REF_V),
        Ohms::new(150.0),
    );
    da.prepare(rate());
    let input = [Lane::Sample(SampleBuffer::from_samples(
        digital.to_vec(),
        digital_rate,
        BitDepth::new(16),
        ClockDomainId::SINGLE,
    ))];
    let mut analog = [Lane::Voltage(VoltageBuffer::zeros(
        digital.len() * SAW_M,
        rate(),
    ))];
    da.process(&Params::EMPTY, &input, &mut analog);
    analog[0].voltage().as_slice().to_vec()
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
    g.connect_ideal(src, 0, stage, 0);
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
    g.connect_ideal(src, 0, pre, 0);
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
