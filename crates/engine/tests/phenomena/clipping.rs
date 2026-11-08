//! Headroom & clipping at the rail voltage, and the harmonic distortion that emerges, on compiled
//! patches. Tests are the oracle: you can't hear a clip onset or count harmonics by ear, so each
//! number is hand-computed with the calc inline.

use crate::support::{SineSource, rate, tone_amplitude};
use approx::assert_relative_eq;
use engine::{GainStage, Graph, InputZ, Ohms, VoltageBuffer, Volts, compile, headroom_db};

/// `SineSource(amp, freq) → GainStage(gain, rail)`, tapped at the stage output. The source
/// drives from 1 Ω into the stage's 1 MΩ input, so the loading divider is ~unity (`DIVIDER`)
/// and the only thing shaping the signal is the gain and its rail clip. `len` is a whole
/// number of cycles of `freq` so [`tone_amplitude`] reads harmonics exactly.
fn run_tone(amp: Volts, freq: f64, gain: f32, rail: Volts, len: usize) -> Vec<f32> {
    let mut g = Graph::new();
    let src = g.add(SineSource::new(freq, amp, Volts::new(0.0), Ohms::new(1.0)));
    let stage = g.add(GainStage::new(
        gain,
        rail,
        InputZ::new(Ohms::new(1_000_000.0)),
        Ohms::new(150.0),
    ));
    g.connect_ideal(src, 0, stage, 0);
    g.set_output(stage, 0);
    let mut sched = compile(g, len, rate(), 0).expect("valid clip chain");
    let mut out = VoltageBuffer::zeros(len, rate());
    sched.process(&mut out);
    out.as_slice().to_vec()
}

/// 1 Ω source into a 1 MΩ input: 1e6/(1+1e6) = 0.999999, i.e. ~unity loading.
const DIVIDER: f32 = 0.999_999;
/// 200 whole cycles of a 1 kHz tone at 384 kHz (384 samples/cycle) — also whole cycles of
/// 2, 3, 4, 5 kHz, so the harmonic bins stay orthogonal.
const LEN: usize = 384 * 200;

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, &v| m.max(v.abs()))
}

#[test]
fn output_clips_to_the_rail_past_clip_onset() {
    // Stage: ×5 gain into a 10 V rail. The stage clips when its output wants to exceed the
    // rail, i.e. at source amplitude  amp_onset = rail / (DIVIDER · gain) = 10 / (·5) = 2.0 V.
    let onset = 10.0 / (DIVIDER * 5.0);
    assert_relative_eq!(onset, 2.0, epsilon = 1e-4);

    // Below onset (1.8 V): wanted peak = 1.8 · 0.999999 · 5 = 9.0 V < 10 V rail → clean,
    // unclipped, peak sits at the wanted 9.0 V.
    let clean = run_tone(Volts::new(1.8), 1_000.0, 5.0, Volts::new(10.0), LEN);
    assert_relative_eq!(peak(&clean), 1.8 * DIVIDER * 5.0, max_relative = 1e-2);
    assert!(peak(&clean) < 10.0, "below onset must not clip");

    // Above onset (3.0 V): wanted peak = 3.0 · ~1 · 5 = 15 V > 10 V → the output flat-tops
    // at exactly the ±10 V rail. Clipping emergent from the rail in volts, not a flag.
    let clipped = run_tone(Volts::new(3.0), 1_000.0, 5.0, Volts::new(10.0), LEN);
    assert_relative_eq!(peak(&clipped), 10.0, max_relative = 1e-3);
}

#[test]
fn a_clean_signal_below_the_rail_is_undistorted() {
    // ×2 into a 10 V rail, 1 V source → ~2 V peak, far under the rail: a pure sine.
    let out = run_tone(Volts::new(1.0), 1_000.0, 2.0, Volts::new(10.0), LEN);
    let p = peak(&out);

    // The fundamental carries the whole signal; the 3rd harmonic is negligible (no clip).
    let fund = tone_amplitude(&out, 1_000.0, rate());
    let third = tone_amplitude(&out, 3_000.0, rate());
    assert_relative_eq!(fund, 2.0 * DIVIDER, max_relative = 1e-3);
    assert!(third / fund < 0.01, "an unclipped sine has no harmonics");

    // Headroom: a ~2 V peak under a 10 V rail = 20·log10(10/2) = 13.98 dB of room left.
    assert_relative_eq!(
        headroom_db(Volts::new(p), Volts::new(10.0)),
        13.979,
        epsilon = 5e-2
    );
}

#[test]
fn hard_clipping_generates_odd_harmonics() {
    // Overdrive ×100 into a 1 V rail: the sine is clamped almost the instant it leaves zero,
    // so the output is essentially a ±1 V square wave. A square wave of amplitude R has the
    // Fourier series (4R/π)·(sin ωt + ⅓ sin 3ωt + ⅕ sin 5ωt + …): only ODD harmonics, each
    // falling as 1/n. Symmetric clipping ⇒ no even harmonics. (This is *why* clipping sounds
    // harsh — it injects a stack of odd overtones.)
    let out = run_tone(Volts::new(1.0), 1_000.0, 100.0, Volts::new(1.0), LEN);
    let fund = tone_amplitude(&out, 1_000.0, rate());
    let second = tone_amplitude(&out, 2_000.0, rate());
    let third = tone_amplitude(&out, 3_000.0, rate());
    let fifth = tone_amplitude(&out, 5_000.0, rate());

    // Fundamental of a ±1 V square wave: 4·R/π = 1.2732 V.
    assert_relative_eq!(fund, 4.0 / core::f32::consts::PI, max_relative = 2e-2);
    // Odd harmonics fall as 1/n: 3rd/1st = 1/3, 5th/1st = 1/5.
    assert_relative_eq!(third / fund, 1.0 / 3.0, max_relative = 3e-2);
    assert_relative_eq!(fifth / fund, 1.0 / 5.0, max_relative = 3e-2);
    // Symmetric clip ⇒ the even harmonics are absent.
    assert!(
        second / fund < 0.02,
        "symmetric clipping has no even harmonics"
    );
}
