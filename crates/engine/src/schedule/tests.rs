mod compile_and_solve {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::graph::NodeId;
    use crate::node::{GainStage, PassiveSum, TestSource};
    use crate::signal::Volts;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn gain(g: f32) -> GainStage {
        GainStage::new(
            g,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
    }

    #[test]
    fn source_gain_sum_chain_matches_hand_calc() {
        // source(1.0 V, 100 Ω) → gain(×2) → sum(1 input). No cables (ideal wires), DC.
        //   edge s→g:  10000/(100+10000)  = 0.990099  → gain in  = 0.990099 V
        //   gain out (open-circuit):       0.990099 × 2 = 1.980198 V  (below the 10 V rail)
        //   edge g→sum: 10000/(150+10000) = 0.985222  → sum in   = 1.980198 × 0.985222 = 1.950931 V
        //   sum out (1 input, unity)      = 1.950931 V  ← the tapped output
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        let amp = g.add(gain(2.0));
        let sum = g.add(PassiveSum::new(
            vec![InputZ::new(Ohms::new(10_000.0))],
            Ohms::new(150.0),
        ));
        g.connect_ideal(src, 0, amp, 0);
        g.connect_ideal(amp, 0, sum, 0);
        g.set_output(sum, 0);

        let mut sched = compile(g, 8, rate(), 0).expect("valid chain");
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
        for &v in out.as_slice() {
            assert_relative_eq!(v, 1.950931, epsilon = 1e-4);
        }
    }

    #[test]
    fn fan_out_then_sum_matches_hand_calc() {
        // source(1.0 V, 100 Ω) fans out to two ×2 gains, summed.
        //   fan-out: two 10 kΩ in parallel = 5 kΩ; node = 5000/5100 = 0.980392
        //     → each gain in = 0.980392 V; ×2 = 1.960784 V
        //   each edge gain→sum: 10000/(150+10000) = 0.985222 → 1.960784 × 0.985222 = 1.931807 V
        //   sum of the two inputs = 3.863614 V
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        let a = g.add(gain(2.0));
        let b = g.add(gain(2.0));
        let sum = g.add(PassiveSum::new(
            vec![
                InputZ::new(Ohms::new(10_000.0)),
                InputZ::new(Ohms::new(10_000.0)),
            ],
            Ohms::new(150.0),
        ));
        g.connect_ideal(src, 0, a, 0);
        g.connect_ideal(src, 0, b, 0);
        g.connect_ideal(a, 0, sum, 0);
        g.connect_ideal(b, 0, sum, 1);
        g.set_output(sum, 0);

        let mut sched = compile(g, 4, rate(), 0).expect("valid fan-out chain");
        let mut out = VoltageBuffer::zeros(4, rate());
        sched.process(&mut out);
        for &v in out.as_slice() {
            assert_relative_eq!(v, 3.863614, epsilon = 1e-4);
        }
    }

    #[test]
    fn edge_gain_exposes_the_baked_loading_divider() {
        // source(100 Ω) → gain → sum: the two analog edges' baked loading gains, by graph edge
        // order. edge 0 (src→gain): 10000/(100+10000) = 0.990099; edge 1 (gain→sum):
        // 10000/(150+10000) = 0.985222 (same dividers as `source_gain_sum_chain_matches_hand_calc`).
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        let amp = g.add(gain(2.0));
        let sum = g.add(PassiveSum::new(
            vec![InputZ::new(Ohms::new(10_000.0))],
            Ohms::new(150.0),
        ));
        g.connect_ideal(src, 0, amp, 0); // edge 0
        g.connect_ideal(amp, 0, sum, 0); // edge 1
        g.set_output(sum, 0);

        let sched = compile(g, 8, rate(), 0).expect("valid chain");
        assert_relative_eq!(
            sched.edge_gain(0).expect("edge 0"),
            0.990_099,
            epsilon = 1e-5
        );
        assert_relative_eq!(
            sched.edge_gain(1).expect("edge 1"),
            0.985_222,
            epsilon = 1e-5
        );
        assert!(sched.edge_gain(2).is_none(), "no third edge");
    }

    #[test]
    fn rejects_missing_output() {
        let mut g = Graph::new();
        g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        assert_eq!(compile(g, 8, rate(), 0).err(), Some(CompileError::NoOutput));
    }

    #[test]
    fn rejects_a_cycle() {
        // a → b → a is a loop.
        let mut g = Graph::new();
        let a = g.add(gain(1.0));
        let b = g.add(gain(1.0));
        g.connect_ideal(a, 0, b, 0);
        g.connect_ideal(b, 0, a, 0);
        g.set_output(b, 0);
        assert_eq!(compile(g, 8, rate(), 0).err(), Some(CompileError::Cycle));
    }

    #[test]
    fn rejects_double_connected_input() {
        let mut g = Graph::new();
        let s1 = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        let s2 = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        let sum = g.add(PassiveSum::new(
            vec![InputZ::new(Ohms::new(10_000.0))],
            Ohms::new(150.0),
        ));
        g.connect_ideal(s1, 0, sum, 0);
        g.connect_ideal(s2, 0, sum, 0); // same input port 0
        g.set_output(sum, 0);
        assert_eq!(
            compile(g, 8, rate(), 0).err(),
            Some(CompileError::InputAlreadyConnected { node: 2, port: 0 })
        );
    }

    #[test]
    fn rejects_output_port_out_of_range() {
        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        g.set_output(src, 5); // a source has only output port 0
        assert_eq!(
            compile(g, 8, rate(), 0).err(),
            Some(CompileError::OutputPortOutOfRange { node: 0, port: 5 })
        );
    }

    #[test]
    fn rejects_unknown_node() {
        let mut g = Graph::new();
        g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        g.set_output(NodeId(9), 0); // no such node
        assert_eq!(
            compile(g, 8, rate(), 0).err(),
            Some(CompileError::NodeOutOfRange { node: 9 })
        );
    }
}

/// Device noise floors emerging from the voltage math, on real compiled chains.
///
/// Tests are the oracle: you can't hear a µV noise floor, so each assert is a number
/// computed by hand, with the calc in a comment. RMS converges to the true `σ` only in the
/// limit, so the tolerances are the finite-sample sampling error (`~1/√(2N)`), not slop.
mod noise_phenomena {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::node::{GainStage, TestSource};
    use crate::noise::NoiseDensity;
    use crate::signal::Volts;
    use crate::test_util::rms;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

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
}

/// A DC offset riding the AC, removed by a DC-blocking high-pass, on a compiled patch.
/// Tests are the oracle: the numbers are hand-computed, with the calc inline.
mod dc_phenomena {
    use super::super::*;
    use crate::electrical::{Farads, Ohms};
    use crate::node::DcBlocker;
    use crate::signal::Volts;
    use crate::test_util::{SineSource, rms};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

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
}

/// Headroom & clipping at the rail voltage, and the harmonic distortion that emerges, on compiled
/// patches. Tests are the oracle: you can't hear a clip onset or count harmonics by ear, so each
/// number is hand-computed with the calc inline.
mod clipping_phenomena {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::level::headroom_db;
    use crate::node::GainStage;
    use crate::signal::Volts;
    use crate::test_util::{SineSource, tone_amplitude};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

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
}

/// Two-conductor balanced lines: a differential signal survives the trip, and a common-mode offset
/// cancels at the receiver difference (`V+ − V−`). The rejection *emerges* from the subtraction; it
/// is not a flag. Tests are the oracle — numbers hand-computed.
mod balanced_phenomena {
    use super::super::*;
    use crate::electrical::{Farads, Ohms};
    use crate::node::{BalancedDriver, BalancedReceiver, DcBlocker, GainStage, TestSource};
    use crate::signal::Volts;
    use crate::test_util::{BalancedTestSource, SineSource, rms};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

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
        // driver's 2-conductor output infers it to 2 and replicates it per leg. Before the lift
        // this very wiring would be a LaneCountMismatch — now an ordinary processor just composes,
        // "balanced" never a label. (This is the mechanism phantom rides on in 1.5.3.)
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
        let mut sched =
            compile(g, len, rate(), 0).expect("balanced chain with a lifted DC blocker");
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
}

/// Cable pickup: broadband interference (EMI) coupling onto the wire as a noise voltage. On an
/// unbalanced edge it lands on the signal at µV scale (the balanced *rejection* of it is the CMRR
/// case). Tests are the oracle: the floor is a hand-computed number, with the calc inline.
mod pickup_phenomena {
    use super::super::*;
    use crate::electrical::{Cable, Farads};
    use crate::node::{GainStage, TestSource};
    use crate::signal::Volts;
    use crate::test_util::rms;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

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
}

/// Common-mode rejection: the same cable pickup that contaminates an unbalanced line cancels at a
/// balanced receiver's difference. **Ideal rejection only** — both conductors carry the *identical*
/// common-mode draw, so `V+ − V−` cancels it to **bit-exact zero** (infinite CMRR). Finite CMRR is
/// leg *asymmetry*, not modeled. Tests are the oracle.
mod cmrr_phenomena {
    use super::super::*;
    use crate::electrical::{Cable, Farads};
    use crate::node::{BalancedDriver, BalancedReceiver, GainStage, TestSource};
    use crate::signal::Volts;
    use crate::test_util::rms;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

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
}

/// Ground-loop hum: a 50/60 Hz common-mode tone coupled onto the cable. Audible on
/// an unbalanced line, rejected (bit-exact) on a balanced one — the "lift the ground" lesson. It
/// rides the same edge-injection seam as pickup, just a deterministic generator instead of noise.
mod hum_phenomena {
    use super::super::*;
    use crate::electrical::{Cable, Farads};
    use crate::node::{BalancedDriver, BalancedReceiver, GainStage, TestSource};
    use crate::signal::Volts;
    use crate::test_util::tone_amplitude;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

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
}

/// Phantom power: +48 V common-mode DC powering a condenser mic. The mic puts it on
/// the line common-mode (asserted at the node in `node::condenser`); here, end-to-end, a balanced
/// receiver recovers just the audio and rejects the 48 V, and an unpowered mic is silent. Phantom
/// rides the *same* common-mode rejection as pickup and hum — not a special case.
mod phantom_phenomena {
    use super::super::*;
    use crate::node::{BalancedReceiver, CondenserMic};
    use crate::signal::Volts;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn receiver_recovers_audio_and_rejects_phantom() {
        // Powered mic: V+ = 48 + 1, V− = 48 − 1 (a 2 V differential signal on the +48 V common-mode
        // pedestal). The balanced receiver returns V+ − V− ≈ 2 V — the audio — and the 48 V, being
        // common-mode, cancels. Phantom and signal share one wire pair, separated by the difference.
        let mut g = Graph::new();
        let mic = g.add(CondenserMic::new(Volts::new(2.0), Ohms::new(150.0)));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect_ideal(mic, 0, rcv, 0);
        g.set_output(rcv, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("phantom mic chain");
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
        for &v in out.as_slice() {
            assert_relative_eq!(v, 2.0, epsilon = 1e-3);
        }
    }

    #[test]
    fn unpowered_mic_yields_silence() {
        // No phantom ⇒ the mic produces nothing on either conductor ⇒ the receiver difference is 0.
        let mut g = Graph::new();
        let mic = g.add(CondenserMic::new(Volts::new(2.0), Ohms::new(150.0)).unpowered());
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        g.connect_ideal(mic, 0, rcv, 0);
        g.set_output(rcv, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("unpowered mic chain");
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
        assert!(out.as_slice().iter().all(|&v| v == 0.0));
    }
}

/// The **digital carrier seam**: the schedule pool carries `Lane::Sample` lanes sized to
/// `block_len / M`, a digital edge is a same-clock-domain copy, and `compile` rejects cross-domain
/// edges, non-integer rates, indivisible block lengths, and clock crossings. These test nodes are
/// pure digital scaffolding to exercise the plumbing, without a converter. Tests inspect the private
/// pools (white-box).
mod digital_seam {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::node::TestSource;
    use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
    use crate::signal::{BitDepth, SampleRate, Volts};

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A mono digital format at `rate_hz`, 24-bit.
    fn fmt(rate_hz: f64) -> AudioFormat {
        AudioFormat::new(SampleRate::new(rate_hz), BitDepth::new(24), 1)
    }

    /// A digital source: no inputs, one digital output filled with a constant sample value.
    struct DigitalSource {
        level: f32,
        outputs: [OutputPort; 1],
    }
    impl DigitalSource {
        fn new(level: f32, format: AudioFormat) -> Self {
            Self {
                level,
                outputs: [DigitalFace::new(format).into()],
            }
        }
    }
    impl Node for DigitalSource {
        fn inputs(&self) -> &[InputPort] {
            &[]
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
            outputs[0].sample_mut().fill(self.level);
        }
    }

    /// A digital sink: one digital input, no outputs. A no-op — tests read its input lane.
    struct DigitalSink {
        inputs: [InputPort; 1],
    }
    impl DigitalSink {
        fn new(format: AudioFormat) -> Self {
            Self {
                inputs: [DigitalFace::new(format).into()],
            }
        }
    }
    impl Node for DigitalSink {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &[]
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], _outputs: &mut [Lane]) {}
    }

    #[test]
    fn digital_lanes_are_sized_by_the_decimation_factor() {
        // analog 384 kHz, digital 48 kHz ⇒ M = 8; a block of 16 analog samples ⇒ 2 digital samples.
        let mut g = Graph::new();
        let src = g.add(DigitalSource::new(0.5, fmt(48_000.0)));
        let sink = g.add(DigitalSink::new(fmt(48_000.0)));
        g.connect_ideal(src, 0, sink, 0);
        g.set_output(src, 0); // digital tap; this test inspects the pool, never calls process
        let sched = compile(g, 16, analog_rate(), 0).expect("valid digital chain");

        let sample_lanes: Vec<&Lane> = sched
            .output_pool
            .iter()
            .chain(sched.input_pool.iter())
            .filter(|l| matches!(l, Lane::Sample(_)))
            .collect();
        assert_eq!(
            sample_lanes.len(),
            2,
            "one source-output + one sink-input sample lane"
        );
        for lane in sample_lanes {
            assert_eq!(lane.domain(), Domain::DigitalAudio);
            assert_eq!(lane.len(), 2, "digital lane is block_len / M = 16 / 8");
        }
    }

    #[test]
    fn digital_route_copies_samples() {
        // A separate analog node provides the (voltage) output tap so `process` can run; the
        // digital source → sink component runs alongside, and its DigitalRoute copies the samples.
        let mut g = Graph::new();
        let atap = g.add(TestSource::new(Volts::new(1.0), Ohms::new(150.0)));
        g.set_output(atap, 0);
        let src = g.add(DigitalSource::new(0.5, fmt(48_000.0)));
        let sink = g.add(DigitalSink::new(fmt(48_000.0)));
        g.connect_ideal(src, 0, sink, 0);

        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid mixed chain");
        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process(&mut out);

        // The analog tap is unaffected by the digital component.
        assert!(out.as_slice().iter().all(|&v| (v - 1.0).abs() < 1e-3));
        // The sink's input sample lane received the source's 0.5 via the DigitalRoute copy.
        let sink_in = sched
            .input_pool
            .iter()
            .find(|l| matches!(l, Lane::Sample(_)))
            .expect("a digital input lane");
        assert!(sink_in.sample().as_slice().iter().all(|&s| s == 0.5));
    }

    #[test]
    fn rejects_domain_mismatch() {
        // An analog output into a digital input: no physics bridges domains on a wire.
        let mut g = Graph::new();
        let asrc = g.add(TestSource::new(Volts::new(1.0), Ohms::new(150.0)));
        let dsink = g.add(DigitalSink::new(fmt(48_000.0)));
        g.connect_ideal(asrc, 0, dsink, 0);
        g.set_output(asrc, 0);
        assert_eq!(
            compile(g, 16, analog_rate(), 0).err(),
            Some(CompileError::DomainMismatch {
                from_node: 0,
                from_port: 0,
                to_node: 1,
                to_port: 0,
            })
        );
    }

    #[test]
    fn rejects_non_integer_rate() {
        // 44.1 kHz does not integer-divide 384 kHz (384000 / 44100 = 8.707…).
        let mut g = Graph::new();
        let src = g.add(DigitalSource::new(0.5, fmt(44_100.0)));
        g.set_output(src, 0);
        let err = compile(g, 16, analog_rate(), 0).err().unwrap();
        assert!(
            matches!(err, CompileError::RateIndivisible { node: 0, .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_indivisible_block_len() {
        // 48 kHz ⇒ M = 8; a block of 10 isn't a multiple of 8.
        let mut g = Graph::new();
        let src = g.add(DigitalSource::new(0.5, fmt(48_000.0)));
        g.set_output(src, 0);
        assert_eq!(
            compile(g, 10, analog_rate(), 0).err(),
            Some(CompileError::BlockLenIndivisible {
                node: 0,
                block_len: 10,
                factor: 8,
            })
        );
    }

    #[test]
    fn rejects_clock_crossing() {
        // Both ends digital (domain matches) but at different rates ⇒ a resample, deferred.
        let mut g = Graph::new();
        let src = g.add(DigitalSource::new(0.5, fmt(48_000.0)));
        let sink = g.add(DigitalSink::new(fmt(96_000.0)));
        g.connect_ideal(src, 0, sink, 0);
        g.set_output(src, 0);
        assert_eq!(
            compile(g, 16, analog_rate(), 0).err(),
            Some(CompileError::ClockCrossingUnsupported {
                from_node: 0,
                to_node: 1,
            })
        );
    }
}

/// The converter **artifacts**, on real compiled chains through the carrier seam:
/// calibration (+4 dBu = −18 dBFS), aliasing fold-back from a weak anti-alias filter, the TPDF
/// quantization noise floor (RMS `Δ/2`, SNR ≈ `6.02·N − 3`), and the end-to-end capstone
/// `analog → AD → digital → DA → analog`. Tests are the oracle: every number is a hand
/// calc, inline. Digital-domain assertions read the AD's output sample lane (white-box, as in
/// [`digital_seam`]); the capstone taps the DA's analog output through `process`.
mod converter_phenomena {
    use super::super::*;
    use crate::electrical::{InputZ, Ohms};
    use crate::level::{dbu_to_volts, sample_to_dbfs};
    use crate::node::{AdConverter, BalancedDriver, BalancedReceiver, DaConverter, TestSource};
    use crate::signal::{BitDepth, SampleRate, Volts};
    use crate::test_util::{SineSource, rms, tone_amplitude};
    use approx::assert_relative_eq;

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn digital_rate() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    /// 48 kHz expressed as an `AnalogRate`, so [`tone_amplitude`]'s DFT runs at the digital rate
    /// when it reads the AD's 48 kHz output samples (it only needs the sample period).
    fn digital_as_analog() -> AnalogRate {
        AnalogRate::new(48_000.0)
    }

    /// Drive a configured `ad` from `src` over one block and return its digital output samples.
    /// The AD's output is digital, so it can't be the schedule's voltage tap; a standalone silent
    /// analog source supplies that tap, and the AD samples are read white-box from the pool (the
    /// only `Lane::Sample` there, since the chain has a single converter).
    fn ad_samples(src: SineSource, ad: AdConverter, block_len: usize, seed: u64) -> Vec<f32> {
        let mut g = Graph::new();
        let s = g.add(src);
        let a = g.add(ad);
        g.connect_ideal(s, 0, a, 0);
        let tap = g.add(TestSource::new(Volts::new(0.0), Ohms::new(150.0)));
        g.set_output(tap, 0);

        let mut sched = compile(g, block_len, analog_rate(), seed).expect("valid converter chain");
        let mut sink = VoltageBuffer::zeros(block_len, analog_rate());
        sched.process(&mut sink);
        sched
            .output_pool
            .iter()
            .find(|l| matches!(l, Lane::Sample(_)))
            .expect("an AD output sample lane")
            .sample()
            .as_slice()
            .to_vec()
    }

    #[test]
    fn plus_4_dbu_calibrates_to_minus_18_dbfs_through_the_seam() {
        // +4 dBu = 1.2283 V RMS = 1.7372 V peak. Source 1 Ω into the AD's 1 MΩ input ⇒ divider
        // 1e6/(1+1e6) ≈ 0.999999, so the AD sees the full peak. Against a 13.80 V-peak reference:
        //   1.7372 / 13.80 = 0.12589 normalized peak ⇒ 20·log10(0.12589) = −18.0 dBFS.
        let peak = dbu_to_volts(4.0).get() * core::f32::consts::SQRT_2;
        let src = SineSource::new(1_000.0, Volts::new(peak), Volts::new(0.0), Ohms::new(1.0));
        let ad = AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(13.80),
            Ohms::new(1e6),
        );
        // 7680 analog ⇒ 960 digital = 20 whole cycles of 1 kHz at 48 kHz (48 samples/cycle).
        let out = ad_samples(src, ad, 7_680, 1);
        let amp = tone_amplitude(&out[480..], 1_000.0, digital_as_analog());
        assert_relative_eq!(sample_to_dbfs(amp), -18.0, epsilon = 0.1);
    }

    #[test]
    fn a_weak_anti_alias_filter_folds_back_more_than_a_strong_one() {
        // A 40 kHz tone is above the 24 kHz decimated Nyquist; unrejected it folds to 48 − 40 =
        // 8 kHz. A steep filter (the default 161 taps) attenuates it deep into the stopband; a
        // short one (15 taps) can't, so far more leaks back. Measure the 8 kHz alias bin.
        let tone = || SineSource::new(40_000.0, Volts::new(0.5), Volts::new(0.0), Ohms::new(1.0));
        let strong = AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(1.0),
            Ohms::new(1e6),
        );
        let weak = AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(1.0),
            Ohms::new(1e6),
        )
        .with_aa_taps(15);
        // 12288 analog ⇒ 1536 digital = 256 whole cycles of 8 kHz at 48 kHz (6 samples/cycle).
        let s = ad_samples(tone(), strong, 12_288, 1);
        let w = ad_samples(tone(), weak, 12_288, 1);
        let alias_strong = tone_amplitude(&s[200..], 8_000.0, digital_as_analog());
        let alias_weak = tone_amplitude(&w[200..], 8_000.0, digital_as_analog());
        assert!(
            alias_weak > alias_strong * 5.0,
            "a weak (short) AA filter must fold back far more: weak {alias_weak} vs strong \
             {alias_strong}"
        );
    }

    #[test]
    fn the_quantization_noise_floor_is_delta_over_two() {
        // TPDF-dithered quantization of silence: the output is pure dither noise of variance
        //   Δ²/12 (quantization) + Δ²/6 (TPDF, two ±½-LSB draws) = Δ²/4  ⇒  RMS = Δ/2,
        // independent of the signal. For a ±1.0 full scale Δ = 1/2^(N−1), so the floor is 2^−N:
        //   16-bit ⇒ 2^−16 = 1.526e-5;  24-bit ⇒ 2^−24 = 5.96e-8 (256× quieter).
        fn floor(bits: u32, seed: u64) -> f32 {
            let silence =
                SineSource::new(1_000.0, Volts::new(0.0), Volts::new(0.0), Ohms::new(1.0));
            let ad = AdConverter::new(
                digital_rate(),
                BitDepth::new(bits),
                Volts::new(1.0),
                Ohms::new(1e6),
            );
            // 80000 analog ⇒ 10000 digital samples: RMS converges to ~1% (≈ 1/√(2N)).
            rms(&ad_samples(silence, ad, 80_000, seed))
        }
        let floor_16 = floor(16, 1);
        let floor_24 = floor(24, 2);

        // Each floor matches Δ/2 = 2^−N, and more bits buy a much lower floor.
        assert_relative_eq!(floor_16, 2.0_f32.powi(-16), max_relative = 0.05);
        assert_relative_eq!(floor_24, 2.0_f32.powi(-24), max_relative = 0.05);
        assert!(
            floor_16 > floor_24 * 100.0,
            "more bits ⇒ a far lower noise floor: 16-bit {floor_16} vs 24-bit {floor_24}"
        );

        // SNR of a full-scale sine (RMS 1/√2) against the measured 16-bit floor:
        //   20·log10((1/√2) / floor) ≈ 6.02·16 − 3.01 = 93.3 dB — the flat-noise SNR law.
        let snr = 20.0 * ((1.0 / core::f32::consts::SQRT_2) / floor_16).log10();
        assert_relative_eq!(snr, 6.0206 * 16.0 - 3.01, epsilon = 0.5);
    }

    #[test]
    fn capstone_balanced_analog_through_ad_da_back_to_analog() {
        // The whole converter chain, balanced-fronted, through the generalized carrier seam:
        //   sine(2 V) → balanced driver → balanced receiver → AD → DA → analog tap.
        // Every analog face is near-ideal (1 Ω out into ≥ 1 MΩ in) so dividers ≈ unity: the
        // receiver returns the 2 V differential single-ended; the AD digitizes it (2 V / 10 V
        // reference = 0.2 full scale); the DA reconstructs it (0.2 × 10 V = 2 V). A 1 kHz tone,
        // deep in the passband, should survive end-to-end at ≈ 2 V — the analog physics of
        // Stories 1.2–1.5 intact across the digital round trip.
        let mut g = Graph::new();
        let src = g.add(SineSource::new(
            1_000.0,
            Volts::new(2.0),
            Volts::new(0.0),
            Ohms::new(1.0),
        ));
        let drv = g.add(BalancedDriver::new(
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(1.0)));
        let ad = g.add(AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(1e6),
        ));
        let da = g.add(DaConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(150.0),
        ));
        g.connect_ideal(src, 0, drv, 0);
        g.connect_ideal(drv, 0, rcv, 0);
        g.connect_ideal(rcv, 0, ad, 0);
        g.connect_ideal(ad, 0, da, 0);
        g.set_output(da, 0);

        // 15360 analog ⇒ 1920 digital = 40 whole cycles of 1 kHz; drop the first half as the
        // combined AA + reconstruction filter transient (their group delays add).
        let block = 15_360;
        let mut sched = compile(g, block, analog_rate(), 0).expect("valid capstone chain");
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process(&mut out);
        let amp = tone_amplitude(&out.as_slice()[block / 2..], 1_000.0, analog_rate());
        assert_relative_eq!(amp, 2.0, max_relative = 0.02);
    }
}

