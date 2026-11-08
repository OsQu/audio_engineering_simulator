//! Two-conductor balanced lines: a differential signal survives the trip, and a common-mode offset
//! cancels at the receiver difference (`V+ − V−`). The rejection *emerges* from the subtraction; it
//! is not a flag. Tests are the oracle — numbers hand-computed.

use crate::support::{BalancedTestSource, SineSource, rate, rms};
use approx::assert_relative_eq;
use engine::{
    BalancedDriver, BalancedReceiver, CompileError, DcBlocker, Farads, GainStage, Graph, InputZ,
    Ohms, TestSource, VoltageBuffer, Volts, compile,
};

#[test]
fn balanced_chain_preserves_the_differential_signal() {
    // source(2 V, 1 Ω) → balanced driver → balanced receiver → tap. Every face is near-ideal
    // (1 Ω out into 1 GΩ in), so each divider ≈ 1:
    //   driver in ≈ 2 V → V+ = +1 V, V− = −1 V
    //   balanced edge ≈ unity per conductor → V+ ≈ +1, V− ≈ −1
    //   receiver out = V+ − V− ≈ 2 V  ← the differential survives unity end-to-end.
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(2.0), Ohms::new(1.0)));
    let drv = g.add(BalancedDriver::new(
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    g.connect_ideal(src, 0, drv, 0);
    g.connect_ideal(drv, 0, rcv, 0);
    g.set_output(rcv, 0);

    let mut sched = compile(g, 8, rate(), 0).expect("valid balanced chain");
    let mut out = VoltageBuffer::zeros(8, rate());
    sched.process(&mut out);
    for &v in out.as_slice() {
        assert_relative_eq!(v, 2.0, epsilon = 1e-4);
    }
}

#[test]
fn balanced_receiver_rejects_common_mode() {
    // A balanced source emits a 2 V differential signal on a common-mode pedestal `cm`:
    //   V+ = cm + 1, V− = cm − 1. The edge scales both conductors by the same ≈unity gain,
    //   so the receiver difference is (cm+1) − (cm−1) = 2 V, *independent of cm*. That equal
    //   scaling is why common-mode cancels — the headline of a balanced line.
    fn run(cm: f32) -> f32 {
        let mut g = Graph::new();
        let src = g.add(BalancedTestSource::new(
            Volts::new(2.0),
            Volts::new(cm),
            Ohms::new(1.0),
        ));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect_ideal(src, 0, rcv, 0);
        g.set_output(rcv, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("valid balanced chain");
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
        out.get(0).get()
    }

    // No common-mode and a large +100 V common-mode pedestal give the same 2 V differential.
    assert_relative_eq!(run(0.0), 2.0, epsilon = 1e-4);
    assert_relative_eq!(run(100.0), 2.0, epsilon = 1e-4);
    // Ideal rejection: the 100 V pedestal leaves no residue beyond float epsilon.
    assert_relative_eq!(run(100.0), run(0.0), epsilon = 1e-4);
}

#[test]
fn rejects_conductor_count_mismatch() {
    // A balanced output (2 conductors) into an unbalanced input (1) is a conductor mismatch:
    // cross-type connections aren't modeled yet, so compile rejects it rather than guessing.
    let mut g = Graph::new();
    let drv = g.add(BalancedDriver::new(
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    let amp = g.add(GainStage::new(
        2.0,
        Volts::new(10.0),
        InputZ::new(Ohms::new(10_000.0)),
        Ohms::new(150.0),
    ));
    g.connect_ideal(drv, 0, amp, 0); // balanced out → unbalanced in
    g.set_output(amp, 0);
    assert_eq!(
        compile(g, 8, rate(), 0).err(),
        Some(CompileError::LaneCountMismatch {
            from_node: 0,
            from_port: 0,
            to_node: 1,
            to_port: 0,
        })
    );
}

#[test]
fn dc_blocker_composes_on_the_balanced_pair() {
    // The DC blocker is a per-conductor node, so the compiler lifts it across the pair: the
    // driver's 2-conductor output infers it to 2 and replicates it per leg. Without the lift
    // this very wiring would be a LaneCountMismatch — with it an ordinary processor just
    // composes, "balanced" never a label. (This is the mechanism phantom power rides on.)
    //
    //   source:  2 V DC + 1 V·sin(2π·10k)                    (single-ended)
    //   driver:  V+ = 1 + 0.5·sin, V− = −(1 + 0.5·sin)       (≈unity edge)
    //   per-leg DC block (1 kHz corner): strips the ±1 V DC on each leg → leaves ±0.5·sin
    //   receiver: V+ − V− = sin  → amp 1 V, RMS 0.7071, mean 0
    let mut g = Graph::new();
    let src = g.add(SineSource::new(
        10_000.0,
        Volts::new(1.0),
        Volts::new(2.0),
        Ohms::new(1.0),
    ));
    let drv = g.add(BalancedDriver::new(
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    let blk = g.add(DcBlocker::new(
        Farads::new(15.915e-9), // with r = 10 kΩ → f_c = 1 kHz, a decade below the 10 kHz tone
        Ohms::new(10_000.0),
        Ohms::new(150.0),
    ));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    g.connect_ideal(src, 0, drv, 0);
    g.connect_ideal(drv, 0, blk, 0);
    g.connect_ideal(blk, 0, rcv, 0);
    g.set_output(rcv, 0);

    let len = 40_000; // ≫ settling (τ = RC ≈ 61 samples)
    let mut sched = compile(g, len, rate(), 0).expect("balanced chain with a lifted DC blocker");
    let mut out = VoltageBuffer::zeros(len, rate());
    sched.process(&mut out);
    let tail = &out.as_slice()[len / 2..];

    // DC stripped on each leg → the recovered differential averages to ≈ 0.
    let mean: f64 = tail.iter().map(|&v| f64::from(v)).sum::<f64>() / tail.len() as f64;
    assert!(
        mean.abs() < 1e-2,
        "per-leg DC block should remove the offset, mean = {mean}"
    );
    // Differential audio survives the passband: RMS ≈ amp/√2.
    assert_relative_eq!(rms(tail), 0.707_106_77, max_relative = 2e-2);
}
