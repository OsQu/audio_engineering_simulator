//! A dynamic microphone: a passive balanced acoustic→analog transducer.

use super::Node;
use crate::electrical::{Ohms, OutputZ};
use crate::param::Params;
use crate::port::{AcousticFace, InputPort, OutputPort};
use crate::signal::Lane;

/// A dynamic microphone — a **passive** balanced acoustic→analog transducer that needs **no power to
/// operate**.
///
/// Like the [`CondenserMic`](super::CondenserMic) it converts capsule sound pressure to a balanced
/// mic-level voltage (`s = pressure · sensitivity`, the sensitivity set at [`new`](Self::new) in
/// volts per pascal),
/// but where the condenser carries active impedance-converter electronics that must be phantom-fed,
/// a dynamic mic is just a coil moving in a magnet's field (a tiny generator): it produces its EMF
/// from the diaphragm's motion, so it draws **no DC** and declares **no
/// [`phantom_load`](Node::phantom_load)**. It is therefore **always alive** — there is no operating
/// point to solve, no [`resolve_phantom`](Node::resolve_phantom), no threshold to cross. A +48 V
/// supply facing this output simply finds nothing to power (the compile-time solve skips it), so the
/// mic sounds identical whether phantom is on or off.
///
/// Because nothing biases the line, the output sits at a **0 V common-mode pedestal** — the audio is
/// purely differential, floating symmetrically about ground:
///
/// ```text
///   V+ = +s/2,  V− = −s/2    ⇒    common-mode = 0,  differential = s
/// ```
///
/// This is the passive counterpart to the condenser's DC-pedestal output: the *audio* recovered at a
/// balanced receiver's difference is the same species of signal `s`, only here there is no phantom
/// DC riding underneath it.
///
/// # Less sensitive than a condenser
/// A moving coil is a far less efficient transducer than a charged capsule with an active head amp,
/// so a dynamic mic delivers a **weaker signal for the same sound** — typically ~14 dB below a studio
/// condenser. That is modelled directly as a lower **sensitivity** (the catalog builds it at
/// ~2 mV/Pa vs the condenser's ~10 mV/Pa): same units, just a less efficient capsule, so it leans
/// harder on the preamp's gain. Sensitivity is a fixed property, not a knob — the mic has no control
/// params, and an **unpatched** mic (no sound at its capsule) is silent.
///
/// One acoustic input; one balanced analog output.
pub struct DynamicMic {
    /// Transducer sensitivity, volts per pascal (`s = pressure · sensitivity`).
    sensitivity: f32,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl DynamicMic {
    /// A dynamic mic of `sensitivity` volts per pascal (~0.002 = 2 mV/Pa is a typical, quieter
    /// moving-coil value), driving from balanced output impedance `z_out`. Being passive, it needs
    /// no power: it is alive the moment it is processed.
    #[must_use]
    pub fn new(sensitivity: f32, z_out: Ohms) -> Self {
        Self {
            sensitivity,
            inputs: [AcousticFace::new().into()],
            outputs: [OutputZ::balanced(z_out).into()],
        }
    }
}

impl Node for DynamicMic {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    // Deliberately no `phantom_load` / `resolve_phantom`: a passive coil draws no DC, so it declares
    // no operating point and the compile-time phantom solve has nothing to resolve against it.

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // A passive mic is always alive: the audio is purely differential about a 0 V common-mode,
        // so the hot loop transduces the capsule pressure (× sensitivity) and splits it across the
        // two legs. A microphone is the acoustic→analog converter.
        let pressure = inputs[0].pressure().as_slice();
        let (hot, cold) = outputs.split_at_mut(1);
        let hot = hot[0].voltage_mut().as_mut_slice();
        let cold = cold[0].voltage_mut().as_mut_slice();
        for ((h, c), &p) in hot.iter_mut().zip(cold.iter_mut()).zip(pressure) {
            let half = 0.5 * self.sensitivity * p;
            *h = half;
            *c = -half;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::level::spl_to_pascals;
    use crate::node::{BalancedReceiver, SoundSource};
    use crate::schedule::compile;
    use crate::signal::{AnalogRate, Domain, Pascals, PressureBuffer, VoltageBuffer};
    use approx::assert_relative_eq;

    const RATE_HZ: f64 = 384_000.0;

    fn rate() -> AnalogRate {
        AnalogRate::new(RATE_HZ)
    }

    /// A mic of `sensitivity` (V/Pa) fed a constant capsule pressure `p` for `n` samples; returns
    /// the `[hot, cold]` leg buffers.
    fn run_mic(sensitivity: f32, p: Pascals, n: usize) -> [VoltageBuffer; 2] {
        let mut m = DynamicMic::new(sensitivity, Ohms::new(200.0));
        let in_lane = [Lane::Pressure(PressureBuffer::from_pascals(
            vec![p.get(); n],
            rate(),
        ))];
        let mut out = [
            Lane::Voltage(VoltageBuffer::zeros(n, rate())),
            Lane::Voltage(VoltageBuffer::zeros(n, rate())),
        ];
        m.process(&Params::EMPTY, &in_lane, &mut out);
        let mut it = out.into_iter().map(|l| match l {
            Lane::Voltage(b) => b,
            _ => unreachable!(),
        });
        [it.next().unwrap(), it.next().unwrap()]
    }

    #[test]
    fn declares_an_acoustic_input_and_a_passive_balanced_output() {
        let m = DynamicMic::new(0.002, Ohms::new(200.0));
        assert_eq!(m.inputs().len(), 1);
        assert_eq!(m.inputs()[0].domain(), Domain::Acoustic);
        assert_eq!(m.outputs()[0].lane_count(), 2);
        // A passive source declares no DC appetite — the defining difference from the condenser.
        assert!(
            m.phantom_load(0).is_none(),
            "a dynamic mic draws no phantom"
        );
        assert!(m.phantom_load(1).is_none());
        // No control params — sensitivity is fixed.
        assert!(m.params().is_empty());
    }

    #[test]
    fn transduces_pressure_about_zero_with_no_power() {
        // No phantom is ever resolved — a passive mic is alive immediately. At 94 dB SPL (1 Pa) with
        // 2 mV/Pa sensitivity the differential is 1 Pa · 0.002 = 2 mV, and:
        //   1. common-mode (V+ + V−)/2 = 0, exactly — no DC pedestal, the audio floats.
        //   2. differential V+ − V− = 0.002 V — the transduced 94 dB source.
        let [hot, cold] = run_mic(0.002, spl_to_pascals(94.0), 8);
        for k in 0..8 {
            let (vp, vn) = (hot.get(k).get(), cold.get(k).get());
            assert_relative_eq!((vp + vn) / 2.0, 0.0, epsilon = 1e-9); // no pedestal, ever
            // 94 dB SPL = 1.00237 Pa · 0.002 = 0.00200474 V.
            assert_relative_eq!(vp - vn, 0.002_004_74, epsilon = 1e-6);
            assert_relative_eq!(vp, -vn, epsilon = 1e-9); // exact mirror about 0 V
        }
    }

    #[test]
    fn sensitivity_spans_the_micing_range() {
        // A passive 2 mV/Pa mic transduces linearly across the whole loudness range (no pedestal to
        // swamp small signals, so these are exact). A whisper (30 dB SPL = 632.46 µPa) is a tiny
        // 1.26 µV differential; a cranked amp (120 dB SPL = 20 Pa) is a hot 40 mV.
        let [hot, cold] = run_mic(0.002, spl_to_pascals(30.0), 4);
        // 632.456 µPa · 0.002 = 1.26491 µV.
        assert_relative_eq!(
            hot.get(0).get() - cold.get(0).get(),
            1.264_91e-6,
            epsilon = 1e-10
        );

        let [hot, cold] = run_mic(0.002, spl_to_pascals(120.0), 4);
        // 20 Pa · 0.002 = 0.040 V.
        assert_relative_eq!(hot.get(0).get() - cold.get(0).get(), 0.040, epsilon = 1e-5);
    }

    #[test]
    fn about_14_db_less_sensitive_than_a_condenser() {
        // 2 mV/Pa vs 10 mV/Pa is a factor of 5 = 20·log10(5) = 13.98 dB — the emergent reason a
        // dynamic mic needs markedly more preamp gain than a condenser on the same source.
        let db = 20.0 * (0.01_f32 / 0.002).log10();
        assert_relative_eq!(db, 13.979, epsilon = 1e-2);
    }

    #[test]
    fn end_to_end_source_over_air_to_mic_recovers_pressure_times_sensitivity() {
        // The Story 8.1 gate: a SoundSource radiates over an ideal acoustic edge into a dynamic mic,
        // whose balanced output a receiver differences back to single-ended. The recovered audio is
        // `pressure · sensitivity`: a 94 dB SPL (peak 1.00237 Pa) source into a 2 mV/Pa mic ⇒ a
        // 2.0047 mV peak (the mic→receiver edge is ≈unity: 1e9/(200 + 1e9) ≈ 1).
        fn recovered_peak(spl_db: f32, sensitivity: f32) -> f32 {
            let mut g = Graph::new();
            let src = g.add(SoundSource::new(spl_to_pascals(spl_db)));
            let mic = g.add(DynamicMic::new(sensitivity, Ohms::new(200.0)));
            let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
            g.connect_ideal(src, 0, mic, 0); // acoustic edge (ideal air)
            g.connect_ideal(mic, 0, rcv, 0); // balanced analog edge
            g.set_output(rcv, 0);
            let block = 8192;
            let mut sched = compile(g, block, rate(), 0).expect("source → mic → receiver builds");
            let mut out = VoltageBuffer::zeros(block, rate());
            sched.process(&mut out);
            out.as_slice().iter().fold(0.0_f32, |m, &v| m.max(v.abs()))
        }
        // 94 dB SPL = 1.00237 Pa · 0.002 V/Pa = 2.00474 mV.
        assert_relative_eq!(recovered_peak(94.0, 0.002), 0.002_004_74, epsilon = 5e-6);
        // A cranked 120 dB SPL source = 20 Pa · 0.002 = 40 mV — 20× hotter, as the linear scale says.
        assert_relative_eq!(recovered_peak(120.0, 0.002), 0.040, epsilon = 1e-4);
    }
}