/// The **events carrier seam** (the third carrier): the schedule pool carries `Lane::Events` lanes
/// pre-allocated to a per-port capacity, an event edge is a sparse `EventRoute` copy, and `compile`
/// rejects an event↔non-event edge as a `DomainMismatch`. These test nodes are pure scaffolding to
/// exercise the plumbing, mirroring [`digital_seam`]. Tests inspect the private pools (white-box).
mod event_seam {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::node::{GainStage, TestSource};
    use crate::port::{EventFace, InputPort, OutputPort};
    use crate::signal::{EventMessage, TimedEvent, Volts};

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A note-on at offset 0 — the message the scaffolding source emits each block.
    fn note_on() -> TimedEvent {
        TimedEvent {
            offset: 0,
            message: EventMessage::NoteOn {
                note: 69, // A4
                velocity: 100,
            },
        }
    }

    /// An event source: no inputs, one event output it (re)fills with a single note-on each block.
    struct EventSource {
        outputs: [OutputPort; 1],
    }
    impl EventSource {
        fn new(capacity: usize) -> Self {
            Self {
                outputs: [EventFace::new(capacity).into()],
            }
        }
    }
    impl Node for EventSource {
        fn inputs(&self) -> &[InputPort] {
            &[]
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
            let ev = outputs[0].events_mut();
            ev.clear(); // a producer owns its lane each block — clear stale events, then emit.
            ev.push(note_on());
        }
    }

