//! Control params & de-zippering: a swept knob reaches the engine as a **smoothed**
//! value (a within-block linear ramp), so it never clicks. The headline lesson is the contrast
//! with a raw jump; here we sweep [`GainStage::GAIN`] and assert the output glides continuously to
//! the new gain rather than snapping. White-box where convenient, as elsewhere in this file.

use crate::support::rate;
use approx::assert_relative_eq;
use engine::{
    GainStage, Graph, InputZ, Ohms, ParamQueue, TestSource, VoltageBuffer, Volts, compile,
};

#[test]
fn a_swept_gain_param_de_zippers_without_discontinuity() {
    // 1 V DC → GainStage(gain 1.0) → tap. Near-ideal faces (1 Ω out into 1 GΩ in, bridging
    // tap) make the output ≈ gain·1 V. Sweep the gain param 1 → 5: a de-zippered value ramps
    // there smoothly; a raw write would jump +4 V in a single sample. We assert no
    // sample-to-sample step exceeds a tiny bound, the ramp is monotonic, and it lands at 5 V.
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(1.0)));
    let amp = g.add(GainStage::new(
        1.0,
        Volts::new(100.0),
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    g.connect_ideal(src, 0, amp, 0);
    g.set_output(amp, 0);

    let block = 64;
    let mut sched = compile(g, block, rate(), 0).expect("valid param chain");
    let gain = sched.param(amp, GainStage::GAIN).expect("gain param");

    // Settled at the default gain 1.0 → output ≈ 1 V.
    let mut out = VoltageBuffer::zeros(block, rate());
    sched.process(&mut out);
    assert_relative_eq!(out.get(0).get(), 1.0, max_relative = 1e-3);

    // Aim at 5.0 and collect the whole glide. Smooth time 5 ms @ 384 kHz = 1920 samples = 30
    // blocks of 64; 40 blocks over-covers, so it reaches and holds 5 V.
    let mut q = ParamQueue::with_capacity(1);
    q.set(gain, 5.0);
    let mut swept = Vec::new();
    for b in 0..40 {
        if b == 0 {
            sched.process_with_params(&mut out, &mut q);
        } else {
            sched.process(&mut out);
        }
        swept.extend_from_slice(out.as_slice());
    }

    // No discontinuity: a de-zippered sweep moves at the ramp step (≈ (5−1)/1920 ≈ 0.0021
    // V/sample); a raw jump would show a ~4 V step. 0.005 cleanly separates the two.
    let max_step = swept
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        max_step < 0.005,
        "the sweep must not jump (max sample step {max_step} V)"
    );

    // Monotonic upward (no overshoot/ringing) and settled at the new gain.
    assert!(
        swept.windows(2).all(|w| w[1] - w[0] >= -1e-6),
        "a 1→5 glide should be non-decreasing"
    );
    assert_relative_eq!(*swept.last().unwrap(), 5.0, max_relative = 1e-3);
    // And it genuinely moved off the start (not stuck at 1 V).
    assert!(swept.iter().any(|&v| v > 2.0));
}

#[test]
fn powering_a_stage_off_gates_its_output_to_silence() {
    // 1 V DC → GainStage(gain 4) → tap: powered on, the tap sits at ≈ 4 V. Drive POWERED to 0
    // and, once the de-click glide settles, the output is gated to silence — a powered-off stage
    // passes nothing, and it's a value param (no recompile).
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(1.0)));
    let amp = g.add(GainStage::new(
        4.0,
        Volts::new(100.0),
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    g.connect_ideal(src, 0, amp, 0);
    g.set_output(amp, 0);

    let block = 64;
    let mut sched = compile(g, block, rate(), 0).expect("valid power chain");
    let powered = sched.param(amp, GainStage::POWERED).expect("powered param");

    // Settled, powered-on (default 1.0): ≈ gain·1 V = 4 V.
    let mut out = VoltageBuffer::zeros(block, rate());
    sched.process(&mut out);
    assert_relative_eq!(out.get(0).get(), 4.0, max_relative = 1e-3);

    // Power off and run past the 5 ms / 1920-sample (30-block) glide; the last block is silence.
    let mut q = ParamQueue::with_capacity(1);
    q.set(powered, 0.0);
    for b in 0..40 {
        if b == 0 {
            sched.process_with_params(&mut out, &mut q);
        } else {
            sched.process(&mut out);
        }
    }
    let settled = out.as_slice().iter().fold(0.0_f32, |m, &v| m.max(v.abs()));
    assert!(
        settled < 1e-4,
        "a powered-off stage must gate its output to silence, got {settled} V"
    );
}
