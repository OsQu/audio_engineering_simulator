//! The **grounding edge**: a 1-conductor (unbalanced) output into a 2-conductor (balanced) input —
//! the honest model of a TS plug seated in a combo/TRS jack, whose sleeve shorts the cold pin to
//! ground. The hot leg carries the ordinary divider transform (its gain is *exactly* the 1→1 case —
//! the differential formula is unchanged), the cold leg is grounded 0 V, and coupled interference
//! lands common-mode on both legs so it still cancels at the receiver difference. The reverse
//! (2 conductors into 1) has no such physics and stays a mismatch. Tests are the oracle.

use crate::support::{BalancedTestSource, rate, rms};
use approx::assert_relative_eq;
use engine::{
    BalancedReceiver, Cable, CompileError, Farads, GainStage, Graph, InputZ, NoiseDensity, Ohms,
    TestSource, VoltageBuffer, Volts, compile, divider_gain,
};

#[test]
fn hot_leg_gain_equals_the_unbalanced_divider() {
    // Unbalanced 100 Ω source into a balanced 10 kΩ **differential** input, no cable. The
    // grounding edge's hot leg uses the ordinary 1→1 divider — the differential Zin plays
    // exactly the role an unbalanced Zin would:
    //   gain = Zin / (Zout + Zcable + Zin) = 10000 / (100 + 0 + 10000) = 0.990099.
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(10_000.0), Ohms::new(150.0)));
    g.connect_ideal(src, 0, rcv, 0);
    g.set_output(rcv, 0);
    let sched = compile(g, 8, rate(), 0).expect("unbal→bal is a legal grounding edge");

    // Identical to the canonical 1→1 divider solve, and to the hand number.
    let one_to_one = divider_gain(
        Ohms::new(100.0),
        Ohms::ZERO,
        InputZ::new(Ohms::new(10_000.0)),
    );
    assert_relative_eq!(
        sched.edge_gain(0).expect("analog edge"),
        one_to_one,
        epsilon = 1e-7
    );
    assert_relative_eq!(
        sched.edge_gain(0).expect("analog edge"),
        0.990_099,
        epsilon = 1e-5
    );
}

#[test]
fn balanced_receiver_recovers_the_full_signal() {
    // Near-ideal unbalanced source (2 V, 1 Ω) into a balanced receiver (1 GΩ differential in).
    // The edge divider ≈ 1, so the hot leg carries ≈2 V and the cold leg is grounded 0 V. The
    // receiver difference recovers the *full* signal: V+ − V− = 2 − 0 = 2 V (no halving).
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(2.0), Ohms::new(1.0)));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    g.connect_ideal(src, 0, rcv, 0);
    g.set_output(rcv, 0);
    let mut sched = compile(g, 8, rate(), 0).expect("legal grounding edge");
    let mut out = VoltageBuffer::zeros(8, rate());
    sched.process(&mut out);
    for &v in out.as_slice() {
        assert_relative_eq!(v, 2.0, epsilon = 1e-4);
    }
}

#[test]
fn coupled_interference_cancels_at_the_receiver() {
    // A silent unbalanced source through a cable coupling 50 nV/√Hz of pickup, into the balanced
    // receiver. The pickup lands *identically* on the hot and the grounded cold leg (the wire
    // pair is physically adjacent → common-mode), so the receiver difference cancels it to
    // bit-exact zero — even though the cold leg carries no signal. Contrast: the same cable into
    // an unbalanced buffer passes the full floor, σ = 50e-9·√(384000/2) = 50e-9·√192000 = 21.9 µV.
    fn pickup_cable() -> Cable {
        Cable::new(Ohms::ZERO, Farads::ZERO).with_pickup(NoiseDensity::new(50e-9))
    }
    let len = 200_000;
    let seed = 0xC0DE_C01D;

    // Balanced grounding edge: the common-mode pickup cancels to bit-exact zero.
    let mut gb = Graph::new();
    let bs = gb.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
    let brcv = gb.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    gb.connect_cabled(bs, 0, brcv, 0, pickup_cable());
    gb.set_output(brcv, 0);
    let mut bsched = compile(gb, len, rate(), seed).expect("grounding edge with pickup");
    let mut bout = VoltageBuffer::zeros(len, rate());
    bsched.process(&mut bout);
    assert!(
        bout.as_slice().iter().all(|&v| v == 0.0),
        "common-mode pickup on both legs must cancel to bit-exact zero, got rms {}",
        rms(bout.as_slice())
    );

    // Unbalanced sibling: the pickup is genuinely present (σ ≈ 21.9 µV reaches the receiver).
    let sigma = NoiseDensity::new(50e-9).per_sample_sigma(rate());
    let mut gu = Graph::new();
    let us = gu.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
    let ubuf = gu.add(GainStage::new(
        1.0,
        Volts::new(10.0),
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    gu.connect_cabled(us, 0, ubuf, 0, pickup_cable());
    gu.set_output(ubuf, 0);
    let mut usched = compile(gu, len, rate(), seed).expect("unbalanced pickup chain");
    let mut uout = VoltageBuffer::zeros(len, rate());
    usched.process(&mut uout);
    assert_relative_eq!(rms(uout.as_slice()), sigma, max_relative = 0.02);
}

#[test]
fn balanced_into_unbalanced_still_errors() {
    // The reverse direction has no grounding-plug physics: a 2-conductor output into a
    // 1-conductor input stays a hard conductor mismatch, rejected at compile.
    let mut g = Graph::new();
    let src = g.add(BalancedTestSource::new(
        Volts::new(2.0),
        Volts::new(0.0),
        Ohms::new(1.0),
    ));
    let amp = g.add(GainStage::new(
        1.0,
        Volts::new(10.0),
        InputZ::new(Ohms::new(10_000.0)),
        Ohms::new(150.0),
    ));
    g.connect_ideal(src, 0, amp, 0); // balanced out → unbalanced in
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
