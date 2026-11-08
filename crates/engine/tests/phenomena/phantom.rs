//! Phantom power resolved at compile: the DC bias network — an engaged supply on the consumer's
//! input face, a load on the producer's output face, the edge's cable R between — is solved once
//! (the DC operating point) and the mic runs on the *earned* pedestal. Sag, dead-mic, and the
//! no-supply cases all emerge from the divider; the fan-in of two engaged supplies is a legible
//! compile error. Tests are the oracle — every number hand-computed.

use crate::support::rate;
use approx::assert_relative_eq;
use engine::{
    BalancedReceiver, Cable, CompileError, CondenserMic, Farads, Graph, InputZ, MicPreamp, Ohms,
    Pascals, SoundSource, TestSource, VoltageBuffer, Volts, compile,
};
use std::f64::consts::TAU;

/// A typical studio-condenser sensitivity (10 mV/Pa) — the mic's fixed transducer constant. The
/// phantom tests below mostly care about the DC operating point, not the audio, so most build the
/// mic with an **unconnected** capsule (silent) and read the pedestal directly.
const SENS: f32 = 0.01;

/// A line-Z preamp (10 kΩ balanced in, 150 Ω out) with its 48V feed `engaged` or not.
fn preamp(gain: f32, engaged: bool) -> MicPreamp {
    MicPreamp::new(
        gain,
        Volts::new(10.0),
        InputZ::balanced(Ohms::new(10_000.0)),
        Ohms::new(150.0),
    )
    .with_phantom(engaged)
}

/// Build `mic → preamp` (optionally cabled) with the mic's **capsule unconnected** (silent), tap
/// the mic or the preamp, run one block, return it. With no sound the mic's audio is 0, so its
/// hot leg is the earned pedestal alone — which is what these DC-operating-point tests read.
fn run(engaged: bool, cable: Option<Cable>, tap_mic: bool) -> VoltageBuffer {
    let mut g = Graph::new();
    let mic = g.add(CondenserMic::new(SENS, Ohms::new(150.0)));
    let pre = g.add(preamp(100.0, engaged));
    match cable {
        Some(c) => g.connect_cabled(mic, 0, pre, 0, c),
        None => g.connect_ideal(mic, 0, pre, 0),
    };
    g.set_output(if tap_mic { mic } else { pre }, 0);
    let mut sched = compile(g, 8, rate(), 0).expect("mic → preamp builds");
    let mut out = VoltageBuffer::zeros(8, rate());
    sched.process(&mut out);
    out
}

#[test]
fn solved_operating_point_rides_the_wire() {
    // The P48 reference solve, no cable: V_dc = 48·12 700/(3 400 + 0 + 12 700) = 37.8634 V.
    // With the capsule unconnected there is no audio, so the mic's open-circuit hot leg is the
    // earned pedestal *alone* on every sample: 37.8634 V — power arrived from the patch, not from
    // any flag. (When a source is patched the ±½·audio wiggle rides on top of this same pedestal,
    // and the difference-first preamp rejects the pedestal — see
    // `preamp_recovers_audio_and_rejects_the_solved_pedestal`.)
    let out = run(true, None, true);
    for &v in out.as_slice() {
        assert_relative_eq!(v, 37.8634, epsilon = 1e-3);
    }
}

#[test]
fn cable_resistance_sags_the_operating_point() {
    // 100 Ω of cable series R joins the DC divider: 48·12 700/(3 400 + 100 + 12 700) =
    // 609 600/16 200 = 37.6296 V (no audio ⇒ the hot leg is exactly the pedestal). Sag is
    // solved physics, not a special case.
    let cable = Cable::new(Ohms::new(100.0), Farads::ZERO);
    let out = run(true, Some(cable), true);
    for &v in out.as_slice() {
        assert_relative_eq!(v, 37.6296, epsilon = 1e-3);
    }
}

#[test]
fn enough_cable_resistance_kills_the_mic() {
    // The dead-mic threshold, crossed by hand: V_dc < 35 needs R_cable > 48·12 700/35 − 16 100
    // = 17 417 − 16 100 = 1 317 Ω. At 1.5 kΩ: 48·12 700/(3 400 + 1 500 + 12 700) =
    // 609 600/17 600 = 34.64 V < 35 ⇒ the mic's electronics starve — both legs dead, so the
    // preamp difference is 0 too. (The supply is engaged; only the cable changed.)
    let cable = Cable::new(Ohms::new(1_500.0), Farads::ZERO);
    let mic_out = run(true, Some(cable), true);
    assert!(mic_out.as_slice().iter().all(|&v| v == 0.0), "mic dead");
    let pre_out = run(true, Some(cable), false);
    assert!(pre_out.as_slice().iter().all(|&v| v == 0.0), "chain silent");
}

#[test]
fn no_supply_or_disengaged_supply_is_a_dead_mic() {
    // A bare balanced receiver declares no supply ⇒ the mic's load resolves to an explicit
    // 0 V ⇒ dead. (Before the DC solve the mic self-asserted 48 V here — that flag is gone.)
    let mut g = Graph::new();
    let mic = g.add(CondenserMic::new(SENS, Ohms::new(150.0)));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    g.connect_ideal(mic, 0, rcv, 0);
    g.set_output(rcv, 0);
    let mut sched = compile(g, 8, rate(), 0).expect("supply-less chain builds");
    let mut out = VoltageBuffer::zeros(8, rate());
    sched.process(&mut out);
    assert!(out.as_slice().iter().all(|&v| v == 0.0), "no supply ⇒ dead");

    // A supply that exists but is switched off resolves 0 V just the same.
    let out = run(false, None, false);
    assert!(out.as_slice().iter().all(|&v| v == 0.0), "48V off ⇒ dead");
}