    /// An event sink: one event input, no outputs. A no-op — tests read its input lane.
    struct EventSink {
        inputs: [InputPort; 1],
    }
    impl EventSink {
        fn new(capacity: usize) -> Self {
            Self {
                inputs: [EventFace::new(capacity).into()],
            }
        }
    }
    impl Node for EventSink {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &[]
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], _outputs: &mut [Lane]) {}
    }

    /// A silent analog source supplies the (voltage) output tap so `process` can run, since the
    /// tap must be a voltage lane; the event component runs alongside it.
    fn analog_tap(g: &mut Graph) {
        let tap = g.add(TestSource::new(Volts::new(0.0), Ohms::new(150.0)));
        g.set_output(tap, 0);
    }

    #[test]
    fn event_lanes_are_sized_to_their_capacity() {
        // The pool holds one source-output and one sink-input event lane, each pre-allocated to its
        // port's capacity (the bound the hot path never grows past), and both start empty.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(32));
        let sink = g.add(EventSink::new(16));
        g.connect_ideal(src, 0, sink, 0);
        analog_tap(&mut g);
        let sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");

        let event_lanes: Vec<&Lane> = sched
            .output_pool
            .iter()
            .chain(sched.input_pool.iter())
            .filter(|l| matches!(l, Lane::Events(_)))
            .collect();
        assert_eq!(
            event_lanes.len(),
            2,
            "one source-output + one sink-input event lane"
        );
        let caps: Vec<usize> = event_lanes
            .iter()
            .map(|l| {
                assert_eq!(l.domain(), Domain::Events);
                assert!(l.is_empty(), "event lanes start empty");
                l.events().capacity()
            })
            .collect();
        assert!(caps.contains(&32) && caps.contains(&16), "got {caps:?}");
    }

    #[test]
    fn event_route_copies_events() {
        // Source emits a note-on; the EventRoute copies it into the sink's input lane.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(32));
        let sink = g.add(EventSink::new(32));
        g.connect_ideal(src, 0, sink, 0);
        analog_tap(&mut g);

        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process(&mut out);

        let sink_in = sched
            .input_pool
            .iter()
            .find(|l| matches!(l, Lane::Events(_)))
            .expect("a sink input event lane");
        assert_eq!(sink_in.events().as_slice(), &[note_on()]);

        // Running again must not accumulate — the source clears and the route overwrites.
        sched.process(&mut out);
        let sink_in = sched
            .input_pool
            .iter()
            .find(|l| matches!(l, Lane::Events(_)))
            .expect("a sink input event lane");
        assert_eq!(
            sink_in.events().len(),
            1,
            "events must not accumulate across blocks"
        );
    }

    #[test]
    fn event_output_fans_out_to_several_sinks() {
        // One event source into two sinks: each edge is its own EventRoute, so both receive it.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(8));
        let a = g.add(EventSink::new(8));
        let b = g.add(EventSink::new(8));
        g.connect_ideal(src, 0, a, 0);
        g.connect_ideal(src, 0, b, 0);
        analog_tap(&mut g);

        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event fan-out");
        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process(&mut out);

        let received: Vec<&Lane> = sched
            .input_pool
            .iter()
            .filter(|l| matches!(l, Lane::Events(_)))
            .collect();
        assert_eq!(received.len(), 2);
        for lane in received {
            assert_eq!(lane.events().as_slice(), &[note_on()]);
        }
    }

    #[test]
    fn rejects_event_to_analog_domain_mismatch() {
        // An event output into an analog input: no carrier bridges domains on a wire.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(8));
        let amp = g.add(GainStage::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        ));
        g.connect_ideal(src, 0, amp, 0);
        g.set_output(amp, 0);
        assert_eq!(
            compile(g, 16, analog_rate(), 0).err(),
            Some(CompileError::DomainMismatch {
                from_node: 0,
                from_port: 0,
                to_node: 1,
                to_port: 0,
            })
        );
    }

    // --- External event queue + timestamped delivery into open event inputs. ---

    fn note_on_msg(note: u8) -> EventMessage {
        EventMessage::NoteOn {
            note,
            velocity: 100,
        }
    }
    fn note_off_msg(note: u8) -> EventMessage {
        EventMessage::NoteOff { note }
    }

    /// The single event input lane of these one-sink chains (the only `Events` lane in the input
    /// pool). White-box, as elsewhere in this file.
    fn sink_events(sched: &Schedule) -> &EventBuffer {
        sched
            .input_pool
            .iter()
            .find(|l| matches!(l, Lane::Events(_)))
            .expect("an event input lane")
            .events()
    }

    #[test]
    fn external_events_land_at_their_offsets() {
        // Two events due this block land at the matching block-relative offsets, in order.
        let mut g = Graph::new();
        let sink = g.add(EventSink::new(8));
        analog_tap(&mut g);
        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let id = sched.event_input(sink, 0).expect("open event input");

        let mut q = EventQueue::with_capacity(8);
        q.push(3, id, note_on_msg(69));
        q.push(10, id, note_off_msg(69));

        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process_with_events(&mut out, &mut q);

        assert_eq!(
            sink_events(&sched).as_slice(),
            &[
                TimedEvent {
                    offset: 3,
                    message: note_on_msg(69)
                },
                TimedEvent {
                    offset: 10,
                    message: note_off_msg(69)
                },
            ]
        );
        assert!(q.is_empty(), "both events were due and consumed");
    }

    #[test]
    fn events_bucket_across_blocks() {
        // An event past this block stays queued, then arrives next block at its rebased offset:
        // absolute 20 with block_len 16 ⇒ block 1, offset 20 − 16 = 4.
        let mut g = Graph::new();
        let sink = g.add(EventSink::new(8));
        analog_tap(&mut g);
        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let id = sched.event_input(sink, 0).expect("open event input");

        let mut q = EventQueue::with_capacity(8);
        q.push(3, id, note_on_msg(60));
        q.push(20, id, note_on_msg(62));

        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process_with_events(&mut out, &mut q);
        assert_eq!(
            sink_events(&sched).as_slice(),
            &[TimedEvent {
                offset: 3,
                message: note_on_msg(60)
            }]
        );
        assert_eq!(q.len(), 1, "the second event is not yet due");

        sched.process_with_events(&mut out, &mut q);
        assert_eq!(
            sink_events(&sched).as_slice(),
            &[TimedEvent {
                offset: 4,
                message: note_on_msg(62)
            }]
        );
        assert!(q.is_empty());
    }

    #[test]
    fn a_late_event_clamps_to_offset_zero() {
        // After one block the clock is at sample 16; an event stamped before that (a late arrival)
        // fires immediately, at offset 0, rather than being dropped or panicking.
        let mut g = Graph::new();
        let sink = g.add(EventSink::new(8));
        analog_tap(&mut g);
        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let id = sched.event_input(sink, 0).expect("open event input");

        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process(&mut out); // advance the clock to sample 16

        let mut q = EventQueue::with_capacity(4);
        q.push(5, id, note_on_msg(60)); // 5 < 16 — late
        sched.process_with_events(&mut out, &mut q);
        assert_eq!(
            sink_events(&sched).as_slice(),
            &[TimedEvent {
                offset: 0,
                message: note_on_msg(60)
            }]
        );
    }

    #[test]
    fn open_event_inputs_are_cleared_each_block() {
        // Events delivered one block don't linger into the next: the open input is cleared, so a
        // following block with no events sees silence.
        let mut g = Graph::new();
        let sink = g.add(EventSink::new(8));
        analog_tap(&mut g);
        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");
        let id = sched.event_input(sink, 0).expect("open event input");

        let mut q = EventQueue::with_capacity(4);
        q.push(2, id, note_on_msg(60));
        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process_with_events(&mut out, &mut q);
        assert_eq!(sink_events(&sched).len(), 1);

        sched.process(&mut out); // no events this block
        assert!(
            sink_events(&sched).is_empty(),
            "an open event input is cleared each block"
        );
    }

    #[test]
    fn event_input_resolves_only_open_event_ports() {
        // Open event input → a handle; edge-fed event input → None (filled by the graph, not the
        // host); a non-event / nonexistent port → None.
        let mut g = Graph::new();
        let src = g.add(EventSource::new(8));
        let fed = g.add(EventSink::new(8)); // node 1: event input fed by an edge
        let open = g.add(EventSink::new(8)); // node 2: event input left open
        g.connect_ideal(src, 0, fed, 0);
        analog_tap(&mut g); // node 3: the voltage tap
        let sched = compile(g, 16, analog_rate(), 0).expect("valid event chain");

        assert!(
            sched.event_input(open, 0).is_some(),
            "an unwired event input is open"
        );
        assert!(
            sched.event_input(fed, 0).is_none(),
            "an edge-fed event input is not host-feedable"
        );
        assert!(
            sched.event_input(src, 0).is_none(),
            "the source has no input ports"
        );
        assert!(
            sched.event_input(NodeId(3), 0).is_none(),
            "the tap's port 0 is an analog output, not an event input"
        );
    }
}

