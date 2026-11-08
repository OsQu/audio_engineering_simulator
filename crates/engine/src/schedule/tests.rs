mod compile_and_solve {
    use super::super::*;
    use crate::electrical::Ohms;
    use crate::graph::NodeId;
    use crate::node::{GainStage, PassiveSum, TestSource};
    use crate::signal::Volts;
    use crate::test_util::rate;
    use approx::assert_relative_eq;

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

    /// A digital `out = in + 1` node — the digital twin of the analog `Accumulator`. One digital in,
    /// one digital out; used to exercise auto-broken digital feedback loops.
    struct DigitalAccumulator {
        inputs: [InputPort; 1],
        outputs: [OutputPort; 1],
    }
    impl DigitalAccumulator {
        fn new(format: AudioFormat) -> Self {
            Self {
                inputs: [DigitalFace::new(format).into()],
                outputs: [DigitalFace::new(format).into()],
            }
        }
    }
    impl Node for DigitalAccumulator {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _p: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
            let src = inputs[0].sample().as_slice();
            let dst = outputs[0].sample_mut().as_mut_slice();
            for (o, &i) in dst.iter_mut().zip(src) {
                *o = i + 1.0;
            }
        }
    }

    /// A two-node **digital** loop `A → B → A` wired with two **ideal** edges compiles: `compile`
    /// auto-breaks the cycle by delaying one digital edge (the digital-transport analogue of a
    /// buffered round-trip). The all-analog twin of this topology is rejected (`rejects_a_cycle`) —
    /// only a digital link can carry the block of latency that cuts the loop.
    #[test]
    fn a_digital_cycle_of_ideal_edges_auto_compiles() {
        let mut g = Graph::new();
        let atap = g.add(TestSource::new(Volts::new(1.0), Ohms::new(150.0)));
        g.set_output(atap, 0); // analog tap so the schedule is well-formed
        let a = g.add(DigitalAccumulator::new(fmt(48_000.0)));
        let b = g.add(DigitalAccumulator::new(fmt(48_000.0)));
        g.connect_ideal(a, 0, b, 0);
        g.connect_ideal(b, 0, a, 0);
        assert!(
            compile(g, 16, analog_rate(), 0).is_ok(),
            "a digital 2-cycle is auto-broken at a digital edge, so it compiles"
        );
    }

    /// A **digital self-loop** `acc → acc` wired with an *ideal* edge — no `connect_delayed` — turns the
    /// `out = in + 1` accumulator into a one-block integrator, exactly as the manually-delayed analog
    /// self-loop does. Proof the auto-inserted delay is functionally identical: reads *last* block's
    /// output, so after N blocks the sample value is N. (A zero-block cut would be a rejected cycle.)
    #[test]
    fn an_auto_broken_digital_self_loop_integrates_one_block_at_a_time() {
        let mut g = Graph::new();
        let atap = g.add(TestSource::new(Volts::new(1.0), Ohms::new(150.0)));
        g.set_output(atap, 0); // analog tap drives `process`; the digital loop runs alongside
        let acc = g.add(DigitalAccumulator::new(fmt(48_000.0)));
        g.connect_ideal(acc, 0, acc, 0); // ideal self-loop — compile must auto-delay it
        let mut sched = compile(g, 16, analog_rate(), 0).expect("ideal digital self-loop compiles");

        let mut out = VoltageBuffer::zeros(16, analog_rate());
        for expected in 1..=5 {
            sched.process(&mut out);
            // The accumulator owns the only digital output lane in the pool.
            let acc_out = sched
                .output_pool
                .iter()
                .find(|l| matches!(l, Lane::Sample(_)))
                .expect("the accumulator's digital output lane");
            assert!(
                acc_out
                    .sample()
                    .as_slice()
                    .iter()
                    .all(|&s| (s - expected as f32).abs() < 1e-6),
                "block {expected}: auto-broken loop should have integrated to {expected}"
            );
        }
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
        // deep in the passband, should survive end-to-end at ≈ 2 V — the analog physics (local
        // solve, cables, balanced lines) intact across the digital round trip.
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
    use crate::test_util::rate;

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
    use crate::test_util::rate;

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
    use crate::test_util::rate;

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

/// **Delayed edges** — the round-trip-latency seam. A delayed edge is cut from the topological sort (so
/// it can close a loop without making the schedule cyclic) and delivers the producer's *previous*-block
/// output (its copy runs before the step loop, reading the persistent output pool). These oracles pin
/// the two guarantees: a loop with a delayed edge compiles (an undelayed one still errors), and the
/// delay is *exactly one block*.
mod delayed_edges {
    use super::super::*;
    use crate::electrical::{InputZ, Ohms, OutputZ};
    use crate::param::Params;
    use crate::port::{InputPort, OutputPort};
    use crate::test_util::rate;

    /// An analog passthrough: out = in (one conductor). Enough to route a signal around a loop.
    struct Pass {
        inputs: [InputPort; 1],
        outputs: [OutputPort; 1],
    }
    impl Pass {
        fn new() -> Self {
            Self {
                inputs: [InputZ::new(Ohms::new(1e9)).into()],
                outputs: [OutputZ::new(Ohms::new(1.0)).into()],
            }
        }
    }
    impl Node for Pass {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _p: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
            let src = inputs[0].voltage().as_slice();
            let dst = outputs[0].voltage_mut().as_mut_slice();
            dst.copy_from_slice(src);
        }
    }

    /// An accumulator: out = in + 1 V per block. With a delayed self-loop (out → in) it integrates,
    /// adding exactly 1 V each block — a feedback loop that only exists because the edge is delayed.
    struct Accumulator {
        inputs: [InputPort; 1],
        outputs: [OutputPort; 1],
    }
    impl Accumulator {
        fn new() -> Self {
            Self {
                inputs: [InputZ::new(Ohms::new(1e9)).into()],
                outputs: [OutputZ::new(Ohms::new(1.0)).into()],
            }
        }
    }
    impl Node for Accumulator {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _p: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
            let src = inputs[0].voltage().as_slice();
            let dst = outputs[0].voltage_mut().as_mut_slice();
            for (o, &i) in dst.iter_mut().zip(src) {
                *o = i + 1.0;
            }
        }
    }

    /// A source whose per-block output steps up by 10 V each block (block 1 → 10, block 2 → 20, …), so
    /// a one-block lag is visible in the value itself.
    struct BlockRamp {
        level: f32,
        outputs: [OutputPort; 1],
    }
    impl BlockRamp {
        fn new() -> Self {
            Self {
                level: 0.0,
                outputs: [OutputZ::new(Ohms::new(1.0)).into()],
            }
        }
    }
    impl Node for BlockRamp {
        fn inputs(&self) -> &[InputPort] {
            &[]
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _p: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
            self.level += 10.0;
            for v in outputs[0].voltage_mut().as_mut_slice() {
                *v = self.level;
            }
        }
    }

    /// A two-node loop `A → B → A` errors when both edges are ideal (an unbreakable same-block cycle),
    /// but **compiles** when the back edge is delayed — the delayed edge is cut from the sort.
    #[test]
    fn a_delayed_edge_breaks_a_cycle_an_ideal_one_does_not() {
        let ideal = {
            let mut g = Graph::new();
            let a = g.add(Pass::new());
            let b = g.add(Pass::new());
            g.connect_ideal(a, 0, b, 0);
            g.connect_ideal(b, 0, a, 0);
            g.set_output(b, 0);
            compile(g, 8, rate(), 0)
        };
        assert_eq!(
            ideal.err(),
            Some(CompileError::Cycle),
            "ideal loop is a cycle"
        );

        let mut g = Graph::new();
        let a = g.add(Pass::new());
        let b = g.add(Pass::new());
        g.connect_ideal(a, 0, b, 0);
        g.connect_delayed(b, 0, a, 0); // the back edge carries a block of latency
        g.set_output(b, 0);
        assert!(
            compile(g, 8, rate(), 0).is_ok(),
            "a delayed back edge breaks the cycle"
        );
    }

    /// A delayed self-loop turns an `out = in + 1` node into an integrator: each block reads *last*
    /// block's output, so after N blocks the output is exactly N V — proof the loop runs and the delay
    /// is one block (a two-block delay would lag, a zero-block one would be a cycle).
    #[test]
    fn a_delayed_self_loop_accumulates_one_block_at_a_time() {
        let mut g = Graph::new();
        let acc = g.add(Accumulator::new());
        g.connect_delayed(acc, 0, acc, 0);
        g.set_output(acc, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("delayed self-loop compiles");

        let mut out = VoltageBuffer::zeros(8, rate());
        for expected in 1..=6 {
            sched.process(&mut out);
            let v = out.as_slice()[0];
            assert!(
                (v - expected as f32).abs() < 1e-6,
                "block {expected}: accumulated {v} V, expected {expected}"
            );
        }
    }

    /// A delayed edge from a per-block ramp source delivers the *previous* block's value: the tap reads
    /// 0 (initial), then 10, 20, … — exactly one block behind the source's 10, 20, 30, ….
    #[test]
    fn a_delayed_edge_delivers_the_previous_block() {
        let mut g = Graph::new();
        let ramp = g.add(BlockRamp::new());
        let pass = g.add(Pass::new());
        g.connect_delayed(ramp, 0, pass, 0);
        g.set_output(pass, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("compiles");

        let mut out = VoltageBuffer::zeros(8, rate());
        for expected in [0.0_f32, 10.0, 20.0, 30.0] {
            sched.process(&mut out);
            let v = out.as_slice()[0];
            assert!(
                (v - expected).abs() < 1e-6,
                "tap should lag the ramp by one block: got {v}, expected {expected}"
            );
        }
    }
}

/// **Acoustic feedback** — the air hop is delay-eligible, so `compile` auto-breaks an
/// acoustic feedback loop by delaying the **acoustic** edge (preferring it over any digital edge on
/// the same cycle, since the air physically carries the latency). Howlround then emerges from the
/// delayed loop: a below-unity loop settles, a super-unity one grows and self-limits at the rail.
/// A pure-analog loop still has nowhere to carry latency and is rejected (`delayed_edges` above +
/// `rejects_a_cycle`). Tests inspect the private `delayed_steps`/pools (white-box).
mod acoustic_feedback {
    use super::super::*;
    use crate::electrical::{InputZ, Ohms};
    use crate::node::{
        BalancedReceiver, DynamicMic, GainStage, PassiveSum, PoweredMonitor, TestSource,
    };
    use crate::port::{AcousticFace, AudioFormat, DigitalFace, InputPort, OutputPort};
    use crate::signal::{BitDepth, Pascals, SampleRate, Volts};
    use crate::test_util::rate;

    /// A one-block acoustic feedback recurrence in a single node: `out = clamp(k·in + c, ±rail)`
    /// (pressure in, pressure out). Closed with an ideal acoustic **self-loop** it becomes a delayed
    /// feedback loop `pₙ = clamp(k·pₙ₋₁ + c, ±rail)` — `k` is the loop gain, `c` a constant drive, and
    /// the rail the self-limit a real howl clips against. A stand-in for a whole mic→amp→monitor loop,
    /// so the *mechanism* (a delayed pressure edge carrying feedback) is testable in one node.
    struct AcousticFeedbackCell {
        k: f32,
        c: f32,
        rail: f32,
        inputs: [InputPort; 1],
        outputs: [OutputPort; 1],
    }
    impl AcousticFeedbackCell {
        fn new(k: f32, c: f32, rail: f32) -> Self {
            Self {
                k,
                c,
                rail,
                inputs: [AcousticFace::new().into()],
                outputs: [AcousticFace::new().into()],
            }
        }
    }
    impl Node for AcousticFeedbackCell {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _p: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
            let src = inputs[0].pressure().as_slice();
            let dst = outputs[0].pressure_mut().as_mut_slice();
            for (o, &i) in dst.iter_mut().zip(src) {
                *o = (self.k * i + self.c).clamp(-self.rail, self.rail);
            }
        }
    }

    /// Digital in → acoustic out (a stand-in "DA + monitor"): writes silence, used only to give a
    /// mixed digital/acoustic cycle its acoustic edge.
    struct DigitalToAcoustic {
        inputs: [InputPort; 1],
        outputs: [OutputPort; 1],
    }
    impl DigitalToAcoustic {
        fn new(format: AudioFormat) -> Self {
            Self {
                inputs: [DigitalFace::new(format).into()],
                outputs: [AcousticFace::new().into()],
            }
        }
    }
    impl Node for DigitalToAcoustic {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _p: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
            outputs[0].pressure_mut().fill(Pascals::new(0.0));
        }
    }

    /// Acoustic in → digital out (a stand-in "mic + AD"): the other half of a mixed cycle.
    struct AcousticToDigital {
        inputs: [InputPort; 1],
        outputs: [OutputPort; 1],
    }
    impl AcousticToDigital {
        fn new(format: AudioFormat) -> Self {
            Self {
                inputs: [AcousticFace::new().into()],
                outputs: [DigitalFace::new(format).into()],
            }
        }
    }
    impl Node for AcousticToDigital {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _p: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
            for s in outputs[0].sample_mut().as_mut_slice() {
                *s = 0.0;
            }
        }
    }

    /// The count of delayed edge-copy steps, and how many of them copy an acoustic (pressure) lane.
    fn delayed_edge_kinds(sched: &Schedule) -> (usize, usize) {
        let total = sched
            .delayed_steps
            .iter()
            .filter(|s| matches!(s, Step::Edge { .. }))
            .count();
        let acoustic = sched
            .delayed_steps
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    Step::Edge {
                        kind: EdgeKind::AcousticRoute,
                        ..
                    }
                )
            })
            .count();
        (total, acoustic)
    }

    /// The pressure at sample 0 of the (single) acoustic output lane in the pool.
    fn pool_pressure(sched: &Schedule) -> f32 {
        sched
            .output_pool
            .iter()
            .find_map(|l| match l {
                Lane::Pressure(b) => Some(b.get(0).get()),
                _ => None,
            })
            .expect("an acoustic output lane in the pool")
    }

    /// A real feedback loop — `mic → receiver → gain → monitor → air → mic` — compiles: `compile`
    /// auto-breaks it by delaying the **air** edge (the only delay-eligible edge on this all-analog +
    /// one-acoustic loop). The delayed edge is the acoustic one, and it is the *only* delayed edge.
    #[test]
    fn a_real_mic_monitor_loop_auto_breaks_at_the_air_edge() {
        let mut g = Graph::new();
        let mic = g.add(DynamicMic::new(0.002, Ohms::new(200.0)));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
        let gain = g.add(GainStage::new(
            2.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        ));
        let mon = g.add(PoweredMonitor::new(1.0, InputZ::new(Ohms::new(10_000.0))));
        g.connect_ideal(mic, 0, rcv, 0); // balanced analog (mic → receiver)
        g.connect_ideal(rcv, 0, gain, 0); // analog
        g.connect_ideal(gain, 0, mon, 0); // analog
        g.connect_ideal(mon, 0, mic, 0); // ACOUSTIC air hop — closes the loop
        g.set_output(gain, 0); // a voltage tap so the schedule is well-formed
        let sched =
            compile(g, 8, rate(), 0).expect("acoustic feedback loop auto-breaks at the air");
        assert_eq!(
            delayed_edge_kinds(&sched),
            (1, 1),
            "exactly one delayed edge, and it is the acoustic (air) edge"
        );
    }

    /// When a cycle has **both** a digital and an acoustic edge, the acoustic one carries the delay —
    /// the air hop is where a howlround loop's latency physically lives. A 2-cycle `X →(air)→ Y
    /// →(digital)→ X`: the delayed edge must be the acoustic one, leaving the digital edge same-block.
    #[test]
    fn a_mixed_cycle_prefers_the_acoustic_edge_for_the_delay() {
        let f = AudioFormat::new(SampleRate::new(48_000.0), BitDepth::new(24), 1);
        let mut g = Graph::new();
        let x = g.add(DigitalToAcoustic::new(f)); // digital in, acoustic out
        let y = g.add(AcousticToDigital::new(f)); // acoustic in, digital out
        g.connect_ideal(x, 0, y, 0); // acoustic edge  x → y
        g.connect_ideal(y, 0, x, 0); // digital edge   y → x
        let tap = g.add(TestSource::new(Volts::new(0.0), Ohms::new(150.0)));
        g.set_output(tap, 0);
        let sched = compile(g, 16, rate(), 0).expect("mixed cycle auto-breaks");
        assert_eq!(
            delayed_edge_kinds(&sched),
            (1, 1),
            "the acoustic edge is delayed, not the digital one"
        );
    }

    /// A **below-unity** acoustic feedback loop settles: `pₙ = 0.5·pₙ₋₁ + 1` (a delayed acoustic
    /// self-loop, drive 1) reads *last* block's pressure back, so it climbs 1 → 1.5 → 1.75 → …,
    /// converging geometrically to the fixed point `c/(1−k) = 2`. Proof the delayed **pressure** copy
    /// carries feedback and the delay is exactly one block.
    #[test]
    fn a_below_unity_loop_settles_one_block_at_a_time() {
        let mut g = Graph::new();
        let cell = g.add(AcousticFeedbackCell::new(0.5, 1.0, 100.0));
        g.connect_ideal(cell, 0, cell, 0); // ideal acoustic self-loop — compile must auto-delay it
        let tap = g.add(TestSource::new(Volts::new(0.0), Ohms::new(150.0)));
        g.set_output(tap, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("acoustic self-loop compiles");
        assert_eq!(
            delayed_edge_kinds(&sched),
            (1, 1),
            "the air edge is delayed"
        );

        let mut out = VoltageBuffer::zeros(8, rate());
        // pₙ = 0.5·pₙ₋₁ + 1, p starts 0: 1, 1.5, 1.75, 1.875, 1.9375 → 2.
        for expected in [1.0_f32, 1.5, 1.75, 1.875, 1.9375] {
            sched.process(&mut out);
            let p = pool_pressure(&sched);
            assert!(
                (p - expected).abs() < 1e-5,
                "below-unity loop: got {p}, expected {expected}"
            );
        }
    }

    /// A **super-unity** loop builds toward feedback and self-limits at the rail: `pₙ = 2·pₙ₋₁ + 1`
    /// grows 1, 3, 7, 15, 31, 63, then 2·63+1 = 127 clamps to the 100 rail and stays — the emergent
    /// "it howls until it pins" ceiling, no feedback special-casing anywhere.
    #[test]
    fn a_super_unity_loop_grows_then_self_limits_at_the_rail() {
        let mut g = Graph::new();
        let cell = g.add(AcousticFeedbackCell::new(2.0, 1.0, 100.0));
        g.connect_ideal(cell, 0, cell, 0);
        let tap = g.add(TestSource::new(Volts::new(0.0), Ohms::new(150.0)));
        g.set_output(tap, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("acoustic self-loop compiles");

        let mut out = VoltageBuffer::zeros(8, rate());
        let mut prev = 0.0_f32;
        let mut pinned = false;
        for _ in 0..9 {
            sched.process(&mut out);
            let p = pool_pressure(&sched);
            if pinned {
                assert!(
                    (p - 100.0).abs() < 1e-4,
                    "stays pinned at the rail, got {p}"
                );
            } else {
                assert!(p > prev, "super-unity loop must grow: {p} !> {prev}");
                if (p - 100.0).abs() < 1e-4 {
                    pinned = true;
                }
            }
            prev = p;
        }
        assert!(
            pinned,
            "a super-unity loop must reach the rail and self-limit"
        );
    }

    // --- The feedback scenario through real transducers, by hand calc. ---

    /// Run a **real** acoustic feedback loop for `blocks` blocks and return the monitor's radiated
    /// pressure (Pa) after each block. The loop is `mic → receiver → sum(+drive) → gain → monitor →
    /// air → mic`, with a constant analog `drive` seeding it (a faint steady stimulus the loop
    /// amplifies — real feedback rings up from room ambience / the noise floor; a deterministic drive
    /// stands in). Every impedance is bridging (edge dividers ≈ 1) and the monitor is 1 Pa/V at unity
    /// volume, so the per-block recurrence is exactly `pₙ = clamp(gain·(drive + sensitivity·pₙ₋₁),
    /// ±rail)`, i.e. loop gain `L = gain · sensitivity`. The air edge is the auto-delayed leg.
    fn run_feedback_loop(
        sensitivity: f32,
        gain: f32,
        drive: f32,
        rail: f32,
        blocks: usize,
    ) -> Vec<f32> {
        let mut g = Graph::new();
        let drv = g.add(TestSource::new(Volts::new(drive), Ohms::new(150.0)));
        let mic = g.add(DynamicMic::new(sensitivity, Ohms::new(200.0)));
        let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(1.0)));
        let sum = g.add(PassiveSum::new(
            vec![InputZ::new(Ohms::new(1e9)), InputZ::new(Ohms::new(1e9))],
            Ohms::new(1.0),
        ));
        let amp = g.add(GainStage::new(
            gain,
            Volts::new(rail),
            InputZ::new(Ohms::new(1e9)),
            Ohms::new(1.0),
        ));
        let mon = g.add(PoweredMonitor::new(1.0, InputZ::new(Ohms::new(1e9))));
        g.connect_ideal(mic, 0, rcv, 0); // balanced analog → single-ended
        g.connect_ideal(rcv, 0, sum, 0); // the loop return into the summing node
        g.connect_ideal(drv, 0, sum, 1); // the constant drive (seed) into the summing node
        g.connect_ideal(sum, 0, amp, 0);
        g.connect_ideal(amp, 0, mon, 0);
        g.connect_ideal(mon, 0, mic, 0); // ACOUSTIC air hop — auto-delayed, closes the loop
        g.set_output(amp, 0);
        let mut sched = compile(g, 8, rate(), 0).expect("feedback loop auto-breaks and compiles");
        let mut out = VoltageBuffer::zeros(8, rate());
        (0..blocks)
            .map(|_| {
                sched.process(&mut out);
                pool_pressure(&sched)
            })
            .collect()
    }

    /// **Below unity, the loop settles.** A sensitive (condenser-like, 10 mV/Pa) mic at gain 50 ⇒ loop
    /// gain `L = 50·0.01 = 0.5`; drive 0.01 V ⇒ per-block injection `A = 50·0.01 = 0.5`. So
    /// `pₙ = 0.5 + 0.5·pₙ₋₁` climbs 0.5, 0.75, 0.875, … and converges to `A/(1−L) = 1.0 Pa`, bounded —
    /// the "under the feedback threshold, the monitor just adds a little" case.
    #[test]
    fn a_below_unity_loop_through_real_transducers_settles() {
        let ps = run_feedback_loop(0.01, 50.0, 0.01, 10.0, 20);
        assert!((ps[0] - 0.5).abs() < 1e-4, "block 1: got {}", ps[0]);
        assert!((ps[1] - 0.75).abs() < 1e-4, "block 2: got {}", ps[1]);
        assert!((ps[2] - 0.875).abs() < 1e-4, "block 3: got {}", ps[2]);
        assert!(
            (ps[19] - 1.0).abs() < 1e-3,
            "settles to 1 Pa, got {}",
            ps[19]
        );
        assert!(
            ps.iter().all(|&p| p <= 1.0 + 1e-6),
            "a below-unity loop stays bounded below its fixed point"
        );
    }

    /// **Above unity, howlround emerges and self-limits at the rail.** The same mic at gain 200 ⇒
    /// `L = 2.0`: the pressure climbs 2, 6, … until the amp pins its ±10 V rail, which (at 1 Pa/V)
    /// caps the monitor at **10 Pa** — the Larsen howl, ceilinged by the physics, not special-cased.
    #[test]
    fn a_super_unity_loop_through_real_transducers_rings_up_to_the_rail() {
        let ps = run_feedback_loop(0.01, 200.0, 0.01, 10.0, 8);
        assert!((ps[0] - 2.0).abs() < 1e-3, "block 1: got {}", ps[0]);
        assert!((ps[1] - 6.0).abs() < 1e-3, "block 2: got {}", ps[1]);
        assert!(ps[1] > ps[0], "the loop is building toward feedback");
        assert!(
            (ps[7] - 10.0).abs() < 1e-3,
            "pinned at the 10 Pa rail ceiling, got {}",
            ps[7]
        );
        assert!(
            ps.iter().all(|&p| p <= 10.0 + 1e-3),
            "the howl never exceeds the rail ceiling"
        );
    }

    /// **A dynamic resists feedback that makes a condenser howl.** At the *same* gain (200), the
    /// ~14 dB-less-sensitive dynamic (2 mV/Pa) stays stable where the sensitive condenser-like mic
    /// (10 mV/Pa) rings away:
    ///   condenser: `L = 200·0.01 = 2.0` (> 1) ⇒ howls up to the 10 Pa rail
    ///   dynamic:   `L = 200·0.002 = 0.4` (< 1) ⇒ settles at `A/(1−L) = 2.0/0.6 = 3.333 Pa`, clear of it
    /// This is the emergent "why loud stages favour dynamics" lesson — the same condenser-vs-dynamic
    /// sensitivity gap, now as feedback headroom.
    #[test]
    fn a_dynamic_resists_feedback_that_makes_a_condenser_howl() {
        let cond = run_feedback_loop(0.01, 200.0, 0.01, 10.0, 12);
        let dynamic = run_feedback_loop(0.002, 200.0, 0.01, 10.0, 40);
        assert!(
            (cond[11] - 10.0).abs() < 1e-3,
            "the condenser howls to the rail, got {}",
            cond[11]
        );
        assert!(
            (dynamic[39] - 3.3333).abs() < 1e-2,
            "the dynamic settles below the rail, got {}",
            dynamic[39]
        );
        assert!(
            dynamic.iter().all(|&p| p < 10.0),
            "the dynamic never reaches the feedback ceiling"
        );
        // The gap is the sensitivity ratio: the dynamic needs 20·log10(0.01/0.002) = 13.98 dB more
        // gain to reach the same loop unity.
        let gap_db = 20.0 * (0.01_f32 / 0.002).log10();
        assert!(
            (gap_db - 13.979).abs() < 1e-2,
            "the feedback-resistance headroom is ~14 dB, got {gap_db}"
        );
    }
}

