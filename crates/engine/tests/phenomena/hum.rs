//! Ground-loop hum: a 50/60 Hz common-mode tone coupled onto the cable. Audible on
//! an unbalanced line, rejected (bit-exact) on a balanced one — the "lift the ground" lesson. It
//! rides the same edge-injection seam as pickup, just a deterministic generator instead of noise.

use crate::support::{rate, tone_amplitude};
use approx::assert_relative_eq;
use engine::{
    BalancedDriver, BalancedReceiver, Cable, Farads, GainStage, Graph, InputZ, Ohms, TestSource,
    VoltageBuffer, Volts, compile,
};

const HUM_HZ: f64 = 60.0; // US mains; 50 Hz in the EU — just the parameter
const HUM_V: f32 = 0.1;
const LEN: usize = 64_000; // 10 whole cycles of 60 Hz at 384 kHz (6400 samples/cycle)

fn hum_cable() -> Cable {
    Cable::new(Ohms::ZERO, Farads::ZERO).with_hum(HUM_HZ, Volts::new(HUM_V))
}

#[test]
fn unbalanced_carries_hum() {
    // Silent source → humming cable → unity buffer → tap: the 60 Hz tone reaches the output at
    // its full amplitude (≈0.1 V) — an unbalanced line has nothing to subtract it against.
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
    let buf = g.add(GainStage::new(
        1.0,
        Volts::new(10.0),
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    g.connect_cabled(src, 0, buf, 0, hum_cable());
    g.set_output(buf, 0);
    let mut sched = compile(g, LEN, rate(), 9).expect("unbalanced hum chain");
    let mut out = VoltageBuffer::zeros(LEN, rate());
    sched.process(&mut out);
    assert_relative_eq!(
        tone_amplitude(out.as_slice(), HUM_HZ, rate()),
        HUM_V,
        max_relative = 1e-2
    );
}

#[test]
fn balanced_rejects_hum() {
    // The same humming cable between a balanced driver and receiver: the identical 60 Hz
    // common-mode tone on both legs cancels at V+ − V− to bit-exact zero.
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(0.0), Ohms::new(1.0)));
    let drv = g.add(BalancedDriver::new(
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    g.connect_ideal(src, 0, drv, 0);
    g.connect_cabled(drv, 0, rcv, 0, hum_cable());
    g.set_output(rcv, 0);
    let mut sched = compile(g, LEN, rate(), 9).expect("balanced hum chain");
    let mut out = VoltageBuffer::zeros(LEN, rate());
    sched.process(&mut out);
    assert!(
        out.as_slice().iter().all(|&v| v == 0.0),
        "balanced should reject common-mode hum to bit-exact zero"
    );
}