#[test]
fn engaged_supply_into_a_non_load_is_harmless() {
    // 48V engaged with a synth-like source plugged in: the source declares no phantom load, so
    // the solve is a no-op (the real-world "mostly harmless" case, simplified — no DC is
    // back-driven). Audio is untouched: 2 V through the ≈unity grounding edge
    // (10 000/(1 + 10 000) = 0.9999) at gain 1 ⇒ 1.9998 V, identical engaged or not.
    let run_src = |engaged: bool| {
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(2.0), Ohms::new(1.0)));
        let pre = g.add(preamp(1.0, engaged));
        g.connect_ideal(src, 0, pre, 0);
        g.set_output(pre, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("48V into a non-load builds");
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
        out
    };
    let on = run_src(true);
    let off = run_src(false);
    for &v in on.as_slice() {
        assert_relative_eq!(v, 1.9998, epsilon = 1e-3);
    }
    assert_eq!(on.as_slice(), off.as_slice(), "audio identical either way");
}

/// Build `source → mic → preamp` and tap the preamp; the vox radiates `src_pa` peak pascals at
/// its default pitch into the mic's capsule over an ideal acoustic edge.
fn run_miced(src_pa: f32, engaged: bool) -> VoltageBuffer {
    let mut g = Graph::new();
    let src = g.add(SoundSource::new(Pascals::new(src_pa)));
    let mic = g.add(CondenserMic::new(SENS, Ohms::new(150.0)));
    let pre = g.add(preamp(100.0, engaged));
    g.connect_ideal(src, 0, mic, 0); // acoustic edge (ideal air)
    g.connect_ideal(mic, 0, pre, 0); // balanced analog edge
    g.set_output(pre, 0);
    let mut sched = compile(g, 8, rate(), 0).expect("source → mic → preamp builds");
    let mut out = VoltageBuffer::zeros(8, rate());
    sched.process(&mut out);
    out
}

#[test]
fn preamp_recovers_audio_and_rejects_the_solved_pedestal() {
    // End-to-end micing: a vox at 2 Pa peak (default 220 Hz) → condenser (10 mV/Pa) ⇒ a
    // 2·0.01 = 0.02 V differential on the solved 37.8634 V pedestal → engaged preamp, gain 100,
    // rail ±10 V. The edge divider is 10 000/(150 + 10 000) = 0.985222 on every conductor, so the
    // preamp sees a 37.30 V common-mode pedestal and a 0.0197044 V differential. Difference-first
    // rejects the pedestal *before* the rail (clamp-first would pin both legs at ±10 and
    // annihilate the audio). The differential is the transduced sine `0.02·sin(2π·220·k/384000)`,
    // so the recovered audio is that sine scaled by 100 · 0.985222 = 98.5222 ⇒ amplitude
    // 1.97044 V, with **zero DC** (no trace of the 37 V pedestal). Toggling 48V off (a rebuild —
    // the switch is structural) ⇒ dead mic ⇒ silence.
    let out = run_miced(2.0, true);
    for (k, &v) in out.as_slice().iter().enumerate() {
        let want = 1.970_44 * (TAU * 220.0 * k as f64 / 384_000.0).sin();
        // f32 note: recovered from two ~37.3 V legs, so ~µV-scale cancellation noise ×100.
        assert_relative_eq!(f64::from(v), want, epsilon = 1e-3);
    }
    let out = run_miced(2.0, false);
    assert!(out.as_slice().iter().all(|&v| v == 0.0));
}

#[test]
fn fan_in_of_two_engaged_supplies_is_rejected() {
    // One mic output fanned into *two* engaged supplies is the documented-unresolved bias
    // network: two feeds in parallel. Rejected legibly at compile, never solved wrong.
    let mut g = Graph::new();
    let mic = g.add(CondenserMic::new(SENS, Ohms::new(150.0)));
    let pre_a = g.add(preamp(100.0, true));
    let pre_b = g.add(preamp(100.0, true));
    g.connect_ideal(mic, 0, pre_a, 0);
    g.connect_ideal(mic, 0, pre_b, 0);
    g.set_output(pre_a, 0);
    assert_eq!(
        compile(g, 8, rate(), 0).err(),
        Some(CompileError::PhantomFanIn { node: 0, port: 0 })
    );
}

#[test]
fn one_engaged_supply_among_passive_consumers_resolves() {
    // Fan-out into one engaged supply + a passive receiver is fine: the load resolves from the
    // single feed (37.8634 V, no cable) and the mic runs. With the capsule unconnected there is
    // no audio, so the mic's open-circuit hot leg is the earned pedestal alone, 37.8634 V.
    let mut g = Graph::new();
    let mic = g.add(CondenserMic::new(SENS, Ohms::new(150.0)));
    let pre = g.add(preamp(100.0, true));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    g.connect_ideal(mic, 0, pre, 0);
    g.connect_ideal(mic, 0, rcv, 0);
    g.set_output(mic, 0);
    let mut sched = compile(g, 8, rate(), 0).expect("one supply among passives resolves");
    let mut out = VoltageBuffer::zeros(8, rate());
    sched.process(&mut out);
    assert_relative_eq!(out.get(0).get(), 37.8634, epsilon = 1e-3);
}

#[test]
fn miced_chain_is_deterministic() {
    // Structural determinism (no ambient entropy): two identically-built `source → mic → preamp`
    // schedules render bit-for-bit identical buffers — the source's phase accumulator, the
    // transduction, and the solved pedestal are all reproducible.
    let a = run_miced(2.0, true);
    let b = run_miced(2.0, true);
    assert_eq!(a.as_slice(), b.as_slice(), "same build ⇒ identical buffers");
}