/// White-box tests for the **acoustic carrier**: an ideal-air pressure edge (`AcousticRoute`) is a
/// unity copy at the analog rate, and an acoustic↔analog edge has no bridge on a wire (a microphone
/// is the transducer, and it lives *inside* a node), so `compile` rejects it as a `DomainMismatch`.
/// Scaffolding mirrors `digital_seam`/`event_seam`; tests inspect the private pools (white-box).
mod acoustic_seam {
    use super::super::*;
    use crate::electrical::{InputZ, Ohms};
    use crate::node::TestSource;
    use crate::port::{AcousticFace, InputPort, OutputPort};
    use crate::signal::{Pascals, Volts};

    fn analog_rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A sound source: no inputs, one acoustic output filled with a constant pressure.
    struct AcousticSource {
        pressure: f32,
        outputs: [OutputPort; 1],
    }
    impl AcousticSource {
        fn new(pressure: Pascals) -> Self {
            Self {
                pressure: pressure.get(),
                outputs: [AcousticFace::new().into()],
            }
        }
    }
    impl Node for AcousticSource {
        fn inputs(&self) -> &[InputPort] {
            &[]
        }
        fn outputs(&self) -> &[OutputPort] {
            &self.outputs
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
            outputs[0].pressure_mut().fill(Pascals::new(self.pressure));
        }
    }

