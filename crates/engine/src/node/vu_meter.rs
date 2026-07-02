//! A VU meter — a voltage-native metering node.

use super::Node;
use crate::dsp::flush_denormal;
use crate::electrical::{InputZ, Ohms, OutputZ};
use crate::level::volts_to_dbu;
use crate::param::Params;
use crate::port::{InputPort, OutputPort};
use crate::readout::{ReadoutDecl, ReadoutId};
use crate::signal::{AnalogRate, Lane, Volts};

/// A VU meter: a buffered, **bridging** inline meter that measures the voltage passing through it
/// and passes the signal on unchanged. It taps volts and computes two scalar
/// [`readouts`](Node::readouts) — a ballistic **VU** reading and a **peak** level in dBu — that the
/// host reads back over the node→host lane.
///
/// **Measurement emerges from the volts.** Metering is a *node*, not a flag on other nodes: the VU
/// reading is derived from the real voltage the meter sees, exactly as a physical meter would derive
/// it. Being **inline passthrough** (unity gain, a high bridging `InputZ`, a buffered `OutputZ`), it
/// can be inserted anywhere in a chain without changing the sound — it bridges the line rather than
/// terminating it.
///
/// **Ballistics + calibration.** The VU scale is `0 VU ≙ +4 dBu ≙ 1.228 V RMS`. Like a real
/// averaging VU meter it rectifies the signal and smooths it with a one-pole averager, so it reads a
/// *quasi-RMS* level with the classic ~300 ms integration (a `τ` of ~65 ms one-pole gives ~300 ms to
/// 99 %). The rectified average is converted back to an equivalent **sine** RMS (dividing by the
/// sine form factor `2√2/π`) so a sine at 1.228 V RMS reads exactly 0 VU — the meter is calibrated
/// for sine, and (faithfully) mis-reads non-sinusoidal waveforms just as an averaging meter does.
///
/// One input; one output.
pub struct VuMeter {
    /// One-pole averager coefficient `a = 1 − e^(−1/(τ·fs))`, baked in [`prepare`](Node::prepare)
    /// from the analog rate; 0 until prepared (the meter reads silence).
    coeff: f64,
    /// The rectified-average envelope, in volts (`f64` so the recurrence doesn't drift). Persists
    /// across blocks — it *is* the meter's ballistic state.
    env: f64,
    /// The largest `|v|` seen in the block just processed, for the peak-dBu readout. Reset each
    /// block, so it's a per-block peak the host can hold/decay as it likes.
    peak: f32,
    readouts: [ReadoutDecl; 2],
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl VuMeter {
    /// The ballistic **VU** reading, in VU (0 VU = +4 dBu). The host drives nothing here — it reads
    /// the value back with `(node, VuMeter::VU)`.
    pub const VU: ReadoutId = ReadoutId(0);
    /// The block **peak** level, in dBu (`20·log10(peak / 0.7746 V)`). Sits ~3 dB above the VU
    /// reading for a steady sine (the crest factor).
    pub const PEAK_DBU: ReadoutId = ReadoutId(1);

    /// 0 VU reference on the pro-audio scale: +4 dBu (≙ 1.228 V RMS). The VU reading is expressed
    /// relative to this.
    const ZERO_VU_DBU: f32 = 4.0;
    /// Rectified-average ÷ RMS for a sine, `2√2/π`. The averaging detector reads the rectified
    /// average; dividing by this recovers the equivalent sine RMS, calibrating the meter for sine.
    const SINE_AVG_PER_RMS: f64 = 0.900_316_316_157_106;
    /// Reading floor so silence (−∞ dB) reports a finite, off-scale value.
    const READING_FLOOR_DB: f32 = -60.0;
    /// The one-pole averaging time constant. ~65 ms gives the classic VU ~300 ms rise to 99 %
    /// (300 ms / ln 100 ≈ 65 ms). The steady-state reading is independent of it.
    const TIME_CONSTANT_MS: f64 = 65.0;

    /// The bridging input impedance (high, so the meter doesn't load the line it taps) and the
    /// buffered output impedance it re-drives from.
    const Z_IN: f32 = 1_000_000.0;
    const Z_OUT: f32 = 150.0;

    /// A VU meter at unity, bridging the line with a high `InputZ`. Its ballistics coefficient is
    /// baked at [`prepare`](Node::prepare); until then it reads silence.
    #[must_use]
    pub fn new() -> Self {
        Self {
            coeff: 0.0,
            env: 0.0,
            peak: 0.0,
            readouts: [
                ReadoutDecl { id: Self::VU },
                ReadoutDecl { id: Self::PEAK_DBU },
            ],
            inputs: [InputZ::new(Ohms::new(Self::Z_IN)).into()],
            outputs: [OutputZ::new(Ohms::new(Self::Z_OUT)).into()],
        }
    }

    /// A level in volts as dBu, floored so silence reads a finite off-scale value rather than −∞.
    fn floored_dbu(level: f32) -> f32 {
        if level > 0.0 {
            volts_to_dbu(Volts::new(level)).max(Self::READING_FLOOR_DB)
        } else {
            Self::READING_FLOOR_DB
        }
    }
}

impl Default for VuMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl Node for VuMeter {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn readouts(&self) -> &[ReadoutDecl] {
        &self.readouts
    }

