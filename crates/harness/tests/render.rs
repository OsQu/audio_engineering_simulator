//! Integration tests for the offline render driver (Story 2.1.5).
//!
//! These render a played note through the real engine + capture + driver and assert the result
//! numerically — the analog-domain oracle (PROJECT_PLAN §9): you can't ear-check a CI run, so the
//! rendered audio is checked against hand calcs (fundamental, causal onset, level) and for
//! determinism. We use the **analog-only** patch (`synth → speaker`, no modeled AD): with no
//! converter dither the pre-onset output is true silence, so the causal-onset check is exact, and
//! the render is bit-reproducible.

use engine::{
    AdConverter, AnalogRate, BitDepth, Compressor, DaConverter, EqBand, EventMessage, EventQueue,
    Graph, InputZ, NodeId, Ohms, SampleRate, Speaker, SynthVoice, ThreeBandEq, Volts, compile,
};
use harness::render::{RenderConfig, render_to_samples};

const ANALOG_HZ: f64 = 384_000.0;
const HOST_HZ: f64 = 48_000.0;
const BLOCK_LEN: usize = 384;
/// A4 = 440 Hz.
const NOTE_A4: u8 = 69;
const A4_HZ: f64 = 440.0;
/// Monitor full scale; the voice peaks ≈ 0.7 V, so it renders a few dB below full scale.
const FULL_SCALE: f32 = 1.0;

fn analog_rate() -> AnalogRate {
    AnalogRate::new(ANALOG_HZ)
}

/// Render `synth → speaker` playing A4 from `note_on` (absolute analog samples) for `seconds` of
/// host audio. Deterministic (compile seed 0; no RNG on this analog-only path).
fn render_voice(note_on: u64, seconds: f64) -> Vec<f32> {
    let mut g = Graph::new();
    let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
    let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
    g.connect(voice, 0, spk, 0);
    g.set_output(spk, 0);

    let mut schedule = compile(g, BLOCK_LEN, analog_rate(), 0).expect("voice patch compiles");
    let ev = schedule
        .event_input(voice, 0)
        .expect("the voice's event input");
    let mut events = EventQueue::with_capacity(4);
    events.push(
        note_on,
        ev,
        EventMessage::NoteOn {
            note: NOTE_A4,
            velocity: 100,
        },
    );

    let cfg = RenderConfig {
        host_rate: SampleRate::new(HOST_HZ),
        full_scale_volts: FULL_SCALE,
        seconds,
    };
    render_to_samples(&mut schedule, analog_rate(), &mut events, &cfg)
}

/// Render A4 from t=0 through `synth → AD → [optional digital processor] → DA → speaker` for
/// `seconds` of host audio. `insert` adds the processor node (given the digital format) and returns
/// it, or `None` for the flat chain (AD straight to DA). Lets the EQ / compressor tests compare a
/// processed render against the flat one through the *same* converters.
fn render_through<F>(insert: F, seconds: f64) -> Vec<f32>
where
    F: FnOnce(&mut Graph, SampleRate, BitDepth) -> Option<NodeId>,
{
    let host_rate = SampleRate::new(HOST_HZ);
    let bits = BitDepth::new(16);
    let mut g = Graph::new();
    let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
    let ad = g.add(AdConverter::new(
        host_rate,
        bits,
        Volts::new(FULL_SCALE),
        Ohms::new(1e6),
    ));
    let da = g.add(DaConverter::new(
        host_rate,
        bits,
        Volts::new(FULL_SCALE),
        Ohms::new(150.0),
    ));
    let spk = g.add(Speaker::new(1.0, InputZ::new(Ohms::new(10_000.0))));
    g.connect(voice, 0, ad, 0);
    match insert(&mut g, host_rate, bits) {
        Some(proc) => {
            g.connect(ad, 0, proc, 0);
            g.connect(proc, 0, da, 0);
        }
        None => g.connect(ad, 0, da, 0),
    }
    g.connect(da, 0, spk, 0);
    g.set_output(spk, 0);

    let mut schedule = compile(g, BLOCK_LEN, analog_rate(), 0).expect("converter patch compiles");
    let ev = schedule
        .event_input(voice, 0)
        .expect("the voice's event input");
    let mut events = EventQueue::with_capacity(4);
    events.push(
        0,
        ev,
        EventMessage::NoteOn {
            note: NOTE_A4,
            velocity: 100,
        },
    );
    let cfg = RenderConfig {
        host_rate,
        full_scale_volts: FULL_SCALE,
        seconds,
    };
    render_to_samples(&mut schedule, analog_rate(), &mut events, &cfg)
}

/// The steady sustain window (past the envelope attack and the converters' FIR latency).
fn sustain(samples: &[f32]) -> &[f32] {
    &samples[(0.20 * HOST_HZ) as usize..(0.45 * HOST_HZ) as usize]
}

