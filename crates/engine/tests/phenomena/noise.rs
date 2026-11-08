//! Device noise floors emerging from the voltage math, on real compiled chains.
//!
//! Tests are the oracle: you can't hear a µV noise floor, so each assert is a number
//! computed by hand, with the calc in a comment. RMS converges to the true `σ` only in the
//! limit, so the tolerances are the finite-sample sampling error (`~1/√(2N)`), not slop.

use crate::support::{rate, rms};
use approx::assert_relative_eq;
use engine::{
    GainStage, Graph, InputZ, NoiseDensity, Ohms, TestSource, VoltageBuffer, Volts, compile,
};

/// A near-ideal unity buffer: huge `Zin`, tiny `Zout`, so every edge divider is ~1 and the
/// only thing the stage does to the signal is add its own input-referred noise floor.
/// That keeps the hand calc clean — no gain or loss bookkeeping muddying the noise power.
fn noisy_buffer(density: NoiseDensity) -> GainStage {
    GainStage::new(
        1.0,
        Volts::new(10.0),
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    )
    .with_noise(density)
}

/// Run a silent source through a chain of unity noisy buffers; return the tapped output.
fn run_silence(densities: &[NoiseDensity], len: usize, seed: u64) -> Vec<f32> {
    let mut g = Graph::new();
    let mut tail = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
    for &d in densities {
        let b = g.add(noisy_buffer(d));
        g.connect_ideal(tail, 0, b, 0);
        tail = b;
    }
    g.set_output(tail, 0);
    let mut sched = compile(g, len, rate(), seed).expect("valid noise chain");
    let mut out = VoltageBuffer::zeros(len, rate());
    sched.process(&mut out);
    out.as_slice().to_vec()
}

#[test]
fn device_noise_floor_matches_density() {
    // One unity buffer, silent input. With the noise referred to the input and unity gain,
    // the output RMS is exactly the per-sample σ on the wire:
    //   σ = D·√(fs/2) = 10e-9 · √(384000/2) = 10e-9 · 438.178 = 4.3818 µV.
    let d = NoiseDensity::new(10e-9);
    let sigma = d.per_sample_sigma(rate());
    let out = run_silence(&[d], 200_000, 0x0A11_CE00);
    // 200k Gaussian samples ⇒ RMS converges to σ to ~0.16% (1/√(2N)); 2% is comfortable.
    assert_relative_eq!(rms(&out), sigma, max_relative = 0.02);
}

#[test]
fn noise_adds_in_quadrature_down_the_chain() {
    // Two identical unity noise stages, same compile seed ⇒ stage 1's noise stream is the
    // *same realization* in both graphs (split is by node index), so the second stage only
    // adds uncorrelated power:  σ_total = √(σ1² + σ2²). Two equal stages ⇒ √2·σ, i.e. the
    // floor rises +3.01 dB and a fixed signal's SNR drops the same 3.01 dB. (The classic
    // "the first preamp sets your SNR; every later stage can only add noise" lesson.)
    let d = NoiseDensity::new(10e-9);
    let sigma = d.per_sample_sigma(rate());

    let one = run_silence(&[d], 200_000, 7);
    let two = run_silence(&[d, d], 200_000, 7);
    let n1 = rms(&one);
    let n2 = rms(&two);

    // Stage 1 alone is the device floor; the chain is strictly noisier (monotonic).
    assert_relative_eq!(n1, sigma, max_relative = 0.02);
    assert!(
        n2 > n1,
        "the chain must be noisier than one stage: {n2} vs {n1}"
    );

    // Quadrature sum of two equal stages: √(σ² + σ²) = √2·σ.
    assert_relative_eq!(n2, core::f32::consts::SQRT_2 * sigma, max_relative = 0.02);

    // SNR cost of the second stage, signal held fixed: 20·log10(n2/n1) = 3.01 dB.
    let snr_loss_db = 20.0 * (n2 / n1).log10();
    assert_relative_eq!(snr_loss_db, 3.0103, epsilon = 0.1);
}
