//! A DC offset riding the AC, removed by a DC-blocking high-pass, on a compiled patch.
//! Tests are the oracle: the numbers are hand-computed, with the calc inline.

use crate::support::{SineSource, rate, rms};
use approx::assert_relative_eq;
use engine::{DcBlocker, Farads, Graph, Ohms, VoltageBuffer, Volts, compile};

#[test]
fn dc_blocker_strips_the_offset_and_passes_the_audio() {
    // A 1 kHz, 1 V sine riding on a 2 V DC pedestal → a DC blocker → tap.
    //   source:   2.0 + 1.0·sin(2π·1000·t), from a near-ideal 1 Ω output
    //   blocker:  c = 31.831 nF, r = 1 MΩ ⇒ f_c = 1/(2π·1e6·31.831e-9) = 5.00 Hz
    //   edge:     1 Ω into 1 MΩ ⇒ divider 1e6/(1+1e6) = 0.999999 ≈ unity (loading isolated)
    // 1 kHz sits 200× above the 5 Hz corner → the AC passes ~untouched; DC (0 Hz) is a zero
    // of the high-pass → fully blocked. So after settling the output is a 1 V sine on 0 V.
    let mut g = Graph::new();
    let src = g.add(SineSource::new(
        1_000.0,
        Volts::new(1.0),
        Volts::new(2.0),
        Ohms::new(1.0),
    ));
    let blk = g.add(DcBlocker::new(
        Farads::new(31.831e-9),
        Ohms::new(1_000_000.0),
        Ohms::new(150.0),
    ));
    g.connect_ideal(src, 0, blk, 0);
    g.set_output(blk, 0);

    // One long block: 200k samples ≫ the settling time (τ = RC ≈ 12.2k samples), so the
    // second half is fully steady. Drop the first half as the high-pass transient.
    let len = 200_000;
    let mut sched = compile(g, len, rate(), 0).expect("valid DC-block chain");
    let mut out = VoltageBuffer::zeros(len, rate());
    sched.process(&mut out);
    let tail = &out.as_slice()[len / 2..];

    // DC removed: the 2 V pedestal is gone — the steady tail averages to ≈ 0.
    let mean: f64 = tail.iter().map(|&v| f64::from(v)).sum::<f64>() / tail.len() as f64;
    assert!(
        mean.abs() < 5e-3,
        "DC offset should be blocked, mean = {mean}"
    );

    // AC preserved: a 1 V sine through the unity divider has RMS amp/√2 = 0.7071, and the
    // 5 Hz corner takes nothing off a 1 kHz tone.
    assert_relative_eq!(rms(tail), 0.707_106_77, max_relative = 2e-2);
}