/// A low-shelf cut below the fundamental attenuates the 440 Hz tone; the EQ shifts spectral
/// balance, measured against the flat render through the same converters.
#[test]
fn eq_low_shelf_cut_attenuates_the_fundamental() {
    let flat = render_through(|_, _, _| None, 0.5);
    // Low shelf −12 dB at 700 Hz: A4 (440 Hz) sits below the corner, so its fundamental is cut to
    // roughly a quarter; the other two bands are flat (0 dB ⇒ exactly transparent).
    let cut = render_through(
        |g, rate, bits| {
            Some(g.add(ThreeBandEq::new(
                rate,
                bits,
                EqBand::new(700.0, 0.707, -12.0),
                EqBand::new(1_000.0, 1.0, 0.0),
                EqBand::new(8_000.0, 0.707, 0.0),
            )))
        },
        0.5,
    );

    let f_flat = bin_magnitude(sustain(&flat), A4_HZ);
    let f_cut = bin_magnitude(sustain(&cut), A4_HZ);
    assert!(
        f_cut < 0.6 * f_flat,
        "a −12 dB low shelf should clearly attenuate the 440 Hz fundamental: cut {f_cut:.3} vs flat {f_flat:.3}"
    );
}

/// A low threshold and a high ratio pull the note's sustain level down; the compressor reduces
/// peak level, measured against the flat render through the same converters.
#[test]
fn compressor_reduces_the_sustain_level() {
    let flat = render_through(|_, _, _| None, 0.5);
    // Threshold −24 dBFS (well below the ~−9 dBFS sustain peak), 8:1, no makeup ⇒ heavy reduction.
    let compressed = render_through(
        |g, rate, bits| Some(g.add(Compressor::new(rate, bits, -24.0, 8.0, 5.0, 80.0))),
        0.5,
    );

    let p_flat = peak(sustain(&flat));
    let p_comp = peak(sustain(&compressed));
    assert!(
        p_comp < 0.6 * p_flat,
        "8:1 compression below threshold should clearly lower the sustain peak: compressed {p_comp:.3} vs flat {p_flat:.3}"
    );
}

/// Single-bin DFT magnitude of `x` at `freq` Hz (host rate), normalized so a unit-amplitude
/// sinusoid reads ≈ its amplitude. Enough to compare harmonic content without an FFT crate.
fn bin_magnitude(x: &[f32], freq: f64) -> f64 {
    let omega = std::f64::consts::TAU * freq / HOST_HZ;
    let (mut re, mut im) = (0.0_f64, 0.0_f64);
    for (n, &v) in x.iter().enumerate() {
        let phase = omega * n as f64;
        re += f64::from(v) * phase.cos();
        im += f64::from(v) * phase.sin();
    }
    2.0 / x.len() as f64 * re.hypot(im)
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0_f32, |m, &v| m.max(v.abs()))
}

/// The rendered tone is A4: the 440 Hz fundamental dominates, matches the sawtooth hand calc, and
/// the energy sits on the harmonics (not between them).
#[test]
fn renders_the_a4_fundamental() {
    let samples = render_voice(0, 0.5);
    // A steady sustain window, past the envelope attack and the capture's FIR latency.
    let win = &samples[(0.20 * HOST_HZ) as usize..(0.45 * HOST_HZ) as usize];

    let fundamental = bin_magnitude(win, A4_HZ);
    let second = bin_magnitude(win, 2.0 * A4_HZ); // 880 Hz harmonic
    let between = bin_magnitude(win, 1.5 * A4_HZ); // 660 Hz, between harmonics ⇒ ≈ 0

    // Ideal sawtooth of peak A has fundamental amplitude 2A/π; A = sustain·level ≈ 0.7 V at the
    // 1 V monitor reference ⇒ ≈ 0.45. Band-limiting leaves the fundamental untouched.
    assert!(
        (0.30..0.60).contains(&fundamental),
        "A4 fundamental amplitude {fundamental:.3} (expected ≈ 0.45 = 2·0.7/π)"
    );
    assert!(
        fundamental > second,
        "fundamental {fundamental:.3} should exceed the 2nd harmonic {second:.3} (sawtooth 1/n)"
    );
    assert!(
        fundamental > 10.0 * between,
        "energy should sit on harmonics, not between them ({fundamental:.3} vs {between:.3} at 660 Hz)"
    );
}

/// The note's onset is causal: with no modeled converter (hence no dither), the render is pure
/// silence until the note plays, then signal once the envelope rises.
#[test]
fn note_onset_is_causal() {
    let note_on_s = 0.25;
    let samples = render_voice((note_on_s * ANALOG_HZ) as u64, 0.6);
    let onset = (note_on_s * HOST_HZ) as usize;

    // Latency only *delays* the signal, so everything strictly before the note is exact silence.
    assert!(
        peak(&samples[..onset]) < 1e-6,
        "expected silence before the note onset, got peak {}",
        peak(&samples[..onset])
    );
    // Well after onset (envelope risen, latency elapsed) the tone is clearly present.
    let after = (0.40 * HOST_HZ) as usize;
    assert!(
        peak(&samples[after..]) > 0.1,
        "expected signal after the onset, got peak {}",
        peak(&samples[after..])
    );
}

/// Same seed + same patch ⇒ bit-identical render (the determinism golden-file tests will rest on).
#[test]
fn render_is_deterministic() {
    assert_eq!(render_voice(0, 0.2), render_voice(0, 0.2));
}