/// Control params & de-zippering: a swept knob reaches the engine as a **smoothed**
/// value (a within-block linear ramp), so it never clicks. The headline lesson is the contrast
/// with a raw jump; here we sweep [`GainStage::GAIN`] and assert the output glides continuously to
/// the new gain rather than snapping. White-box where convenient, as elsewhere in this file.
mod param_phenomena {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::node::{GainStage, TestSource};
    use crate::param::ParamQueue;
    use crate::signal::Volts;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

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
}

/// A **played note travels the full chain** `analog → AD → digital → DA → analog` — the end-to-end
/// "play an instrument" path. The voice is driven by the event lane (note-on at a chosen sample) and
/// a smoothed control param (level); the converters carry it across the digital domain and back.
/// (The swept-param de-zipper gate is also proven in [`param_phenomena`] on a clean DC signal; here
/// we show it survives end-to-end on the voice.) Tests are the oracle — the fundamental level is a
/// hand calc, inline.
mod playable_voice {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::node::{AdConverter, DaConverter, SynthVoice};
    use crate::param::ParamQueue;
    use crate::signal::{BitDepth, EventMessage, SampleRate, Volts};
    use crate::test_util::{rms, tone_amplitude};
    use approx::assert_relative_eq;

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn digital_rate() -> SampleRate {
        SampleRate::new(48_000.0)
    }