    fn prepare(&mut self, rate: AnalogRate) {
        // a = 1 − e^(−1/(τ·fs)): the one-pole averager reaches ~63 % of a step in τ seconds.
        let n = Self::TIME_CONSTANT_MS * 1e-3 * rate.as_hz();
        self.coeff = if n > 0.0 { 1.0 - (-1.0 / n).exp() } else { 1.0 };
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let src = inputs[0].voltage().as_slice();
        let out = outputs[0].voltage_mut().as_mut_slice();
        let mut env = self.env;
        let mut peak = 0.0_f32; // per-block peak: reset each block
        for (o, &v) in out.iter_mut().zip(src) {
            // Pass the signal through unchanged (unity bridging buffer)…
            *o = v;
            // …while rectify-and-average tracks the VU envelope and the block peak is held.
            let rect = f64::from(v.abs());
            env = flush_denormal(env + self.coeff * (rect - env));
            let mag = v.abs();
            if mag > peak {
                peak = mag;
            }
        }
        self.env = env;
        self.peak = peak;
    }

    fn read_readouts(&self, out: &mut [f32]) {
        // Rectified average → equivalent sine RMS → dBu, expressed relative to 0 VU (+4 dBu).
        let rms_equiv = (self.env / Self::SINE_AVG_PER_RMS) as f32;
        out[0] = (Self::floored_dbu(rms_equiv) - Self::ZERO_VU_DBU).max(Self::READING_FLOOR_DB);
        out[1] = Self::floored_dbu(self.peak);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::schedule::compile;
    use crate::signal::VoltageBuffer;
    use crate::test_util::{SineSource, process_voltage, sine};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// Peak amplitude of a 1.228 V RMS sine: A = RMS·√2 = 1.228 × 1.414214 ≈ 1.736706 V.
    fn ref_sine_peak() -> f32 {
        1.228 * std::f32::consts::SQRT_2
    }

    #[test]
    fn declares_faces_and_readouts() {
        let m = VuMeter::new();
        assert_eq!(
            m.inputs(),
            &[InputPort::Analog(InputZ::new(Ohms::new(VuMeter::Z_IN)))]
        );
        assert_eq!(
            m.outputs(),
            &[OutputPort::Analog(OutputZ::new(Ohms::new(VuMeter::Z_OUT)))]
        );
        assert_eq!(m.readouts().len(), 2);
    }

    #[test]
    fn passes_the_signal_through_unchanged() {
        // Inline passthrough: the meter is signal-transparent (unity), so inserting it doesn't
        // change the waveform.
        let mut m = VuMeter::new();
        m.prepare(rate());
        let mut input = [VoltageBuffer::zeros(8, rate())];
        input[0].fill(Volts::new(0.42));
        let mut out = [VoltageBuffer::zeros(8, rate())];
        process_voltage(&mut m, &input, &mut out);
        assert!(out[0].as_slice().iter().all(|&v| (v - 0.42).abs() < 1e-6));
    }

    #[test]
    fn reads_zero_vu_for_the_reference_sine() {
        // Hand calc: 0 VU = +4 dBu = 0.7746·10^(4/20) ≈ 1.228 V RMS. A sine at that RMS has peak
        // A = 1.228·√2 ≈ 1.7367 V; its rectified average is 2A/π ≈ 1.1054 V, which ÷ (2√2/π)
        // recovers 1.228 V RMS ⇒ volts_to_dbu(1.228) − 4 dBu = 0 VU. Feed ~0.5 s (≫ 5τ) so the
        // averager settles.
        let len = 192_000; // 0.5 s at 384 kHz
        let input = [sine(1_000.0, Volts::new(ref_sine_peak()), len, rate())];
        let mut out = [VoltageBuffer::zeros(len, rate())];
        let mut m = VuMeter::new();
        m.prepare(rate());
        process_voltage(&mut m, &input, &mut out);

        let mut r = [0.0_f32; 2];
        m.read_readouts(&mut r);
        assert_relative_eq!(r[0], 0.0, epsilon = 0.1); // VU
        // Peak sits a crest factor above the RMS: peak = 1.7367 V ⇒ volts_to_dbu ≈ +7.01 dBu
        // (i.e. 0 VU + 20·log10(√2) ≈ +3.01 dB above the +4 dBu RMS level).
        assert_relative_eq!(r[1], 7.01, epsilon = 0.05); // peak dBu
    }

    #[test]
    fn silence_reads_the_floor() {
        let mut m = VuMeter::new();
        m.prepare(rate());
        let input = [VoltageBuffer::zeros(64, rate())];
        let mut out = [VoltageBuffer::zeros(64, rate())];
        process_voltage(&mut m, &input, &mut out);
        let mut r = [0.0_f32; 2];
        m.read_readouts(&mut r);
        assert_eq!(r[0], VuMeter::READING_FLOOR_DB);
        assert_eq!(r[1], VuMeter::READING_FLOOR_DB);
    }

    /// End-to-end through a compiled schedule: `compile` prepares the meter (baking its coefficient)
    /// and the reading reaches the readout store. A 1.228 V RMS sine driven through a ~1 Ω source
    /// into the meter's 1 MΩ bridging input is essentially unloaded, so the settled reading is 0 VU.
    #[test]
    fn reading_reaches_the_schedule_store() {
        let block = 384;
        let mut g = Graph::new();
        let src = g.add(SineSource::new(
            1_000.0,
            Volts::new(ref_sine_peak()),
            Volts::new(0.0),
            Ohms::new(1.0),
        ));
        let vu = g.add(VuMeter::new());
        g.connect_ideal(src, 0, vu, 0);
        g.set_output(vu, 0);
        let mut sched = compile(g, block, rate(), 0).expect("compiles");
        let handle = sched.readout(vu, VuMeter::VU).expect("VU readout resolves");

        let mut out = VoltageBuffer::zeros(block, rate());
        for _ in 0..500 {
            // ~0.5 s, past 5τ
            sched.process(&mut out);
        }
        let reading = sched.readout_value(handle).expect("value present");
        assert_relative_eq!(reading, 0.0, epsilon = 0.15);
    }
}
