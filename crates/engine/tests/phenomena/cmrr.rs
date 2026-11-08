//! Common-mode rejection: the same cable pickup that contaminates an unbalanced line cancels at a
//! balanced receiver's difference. **Ideal rejection only** — both conductors carry the *identical*
//! common-mode draw, so `V+ − V−` cancels it to **bit-exact zero** (infinite CMRR). Finite CMRR is
//! leg *asymmetry*, not modeled. Tests are the oracle.

use crate::support::{rate, rms};
use approx::assert_relative_eq;
use engine::{
    BalancedDriver, BalancedReceiver, Cable, Farads, GainStage, Graph, InputZ, NoiseDensity, Ohms,
    TestSource, VoltageBuffer, Volts, compile,
};

/// Pickup density used across the contrast: 50 nV/√Hz ⇒ σ = 50e-9·√192000 = 21.9 µV.
fn pickup_cable() -> Cable {
    Cable::new(Ohms::ZERO, Farads::ZERO).with_pickup(NoiseDensity::new(50e-9))
}

/// Unbalanced: silent source → pickup cable → unity buffer → tap (the pickup passes straight
/// through — no second conductor to subtract it against).
fn run_unbalanced(len: usize, seed: u64) -> Vec<f32> {
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
    let buf = g.add(GainStage::new(
        1.0,
        Volts::new(10.0),
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    g.connect_cabled(src, 0, buf, 0, pickup_cable());
    g.set_output(buf, 0);
    let mut sched = compile(g, len, rate(), seed).expect("unbalanced pickup chain");
    let mut out = VoltageBuffer::zeros(len, rate());
    sched.process(&mut out);
    out.as_slice().to_vec()
}

/// Balanced: silent source → driver → pickup cable → receiver → tap. The pickup couples
/// common-mode (identical on both legs) and is rejected by the receiver difference.
fn run_balanced(len: usize, seed: u64) -> Vec<f32> {
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
    let drv = g.add(BalancedDriver::new(
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    g.connect_ideal(src, 0, drv, 0);
    g.connect_cabled(drv, 0, rcv, 0, pickup_cable());
    g.set_output(rcv, 0);
    let mut sched = compile(g, len, rate(), seed).expect("balanced pickup chain");
    let mut out = VoltageBuffer::zeros(len, rate());
    sched.process(&mut out);
    out.as_slice().to_vec()
}

#[test]
fn unbalanced_passes_interference_while_balanced_rejects_it() {
    let sigma = NoiseDensity::new(50e-9).per_sample_sigma(rate());
    let unbal = run_unbalanced(200_000, 0xCAB1_E001);
    let bal = run_balanced(200_000, 0xCAB1_E001);

    // Unbalanced: the full µV pickup floor reaches the receiver (σ = 21.9 µV).
    assert_relative_eq!(rms(&unbal), sigma, max_relative = 0.02);
    // Balanced: the identical common-mode draw on V+ and V− cancels at the difference — exactly,
    // not just statistically. Ideal (infinite) CMRR, the headline of a balanced line.
    assert!(
        bal.iter().all(|&v| v == 0.0),
        "balanced should reject common-mode pickup to bit-exact zero, got rms {}",
        rms(&bal)
    );
}

#[test]
fn balanced_recovers_the_signal_through_pickup() {
    // Not just zeroing everything: a 2 V DC differential signal driven through the same
    // picking-up cable comes back clean (≈2 V), with the common-mode pickup gone and no noise
    // left on top.
    //   driver: V+ = +1, V− = −1; edge adds identical pickup p to each → V+ = g+p, V− = −g+p
    //   receiver: (g+p) − (−g+p) = 2g ≈ 2 V — the pickup cancels, the signal survives.
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(2.0), Ohms::new(1.0)));
    let drv = g.add(BalancedDriver::new(
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    g.connect_ideal(src, 0, drv, 0);
    g.connect_cabled(drv, 0, rcv, 0, pickup_cable());
    g.set_output(rcv, 0);
    let mut sched = compile(g, 16, rate(), 5).expect("balanced signal+pickup chain");
    let mut out = VoltageBuffer::zeros(16, rate());
    sched.process(&mut out);
    for &v in out.as_slice() {
        assert_relative_eq!(v, 2.0, epsilon = 1e-4);
    }
}