    /// `voice → AD → DA → analog tap`, all near-ideal analog faces. Returns the schedule and the
    /// voice's event-input handle.
    fn voice_through_converters(block: usize) -> (Schedule, EventInputId) {
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        let ad = g.add(AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(1e6),
        ));
        let da = g.add(DaConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(150.0),
        ));
        g.connect_ideal(voice, 0, ad, 0);
        g.connect_ideal(ad, 0, da, 0);
        g.set_output(da, 0);
        let sched = compile(g, block, analog_rate(), 0).expect("valid playable chain");
        let ev = sched.event_input(voice, 0).expect("voice event input");
        (sched, ev)
    }

    #[test]
    fn a_played_note_travels_analog_ad_digital_da_analog() {
        // Play A4 (note 69 = 440 Hz) and recover it after the round trip. The voice's default
        // sustain 0.7 and level 1.0 V make the analog sawtooth's fundamental
        //   (2/π)·sustain·level = 0.63662·0.7·1.0 = 0.4456 V.
        // 440 Hz sits deep in the 24 kHz passband, so the AD/DA pass it at unity ⇒ the output
        // fundamental is ≈ that, and the AD's anti-alias filter has quietly removed the saw's
        // ultrasonic harmonics that would otherwise fold (the oversampled-oscillator payoff).
        let block = 15_360; // 1920 digital samples — many 440 Hz cycles
        let (mut sched, ev) = voice_through_converters(block);
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_events(&mut out, &mut q);

        // Read a steady window of ~whole cycles from after the attack + combined converter group
        // delay, so the single-bin DFT lands cleanly on the 440 Hz bin (low leakage).
        let spc = analog_rate().as_hz() / 440.0; // samples per cycle
        let window = (spc * 8.0) as usize; // 8 whole cycles
        let tail = &out.as_slice()[block / 2..block / 2 + window];

        let fundamental = tone_amplitude(tail, 440.0, analog_rate());
        let expected = core::f32::consts::FRAC_2_PI * 0.7 * 1.0;
        assert_relative_eq!(fundamental, expected, max_relative = 0.05);
        // A real pitched note: the fundamental dominates a detuned bin by a wide margin.
        let detuned = tone_amplitude(tail, 550.0, analog_rate());
        assert!(
            fundamental > detuned * 5.0,
            "the note should be a clean 440 Hz tone, not noise ({fundamental} vs {detuned})"
        );
    }

    #[test]
    fn the_chain_is_silent_before_the_note() {
        // Causality across the converters: a note triggered late produces nothing earlier. Filter
        // latency can only *delay* energy, never advance it — and with no input the only thing the
        // AD emits is its sub-µV dither floor, far below any signal.
        let block = 8_192;
        let (mut sched, ev) = voice_through_converters(block);
        let mut q = EventQueue::with_capacity(4);
        let trigger = block as u64 * 3 / 4;
        q.push(
            trigger,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_events(&mut out, &mut q);

        // The first quarter is well before the trigger (and its latency): silent to the dither floor.
        let head = &out.as_slice()[..block / 4];
        assert!(
            rms(head) < 1e-3,
            "nothing should sound before the note is played, rms {}",
            rms(head)
        );
    }

    #[test]
    fn a_swept_level_de_zippers_on_the_played_voice() {
        // The control-param de-zipper, end-to-end on the voice: hold a high note (so many periods
        // resolve the ramp), then sweep LEVEL 0.3 → 1.2 V (4×, within the 0–1.5 V range). The output's
        // windowed RMS must climb *smoothly* to ≈4× — a raw write would jump it in a single window.
        let block = 8_192;
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        g.set_output(voice, 0);
        let mut sched = compile(g, block, analog_rate(), 0).expect("valid voice chain");
        let ev = sched.event_input(voice, 0).expect("voice event input");
        let level = sched.param(voice, SynthVoice::LEVEL).expect("level param");

        // Block 1: establish a sustained C7 (note 96 ≈ 2093 Hz) at a low 0.3 V (settles within the block).
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 96,
                velocity: 100,
            },
        );
        let mut pq0 = ParamQueue::with_capacity(1);
        pq0.set(level, 0.3);
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_io(&mut out, &mut pq0, &mut q);

        // Block 2: aim LEVEL at 1.2 V and capture the glide (it ramps over the 5 ms smooth time).
        let mut pq = ParamQueue::with_capacity(1);
        pq.set(level, 1.2);
        sched.process_with_params(&mut out, &mut pq);

        // Window RMS over ~2 periods (note 96 period ≈ 183 samples). A smooth ramp spreads the
        // rise across many windows; assert it's non-decreasing, lands at ≈4× the start, and no
        // single window jumps by more than a fraction of the total change (rules out a step).
        let win = 366;
        let rms_windows: Vec<f32> = out.as_slice().chunks(win).map(rms).collect();
        let first = rms_windows[0];
        let last = *rms_windows.last().unwrap();
        assert_relative_eq!(last / first, 4.0, max_relative = 0.15);
        assert!(
            rms_windows.windows(2).all(|w| w[1] >= w[0] - 1e-4),
            "the level glide should be monotonic"
        );
        let total = last - first;
        let max_step = rms_windows
            .windows(2)
            .map(|w| w[1] - w[0])
            .fold(0.0_f32, f32::max);
        assert!(
            max_step < total * 0.5,
            "no single window may jump the whole change (max step {max_step} of {total})"
        );
    }
}