    /// An acoustic sink: one acoustic input, no outputs. A no-op — tests read its input lane.
    struct AcousticSink {
        inputs: [InputPort; 1],
    }
    impl AcousticSink {
        fn new() -> Self {
            Self {
                inputs: [AcousticFace::new().into()],
            }
        }
    }
    impl Node for AcousticSink {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &[]
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], _outputs: &mut [Lane]) {}
    }

    /// An analog sink: one analog input, no outputs — used to force an acoustic↔analog mismatch.
    struct AnalogSink {
        inputs: [InputPort; 1],
    }
    impl AnalogSink {
        fn new() -> Self {
            Self {
                inputs: [InputZ::new(Ohms::new(10_000.0)).into()],
            }
        }
    }
    impl Node for AnalogSink {
        fn inputs(&self) -> &[InputPort] {
            &self.inputs
        }
        fn outputs(&self) -> &[OutputPort] {
            &[]
        }
        fn process(&mut self, _params: &Params, _inputs: &[Lane], _outputs: &mut [Lane]) {}
    }

    #[test]
    fn acoustic_lanes_are_mono_at_the_analog_rate() {
        // Unlike a digital lane (decimated to block_len / M), a pressure lane is a dense block at
        // the analog rate — one lane per port (mono), the full block length.
        let mut g = Graph::new();
        let src = g.add(AcousticSource::new(Pascals::new(1.0)));
        let sink = g.add(AcousticSink::new());
        g.connect_ideal(src, 0, sink, 0);
        g.set_output(src, 0); // acoustic tap; this test only inspects the pool, never calls process
        let sched = compile(g, 16, analog_rate(), 0).expect("valid acoustic chain");

        let pressure_lanes: Vec<&Lane> = sched
            .output_pool
            .iter()
            .chain(sched.input_pool.iter())
            .filter(|l| matches!(l, Lane::Pressure(_)))
            .collect();
        assert_eq!(
            pressure_lanes.len(),
            2,
            "one source-output + one sink-input pressure lane"
        );
        for lane in pressure_lanes {
            assert_eq!(lane.domain(), Domain::Acoustic);
            assert_eq!(
                lane.len(),
                16,
                "an acoustic lane is the full block (no decimation)"
            );
        }
    }

    #[test]
    fn acoustic_route_copies_pressure_unchanged() {
        // A separate analog node is the (voltage) output tap so `process` can run; the acoustic
        // source → sink component runs alongside, and its AcousticRoute copies the pressure 1:1
        // (ideal air — no divider, no rolloff).
        let mut g = Graph::new();
        let atap = g.add(TestSource::new(Volts::new(1.0), Ohms::new(150.0)));
        g.set_output(atap, 0);
        let src = g.add(AcousticSource::new(Pascals::new(0.7)));
        let sink = g.add(AcousticSink::new());
        g.connect_ideal(src, 0, sink, 0);

        let mut sched = compile(g, 16, analog_rate(), 0).expect("valid mixed chain");
        let mut out = VoltageBuffer::zeros(16, analog_rate());
        sched.process(&mut out);

        // The sink's input pressure lane received the source's 0.7 Pa, unattenuated.
        let sink_in = sched
            .input_pool
            .iter()
            .find(|l| matches!(l, Lane::Pressure(_)))
            .expect("an acoustic input lane");
        assert!(sink_in.pressure().as_slice().iter().all(|&p| p == 0.7));
    }

    #[test]
    fn rejects_acoustic_into_analog() {
        // A pressure output into a voltage input: no physics bridges the domains on a wire (the
        // microphone transducer lives inside a node), so compile rejects the edge.
        let mut g = Graph::new();
        let src = g.add(AcousticSource::new(Pascals::new(1.0)));
        let sink = g.add(AnalogSink::new());
        g.connect_ideal(src, 0, sink, 0);
        g.set_output(src, 0);
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
}
