//! Cable pickup: broadband interference (EMI) coupling onto the wire as a noise voltage. On an
//! unbalanced edge it lands on the signal at µV scale (the balanced *rejection* of it is the CMRR
//! case). Tests are the oracle: the floor is a hand-computed number, with the calc inline.

use crate::support::{rate, rms};
use approx::assert_relative_eq;
use engine::{
    Cable, Farads, GainStage, Graph, InputZ, NoiseDensity, Ohms, TestSource, VoltageBuffer, Volts,
    compile,
};

/// A near-ideal unity buffer: huge `Zin`, tiny `Zout`, gain 1, no internal noise — so its
/// output is exactly what arrived at its input (here, the pickup coupled onto the cable).
fn unity_buffer() -> GainStage {
    GainStage::new(
        1.0,
        Volts::new(10.0),
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    )
}

/// Silent source → a cable that picks up `density` → unity buffer → tap; return the output.
fn run_pickup(density: NoiseDensity, len: usize, seed: u64) -> Vec<f32> {
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
    let buf = g.add(unity_buffer());
    g.connect_cabled(
        src,
        0,
        buf,
        0,
        Cable::new(Ohms::ZERO, Farads::ZERO).with_pickup(density),
    );
    g.set_output(buf, 0);
    let mut sched = compile(g, len, rate(), seed).expect("valid pickup chain");
    let mut out = VoltageBuffer::zeros(len, rate());
    sched.process(&mut out);
    out.as_slice().to_vec()
}

#[test]
fn cable_pickup_floor_matches_density() {
    // Pickup couples onto the wire after the (≈unity) divider, so an unbalanced receiver sees
    // the full floor:  σ = D·√(fs/2) = 10e-9·√192000 = 4.3818 µV. (200k samples ⇒ RMS
    // converges to σ within ~0.16%; 2% is comfortable.)
    let d = NoiseDensity::new(10e-9);
    let sigma = d.per_sample_sigma(rate());
    let out = run_pickup(d, 200_000, 0xCAB1_E000);
    assert_relative_eq!(rms(&out), sigma, max_relative = 0.02);
}

#[test]
fn no_pickup_is_silence() {
    // A cable with zero pickup density adds nothing — a silent source stays silent.
    let out = run_pickup(NoiseDensity::ZERO, 1_000, 1);
    assert!(out.iter().all(|&v| v == 0.0));
}

#[test]
fn pickup_is_reproducible() {
    // Same compile seed ⇒ identical pickup realization (determinism for tests/replays).
    let d = NoiseDensity::new(50e-9);
    assert_eq!(run_pickup(d, 1_000, 42), run_pickup(d, 1_000, 42));
}