/// The real-time **hot-path robustness audit**, pinned as standing guards. The audit
/// found the `process` path panic-free and denormal-flushed; these tests keep it that way, because a
/// regression here surfaces on the audio thread — where a panic kills the stream and a denormal
/// storm blows the per-quantum CPU budget — not somewhere a unit test would otherwise catch it.
///
/// Two properties:
/// - **Totality over the cross-thread seam.** Param/event handles arrive from the external queues; a
///   stale or foreign one is skipped (`process_io` indexes them with `.get`), never a panic.
/// - **Exact silence / finiteness.** The voice reaches *exact* zero at idle and after release (the
///   linear ADSR hits 0, so `saw·0·level` is identically 0 — no denormal tail); the full converter
///   chain stays finite under sustained drive and quiet at idle (only the AD's dither floor).
mod hot_path_robustness {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::node::{AdConverter, DaConverter, SynthVoice};
    use crate::param::{ParamHandle, ParamQueue};
    use crate::signal::{BitDepth, EventMessage, SampleRate, Volts};
    use crate::test_util::rms;

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn digital_rate() -> SampleRate {
        SampleRate::new(48_000.0)
    }

    /// A bare voice → analog tap (no converters), with its event-input and level handles.
    fn voice_only(block: usize) -> (Schedule, EventInputId, ParamHandle) {
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        g.set_output(voice, 0);
        let sched = compile(g, block, analog_rate(), 0).expect("valid voice chain");
        let ev = sched.event_input(voice, 0).expect("voice event input");
        let lvl = sched.param(voice, SynthVoice::LEVEL).expect("level param");
        (sched, ev, lvl)
    }

    /// The full live patch: voice → AD → DA → analog tap, near-ideal faces.
    fn voice_through_converters(block: usize) -> (Schedule, EventInputId) {
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        let ad = g.add(AdConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(1e6),
        ));
        let da = g.add(DaConverter::new(
            digital_rate(),
            BitDepth::new(24),
            Volts::new(10.0),
            Ohms::new(150.0),
        ));
        g.connect_ideal(voice, 0, ad, 0);
        g.connect_ideal(ad, 0, da, 0);
        g.set_output(da, 0);
        let sched = compile(g, block, analog_rate(), 0).expect("valid playable chain");
        let ev = sched.event_input(voice, 0).expect("voice event input");
        (sched, ev)
    }

    #[test]
    fn idle_voice_is_exactly_silent_over_many_blocks() {
        // No events ⇒ the envelope never leaves Idle ⇒ env == 0 ⇒ saw·0·level == 0, identically.
        // A denormal creep (an un-flushed asymptotic state) would show as a tiny non-zero tail; the
        // output must stay *exactly* 0.0 (and finite) over a long run.
        let block = 1024;
        let (mut sched, _ev, _lvl) = voice_only(block);
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        for _ in 0..200 {
            sched.process(&mut out);
            assert!(
                out.as_slice().iter().all(|&v| v == 0.0),
                "idle voice must be identically zero — any denormal creep is a bug"
            );
        }
    }

    #[test]
    fn a_released_note_decays_to_exact_zero() {
        // Note-on then note-off; after the (10 ms) release the envelope reaches exactly 0 — a linear
        // ramp clamped to 0, then Idle — so the tail is identically silent, no denormal residue.
        let block = 8_192;
        let (mut sched, ev, _lvl) = voice_only(block);
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        q.push(64, ev, EventMessage::NoteOff { note: 69 }); // 10 ms release ≪ the rest of the block
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_events(&mut out, &mut q);
        // The final stretch is long past the release: identically zero.
        let tail = &out.as_slice()[block - 2048..];
        assert!(
            tail.iter().all(|&v| v == 0.0),
            "the release must reach exact silence"
        );
        // A further idle block stays silent — state truly settled, not drifting.
        sched.process(&mut out);
        assert!(out.as_slice().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn a_sustained_note_through_converters_stays_finite() {
        // Hold a note across many blocks through the AD/DA FIR + edge IIR chain; every output sample
        // must stay finite (no NaN/inf from a runaway filter state) for the whole sustained run.
        let block = 1024;
        let (mut sched, ev) = voice_through_converters(block);
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        for _ in 0..400 {
            sched.process_with_events(&mut out, &mut q);
            assert!(
                out.as_slice().iter().all(|&v| v.is_finite()),
                "sustained output must stay finite"
            );
        }
    }

    #[test]
    fn idle_chain_through_converters_is_finite_and_quiet() {
        // At idle the chain carries only the AD's sub-µV dither floor: finite, and far below any
        // signal — proof there's no denormal / IIR blow-up when the input is silent.
        let block = 1024;
        let (mut sched, _ev) = voice_through_converters(block);
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        for _ in 0..200 {
            sched.process(&mut out);
            assert!(out.as_slice().iter().all(|&v| v.is_finite()));
        }
        assert!(
            rms(out.as_slice()) < 1e-3,
            "idle chain should be near-silent (dither only)"
        );
    }

    #[test]
    fn a_foreign_param_handle_is_skipped_not_panicked() {
        // A handle from another schedule (or a stale one) indexes past this schedule's smoother
        // store. `process_io` must skip it rather than panic — a panic would kill the audio stream.
        // The valid handle pushed alongside still applies.
        let block = 256;
        let (mut sched, _ev, lvl) = voice_only(block);
        let mut pq = ParamQueue::with_capacity(4);
        pq.set(ParamHandle(usize::MAX), 0.5); // bogus: way past the store
        pq.set(lvl, 2.0); // valid
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_params(&mut out, &mut pq); // must not panic
    }

    #[test]
    fn a_foreign_event_id_is_skipped_not_panicked() {
        // Same totality contract for the event lane: an out-of-range target id is skipped, never a
        // panic (and never the `events_mut` `unreachable!`); the valid note still sounds.
        let block = 1024;
        let (mut sched, ev, _lvl) = voice_only(block);
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            EventInputId(usize::MAX), // bogus target
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        q.push(
            0,
            ev, // valid target
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, analog_rate());
        sched.process_with_events(&mut out, &mut q); // must not panic
        assert!(
            out.as_slice().iter().any(|&v| v != 0.0),
            "the valid note should still sound"
        );
    }
}

/// The node→host readout lane: a node declares readouts, the schedule reserves a store, and each
/// block it snapshots the current reading for the host to poll — the observe-side mirror of params.
mod readout_lane {
    use super::super::*;
    use crate::electrical::OutputZ;
    use crate::graph::NodeId;
    use crate::param::Params;
    use crate::port::{InputPort, OutputPort};
    use crate::readout::{ReadoutDecl, ReadoutHandle};

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A minimal readout-bearing node: emits a constant 1 V and reports how many blocks it has
    /// processed as readout 0. Counting blocks (rather than a constant) proves the schedule snapshots
    /// the reading *after* each block's `process`, exactly once per block.
    struct BlockCounter {
        blocks: f32,
        readouts: [ReadoutDecl; 1],
        outputs: [OutputPort; 1],
    }

    impl BlockCounter {
        fn new() -> Self {
            Self {
                blocks: 0.0,
                readouts: [ReadoutDecl { id: ReadoutId(0) }],
                outputs: [OutputZ::new(Ohms::new(150.0)).into()],
            }
        }
    }

    impl Node for BlockCounter {
        fn inputs(&self) -> &[InputPort] {
            &[]
        }

        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }

        fn readouts(&self) -> &[ReadoutDecl] {
            &self.readouts
        }

        fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
            self.blocks += 1.0;
            for v in outputs[0].voltage_mut().as_mut_slice() {
                *v = 1.0;
            }
        }

        fn read_readouts(&self, out: &mut [f32]) {
            out[0] = self.blocks;
        }
    }

    /// A resolved readout starts at the store's zero-init and then, each processed block, reflects
    /// *that* block's reading — proving the snapshot runs after `process`, once per block.
    #[test]
    fn readout_resolves_and_snapshots_each_block() {
        let mut g = Graph::new();
        let n = g.add(BlockCounter::new());
        g.set_output(n, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("compiles");
        let h = sched.readout(n, ReadoutId(0)).expect("readout 0 resolves");

        // Nothing processed yet ⇒ the store holds its zero initialisation.
        assert_eq!(sched.readout_value(h), Some(0.0));

        let mut out = VoltageBuffer::zeros(8, rate());
        for expected in 1..=5 {
            sched.process(&mut out);
            assert_eq!(
                sched.readout_value(h),
                Some(expected as f32),
                "reading reflects this block, snapshotted after process"
            );
        }
    }

    /// Readout resolution is total: an out-of-range id or node returns `None`, and polling a foreign
    /// handle reads `None` rather than panicking — the same defensiveness as the param/event seams.
    #[test]
    fn readout_resolution_is_total() {
        let mut g = Graph::new();
        let n = g.add(BlockCounter::new());
        g.set_output(n, 0);
        let sched = compile(g, 8, rate(), 0).expect("compiles");

        assert!(sched.readout(n, ReadoutId(0)).is_some());
        assert!(sched.readout(n, ReadoutId(1)).is_none(), "id out of range");
        assert!(
            sched.readout(NodeId(99), ReadoutId(0)).is_none(),
            "node out of range"
        );
        assert!(
            sched.readout_value(ReadoutHandle(9999)).is_none(),
            "a foreign handle reads None, never panics"
        );
    }

    /// A node with no readouts reserves no store — resolving any readout on it is `None`, and the
    /// default no-op `read_readouts` is never handed a slice to fill.
    #[test]
    fn a_node_without_readouts_reserves_nothing() {
        use crate::node::TestSource;
        use crate::signal::Volts;

        let mut g = Graph::new();
        let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
        g.set_output(src, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("compiles");
        assert!(sched.readout(src, ReadoutId(0)).is_none());

        // Still processes cleanly with an empty readout store.
        let mut out = VoltageBuffer::zeros(8, rate());
        sched.process(&mut out);
    }
}

/// Multichannel digital: the first end-to-end coverage of an **N-lane digital edge** (one connector
/// carrying many channels) and a **multi-output node** (the demux). The mux's N-channel output feeds
/// the demux's N-channel input as a single edge that compiles to N `DigitalRoute`s; the demux then
/// fans back to N mono output ports — exercising per-port pool allocation and step emission for a node
/// with more than one output, the path that had no users before.
mod multichannel_digital {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::node::{AdConverter, DaConverter, DigitalDemux, DigitalMux, TestSource};
    use crate::signal::{BitDepth, SampleRate, Volts};

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn fs() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    fn bits() -> BitDepth {
        BitDepth::new(16)
    }

    /// A DC signal survives the full chain `source → AD → mux(2) → demux(2) → DA` on **channel 0**:
    /// the multi-lane mux→demux edge carries it, the demux routes channel 0 back to its first output,
    /// and the DA reconstructs the original volts. 0.5 V into a 1 V-reference AD normalizes to 0.5,
    /// copies through mux/demux, and the 1 V-reference DA brings it back to 0.5 V.
    #[test]
    fn n_lane_edge_carries_a_signal_through_mux_and_demux() {
        let mut g = Graph::new();
        // Channel 0 carries 0.5 V; channel 1's mux input is left open (digital silence).
        let src = g.add(TestSource::new(Volts::new(0.5), Ohms::new(1.0)));
        let ad = g.add(AdConverter::new(
            fs(),
            bits(),
            Volts::new(1.0),
            Ohms::new(1e6),
        ));
        let mux = g.add(DigitalMux::new(fs(), bits(), 2));
        let demux = g.add(DigitalDemux::new(fs(), bits(), 2));
        let da = g.add(DaConverter::new(
            fs(),
            bits(),
            Volts::new(1.0),
            Ohms::new(150.0),
        ));
        g.connect_ideal(src, 0, ad, 0);
        g.connect_ideal(ad, 0, mux, 0); // → mux channel 0
        g.connect_ideal(mux, 0, demux, 0); // the single N-lane digital edge (2 channels)
        g.connect_ideal(demux, 0, da, 0); // demux output 0 (first of two output ports)
        g.set_output(da, 0);

        let block = 384; // a multiple of M = 8, per compile's decimation constraint
        let mut sched = compile(g, block, rate(), 0).expect("multichannel digital chain compiles");
        let mut out = VoltageBuffer::zeros(block, rate());
        // Run enough blocks for the AD/DA FIRs (161 taps) to settle well past their group delay.
        for _ in 0..40 {
            sched.process(&mut out);
        }
        let tail = &out.as_slice()[block / 2..];
        assert!(
            tail.iter().all(|&v| (v - 0.5).abs() < 1e-2),
            "channel 0 should reconstruct to 0.5 V through mux→demux, tail max dev = {}",
            tail.iter().fold(0.0_f32, |m, &v| m.max((v - 0.5).abs()))
        );
    }

    /// A digital edge whose channel counts disagree is a [`CompileError::LaneCountMismatch`] — the
    /// same lane-count check that catches an analog balanced/unbalanced mismatch, now reading right for
    /// digital channels (a 4-wide mux send into a 2-wide demux return).
    #[test]
    fn mismatched_channel_counts_are_rejected() {
        let mut g = Graph::new();
        let mux = g.add(DigitalMux::new(fs(), bits(), 4)); // node 0
        let demux = g.add(DigitalDemux::new(fs(), bits(), 2)); // node 1
        // A valid analog tap downstream, so the *only* compile error is the channel-count mismatch on
        // the mux→demux edge (not a missing/again-invalid output tap).
        let da = g.add(DaConverter::new(
            fs(),
            bits(),
            Volts::new(1.0),
            Ohms::new(150.0),
        ));
        g.connect_ideal(mux, 0, demux, 0); // 4-channel out → 2-channel in
        g.connect_ideal(demux, 0, da, 0);
        g.set_output(da, 0);
        assert_eq!(
            compile(g, 384, rate(), 0).err(),
            Some(CompileError::LaneCountMismatch {
                from_node: 0,
                from_port: 0,
                to_node: 1,
                to_port: 0,
            })
        );
    }
}

/// Runtime routing: a [`Matrix`] re-routes inputs → outputs live, via smoothed crosspoint params, with
/// no recompile — the runtime-switchable-routing seam. Two DC sources feed a 2×1 matrix; moving the
/// crosspoints swaps which source reaches the (single) output, all through a compiled schedule.
mod routing_matrix {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::node::{AdConverter, DaConverter, Matrix, TestSource};
    use crate::param::ParamQueue;
    use crate::signal::{BitDepth, SampleRate, Volts};

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn fs() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    fn bits() -> BitDepth {
        BitDepth::new(16)
    }

    #[test]
    fn a_crosspoint_change_reroutes_the_output_live() {
        // in0 = 0.5 V, in1 = 0.25 V, each via its own AD into a 2×1 matrix; matrix out → DA → tap.
        let mut g = Graph::new();
        let src0 = g.add(TestSource::new(Volts::new(0.5), Ohms::new(1.0)));
        let ad0 = g.add(AdConverter::new(
            fs(),
            bits(),
            Volts::new(1.0),
            Ohms::new(1e6),
        ));
        let src1 = g.add(TestSource::new(Volts::new(0.25), Ohms::new(1.0)));
        let ad1 = g.add(AdConverter::new(
            fs(),
            bits(),
            Volts::new(1.0),
            Ohms::new(1e6),
        ));
        // Default routing: out = in0 (crosspoint (0,0) = 1, (1,0) = 0).
        let mx = g.add(Matrix::new(fs(), bits(), 2, 1, vec![1.0, 0.0]));
        let da = g.add(DaConverter::new(
            fs(),
            bits(),
            Volts::new(1.0),
            Ohms::new(150.0),
        ));
        g.connect_ideal(src0, 0, ad0, 0);
        g.connect_ideal(src1, 0, ad1, 0);
        g.connect_ideal(ad0, 0, mx, 0);
        g.connect_ideal(ad1, 0, mx, 1);
        g.connect_ideal(mx, 0, da, 0);
        g.set_output(da, 0);

        let block = 384;
        let mut sched = compile(g, block, rate(), 0).expect("routing chain compiles");
        let c00 = sched
            .param(mx, Matrix::crosspoint(0, 0, 1))
            .expect("crosspoint 0→0");
        let c10 = sched
            .param(mx, Matrix::crosspoint(1, 0, 1))
            .expect("crosspoint 1→0");

        // Default: the output carries in0 (0.5 V) after the converters settle.
        let mut out = VoltageBuffer::zeros(block, rate());
        for _ in 0..40 {
            sched.process(&mut out);
        }
        let tail0 = &out.as_slice()[block / 2..];
        assert!(
            tail0.iter().all(|&v| (v - 0.5).abs() < 1e-2),
            "default routing carries in0 (0.5 V)"
        );

        // Re-route to in1 (0.25 V): close (1,0), open... i.e. crosspoint (0,0) → 0, (1,0) → 1. No
        // recompile — just smoothed param moves; run past the glide.
        let mut q = ParamQueue::with_capacity(2);
        q.set(c00, 0.0);
        q.set(c10, 1.0);
        for b in 0..40 {
            if b == 0 {
                sched.process_with_params(&mut out, &mut q);
            } else {
                sched.process(&mut out);
            }
        }
        let tail1 = &out.as_slice()[block / 2..];
        assert!(
            tail1.iter().all(|&v| (v - 0.25).abs() < 1e-2),
            "after the crosspoint move the output carries in1 (0.25 V), no recompile"
        );
    }
}
